//! INVOKE, SELF, JNIENVPTR and CALL through a function-pointer in the walker: the walker decides by
//! name what a reference names, builds the `InvokePlan` and runs the phrases, and `rt::oo` runs the
//! rest, asking the walker what `OoHost` names.

use super::*;
use crate::oo::{self as classes, MethodCall, OoHost, Returned, Running};
use rt::lir::{CallArg, InvokePlan, MethodName, Receiver, Step};
use std::rc::Rc;

pub(super) use crate::oo::Frame;

/// A name, or a Java type the walker works out from the declarations when `rt::oo` reads it.
#[derive(Clone, Copy)]
pub(super) enum Sym<'a> {
    Text(&'a str),
    OperandType(&'a Operand),
    ItemType(&'a Ref),
}

impl<'p> Machine<'p, '_, '_> {
    /// SELF in a method, and JNIENVPTR; None for anything else.
    pub(super) fn oo_register(&mut self, r: &Ref) -> R<Option<Loc>> {
        if !r.qualifiers.is_empty() || !r.subscripts.is_empty() || !matches!(r.name.as_str(), "SELF" | "JNIENVPTR") || self.layout.resolve(&r.name, &[], r.pos).is_ok() {
            return Ok(None);
        }
        if r.name == "SELF" {
            return classes::self_reference(self.unit, self.oo.method, r.pos).map(Some);
        }
        let cell = classes::jni_environment(self.unit, r.pos)?;
        Ok(Some(Loc { offset: cell, len: 4, kind: Kind::Pointer, item: usize::MAX }))
    }

    pub(super) fn compare_references(&self, a: &Expr, b: &Expr, x: (&Val, Option<Loc>), y: (&Val, Option<Loc>), pos: Pos) -> R<Option<Ordering>> {
        let name = |e: &Expr| match e {
            Expr::Operand(Operand::Ref(r)) => r.name.clone(),
            _ => String::new(),
        };
        classes::compare_references(&self.unit.oo, || (name(a), name(b)), x, y, pos)
    }

    /// An argument as the method receives it, in the bytes of its Java type.
    fn argument(&mut self, op: &Operand, java: &str, pos: Pos) -> R<Vec<u8>> {
        Ok(match op {
            Operand::Ref(r) => {
                let loc = self.locate(r)?;
                let bytes = store::bytes(&self.unit.mem, loc).to_vec();
                if r.refmod.is_some() && java == "C" {
                    let text = self.page.decode(&bytes);
                    text.encode_utf16().take(1).flat_map(u16::to_be_bytes).collect()
                } else {
                    bytes
                }
            }
            Operand::LengthOf(r) => {
                let layout = self.layout;
                (self.locate(&layout.length_of_ref(r))?.len as i32).to_be_bytes().to_vec()
            }
            Operand::Literal(Literal::Number(t)) => {
                let v = t.parse::<i32>().map_err(|_| Abend::ironwork(format!("{t} is not an int"), pos))?;
                v.to_be_bytes().to_vec()
            }
            Operand::Literal(Literal::Figurative(Figurative::Zero)) => vec![0; 4],
            Operand::Literal(Literal::Figurative(f)) => vec![self.collating.figurative(*f)],
            Operand::Literal(Literal::Alnum(s)) => self.page.encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?,
            Operand::Literal(Literal::National(s)) => s.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            _ => return Err(Abend::ironwork("this INVOKE argument is not supported", pos)),
        })
    }

    pub(super) fn invoke(&mut self, i: &'p Invoke) -> R<Flow> {
        let plan = self.invoke_plan(i);
        match classes::invoke(self, &plan, i.pos)? {
            Step::Arm(1) => match &i.on_exception {
                Some(body) => self.run_block(body),
                None => Ok(Flow::Next),
            },
            Step::Arm(_) => match &i.not_on_exception {
                Some(body) => self.run_block(body),
                None => Ok(Flow::Next),
            },
            Step::End(ending) => Ok(Flow::End(ending)),
            _ => Ok(Flow::Next),
        }
    }

    /// The INVOKE as `rt::oo` runs it. SELF, SUPER and a REPOSITORY class-name are the receiver
    /// when written alone and no data item has the name.
    fn invoke_plan(&self, i: &'p Invoke) -> InvokePlan<&'p Ref, &'p Operand, Sym<'p>> {
        let (program, t) = (self.program, &i.target);
        let alone = t.qualifiers.is_empty() && t.subscripts.is_empty() && t.refmod.is_none() && self.layout.resolve(&t.name, &[], t.pos).is_err();
        let external = program.oo.as_deref().and_then(|o| o.external(&t.name));
        let receiver = match (t.name.as_str(), external) {
            ("SELF", _) if alone => Receiver::SelfRef,
            ("SUPER", _) if alone => Receiver::Super,
            (name, Some(external)) if alone => Receiver::Class { name: Sym::Text(name), external: Sym::Text(external) },
            _ => Receiver::Object(t),
        };
        let method = match &i.method {
            InvokeMethod::New => MethodName::New,
            InvokeMethod::Named(n) => MethodName::Named(Sym::Text(n)),
            InvokeMethod::Identifier(r) => MethodName::Dynamic(r),
        };
        let args = i.using.iter().map(|op| (op, Sym::OperandType(op))).collect();
        let returning = i.returning.as_ref().map(|r| (r, Sym::ItemType(r)));
        InvokePlan { receiver, method, args, returning, on_exception: i.on_exception.is_some(), not_on_exception: i.not_on_exception.is_some() }
    }

    /// CALL through a FUNCTION-POINTER or PROCEDURE-POINTER, which `rt::oo` runs as a JNI service.
    /// None when the CALL names a program.
    pub(super) fn call_through_pointer(&mut self, c: &'p Call) -> R<Option<Flow>> {
        let Operand::Ref(r) = &c.target else { return Ok(None) };
        let Ok(Resolved::Item(item)) = self.resolve(r) else { return Ok(None) };
        if self.layout.items[item].kind != Kind::ProgramPointer {
            return Ok(None);
        }
        let args: Vec<CallArg<&'p Ref, &'p Operand>> = c.using.iter().map(|a| a.value.as_ref().map_or(CallArg::Omitted, CallArg::Value)).collect();
        self.parmcheck_set();
        classes::call_through_pointer(self, r, &args, c.returning.as_ref(), c.pos)?;
        self.parmcheck_test(c, &[], |_| r.name.clone())?;
        Ok(Some(match &c.not_on_exception {
            Some(body) => self.run_block(body)?,
            None => Flow::Next,
        }))
    }
}

impl<'a, 'w> OoHost<'w, &'a Ref, &'a Operand, Sym<'a>> for Machine<'_, '_, 'w> {
    fn running(&self) -> Option<Running> {
        self.oo.method
    }

    fn program_id(&self) -> String {
        self.program.id.clone()
    }

    fn symbol(&mut self, symbol: &Sym<'a>, pos: Pos) -> R<String> {
        let (layout, oo) = (self.layout, self.program.oo.as_deref());
        match *symbol {
            Sym::Text(text) => Ok(text.to_owned()),
            Sym::OperandType(op) => classes::operand_type(layout, oo, op).map_err(|m| Abend::ironwork(m, pos)),
            Sym::ItemType(r) => match self.resolve(r)? {
                Resolved::Item(k) => classes::item_type(layout, oo, k).map_err(|m| Abend::ironwork(m, pos)),
                Resolved::Condition(_) => Err(Abend::ironwork(format!("RETURNING {}: a condition-name", r.name), pos)),
            },
        }
    }

    fn place_name(&self, place: &'a Ref) -> String {
        place.name.clone()
    }

    fn operand_name(&self, operand: &&'a Operand) -> Option<String> {
        match operand {
            Operand::Ref(r) => Some(r.name.clone()),
            _ => None,
        }
    }

    fn invoke_argument(&mut self, operand: &&'a Operand, java: &str, pos: Pos) -> R<Vec<u8>> {
        self.argument(operand, java, pos)
    }

    fn initialize_data(&mut self, data: Rc<Compiled>, index: usize) -> R<()> {
        Machine::activation(&data, index, &mut *self.unit, false).map(drop)
    }

    fn run_method(&mut self, call: MethodCall<Rc<Compiled>>, pos: Pos) -> R<Returned> {
        let compiled = call.code;
        let mut callee = Machine::activation(&compiled, call.storage, &mut *self.unit, false)?;
        callee.bind_linkage(&call.records, &compiled.program.using, &call.arguments, true);
        callee.oo = Frame { method: Some(call.running) };
        let ending = callee.run_procedure();
        let value = match (&compiled.program.returning, &ending) {
            (Some(item), Ok(_)) => Some((item.clone(), callee.returned(item, pos)?)),
            _ => None,
        };
        Ok(Returned { ending, value })
    }
}
