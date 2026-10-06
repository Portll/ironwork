//! A program run within the run unit, as CALL, LINK, XCTL, INVOKE and a user-defined function's
//! invocation run one: the addresses a USING phrase passes, the LINKAGE records they bind, and what
//! happens around the callee's run, whose activation and procedure are the executor's. CANCEL,
//! which undoes what a run leaves, is here too.

use crate::abend::{Abend, AbendCode};
use crate::fixed::{align, zoned_digits};
use crate::host::{Host, Values};
use crate::lir::{CallArg, Chars};
use crate::storage::{Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{Loader, RunUnit, UnitHost};
use crate::vocab::{Figurative, Pos};
use numeric::Dialect;
use zarch::decimal;

type R<T> = Result<T, Abend>;

/// What building a USING phrase's arguments asks of the executor beyond `Values` and the run unit.
pub trait Arguments<'w, P: Copy, O>: Values<P, O> + UnitHost<'w> {
    /// The data item an operand names, when it names one.
    fn item(&self, operand: &O) -> Option<P>;
    /// Whether an operand is LENGTH OF, which BY CONTENT passes as a binary fullword.
    fn length_of(&self, operand: &O) -> bool;
    /// A data item BY CONTENT copies, located and given NUMCHECK's test.
    fn content_item(&mut self, place: P) -> R<Loc>;
}

/// Each argument's address in run-unit memory, in order: a data item BY REFERENCE its own, any other
/// argument a temporary's holding its bytes, OMITTED none. A BY VALUE argument's temporary is bound
/// to the parameter whether it is received BY VALUE or BY REFERENCE (assumption C333).
pub fn addresses<'w, P: Copy, O>(x: &mut impl Arguments<'w, P, O>, args: &[CallArg<P, O>], pos: Pos) -> R<Vec<Option<usize>>> {
    let mut addresses = Vec::with_capacity(args.len());
    addresses_into(x, args, pos, &mut addresses)?;
    Ok(addresses)
}

/// `addresses`, appended to `addresses`.
pub fn addresses_into<'w, P: Copy, O>(x: &mut impl Arguments<'w, P, O>, args: &[CallArg<P, O>], pos: Pos, addresses: &mut Vec<Option<usize>>) -> R<()> {
    for arg in args {
        let at = match arg {
            CallArg::Omitted => None,
            CallArg::Reference(p) => Some(x.locate(*p, false)?.offset),
            CallArg::Value(o) => {
                let val = x.value(o, pos)?;
                let bytes = value_argument(val, pos)?;
                Some(x.unit().push_temporary(&bytes))
            }
            CallArg::Content(c) => {
                let bytes = content(x, c, pos)?;
                Some(x.unit().push_temporary(&bytes))
            }
        };
        addresses.push(at);
    }
    Ok(())
}

/// A BY CONTENT argument's bytes: a data item's, a literal's as lowering made them, or another
/// operand's as `content_argument` makes them from its value.
pub fn content<'w, P: Copy, O>(x: &mut impl Arguments<'w, P, O>, chars: &Chars<P, O>, pos: Pos) -> R<Vec<u8>> {
    match chars {
        Chars::Literal(bytes) => Ok(bytes.clone()),
        Chars::Place(p) => {
            let loc = x.content_item(*p)?;
            Ok(store::bytes(x.mem(), loc).to_vec())
        }
        Chars::Value(o) => value_content(x, o, pos),
    }
}

fn value_content<'w, P: Copy, O>(x: &mut impl Arguments<'w, P, O>, operand: &O, pos: Pos) -> R<Vec<u8>> {
    let val = x.value(operand, pos)?;
    Ok(content_argument(&x.facts(), val, x.length_of(operand)))
}

/// A BY CONTENT argument that is not a data item, from its value: a literal as its own data item
/// would hold it (a number as unsigned zoned digits, as many as it has, the last with a minus zone
/// when it is negative; a figurative constant as the collating sequence's one byte), an address as
/// its four bytes, LENGTH OF as a binary fullword.
pub fn content_argument(facts: &dyn ProgramFacts, val: Val, length_of: bool) -> Vec<u8> {
    match val {
        Val::Bytes(b) | Val::All(b) | Val::National(b) | Val::AllNational(b) | Val::Dbcs(b) => b,
        Val::Fig(f) => vec![facts.figurative(f)],
        Val::Address(a) => a.to_be_bytes().to_vec(),
        Val::Num(f) if length_of => (align(&f, 0, false).and_then(|m| m.to_u128()).unwrap_or(0) as u32).to_be_bytes().to_vec(),
        Val::Num(f) => {
            let digits = f.places.total().max(1);
            let magnitude = align(&f, f.places.dec, false).and_then(|m| m.to_u128()).unwrap_or(0);
            zoned_digits(magnitude, digits as usize, if f.negative { decimal::MINUS } else { decimal::UNSIGNED })
        }
        Val::Float(h) => h.to_bytes(),
    }
}

/// A BY VALUE argument from its value: an integer as a binary fullword, an address, NULL, or the
/// bytes of a one-character item.
pub fn value_argument(val: Val, pos: Pos) -> R<Vec<u8>> {
    Ok(match val {
        Val::Num(f) => {
            let whole = align(&f, 0, false).and_then(|m| m.to_u128()).and_then(|m| i32::try_from(m).ok()).ok_or_else(|| Abend::ironwork("a BY VALUE integer beyond a fullword", pos))?;
            (if f.negative { -whole } else { whole }).to_be_bytes().to_vec()
        }
        Val::Address(a) => a.to_be_bytes().to_vec(),
        Val::Fig(Figurative::Null) => vec![0; 4],
        Val::Bytes(b) => b,
        _ => return Err(Abend::ironwork("this BY VALUE argument is not supported", pos)),
    })
}

/// The bytes a CALL passes, one argument after another, as the program's code page reads them: a
/// data item's storage, whatever the BY phrase, and any other argument's bytes as BY CONTENT makes
/// them.
pub fn arguments_text<'w, P: Copy, O>(x: &mut impl Arguments<'w, P, O>, args: &[CallArg<P, O>], pos: Pos) -> R<String> {
    let mut text = String::new();
    for arg in args {
        let bytes = match arg {
            CallArg::Omitted => continue,
            CallArg::Reference(p) | CallArg::Content(Chars::Place(p)) => item_bytes(x, *p)?,
            CallArg::Value(o) => match x.item(o) {
                Some(p) => item_bytes(x, p)?,
                None => value_content(x, o, pos)?,
            },
            CallArg::Content(c) => content(x, c, pos)?,
        };
        text.push_str(&x.facts().page().decode(&bytes));
    }
    Ok(text)
}

fn item_bytes<P: Copy>(x: &mut impl Host<P>, place: P) -> R<Vec<u8>> {
    let loc = x.locate(place, false)?;
    Ok(store::bytes(x.mem(), loc).to_vec())
}

/// What a callee's LINKAGE records are given once it is activated, each record by its ordinal.
pub struct Bindings<'a> {
    /// The data records of the object or factory an INVOKEd method runs on, after its own, and
    /// where each is.
    pub records: &'a [(usize, usize)],
    /// The record each item of the USING phrase the callee is entered by names, in order; None
    /// for an item that names no LINKAGE record.
    pub using: Vec<Option<usize>>,
    /// Each argument's address, in order.
    pub addresses: &'a [Option<usize>],
    /// The RETURNING record and its size. No argument addresses it, so it gets storage of its own
    /// for the call.
    pub returning: Option<(usize, usize)>,
}

impl Bindings<'_> {
    pub fn bind<H: Clone, L: Loader<H>>(&self, unit: &mut RunUnit<'_, H, L>, linkage: &mut [Option<usize>]) {
        for &(ordinal, address) in self.records {
            if let Some(slot) = linkage.get_mut(ordinal) {
                *slot = Some(address);
            }
        }
        for (ordinal, address) in self.using.iter().zip(self.addresses) {
            if let Some(slot) = ordinal.and_then(|o| linkage.get_mut(o)) {
                *slot = *address;
            }
        }
        if let Some((ordinal, size)) = self.returning {
            linkage[ordinal] = Some(unit.push_temporary(&vec![0; size]));
        }
    }
}

/// The statement a callee is run by, for what it does beyond what every one does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    /// Leaving an INITIAL program is a CANCEL of it (Language Reference SC27-8713-03, p. 349).
    Call { initial: bool },
    /// LINK, XCTL or a HANDLE ABEND PROGRAM exit, each in a run unit of its own, where the program
    /// starts from fresh storage, as CICS gives it on each one.
    Link,
    Invoke,
    /// A user-defined function's invocation. Functions are recursive.
    Function,
}

/// A loaded program run as a callee.
#[derive(Clone, Copy, Debug)]
pub struct Callee {
    pub index: usize,
    pub by: By,
    /// Memory's length before the caller pushed the callee's arguments, released back to once it
    /// returns; None when the caller releases them.
    pub mark: Option<usize>,
    pub pos: Pos,
}

/// Runs program `callee.index`: `run` activates it, binds its LINKAGE, runs its procedure and reads
/// what it returns, giving how the run ended with that. An error `run` gives comes before the
/// callee ran or after it ended, and leaves it as it is. Once it has returned the program is
/// inactive unless an earlier activation of it, which a RECURSIVE program or a function can have,
/// is still running (C127), an INITIAL program a CALL entered is cancelled, the temporaries since
/// `mark` are released, and an abend that ended it names its files. The caller passes STOP RUN up.
pub fn run<'w, X: UnitHost<'w>, O, T, E: From<Abend>>(x: &mut X, callee: &Callee, run: impl FnOnce(&mut X) -> Result<(R<O>, T), E>) -> Result<(R<O>, T), E> {
    let index = callee.index;
    let active = x.unit().programs[index].active;
    let (ending, value) = run(x)?;
    let unit = x.unit();
    unit.programs[index].active = active;
    if callee.by == (By::Call { initial: true }) {
        cancel_program(unit, index, callee.pos)?;
    }
    if let Some(mark) = callee.mark {
        unit.release_temporaries(mark);
    }
    // A method's abend is named by its class's source table, which its INVOKE fills.
    Ok((ending.map_err(|a| if callee.by == By::Invoke { a } else { in_loaded(unit, index, a) }), value))
}

/// A user-defined function's argument as its activation takes it: the address of the invoker's
/// data item, or a value the function's own storage takes (assumption C272).
#[derive(Clone, Debug)]
pub enum Bound {
    At(usize),
    Value(Val),
}

/// Each formal parameter's address, for arguments `bound` and the parameters' record sizes: an
/// item's own, or a temporary of zeros the activation then moves the value into.
pub fn bound_addresses<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, bound: &[Bound], sizes: impl Iterator<Item = usize>) -> Vec<Option<usize>> {
    bound
        .iter()
        .zip(sizes)
        .map(|(b, size)| {
            Some(match b {
                Bound::At(at) => *at,
                Bound::Value(_) => unit.push_temporary(&vec![0; size]),
            })
        })
        .collect()
}

/// An abend from program `index` named by that program's files, which a caller's file table would
/// misname: its own source by path when a program library supplied it, otherwise from its source
/// table, where the first program's source is empty. The innermost program an abend leaves names it.
pub fn in_loaded<H: Clone, L: Loader<H>>(unit: &RunUnit<'_, H, L>, index: usize, mut abend: Abend) -> Abend {
    let program = &unit.programs[index];
    if abend.file.is_none() {
        abend.file = Some(match (abend.pos.file, &program.source) {
            (0, Some(source)) => source.display().to_string(),
            (i, _) => program.compiled.as_ref().and_then(|c| L::source(c, usize::from(i))).unwrap_or_default(),
        });
    }
    abend
}

/// A CALL of `program`, of compilation unit `unit`, while it is active and not RECURSIVE: the
/// condition Language Environment signals, which nothing handles, ends the run unit with U4038
/// (Programming Guide SC27-8714-03, p. 557; C127), IGZ0015S for an outermost program and IGZ0064S
/// for a contained one (LE Runtime Messages).
pub fn recursive_call(program: &str, unit: &str, pos: Pos) -> Abend {
    let (program, unit) = (program.to_ascii_uppercase(), unit.to_ascii_uppercase());
    let message = if program == unit {
        format!("IGZ0015S A recursive call was attempted to a program that was already active. The program name is {program}.")
    } else {
        format!("IGZ0064S A recursive call to active program {program} in compilation unit {unit} was attempted.")
    };
    Abend { code: AbendCode::user(4038), message, pos, file: None }
}

/// Whether `names`, a program's callable or hidden contained programs, holds the program a CALL
/// names. Under Enterprise COBOL's scope rules a CALL never looks up a hidden one, and reaches a
/// contained program only when it is callable (Language Reference, Conventions for program-names).
pub fn names<'a>(names: impl IntoIterator<Item = &'a str>, name: &str) -> bool {
    names.into_iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Whether a CALL of an entry name gets a copy of the program with WORKING-STORAGE of its own: a
/// dynamic CALL does (assumption C51), except under gnucobol, where every entry name
/// shares the program's one copy, as cobc's do.
pub fn entry_copy(dynamic: bool, dialect: Dialect) -> bool {
    dynamic && dialect == Dialect::Ibm
}

/// CANCEL of a program a dynamic CALL entered; one only ever called statically, a contained one
/// too, is left as it is: "No action is taken when a CANCEL statement is executed if the specified
/// program has not been dynamically called" (Language Reference SC27-8713-03, CANCEL statement;
/// Programming Guide SC27-8714-03, Canceling a subprogram).
pub fn cancel<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, name: &str, pos: Pos) -> R<()> {
    let Some(index) = unit.find(name) else { return Ok(()) };
    if !unit.programs[index].dynamic {
        return Ok(());
    }
    if unit.programs[index].active {
        return Err(Abend { code: crate::abend::AbendCode::user(4038), message: format!("IGZ0032S A CANCEL was attempted on active program {}.", unit.programs[index].name), pos, file: None });
    }
    cancel_program(unit, index, pos)
}

/// Closes the files of program `index` and of the programs it contains, each of which next starts
/// in its initial state (pp. 103, 349).
pub fn cancel_program<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, index: usize, pos: Pos) -> R<()> {
    let files: Vec<_> = unit.programs[index].files.iter_mut().filter_map(Option::take).collect();
    for f in files {
        f.close().map_err(|e| Abend::ironwork(format!("CANCEL {}: {e}", unit.programs[index].name), pos))?;
    }
    unit.programs[index].initialized = false;
    let nested = unit.programs[index].compiled.as_ref().map(|c| L::nested(c).to_vec()).unwrap_or_default();
    for name in nested {
        if let Some(contained) = unit.find(&name) {
            cancel_program(unit, contained, pos)?;
        }
    }
    Ok(())
}
