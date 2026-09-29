//! SORT and MERGE at compile time: what is checked, and the special registers a program with an
//! SD gets. They run in `machine::sort`.

use crate::layout::{Kind, Layout, Resolved};
use crate::Check;
use syntax::ast::*;
use syntax::{Error, Pos};

/// The sort special registers as Enterprise COBOL defines them implicitly.
const REGISTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    "       01  SORT-RETURN PIC S9(4) BINARY VALUE ZERO.\n",
    "       01  SORT-CONTROL PIC X(8) VALUE 'IGZSRTCD'.\n",
    "       01  SORT-CORE-SIZE PIC S9(8) BINARY VALUE ZERO.\n",
    "       01  SORT-FILE-SIZE PIC S9(8) BINARY VALUE ZERO.\n",
    "       01  SORT-MESSAGE PIC X(8) VALUE 'SYSOUT'.\n",
    "       01  SORT-MODE-SIZE PIC S9(5) BINARY VALUE ZERO.\n",
);

/// A program with an SD gets the sort special registers after its own WORKING-STORAGE, except
/// any it declares itself.
pub(crate) fn with_special_registers(mut program: Program) -> Program {
    if !program.files.iter().any(|f| f.sort) {
        return program;
    }
    let Ok(registers) = syntax::parse(REGISTERS) else { return program };
    let declared: Vec<String> = program
        .working_storage
        .iter()
        .chain(&program.local_storage)
        .chain(&program.linkage)
        .chain(program.files.iter().flat_map(|f| &f.records))
        .filter_map(|e| e.name.clone())
        .collect();
    for mut entry in registers.working_storage {
        if entry.name.as_ref().is_some_and(|n| !declared.contains(n)) {
            entry.pos = Pos::default();
            program.working_storage.push(entry);
        }
    }
    program
}

/// A key of a table SORT: the table's element itself, or an item within it.
pub(crate) fn table_key(layout: &Layout, table: usize, name: &str) -> Option<usize> {
    if layout.items[table].name.as_deref() == Some(name) {
        return Some(table);
    }
    layout.items[table].children.iter().find_map(|&c| table_key(layout, c, name))
}

/// Keys whose comparison a collating sequence changes: alphanumeric and edited ones.
pub(crate) fn collates(kind: Kind) -> bool {
    matches!(kind, Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. })
}

impl Check<'_> {
    pub(crate) fn sorting(&mut self, s: &Sorting) {
        match s {
            Sorting::Sort(st) => match self.program.files.iter().position(|f| f.name == st.subject.name) {
                Some(k) if st.subject.qualifiers.is_empty() && st.subject.subscripts.is_empty() => self.sort_file(st, k),
                _ => self.sort_table(st),
            },
            Sorting::Release { record, from, pos } => {
                self.reference(record);
                let item = self.item(record);
                let sd = item.and_then(|i| self.layout.items[i].file).map(usize::from);
                if item.is_some() && !sd.is_some_and(|k| self.program.files[k].sort) {
                    self.errors.push(Error::at(*pos, format!("RELEASE {}: not a record of a sort file (SD)", record.name)));
                }
                if let Some(op) = from {
                    self.operand(op);
                }
            }
            Sorting::Return { file, into, at_end, pos } => {
                match self.program.files.iter().find(|f| f.name == *file) {
                    None => self.errors.push(Error::at(*pos, format!("no file named {file}"))),
                    Some(f) if !f.sort => self.errors.push(Error::at(*pos, format!("RETURN {file}: not a sort or merge file (SD)"))),
                    Some(_) => {}
                }
                if let Some(r) = into {
                    self.reference(r);
                }
                self.handlers(at_end);
            }
        }
    }

    fn sort_file(&mut self, st: &SortStmt, sd: usize) {
        let verb = if st.merge { "MERGE" } else { "SORT" };
        let name = &st.subject.name;
        let fail = |c: &mut Self, pos: Pos, m: String| c.errors.push(Error::at(pos, m));
        if !self.program.files[sd].sort {
            fail(self, st.pos, format!("{verb} {name}: not a sort or merge file (SD)"));
            return;
        }
        if st.keys.is_empty() {
            fail(self, st.pos, format!("{verb} {name}: no ASCENDING or DESCENDING KEY"));
        }
        let layout = self.layout;
        for (_, key) in &st.keys {
            if key.name == *name {
                fail(self, key.pos, format!("{verb} {name}: KEY needs a data name"));
                continue;
            }
            self.reference(key);
            let Some(i) = self.item(key) else { continue };
            let item = &layout.items[i];
            if item.file != Some(sd as u16) {
                fail(self, key.pos, format!("{}: a key of {verb} {name} must be in its records", key.name));
            } else if !item.dims.is_empty() {
                fail(self, key.pos, format!("{}: a sort key cannot be in a table", key.name));
            } else if matches!(item.kind, Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer) {
                fail(self, key.pos, format!("{}: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key", key.name));
            }
        }
        self.collating_sequence(st);
        let files = |c: &mut Self, io: &Option<SortIo>, phrase: &str, procedure: &str| match io {
            None => fail(c, st.pos, format!("{verb} {name}: no {phrase} or {procedure}")),
            Some(SortIo::Procedure { from, thru }) => {
                c.procedure(from, st.pos);
                if let Some(t) = thru {
                    c.procedure(t, st.pos);
                }
            }
            Some(SortIo::Files(names)) => {
                for f in names {
                    match c.program.files.iter().find(|d| d.name == *f) {
                        None => fail(c, st.pos, format!("no file named {f}")),
                        Some(d) if d.sort => fail(c, st.pos, format!("{phrase} {f}: a sort or merge file (SD) cannot be one")),
                        Some(d) if d.access == Access::Random => fail(c, st.pos, format!("{phrase} {f}: the file's ACCESS MODE is RANDOM")),
                        Some(_) => {}
                    }
                }
                if st.merge && phrase == "USING" && names.len() < 2 {
                    fail(c, st.pos, format!("MERGE {name}: USING names at least two files"));
                }
            }
        };
        files(self, &st.input, "USING", "INPUT PROCEDURE");
        files(self, &st.output, "GIVING", "OUTPUT PROCEDURE");
    }

    fn sort_table(&mut self, st: &SortStmt) {
        let name = &st.subject.name;
        let fail = |c: &mut Self, pos: Pos, m: String| c.errors.push(Error::at(pos, m));
        if st.merge {
            fail(self, st.pos, format!("MERGE {name}: not a merge file (SD)"));
            return;
        }
        if st.input.is_some() || st.output.is_some() {
            fail(self, st.pos, format!("SORT {name}: a table SORT takes no USING, GIVING or procedures"));
        }
        let layout = self.layout;
        let t = match layout.resolve(name, &st.subject.qualifiers, st.subject.pos) {
            Ok(Resolved::Item(t)) => t,
            Ok(Resolved::Condition(_)) => return fail(self, st.pos, format!("SORT {name}: not a table")),
            Err(_) => return fail(self, st.pos, format!("no file or table named {name}")),
        };
        let table = &layout.items[t];
        if !table.table {
            return fail(self, st.pos, format!("SORT {name}: not a table (no OCCURS)"));
        }
        if st.subject.subscripts.len() + 1 != table.dims.len() {
            fail(self, st.pos, format!("SORT {name}: a subscript for each table that contains it, and none for itself"));
        }
        st.subject.subscripts.iter().for_each(|e| self.expr(e));
        let keys: Vec<(bool, Ref)> = if st.keys.is_empty() { table.keys.clone() } else { st.keys.clone() };
        if keys.is_empty() {
            fail(self, st.pos, format!("SORT {name}: no KEY phrase, and its OCCURS has none"));
        }
        for (_, key) in &keys {
            let Some(k) = table_key(layout, t, &key.name) else {
                fail(self, key.pos, format!("{}: a key of SORT {name} must be its element or an item within it", key.name));
                continue;
            };
            let item = &layout.items[k];
            if item.dims.len() != table.dims.len() {
                fail(self, key.pos, format!("{}: a table SORT key cannot be in a table within the element", key.name));
            } else if matches!(item.kind, Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer) {
                fail(self, key.pos, format!("{}: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key", key.name));
            }
        }
        self.collating_sequence(st);
    }

    /// The COLLATING SEQUENCE phrase names an alphabet of SPECIAL-NAMES, which `compile` has checked
    /// with the rest.
    fn collating_sequence(&mut self, st: &SortStmt) {
        if let Some(alphabet) = &st.collating
            && !self.program.environment.alphabets.iter().any(|(n, _)| n == alphabet)
        {
            self.errors.push(Error::at(st.pos, format!("COLLATING SEQUENCE {alphabet}: not an alphabet-name of SPECIAL-NAMES")));
        }
    }
}
