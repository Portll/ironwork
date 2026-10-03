//! `ironwork fuzz --interface`: a subprogram run as a caller would run it, with generated arguments
//! for its PROCEDURE DIVISION USING items (docs/evidence.md §5.2).
// The --interface dispatch arrives with roadmap 5.12 S8, after 4.4.11 lands in fuzz.rs.
#![cfg_attr(not(test), allow(dead_code))]

use exec::layout::{Layout, Resolved};
use rt::storage::Kind;
use rt::vocab::SignPosition;
use syntax::ast::{Arg, ExecKind, Expr, Literal, Operand, Program, Stmt};

use super::{Field, Rng, SPACE, elementary, field_bytes, neutral};

/// The IMS interfaces whose parameters are control blocks the IMS region supplies, not data a
/// caller passes.
const IMS_INTERFACES: &[&str] = &["CBLTDLI", "AIBTDLI", "CEETDLI"];

/// One PROCEDURE DIVISION USING item of a subprogram.
pub(crate) struct Param {
    pub(crate) name: String,
    /// Its LINKAGE record's size, every table at its most occurrences.
    pub(crate) size: usize,
    /// Its elementary items, by offset in the record.
    pub(crate) fields: Vec<Field>,
    /// Each OCCURS DEPENDING ON object in the record, with its table's fewest and most occurrences.
    pub(crate) counts: Vec<(Field, u32, u32)>,
}

/// What one CALL passes in one USING position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Passes {
    Omitted,
    /// An item of this length.
    Item(usize),
    /// A literal's bytes, which no input varies.
    Literal(Vec<u8>),
    /// Something whose length the call site does not tell: a function's result, an item the
    /// caller's layout does not resolve.
    Unknown,
}

/// A static CALL of the subprogram: where it is and what it passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallSite {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) passes: Vec<Passes>,
}

/// Every statement of the program, nested ones included.
fn statements<'a>(list: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
    for s in list {
        out.push(s);
        for body in exec::oo::bodies(s) {
            statements(body, out);
        }
    }
}

fn all_statements(program: &Program) -> Vec<&Stmt> {
    let mut out = Vec::new();
    for p in &program.paragraphs {
        statements(&p.statements, &mut out);
    }
    out
}

fn literal_text(l: &Literal) -> Option<&str> {
    match l {
        Literal::Alnum(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Why an interface run cannot take `compiled`, or None when it can: a program that takes no
/// arguments, one with a pointer among them, an IMS program, whose parameters are control blocks,
/// one that passes an argument on to a CALL whose target nothing names, and a CICS program.
pub(crate) fn refusal(compiled: &exec::Compiled) -> Option<String> {
    let program = &compiled.program;
    let layout = &compiled.layout;
    let id = &program.id;
    if program.using.is_empty() {
        return Some(format!("{id} has no PROCEDURE DIVISION USING items: fuzz it as a main program"));
    }
    for param in &program.using {
        let Some(&root) = layout.linkage_roots.iter().find(|&&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&param.name))) else {
            return Some(format!("{id}: USING item {} is not a LINKAGE record", param.name));
        };
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            let item = &layout.items[i];
            if matches!(item.kind, Kind::Pointer | Kind::ProgramPointer | Kind::ObjectReference) {
                return Some(format!("{id}: USING item {} holds {}, an address a caller supplies and fuzz cannot invent", param.name, item.name.as_deref().unwrap_or("a pointer")));
            }
            stack.extend(item.children.iter().copied());
        }
    }
    let statements = all_statements(program);
    for s in &statements {
        match s {
            Stmt::Entry { name, .. } if name.eq_ignore_ascii_case("DLITCBL") || name.eq_ignore_ascii_case("DLITPLI") => {
                return Some(format!("{id} has ENTRY '{name}': an IMS program, whose parameters are control blocks the region supplies"));
            }
            Stmt::Exec(block) if block.kind == ExecKind::Dli => return Some(format!("{id} has EXEC DLI: an IMS program, whose parameters are control blocks the region supplies")),
            Stmt::Exec(block) if block.kind == ExecKind::Cics => return Some(format!("{id} has EXEC CICS: fuzz it with --cics")),
            Stmt::Call(call) => {
                let named = match &call.target {
                    Operand::Literal(l) => literal_text(l).map(str::to_owned),
                    Operand::Ref(r) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Item(i)) => layout.items[i].value.as_ref().and_then(literal_text).map(|s| s.trim_end().to_owned()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(target) = named.as_deref().filter(|t| IMS_INTERFACES.iter().any(|ims| t.eq_ignore_ascii_case(ims))) {
                    return Some(format!("{id} CALLs {target}: an IMS program, whose parameters are control blocks the region supplies"));
                }
                let passes_linkage = call.using.iter().filter_map(|a| match &a.value {
                    Some(Operand::Ref(r)) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Item(i)) => layout.items[i].linkage.map(|_| r.name.as_str()),
                        _ => None,
                    },
                    _ => None,
                });
                if named.is_none()
                    && let Some(item) = passes_linkage.into_iter().next()
                {
                    return Some(format!("{id} passes {item} to a CALL at line {} whose target no literal or VALUE names: it may be an IMS interface", call.pos.line));
                }
            }
            _ => {}
        }
    }
    None
}

/// The program's USING items in order, each from its LINKAGE record.
pub(crate) fn params(compiled: &exec::Compiled) -> Vec<Param> {
    let layout = &compiled.layout;
    let record_of = |name: &str| layout.linkage_roots.iter().copied().find(|&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    compiled
        .program
        .using
        .iter()
        .filter_map(|param| {
            let root = record_of(&param.name)?;
            let record = &layout.items[root];
            let size = record.size as usize;
            let mut counts = Vec::new();
            let mut stack = vec![root];
            while let Some(i) = stack.pop() {
                let table = &layout.items[i];
                if let Some(r) = &table.depending_on
                    && let Ok(Resolved::Item(c)) = layout.resolve(&r.name, &r.qualifiers, r.pos)
                {
                    let count = &layout.items[c];
                    // A DEPENDING ON object outside the record is the subprogram's own, not the caller's.
                    if count.linkage == record.linkage && count.offset >= record.offset {
                        counts.push((Field { offset: (count.offset - record.offset) as usize, size: count.size as usize, kind: count.kind }, table.occurs_min, table.occurs));
                    }
                }
                stack.extend(table.children.iter().copied());
            }
            Some(Param { name: param.name.clone(), size, fields: elementary(layout, root, record.offset, size), counts })
        })
        .collect()
}

/// `n` stored in field `f` as its kind stores a number: zoned digits, packed decimal or a
/// big-endian binary; high digits that do not fit are dropped.
fn count_bytes(f: Field, n: u32) -> Vec<u8> {
    match f.kind {
        Kind::Zoned { signed, sign, .. } => {
            let separate = sign.is_some_and(|s| s.separate);
            let leading = sign.is_some_and(|s| s.position == SignPosition::Leading);
            let digits = if separate { f.size.saturating_sub(1) } else { f.size };
            let mut value = n;
            let mut d = vec![0xF0; digits];
            for at in d.iter_mut().rev() {
                *at = 0xF0 | (value % 10) as u8;
                value /= 10;
            }
            if separate {
                if leading { d.insert(0, super::PLUS) } else { d.push(super::PLUS) }
            } else if signed && digits > 0 {
                let at = if leading { 0 } else { digits - 1 };
                d[at] = (d[at] & 0x0F) | 0xC0;
            }
            d
        }
        Kind::Packed { signed, .. } => {
            let mut value = n;
            let mut nibbles = vec![0u8; 2 * f.size - 1];
            for at in nibbles.iter_mut().rev() {
                *at = (value % 10) as u8;
                value /= 10;
            }
            nibbles.push(if signed { 0x0C } else { 0x0F });
            nibbles.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
        }
        Kind::Binary { .. } => {
            let bytes = u64::from(n).to_be_bytes();
            let kept = f.size.min(bytes.len());
            let mut out = vec![0; f.size];
            out[f.size - kept..].copy_from_slice(&bytes[bytes.len() - kept..]);
            out
        }
        _ => neutral(f),
    }
}

/// A record of `param`'s size: every field within the first `varied` bytes generated, every other
/// one neutral, and each DEPENDING ON object at a count its table allows, drawn or at its most.
fn record(rng: &mut Rng, param: &Param, varied: usize, draw_counts: bool) -> Vec<u8> {
    let mut buf = vec![SPACE; param.size];
    for &f in &param.fields {
        let bytes = if f.offset + f.size <= varied { field_bytes(rng, f) } else { neutral(f) };
        buf[f.offset..f.offset + f.size].copy_from_slice(&bytes);
    }
    for &(f, min, max) in &param.counts {
        let n = if draw_counts { min + rng.below((max - min + 1) as usize) as u32 } else { max };
        buf[f.offset..f.offset + f.size].copy_from_slice(&count_bytes(f, n));
    }
    buf
}

/// One argument per param. With `site`, each takes what the CALL passes in its position: OMITTED
/// stays OMITTED, a literal is passed as written, padded with spaces, and an item of a known length
/// has only the fields within that length varied.
pub(crate) fn arguments(rng: &mut Rng, params: &[Param], site: Option<&CallSite>) -> Vec<Option<Vec<u8>>> {
    params
        .iter()
        .enumerate()
        .map(|(i, p)| match site.and_then(|s| s.passes.get(i)).unwrap_or(&Passes::Unknown) {
            Passes::Omitted => None,
            Passes::Literal(bytes) => Some(bytes.iter().copied().chain(std::iter::repeat(SPACE)).take(p.size).collect()),
            Passes::Item(len) => Some(record(rng, p, *len, true)),
            Passes::Unknown => Some(record(rng, p, usize::MAX, true)),
        })
        .collect()
}

/// Every param passed, every field neutral and every count at its most: the input whose abend is
/// not the input's doing.
pub(crate) fn neutral_arguments(params: &[Param]) -> Vec<Option<Vec<u8>>> {
    params.iter().map(|p| Some(record(&mut Rng(0), p, 0, false))).collect()
}

/// What one CALL argument passes, as the caller's `layout` and code page tell it.
fn passes_of(arg: &Arg, layout: &Layout, compiled: &exec::Compiled) -> Passes {
    match &arg.value {
        None => Passes::Omitted,
        Some(Operand::Ref(r)) => {
            let refmod_length = r.refmod.as_ref().and_then(|m| m.length.as_deref()).and_then(|e| match e {
                Expr::Operand(Operand::Literal(Literal::Number(n))) => n.parse::<usize>().ok(),
                _ => None,
            });
            match (layout.resolve(&r.name, &r.qualifiers, r.pos), refmod_length) {
                (Ok(Resolved::Item(_)), Some(n)) => Passes::Item(n),
                (Ok(Resolved::Item(i)), None) => Passes::Item(layout.items[i].size as usize),
                _ => Passes::Unknown,
            }
        }
        Some(Operand::Literal(Literal::Alnum(s) | Literal::Number(s))) => compiled.options.code_page().encode(s).map_or(Passes::Unknown, Passes::Literal),
        Some(Operand::Literal(Literal::Hex(bytes))) => Passes::Literal(bytes.clone()),
        Some(Operand::LengthOf(_) | Operand::AddressOf(_)) => Passes::Item(4),
        Some(_) => Passes::Unknown,
    }
}

/// Every CALL of `name` by a literal in `callers` (each a source file and its program), with what it
/// passes, in caller order and then statement order. A CALL through a data item is not one.
pub(crate) fn call_sites(name: &str, callers: &[(String, exec::Compiled)]) -> Vec<CallSite> {
    let mut sites = Vec::new();
    for (file, compiled) in callers {
        let program = &compiled.program;
        for s in all_statements(program) {
            let Stmt::Call(call) = s else { continue };
            let Operand::Literal(Literal::Alnum(target)) = &call.target else { continue };
            if !target.trim().eq_ignore_ascii_case(name) {
                continue;
            }
            let file = match call.pos.file {
                0 => file.clone(),
                n => program.sources.get(n as usize).cloned().unwrap_or_else(|| file.clone()),
            };
            let passes = call.using.iter().map(|a| passes_of(a, &compiled.layout, compiled)).collect();
            sites.push(CallSite { file, line: call.pos.line, passes });
        }
    }
    sites
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(text: &str) -> exec::Compiled {
        let mut programs = syntax::parse_all_with(text, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        exec::compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn subprogram(linkage: &str, procedure: &str) -> String {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  WS-FN PIC X(8) VALUE 'CBLTDLI'.\n       01  WS-ANY PIC X(8).\n       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION USING ARG.\n{procedure}           GOBACK.\n"
        )
    }

    const ARG: &str = "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n";

    #[test]
    fn a_subprogram_taking_data_is_taken() {
        assert_eq!(refusal(&compiled(&subprogram(ARG, "           ADD 1 TO ARG-QTY\n"))), None);
    }

    #[test]
    fn a_pointer_among_the_arguments_is_refused() {
        let linkage = "       01  ARG.\n           05 ARG-PTR USAGE POINTER.\n";
        assert!(refusal(&compiled(&subprogram(linkage, ""))).is_some_and(|r| r.contains("ARG-PTR")));
    }

    #[test]
    fn every_form_of_ims_program_is_refused() {
        for procedure in [
            "           CALL 'CBLTDLI' USING ARG\n",
            "           CALL 'AIBTDLI' USING ARG\n",
            "           CALL 'CEETDLI' USING ARG\n",
            "           CALL WS-FN USING ARG\n",
            "           ENTRY 'DLITCBL' USING ARG\n",
        ] {
            let why = refusal(&compiled(&subprogram(ARG, procedure)));
            assert!(why.as_deref().is_some_and(|r| r.contains("IMS")), "{procedure}: {why:?}");
        }
    }

    #[test]
    fn an_argument_passed_on_to_a_call_nothing_names_is_refused() {
        let why = refusal(&compiled(&subprogram(ARG, "           CALL WS-ANY USING ARG\n")));
        assert!(why.as_deref().is_some_and(|r| r.contains("may be an IMS interface")), "{why:?}");
    }

    #[test]
    fn a_cics_program_is_sent_to_the_cics_entry() {
        let why = refusal(&compiled(&subprogram(ARG, "           EXEC CICS RETURN END-EXEC\n")));
        assert!(why.as_deref().is_some_and(|r| r.contains("--cics")), "{why:?}");
    }

    fn with_linkage(working: &str, linkage: &str, using: &str) -> exec::Compiled {
        compiled(&format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{working}       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION USING {using}.\n           GOBACK.\n"
        ))
    }

    #[test]
    fn each_using_item_is_a_param_of_its_record_s_size_and_fields() {
        let c = with_linkage("", "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n           05 ARG-NAME PIC X(10).\n       01  ARG2.\n           05 ARG2-FLAG PIC X.\n", "ARG ARG2");
        let ps = params(&c);
        assert_eq!(ps.iter().map(|p| (p.name.as_str(), p.size, p.fields.len())).collect::<Vec<_>>(), [("ARG", 15, 2), ("ARG2", 1, 1)]);
    }

    const ODO: &str = "       01  ARG.\n           05 ARG-CNT PIC 9(2).\n           05 ARG-TBL OCCURS 1 TO 10 DEPENDING ON ARG-CNT.\n               10 ARG-ITEM PIC X(4).\n";

    #[test]
    fn a_depending_on_object_in_the_record_always_holds_a_count_its_table_allows() {
        let ps = params(&with_linkage("", ODO, "ARG"));
        assert_eq!(ps[0].counts.iter().map(|(f, min, max)| (f.offset, f.size, *min, *max)).collect::<Vec<_>>(), [(0, 2, 1, 10)]);
        let mut rng = Rng(12345);
        for _ in 0..200 {
            let args = arguments(&mut rng, &ps, None);
            let buf = args[0].as_ref().expect("passed");
            let n = u32::from(buf[0] & 0x0F) * 10 + u32::from(buf[1] & 0x0F);
            assert!((1..=10).contains(&n), "count {n}");
        }
    }

    #[test]
    fn a_depending_on_object_outside_the_record_is_the_subprogram_s_own() {
        let linkage = "       01  ARG.\n           05 ARG-TBL OCCURS 1 TO 10 DEPENDING ON WS-CNT.\n               10 ARG-ITEM PIC X(4).\n";
        let ps = params(&with_linkage("       01  WS-CNT PIC 9(2) VALUE 5.\n", linkage, "ARG"));
        assert!(ps[0].counts.is_empty());
    }

    fn site(passes: Vec<Passes>) -> CallSite {
        CallSite { file: "CALLER.cbl".to_string(), line: 1, passes }
    }

    #[test]
    fn a_call_site_s_omitted_literal_and_shorter_item_shape_the_arguments() {
        let ps = params(&with_linkage("", "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n           05 ARG-NAME PIC X(10).\n", "ARG"));
        let neutral = neutral_arguments(&ps);
        let mut rng = Rng(42);
        assert_eq!(arguments(&mut rng, &ps, Some(&site(vec![Passes::Omitted]))), [None]);
        assert_eq!(arguments(&mut rng, &ps, Some(&site(vec![Passes::Literal(vec![0xC1, 0xC2])]))), [Some([vec![0xC1, 0xC2], vec![SPACE; 13]].concat())]);
        for _ in 0..50 {
            let shorter = arguments(&mut rng, &ps, Some(&site(vec![Passes::Item(5)])));
            assert_eq!(shorter[0].as_ref().map(|b| b[5..].to_vec()), neutral[0].as_ref().map(|b| b[5..].to_vec()));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_arguments() {
        let ps = params(&with_linkage("", ODO, "ARG"));
        assert_eq!(arguments(&mut Rng(777), &ps, None), arguments(&mut Rng(777), &ps, None));
    }

    #[test]
    fn a_count_is_stored_as_its_item_stores_a_number() {
        let field = |kind| Field { offset: 0, size: if matches!(kind, Kind::Zoned { .. }) { 3 } else { 2 }, kind };
        assert_eq!(count_bytes(field(Kind::Zoned { digits: 3, scale: 0, signed: true, sign: None }), 42), [0xF0, 0xF4, 0xC2]);
        assert_eq!(count_bytes(field(Kind::Zoned { digits: 3, scale: 0, signed: false, sign: None }), 1234), [0xF2, 0xF3, 0xF4]);
        assert_eq!(count_bytes(field(Kind::Packed { digits: 3, scale: 0, signed: true }), 42), [0x04, 0x2C]);
        assert_eq!(count_bytes(field(Kind::Binary { digits: 4, scale: 0, signed: true, native: false }), 42), [0x00, 0x2A]);
    }

    const SUB_PROGRAM: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CALLER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  A PIC X(5).\n       01  B PIC X(10).\n       01  FN PIC X(8) VALUE 'SUB'.\n       01  FLAG PIC X VALUE 'Y'.\n       PROCEDURE DIVISION.\n";

    fn caller(procedure: &str) -> exec::Compiled {
        compiled(&format!("{SUB_PROGRAM}{procedure}           GOBACK.\n"))
    }

    #[test]
    fn a_call_site_says_what_it_passes_in_each_position() {
        let c = caller("           CALL 'SUB' USING A OMITTED 'XY' BY CONTENT LENGTH OF B\n");
        let xy = c.options.code_page().encode("XY").expect("encodes");
        let sites = call_sites("SUB", &[("CALLER.cbl".to_string(), c)]);
        assert_eq!(sites, [CallSite { file: "CALLER.cbl".to_string(), line: 10, passes: vec![Passes::Item(5), Passes::Omitted, Passes::Literal(xy), Passes::Item(4)] }]);
    }

    #[test]
    fn a_nested_call_a_lower_case_name_and_a_reference_modification_are_found() {
        let c = caller("           IF FLAG = 'Y'\n               CALL 'sub' USING B(1:3)\n           END-IF\n");
        let sites = call_sites("SUB", &[("CALLER.cbl".to_string(), c)]);
        assert_eq!(sites.iter().map(|s| s.passes.clone()).collect::<Vec<_>>(), [vec![Passes::Item(3)]]);
    }

    #[test]
    fn a_call_of_another_name_or_through_a_data_item_is_not_a_call_site() {
        let c = caller("           CALL 'OTHER' USING A\n           CALL FN USING A\n");
        assert!(call_sites("SUB", &[("CALLER.cbl".to_string(), c)]).is_empty());
    }

    #[test]
    fn call_sites_come_in_caller_order() {
        let sites = call_sites("SUB", &[("C1.cbl".to_string(), caller("           CALL 'SUB' USING A\n")), ("C2.cbl".to_string(), caller("           CALL 'SUB' USING B\n"))]);
        assert_eq!(sites.iter().map(|s| (s.file.as_str(), s.passes.clone())).collect::<Vec<_>>(), [("C1.cbl", vec![Passes::Item(5)]), ("C2.cbl", vec![Passes::Item(10)])]);
    }
}
