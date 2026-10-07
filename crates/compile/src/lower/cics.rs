//! EXEC CICS (lir.md §9.5): each block bound by `crate::cics_bind::bind`, its handles
//! lowered, as one `Op::Cics`. HANDLE CONDITION keeps the paragraphs its labels name, which the op
//! returns as `Step::GoTo` when a condition takes one; HANDLE ABEND keeps its LABEL's. A block the
//! walker refuses as it binds it lowers to `Cics::Refused` with the walker's message. A symbolic
//! map the walker finds by name becomes SEND MAP's FROM or RECEIVE MAP's INTO.

use super::{Lower, LowerError, R, push, unsupported};
use crate::layout::Resolved;
use crate::cics_bind;
use rt::abend::{Abend, AbendCode};
use rt::cics::{Cics, CicsCommand, Datum, Handles, Resp};
use rt::lir::{self, Op, PlaceId, SymId};
use syntax::Pos;
use syntax::ast::{ExecBlock, Expr, FunctionCall, Literal, Operand, Ref};

/// The walker's references as the LIR's ids: a data item as a place located as the walker locates
/// it, not as a receiving item; any other operand as its value; text as a symbol.
struct Lowering<'l, 'c> {
    l: &'l mut Lower<'c>,
    pos: Pos,
}

impl<'b> Handles<&'b Ref, &'b Operand, &'b str> for Lowering<'_, '_> {
    type Place = PlaceId;
    type Value = lir::Operand;
    type Text = SymId;
    type Error = LowerError;

    fn place(&mut self, r: &'b Ref) -> R<PlaceId> {
        self.l.place(r, false)
    }

    fn value(&mut self, op: &'b Operand) -> R<lir::Operand> {
        if let Operand::Function(f) = op
            && self.l.float_argument(f)?
        {
            return unsupported("a FUNCTION with a floating-point argument expression as an EXEC CICS option", self.pos);
        }
        Ok(self.l.operand(op, self.pos)?.operand)
    }

    fn text(&mut self, text: &'b str) -> R<SymId> {
        Ok(self.l.sym(text))
    }
}

impl Lower<'_> {
    pub(super) fn cics(&mut self, block: &ExecBlock, pos: Pos, para: usize) -> R<()> {
        let program = self.program;
        let command = match cics_bind::bind(block, &|text| cics_bind::label(program, block, text, para)) {
            Ok(bound) => {
                let symbolic = self.symbolic_map(&bound.command, pos)?;
                let mut command = bound.map(&mut Lowering { l: self, pos })?;
                match (&mut command.command, symbolic) {
                    (Cics::SendMap { from, .. }, Some(map)) => *from = Some(Datum::Place(map)),
                    (Cics::ReceiveMap { into, .. }, Some(map)) => *into = Some(Datum::Place(map)),
                    _ => {}
                }
                command
            }
            Err(abend) => self.refused_cics(block, abend, pos)?,
        };
        let id = push(&mut self.services.cics, command, "EXEC CICS commands")?;
        self.op(Op::Cics(id), pos)
    }

    /// The symbolic map SEND MAP without FROM and RECEIVE MAP without INTO or SET find by name,
    /// mapO or mapI, when MAP is a literal and the name a data item's: written as FROM or INTO, it
    /// is located and read or stored as `rt::cics::maps` treats the item the name finds.
    fn symbolic_map(&mut self, command: &Cics<&Ref, &Operand, &str>, pos: Pos) -> R<Option<PlaceId>> {
        let (map, suffix) = match command {
            Cics::SendMap { map, from: None, maponly: false, .. } => (map, 'O'),
            Cics::ReceiveMap { map, into: None, set: None, .. } => (map, 'I'),
            _ => return Ok(None),
        };
        let name = match map {
            Some(Datum::Text(t)) => t.trim().trim_matches(|c| c == '\'' || c == '"').to_owned(),
            Some(Datum::Value(Operand::Literal(Literal::Alnum(s)))) => match self.page.encode(s) {
                Ok(bytes) => self.page.decode(&bytes).trim_end().to_owned(),
                Err(_) => return Ok(None),
            },
            _ => return Ok(None),
        };
        let name = format!("{}{suffix}", name.to_ascii_uppercase());
        if !matches!(self.layout.resolve(&name, &[], pos), Ok(Resolved::Item(_))) {
            return Ok(None);
        }
        let r = Ref { name, qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
        self.place(&r, false).map(Some)
    }

    /// Whether an intrinsic FUNCTION's arguments, or a nested one's, hold an arithmetic expression
    /// in floating point: `Machine::integer` computes it in fixed point, a value read in floating
    /// point, and the LIR keeps one operand for both.
    fn float_argument(&mut self, f: &FunctionCall) -> R<bool> {
        if self.user_defined(&f.name).is_some() {
            return Ok(false);
        }
        for a in &f.args {
            let float = match a {
                Expr::Operand(Operand::Function(g)) => self.float_argument(g)?,
                Expr::Operand(_) => false,
                _ => self.uses_float(a)?,
            };
            if float {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// A block whose binding the walker refuses, which it abends at only once the task check and
    /// the observer's sinks have passed.
    fn refused_cics(&mut self, block: &ExecBlock, abend: Abend, pos: Pos) -> R<CicsCommand> {
        if abend.code != AbendCode::Ironwork || abend.pos != pos {
            return unsupported("an EXEC CICS block the walker refuses with another abend than IRONWORK at the block", pos);
        }
        let mut sinks = Vec::new();
        for (r, sink) in cics_bind::sinks(block) {
            sinks.push((self.place(r, false)?, sink));
        }
        let (name, why) = (self.sym(&block.command), self.sym(&abend.message));
        Ok(CicsCommand { name, command: Cics::Refused(why), resp: Resp { resp: None, resp2: None, nohandle: false }, sinks })
    }
}
