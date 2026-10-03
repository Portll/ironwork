//! INVOKE and a CALL through a FUNCTION-POINTER (lir.md §9.3, §9.8), which `rt::oo` runs over the
//! VM as `OoHost`: a class's FACTORY and OBJECT data and its methods are lowered programs, each
//! method run in an activation of its own by Rust recursion, as `call_nested` runs a callee.

use super::{Code, Halt, R, Vm, not_yet};
use crate::abend::{Abend, Ending};
use crate::callee::Bindings;
use crate::lir::{CallPlan, Const, InvokePlan, Operand, PlaceId, Step, SymId};
use crate::oo::{self, MethodCall, OoHost, Returned, Running};
use crate::store::{self, ProgramFacts};
use crate::unit::Loader;
use crate::vocab::{Figurative, Pos};
use std::rc::Rc;

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn invoke(&mut self, plan: &InvokePlan, pos: Pos) -> R<Step> {
        let step = oo::invoke(self, plan, pos);
        self.settle(step)
    }

    /// `Machine::call_through_pointer`: a JNI service, with no depth counted, after which ON
    /// EXCEPTION never runs, PARMCHECK's buffer set before its arguments and tested with none. The
    /// caller has located the pointer, as the walker's look for a SET TO ENTRY entry in it does.
    pub(super) fn call_through_pointer(&mut self, plan: &CallPlan, pointer: PlaceId, pos: Pos) -> R<Step> {
        self.parmcheck_set();
        let ran = oo::call_through_pointer(self, pointer, &plan.args, plan.returning, pos);
        self.settle(ran)?;
        let p = self.p;
        self.parmcheck_test(plan, &[], |_| p.symbols[p.places[pointer as usize].name as usize].clone(), pos)?;
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::argument`: an INVOKE argument in the bytes of its Java type.
    fn invoke_bytes(&mut self, operand: Operand, java: &str, pos: Pos) -> R<Vec<u8>> {
        Ok(match operand {
            Operand::Load(place) => {
                let loc = self.loc(place)?;
                let bytes = store::bytes(&self.unit.mem, loc).to_vec();
                if self.p.places[place as usize].refmod.is_some() && java == "C" {
                    let text = self.facts().page().decode(&bytes);
                    text.encode_utf16().take(1).flat_map(u16::to_be_bytes).collect()
                } else {
                    bytes
                }
            }
            Operand::LengthOf(place) => (self.loc(place)?.len as i32).to_be_bytes().to_vec(),
            Operand::Const(c) => match &self.p.consts[c as usize] {
                Const::Number(f) => {
                    let whole = (f.places.dec == 0).then(|| f.magnitude.to_u128()).flatten().and_then(|m| i32::try_from(m).ok());
                    let Some(whole) = whole else { return Err(not_yet("an INVOKE argument the walker reads from its literal's text")) };
                    (if f.negative { -whole } else { whole }).to_be_bytes().to_vec()
                }
                Const::Figurative(Figurative::Zero) => vec![0; 4],
                Const::Figurative(f) => vec![self.facts().figurative(*f)],
                Const::Bytes(b) | Const::National(b) => b.clone(),
                Const::Refused(abend) => return Err(self.abend(*abend, None).into()),
                Const::All(_) => return Err(Abend::ironwork("this INVOKE argument is not supported", pos).into()),
            },
            Operand::AddressOf(_) | Operand::Function(_) | Operand::UserFunction(_) => return Err(Abend::ironwork("this INVOKE argument is not supported", pos).into()),
        })
    }

    /// `Machine::activation` of class data, which gives it its VALUE clauses.
    fn initialize_part(&mut self, data: &Code, index: usize) -> R<()> {
        let lowered = data.lowered.as_ref().map_err(|why| not_yet(format!("class data that does not lower ({why})")))?;
        Vm::activation(lowered, index, &mut *self.unit, false).map(drop)
    }

    /// The method's activation, its LINKAGE bound to its arguments, the data of its object or
    /// factory and its RETURNING item, run from its start with SELF its receiver.
    fn run_as_method(&mut self, call: MethodCall<Rc<Code>>, pos: Pos) -> R<Returned> {
        let code = call.code;
        let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("a method that does not lower ({why})")))?;
        let program = &lowered.program;
        let mut vm = Vm::activation(lowered, call.storage, &mut *self.unit, false)?;
        let using = program.storage.using.iter().map(|&o| Some(usize::from(o))).collect();
        let returning = program.storage.returning.map(|o| (usize::from(o), program.storage.linkage[usize::from(o)] as usize));
        Bindings { records: &call.records, using, addresses: &call.arguments, returning }.bind(vm.unit, &mut vm.linkage);
        vm.method = Some(call.running);
        let ending: Result<Ending, Abend> = match vm.run_from(None) {
            Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
            Err(Halt::Abend(a)) => Err(a),
            Ok(e) => Ok(e),
        };
        let value = match (program.storage.returning, &ending) {
            (Some(ordinal), Ok(_)) => {
                let item = program.items.iter().find(|i| i.linkage == Some(ordinal) && i.parent.is_none()).and_then(|i| i.name);
                let Some(item) = item else { return Err(not_yet("a RETURNING record with no name")) };
                Some((vm.sym(item).to_owned(), vm.returned(ordinal, pos)?))
            }
            _ => None,
        };
        Ok(Returned { ending, value })
    }
}

impl<'w, L: Loader<Rc<Code>>> OoHost<'w, PlaceId, Operand, SymId> for Vm<'_, '_, 'w, L> {
    fn running(&self) -> Option<Running> {
        self.method
    }

    fn program_id(&self) -> String {
        self.sym(self.p.id).to_owned()
    }

    fn symbol(&mut self, symbol: &SymId, _pos: Pos) -> Result<String, Abend> {
        Ok(self.sym(*symbol).to_owned())
    }

    fn place_name(&self, place: PlaceId) -> String {
        self.sym(self.p.places[place as usize].name).to_owned()
    }

    fn operand_name(&self, operand: &Operand) -> Option<String> {
        match *operand {
            Operand::Load(place) => Some(self.place_name(place)),
            _ => None,
        }
    }

    fn invoke_argument(&mut self, operand: &Operand, java: &str, pos: Pos) -> Result<Vec<u8>, Abend> {
        let bytes = self.invoke_bytes(*operand, java, pos);
        self.lift(bytes, pos)
    }

    fn initialize_data(&mut self, data: Rc<Code>, index: usize) -> Result<(), Abend> {
        let initialized = self.initialize_part(&data, index);
        self.lift(initialized, Pos::default())
    }

    fn run_method(&mut self, call: MethodCall<Rc<Code>>, pos: Pos) -> Result<Returned, Abend> {
        let returned = self.run_as_method(call, pos);
        self.lift(returned, pos)
    }
}
