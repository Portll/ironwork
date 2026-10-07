//! CALL, CANCEL's names, ENTRY and INVOKE (lir.md §9.3, §9.8), as the walker's `call`,
//! `call_through_pointer`, `call_nested`, `program_name` and `invoke` take them.

use super::flow::Ctx;
use super::{Lower, LowerError, R, push, unsupported};
use crate::layout::Resolved;
use rt::storage::literal_fixed;
use compile::oo::{item_type, operand_type};
use rt::lir::{self, AbendId, CallArg, CallId, CallPlan, CallTarget, Chars, InvokeId, InvokePlan, LeService, MethodName, Op, Receiver, Terminator};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{Arg, ArgMode, Call, Invoke, InvokeMethod, Literal, Operand, Param, Ref};
use zarch::decimal;

impl Lower<'_> {
    /// CALL's op and its EXCEPTION phrases, or the abend the walker gives on reaching a literal
    /// program name it cannot read as one.
    pub(super) fn call(&mut self, c: &Call, pos: Pos, ctx: &Ctx) -> R<()> {
        let Call { target, using, returning, on_exception, not_on_exception, pos: _ } = c;
        let plan = match self.call_plan(target, using, returning.as_ref(), on_exception.is_some(), not_on_exception.is_some(), pos)? {
            Ok(plan) => plan,
            Err(abend) => return self.end(Terminator::Abend(abend), pos),
        };
        self.op(Op::Call(plan), pos)?;
        self.phrases(on_exception.as_deref(), not_on_exception.as_deref(), pos, ctx)
    }

    fn call_plan(&mut self, target: &Operand, using: &[Arg], returning: Option<&Ref>, on_exception: bool, not_on_exception: bool, pos: Pos) -> R<Result<CallId, AbendId>> {
        let target = match target {
            Operand::Ref(r) if self.program_pointer(r) && !self.jni_function(r) => CallTarget::Entry(self.place(r, false)?),
            Operand::Ref(r) if self.program_pointer(r) => CallTarget::Pointer(self.place(r, false)?),
            Operand::Literal(lit) => match program_name(self, lit) {
                Ok(name) => CallTarget::Named { name: self.sym(&name), le: le_service(&name, pos)? },
                Err(message) => return self.ironwork(&message).map(Err),
            },
            op => CallTarget::Dynamic(self.operand(op, pos)?.operand),
        };
        let pointer = matches!(target, CallTarget::Pointer(_));
        let mut args = Vec::with_capacity(using.len());
        for Arg { mode, value } in using {
            args.push(match (value, *mode) {
                (None, _) => CallArg::Omitted,
                (Some(op), _) if pointer => CallArg::Value(self.operand(op, pos)?.operand),
                (Some(Operand::Ref(r)), ArgMode::Reference) => CallArg::Reference(self.place(r, false)?),
                (Some(op), ArgMode::Value) => CallArg::Value(self.operand(op, pos)?.operand),
                (Some(op), _) => CallArg::Content(self.content(op, pos)?),
            });
        }
        let returning = returning.map(|r| self.place(r, false)).transpose()?;
        let plan = CallPlan { target, args, returning, on_exception, not_on_exception };
        push(&mut self.services.calls, plan, "CALL plans").map(Ok)
    }

    /// A CALL through a FUNCTION-POINTER or PROCEDURE-POINTER item, as `call_through_pointer`
    /// recognizes one.
    fn program_pointer(&self, r: &Ref) -> bool {
        matches!(self.layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(Resolved::Item(i)) if self.layout.items[i].kind == Kind::ProgramPointer)
    }

    /// A function of the JNI's table, the one kind of pointer `CallTarget::Pointer` calls.
    fn jni_function(&self, r: &Ref) -> bool {
        let layout = self.layout;
        let Ok(Resolved::Item(mut i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return false };
        while let Some(parent) = layout.items[i].parent {
            i = parent;
        }
        layout.items[i].name.as_deref() == Some("JNINATIVEINTERFACE")
    }

    /// `content_argument`: a data item's bytes, a literal's as its own data item would hold them.
    fn content(&mut self, op: &Operand, pos: Pos) -> R<Chars> {
        Ok(match op {
            Operand::Ref(r) => Chars::Place(self.place(r, false)?),
            Operand::Literal(lit) if self.unencodable(lit).is_some() => Chars::Value(self.operand(op, pos)?.operand),
            Operand::Literal(lit) => Chars::Literal(self.content_bytes(lit, pos)?),
            _ => Chars::Value(self.operand(op, pos)?.operand),
        })
    }

    fn content_bytes(&mut self, lit: &Literal, pos: Pos) -> R<Vec<u8>> {
        Ok(match lit {
            Literal::Alnum(s) => self.encode(s, pos)?,
            Literal::Hex(b) => b.clone(),
            Literal::National(s) => s.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            Literal::Dbcs(s) => self.dbcs(s, pos)?,
            Literal::Number(t) => match literal_fixed(t) {
                Some(f) => {
                    let zone = if f.negative { decimal::MINUS } else { decimal::UNSIGNED };
                    zoned(f.magnitude.to_u128().unwrap_or(0), f.places.total().max(1) as usize, zone)
                }
                None => return unsupported("a numeric literal of more than 31 digits", pos),
            },
            Literal::Figurative(f) => vec![self.c.collating.figurative(*f)],
            Literal::All(inner) => match &**inner {
                Literal::Alnum(_) | Literal::Hex(_) | Literal::National(_) | Literal::Figurative(_) => self.content_bytes(inner, pos)?,
                _ => return unsupported("ALL with a literal that is not alphanumeric or national", pos),
            },
        })
    }

    /// Each ENTRY statement with the block after it and the LINKAGE records its USING addresses.
    pub(super) fn entry_points(&mut self) -> R<Vec<lir::EntryPoint>> {
        let layout = self.layout;
        let mut out = Vec::with_capacity(self.c.entries.len());
        for e in &self.c.entries {
            let Some(&block) = self.entry_blocks.get(&(e.paragraph, e.statement)) else {
                return Err(LowerError::Invalid(format!("ENTRY '{}' has no block", e.name)));
            };
            let mut using = Vec::with_capacity(e.using.len());
            for Param { by_value: _, name } in &e.using {
                match layout.linkage_roots.iter().position(|&i| layout.items[i].name.as_deref() == Some(name.as_str())).map(u16::try_from) {
                    Some(Ok(ordinal)) => using.push(ordinal),
                    Some(Err(_)) => return Err(LowerError::Exceeds("LINKAGE records", e.pos)),
                    None => return unsupported("ENTRY USING an item that is not a LINKAGE record", e.pos),
                }
            }
            out.push(lir::EntryPoint { name: self.sym(&e.name), paragraph: e.paragraph as u32, block, using });
        }
        Ok(out)
    }

    /// INVOKE's op and its EXCEPTION phrases.
    pub(super) fn invoke(&mut self, i: &Invoke, pos: Pos, ctx: &Ctx) -> R<()> {
        let plan = self.invoke_plan(i, pos)?;
        self.op(Op::Invoke(plan), pos)?;
        self.phrases(i.on_exception.as_deref(), i.not_on_exception.as_deref(), pos, ctx)
    }

    /// INVOKE's receiver, method, arguments with their Java types, and RETURNING with its own, as
    /// `invoke` works them out on each execution.
    fn invoke_plan(&mut self, i: &Invoke, pos: Pos) -> R<InvokeId> {
        let Invoke { target: t, method, using, returning, on_exception, not_on_exception, pos: _ } = i;
        let (layout, oo) = (self.layout, self.program.oo.as_deref());
        let method = match method {
            InvokeMethod::New => MethodName::New,
            InvokeMethod::Named(name) => MethodName::Named(self.sym(name)),
            InvokeMethod::Identifier(r) => MethodName::Dynamic(self.place(r, false)?),
        };
        let plain = t.qualifiers.is_empty() && t.subscripts.is_empty() && t.refmod.is_none() && layout.resolve(&t.name, &[], t.pos).is_err();
        let external = oo.and_then(|o| o.external(&t.name));
        let receiver = match (t.name.as_str(), external) {
            ("SELF", _) if plain => Receiver::SelfRef,
            ("SUPER", _) if plain => Receiver::Super,
            (name, Some(external)) if plain => Receiver::Class { name: self.sym(name), external: self.sym(external) },
            _ => Receiver::Object(self.place(t, false)?),
        };
        let mut args = Vec::with_capacity(using.len());
        for op in using {
            let Ok(java) = operand_type(layout, oo, op) else { return unsupported("an INVOKE argument of no Java type", pos) };
            args.push((self.operand(op, pos)?.operand, self.sym(&java)));
        }
        let returning = match returning {
            None => None,
            Some(r) => {
                let java = match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                    Ok(Resolved::Item(k)) => item_type(layout, oo, k).ok(),
                    _ => None,
                };
                let Some(java) = java else { return unsupported("an INVOKE RETURNING item of no Java type", pos) };
                Some((self.place(r, false)?, self.sym(&java)))
            }
        };
        let plan = InvokePlan { receiver, method, args, returning, on_exception: on_exception.is_some(), not_on_exception: not_on_exception.is_some() };
        push(&mut self.services.invokes, plan, "INVOKE plans")
    }
}

/// `program_name` of a literal: its text trimmed and upper-cased, or the walker's message when the
/// literal is not alphanumeric or cannot be read.
fn program_name(l: &Lower<'_>, lit: &Literal) -> Result<String, String> {
    literal_error(l, lit)?;
    let bytes = match lit {
        Literal::Alnum(s) => l.page.encode(s).map_err(|e| e.to_string())?,
        Literal::Hex(b) => b.clone(),
        _ => return Err("a program name must be alphanumeric".into()),
    };
    Ok(l.page.decode(&bytes).trim().to_ascii_uppercase())
}

/// The abend `literal_value` gives a literal, if it gives one.
fn literal_error(l: &Lower<'_>, lit: &Literal) -> Result<(), String> {
    match lit {
        Literal::Alnum(s) => l.page.encode(s).map(|_| ()).map_err(|e| e.to_string()),
        Literal::Number(t) if literal_fixed(t).is_none() => Err(format!("the literal {t} has more than 31 digits")),
        Literal::All(inner) => {
            literal_error(l, inner)?;
            match &**inner {
                Literal::Alnum(_) | Literal::Hex(_) | Literal::National(_) | Literal::Figurative(_) => Ok(()),
                _ => Err("ALL takes an alphanumeric or national literal".into()),
            }
        }
        _ => Ok(()),
    }
}

/// The LE callable service `name` runs when no program has the name.
fn le_service(name: &str, pos: Pos) -> R<Option<LeService>> {
    let service = match name {
        "CEE3ABD" => LeService::Cee3abd,
        "CEE3DMP" => LeService::Cee3dmp,
        "CEEDATE" => LeService::Ceedate,
        "CEEDATM" => LeService::Ceedatm,
        "CEEDAYS" => LeService::Ceedays,
        "CEEDYWK" => LeService::Ceedywk,
        "CEEFRST" => LeService::Ceefrst,
        "CEEGMT" => LeService::Ceegmt,
        "CEEGMTO" => LeService::Ceegmto,
        "CEEGTST" => LeService::Ceegtst,
        "CEELOCT" => LeService::Ceeloct,
        "CEEMOUT" => LeService::Ceemout,
        "CEESECS" => LeService::Ceesecs,
        "CEEUTC" => LeService::Ceeutc,
        _ if rt::le::provides(name) => return unsupported("an LE callable service the LIR does not name", pos),
        _ => return Ok(None),
    };
    Ok(Some(service))
}

/// `zoned_digits`: unsigned zoned digits, the last one's zone `sign_zone`.
fn zoned(magnitude: u128, digits: usize, sign_zone: u8) -> Vec<u8> {
    let mut out = vec![0xF0u8; digits];
    let mut m = magnitude;
    for b in out.iter_mut().rev() {
        *b = 0xF0 | (m % 10) as u8;
        m /= 10;
    }
    if let Some(last) = out.last_mut() {
        *last = (sign_zone << 4) | (*last & 0x0F);
    }
    out
}
