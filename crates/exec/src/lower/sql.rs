//! EXEC SQL (lir.md §9.7), as machine/sql.rs runs it: every block's `SqlEntry` by ordinal with its
//! host variables as places, the SQLCA fields the program declares, and each statement as `Op::Sql`
//! followed by the WHENEVER branches in force where it stands.

use super::flow::Ctx;
use super::{Lower, LowerError, R, unsupported};
use crate::layout::Resolved;
use crate::sql::{HostType, host_type};
use rt::abend::AbendCode;
use rt::lir::{self, HostPlace, Op, PlaceId, SqlEntry, SqlStatement, SqlTest, Sqlca, Terminator};
use syntax::Pos;
use syntax::ast::{ExecBlock, ExecKind, Expr, Literal, Operand, ProcName, Ref, Stmt};
use syntax::sql::{Action, ChangeKind, Cursor, HostVar, Statement, Whenever};

/// The EXEC SQL blocks among `stmts` and the statements inside them, in order.
fn sql_blocks<'s>(stmts: &'s [Stmt], out: &mut Vec<&'s ExecBlock>) {
    for s in stmts {
        if let Stmt::Exec(block) = s
            && block.kind == ExecKind::Sql
        {
            out.push(block);
        }
        for body in crate::oo::bodies(s) {
            sql_blocks(body, out);
        }
    }
}

impl Lower<'_> {
    /// Every EXEC SQL block of the program, declarative ones and those of the DATA DIVISION
    /// included, as `Program.sql[k − 1]` for ordinal k; and the SQLCA, which every statement fills.
    pub(super) fn sql_table(&mut self) -> R<(Vec<SqlEntry>, Sqlca)> {
        let program = self.program;
        let mut blocks: Vec<&ExecBlock> = program.exec_declarations.iter().filter(|b| b.kind == ExecKind::Sql).collect();
        for p in &program.paragraphs {
            sql_blocks(&p.statements, &mut blocks);
        }
        blocks.sort_by_key(|b| b.sql.as_ref().map_or(0, |s| s.ordinal));
        let mut table = Vec::with_capacity(blocks.len());
        for (k, block) in (1u32..).zip(&blocks) {
            match &block.sql {
                Some(sql) if sql.ordinal == k => table.push(self.sql_entry(block, &sql.statement, k)?),
                _ => return unsupported("EXEC SQL blocks whose ordinals do not run from 1 without a gap", block.pos),
            }
        }
        let sqlca = match blocks.iter().find(|b| !b.declarative()) {
            Some(first) => self.sqlca(first.pos)?,
            None => Sqlca::default(),
        };
        Ok((table, sqlca))
    }

    /// `sql_entry`: the statement with its host variables bound, and the text its call sends.
    fn sql_entry(&mut self, block: &ExecBlock, statement: &Statement, ordinal: u32) -> R<SqlEntry> {
        let command = block.command.as_str();
        let (statement, text, with_hold) = match statement {
            Statement::Query { text, inputs, into } => {
                let inputs = self.host_places(inputs, command)?;
                (SqlStatement::Query { inputs, into: self.host_places(into, command)? }, text.clone(), false)
            }
            Statement::Change { kind, text, inputs, current_of } => {
                let inputs = self.host_places(inputs, command)?;
                let current_of = current_of.as_deref().map(|c| self.sym(c));
                (SqlStatement::Change { delete: matches!(kind, ChangeKind::Delete), inputs, current_of }, text.clone(), false)
            }
            Statement::Open { cursor, declared: Some(Cursor { name: _, text, inputs, with_hold, statement }), using } => {
                let hold = if *with_hold { " WITH HOLD" } else { "" };
                match statement {
                    Some(name) => {
                        let open = SqlStatement::OpenPrepared { cursor: self.sym(cursor), statement: self.sym(name), inputs: self.host_places(using, command)? };
                        (open, format!("DECLARE {cursor} CURSOR{hold} FOR {name}"), *with_hold)
                    }
                    None => {
                        let text = format!("DECLARE {cursor} CURSOR{hold} FOR {text}");
                        (SqlStatement::Open { cursor: self.sym(cursor), inputs: self.host_places(inputs, command)? }, text, *with_hold)
                    }
                }
            }
            Statement::Fetch { cursor, into } => (SqlStatement::Fetch { cursor: self.sym(cursor), into: self.host_places(into, command)? }, format!("FETCH {cursor}"), false),
            Statement::Close { cursor } => (SqlStatement::Close { cursor: self.sym(cursor) }, format!("CLOSE {cursor}"), false),
            Statement::Commit => (SqlStatement::Commit, "COMMIT".into(), false),
            Statement::Rollback => (SqlStatement::Rollback, "ROLLBACK".into(), false),
            Statement::Prepare { name, source } => {
                let source = self.host_places(std::slice::from_ref(source), command)?;
                (SqlStatement::Prepare { name: self.sym(name), source }, format!("PREPARE {name}"), false)
            }
            Statement::ExecuteImmediate { source } => (SqlStatement::ExecuteImmediate { source: self.host_places(std::slice::from_ref(source), command)? }, "EXECUTE IMMEDIATE".into(), false),
            Statement::Execute { name, inputs } => {
                let inputs = self.host_places(inputs, command)?;
                (SqlStatement::Execute { name: self.sym(name), inputs }, format!("EXECUTE {name}"), false)
            }
            Statement::Whenever { .. } | Statement::Declaration | Statement::DeclareCursor(_) | Statement::DeclareUnsupported { .. } => (SqlStatement::Declaration, String::new(), false),
            Statement::Unsupported(what) => (SqlStatement::Unsupported(self.sym(what)), String::new(), false),
            Statement::Connect { what, target } => (SqlStatement::Connect { what: self.sym(what), location: self.host_places(target.as_slice(), command)? }, String::new(), false),
            Statement::Open { declared: None, .. } | Statement::Malformed(_) => return unsupported("an EXEC SQL statement the compiler refuses", block.pos),
        };
        let fingerprint = crate::sql::fingerprint(&text);
        Ok(SqlEntry { ordinal, verb: self.sym(command), statement, text: self.sym(&text), fingerprint, with_hold })
    }

    /// `host_places`: each host variable with its type, a host structure's members one by one with
    /// the indicator array's elements beside them. An item with no SQL type keeps the walker's abend,
    /// at the host variable, for when the statement reaches it.
    fn host_places(&mut self, vars: &[HostVar], command: &str) -> R<Vec<HostPlace>> {
        let layout = self.layout;
        let mut out = Vec::new();
        for HostVar { var: host, indicator } in vars {
            let var = self.place(host, false)?;
            let indicator = indicator.as_ref().map(|r| self.indicator(r)).transpose()?;
            let element = |k: usize| indicator.map(|p| (p, 2 * k as u32));
            let at = Some(host.pos);
            let ty = match layout.resolve(&host.name, &host.qualifiers, host.pos) {
                Ok(Resolved::Item(item)) => match host_type(layout, item) {
                    Ok(ty) => Ok((item, ty)),
                    Err(why) => Err(self.abend(AbendCode::Exec, &format!("EXEC SQL {command}: {why}"), at)?),
                },
                Ok(_) => Err(self.abend(AbendCode::Ironwork, &format!("{} is a condition-name, not a data item", host.name), at)?),
                Err(e) => Err(self.abend(AbendCode::Ironwork, &e.message, at)?),
            };
            match ty {
                Ok((item, HostType::Structure(members))) => {
                    let start = layout.items[item].offset;
                    for (k, (m, ty)) in members.into_iter().enumerate() {
                        let member = &layout.items[m];
                        out.push(HostPlace { var, member: Some((member.offset - start, member.size)), ty: Ok(ty), indicator: element(k) });
                    }
                }
                Ok((_, ty)) => out.push(HostPlace { var, member: None, ty: Ok(ty), indicator: element(0) }),
                Err(abend) => out.push(HostPlace { var, member: None, ty: Err(abend), indicator: element(0) }),
            }
        }
        Ok(out)
    }

    /// `locate_indicator`: an indicator array named without subscripts at its first element.
    fn indicator(&mut self, r: &Ref) -> R<PlaceId> {
        let dims = match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) if r.subscripts.is_empty() => self.layout.items[i].dims.len(),
            _ => 0,
        };
        if dims == 0 {
            return self.place(r, false);
        }
        let one = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
        self.place(&Ref { subscripts: vec![one; dims], ..r.clone() }, false)
    }

    /// The SQLCA fields the program declares with an SQL type. The walker leaves a field it cannot
    /// locate as it was, so one written with the wrong number of subscripts, which no statement
    /// locates, is left out.
    fn sqlca(&mut self, pos: Pos) -> R<Sqlca> {
        let layout = self.layout;
        let mut fields = Vec::new();
        for (field, r) in crate::machine::sql::sqlca_fields(pos) {
            let Ok(Resolved::Item(item)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { continue };
            let Ok(ty) = host_type(layout, item) else { continue };
            if r.subscripts.len() != layout.items[item].dims.len() {
                continue;
            }
            if matches!(ty, HostType::Structure(_)) {
                return unsupported("an SQLCA field that is a host structure", pos);
            }
            fields.push((field, self.item_place(item, &r, false)?, ty));
        }
        Ok(Sqlca { fields })
    }

    /// `Op::Sql`, then a branch for each WHENEVER condition whose action is GO TO, tested in the
    /// walker's order. The conditions exclude each other, so a CONTINUE needs no test.
    pub(super) fn sql(&mut self, block: &ExecBlock, pos: Pos, ctx: &Ctx) -> R<()> {
        let Some(sql) = &block.sql else { return Err(LowerError::Invalid(format!("EXEC SQL {} has no typed statement", block.command))) };
        self.op(Op::Sql(sql.ordinal), pos)?;
        let declaration = (sql.ordinal as usize).checked_sub(1).and_then(|k| self.sql.get(k)).is_none_or(|e| e.statement == SqlStatement::Declaration);
        if declaration {
            return Ok(());
        }
        let Whenever { sqlerror, not_found, sqlwarning } = &sql.whenever;
        for (test, action) in [(SqlTest::Error, sqlerror), (SqlTest::NotFound, not_found), (SqlTest::Warning, sqlwarning)] {
            let Action::GoTo(label) = action else { continue };
            let cond = self.cond(lir::Cond::Sql(test))?;
            let (taken, next) = (self.new_block()?, self.new_block()?);
            self.end(Terminator::Branch { cond, then: taken, otherwise: next }, pos)?;
            self.switch(taken)?;
            match crate::procedure_from(self.program, &ProcName { name: label.clone(), section: None }, ctx.para) {
                Ok((t, _)) => self.go_to(t, ctx, pos)?,
                Err(message) => {
                    let abend = self.ironwork(&message)?;
                    self.end(Terminator::Abend(abend), pos)?;
                }
            }
            self.switch(next)?;
        }
        Ok(())
    }
}
