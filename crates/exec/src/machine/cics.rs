//! EXEC CICS in the walker: a block is bound into a `CicsCommand` (cics_bind.rs) and run by
//! `rt::cics`, which asks the walker what `CicsHost` names.

use super::*;
use super::cics_bind::operand;
use crate::cics::{self, CicsHost};
use rt::bms::Mapset;
use std::rc::Rc;

use super::cics_bind::has;
pub(super) use crate::cics::Handlers;

fn flow(f: cics::Flow) -> Flow {
    match f {
        cics::Flow::Next => Flow::Next,
        cics::Flow::GoTo(p) => Flow::GoTo(p as usize),
        cics::Flow::End(e) => Flow::End(e),
    }
}

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn cics(&mut self, block: &'p ExecBlock) -> R<Flow> {
        cics::in_task(self.unit, &block.command, block.pos)?;
        if self.unit.observed() {
            self.cics_sinks(block);
        }
        let command = self.bind_cics(block)?;
        cics::run(self, &command, block.pos).map(flow)
    }

    /// The operands of a command an input could steer, told to the observer before the command
    /// runs, so a command ironwork does not carry out yet is still traced. Only a data item is: a
    /// literal operand is the program's own choice. An operand that cannot be read is left to the
    /// command to report, so tracing never changes how a run ends.
    fn cics_sinks(&mut self, block: &ExecBlock) {
        let command = block.command.as_str();
        let queue = matches!(command, "WRITEQ" | "READQ" | "DELETEQ") || command.starts_with("WRITEQ ") || command.starts_with("READQ ") || command.starts_with("DELETEQ ");
        let mut sinks: Vec<(&str, &'static str)> = Vec::new();
        match command {
            "LINK" | "XCTL" => sinks.push(("PROGRAM", "cics-dynamic-transfer")),
            "START" | "START TRANSID" => sinks.push(("TRANSID", "cics-dynamic-transfer")),
            "READ" | "STARTBR" | "RESETBR" => sinks.push(("RIDFLD", "record-key")),
            "DELETE" => sinks.push(("RIDFLD", "record-update")),
            "WRITEQ TD" => sinks.push(("FROM", "log")),
            "WRITE" if has(block, "OPERATOR") => sinks.push(("TEXT", "log")),
            "WRITE" if has(block, "JOURNALNAME") || has(block, "JOURNALNUM") => sinks.push(("FROM", "log")),
            "SEND TEXT" | "SEND MAP" | "SEND" => sinks.push(("FROM", "screen")),
            "WEB SEND" => sinks.push(("FROM", "web-response")),
            "WEB WRITE" => sinks.push(("VALUE", "http-header")),
            "WEB OPEN" => sinks.extend([("HOST", "outbound-host"), ("URL", "outbound-host")]),
            "WEB CONVERSE" => sinks.extend([("PATH", "outbound-host"), ("FROM", "outbound-http")]),
            _ => {}
        }
        if queue {
            sinks.extend([("QUEUE", "queue-name"), ("QNAME", "queue-name")]);
        }
        sinks.push(("SYSID", "cics-sysid"));
        for (option, kind) in sinks {
            let Some(Operand::Ref(r)) = operand(block, option) else { continue };
            if let Ok(loc) = self.locate(r) {
                let text = self.page.decode(self.bytes(loc));
                self.sink(kind, block.pos, &text);
            }
        }
    }

    /// Runs this program as a logical level of the task: an abend that reaches it while its HANDLE
    /// ABEND exit is active goes to the exit, a LABEL taken as a GO TO from the procedure's start
    /// and a PROGRAM in place of the rest of this level (C142).
    pub(crate) fn run_level(&mut self) -> R<Ending> {
        let mut start = None;
        loop {
            let abend = match self.run_from(start) {
                Err(abend) => abend,
                done => return done,
            };
            match cics::abend_exit(self.unit, &mut self.cics_handlers, &abend) {
                None => return Err(abend),
                Some(cics::ExitTarget::Label(p)) => start = Some((p as usize, 0)),
                Some(cics::ExitTarget::Program(name)) => {
                    let ending = cics::enter_exit_program(self, &name, abend.pos)?;
                    return Ok(if ending == Ending::StopRun { ending } else { Ending::Goback });
                }
            }
        }
    }

    /// Fills the EXEC interface block for the task's first program and binds DFHEIBLK and
    /// DFHCOMMAREA, the USING items the translator gave it.
    pub(crate) fn begin_task(&mut self, commarea: Option<usize>, length: usize) {
        cics::begin_task(self.unit, self.page, length);
        let eib = self.unit.eib;
        self.bind(&[Some(eib), commarea]);
    }

    /// A data item a name alone refers to, as the symbolic map's `mapI` and `mapO` are found.
    fn named(name: &str, pos: Pos) -> Ref {
        Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
    }
}

impl<'a, 'w> CicsHost<'w, &'a Ref, &'a Operand, &'a str> for Machine<'_, '_, 'w> {
    type Program = Rc<Compiled>;
    type Loader = crate::unit::Library;

    fn unit(&mut self) -> &mut RunUnit<'w> {
        self.unit
    }

    fn handlers(&mut self) -> &mut Handlers {
        &mut self.cics_handlers
    }

    fn content(&mut self, operand: &&'a Operand, pos: Pos) -> R<Vec<u8>> {
        self.content_argument(operand, pos)
    }

    fn integer_of(&mut self, operand: &&'a Operand, pos: Pos) -> R<i64> {
        Machine::integer(self, &Expr::Operand((*operand).clone()), pos)
    }

    fn text(&self, text: &&'a str) -> String {
        (*text).to_owned()
    }

    fn main(&self) -> bool {
        self.main
    }

    fn program_id(&self) -> String {
        self.program.id.clone()
    }

    fn commarea(&self) -> Option<usize> {
        let ordinal = self.layout.linkage_roots.iter().position(|&i| self.layout.items[i].name.as_deref() == Some("DFHCOMMAREA"));
        ordinal.and_then(|o| self.linkage[o])
    }

    fn mapset(&mut self, name: &str) -> Option<Result<Mapset, String>> {
        syntax::bms::find_mapset(&self.unit.library.copy, name).map(|found| found.map_err(|e| e.message))
    }

    fn item_named(&mut self, name: &str, pos: Pos) -> R<Option<Loc>> {
        let r = Self::named(name, pos);
        match self.resolve(&r) {
            Ok(Resolved::Item(_)) => self.locate(&r).map(Some),
            _ => Ok(None),
        }
    }

    fn locate_named(&mut self, name: &str, pos: Pos) -> R<Loc> {
        self.locate(&Self::named(name, pos))
    }

    fn run_program(&mut self, program: Rc<Compiled>, index: usize, commarea: Option<usize>, xctl: bool) -> R<Ending> {
        let ending = Machine::activation(&program, index, &mut *self.unit, self.main && xctl).and_then(|mut callee| {
            let eib = callee.unit.eib;
            callee.bind(&[Some(eib), commarea]);
            callee.run_level()
        });
        ending.map_err(|a| self.in_loaded(index, &program, a))
    }
}

#[cfg(test)]
mod tests {
    use crate::cics::{EIBAID, EIBCALEN, EIBCPOSN, EIBDATE, EIBFN, EIBRESP, EIBRESP2, EIBRSRCE, EIBTASKN, EIBTIME, EIBTRMID, EIBTRNID};

    #[test]
    fn eib_offsets_match_the_dfheiblk_layout() {
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. EIBT.\n       PROCEDURE DIVISION.\n           EXEC CICS RETURN END-EXEC.\n";
        let program = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap().remove(0);
        let compiled = crate::compile(program, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        let offset = |name: &str| compiled.layout.items.iter().find(|i| i.name.as_deref() == Some(name)).map(|i| i.offset as usize);
        let fields = [
            ("EIBTIME", EIBTIME),
            ("EIBDATE", EIBDATE),
            ("EIBTRNID", EIBTRNID),
            ("EIBTASKN", EIBTASKN),
            ("EIBTRMID", EIBTRMID),
            ("EIBCPOSN", EIBCPOSN),
            ("EIBCALEN", EIBCALEN),
            ("EIBAID", EIBAID),
            ("EIBFN", EIBFN),
            ("EIBRSRCE", EIBRSRCE),
            ("EIBRESP", EIBRESP),
            ("EIBRESP2", EIBRESP2),
        ];
        for (name, at) in fields {
            assert_eq!(offset(name), Some(at), "{name}");
        }
    }
}
