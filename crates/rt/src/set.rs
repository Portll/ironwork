//! SET (lir.md §9.1): the `SetAddress` and `SetUpDown` ops, and the pointer rule of SET TO, which
//! is otherwise a MOVE. SET TO TRUE and TO FALSE are MOVEs the executor resolves. SET TO ENTRY
//! gives a function-pointer or procedure-pointer a value naming the entry a CALL through it enters.

use crate::abend::Abend;
use crate::host::{self, Host};
use crate::loc;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{Figurative, Pos};
use numeric::precision::{Fixed, Places};

type R<T> = Result<T, Abend>;

/// SET ADDRESS OF: the offset in run-unit memory the records are given, None for NULL.
pub fn address(val: Val, mem_len: usize, pos: Pos) -> R<Option<usize>> {
    let address = match val {
        Val::Address(a) => a,
        Val::Fig(Figurative::Null) => 0,
        _ => return Err(Abend::ironwork("SET ADDRESS OF takes a pointer, ADDRESS OF or NULL", pos)),
    };
    loc::offset_of(address, mem_len, pos)
}

/// SET TO: the value and sender MOVE takes into `dest`. A pointer takes only an address or NULL.
pub fn to(dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<(Val, Option<Loc>)> {
    match (dest.kind, val) {
        (Kind::Pointer, v @ (Val::Address(_) | Val::Fig(Figurative::Null))) => Ok((v, None)),
        (Kind::Pointer, _) => Err(Abend::ironwork("SET a pointer TO ADDRESS OF, NULL or another pointer", pos)),
        (_, v) => Ok((v, src)),
    }
}

/// A function-pointer or procedure-pointer set TO ENTRY holds this plus the entry's place in the
/// run unit's list: above any storage address, below the JNI function table's values.
const ENTRY_TAG: u32 = 0x7E00_0000;
const ENTRY_SPAN: u32 = 0x0100_0000;

/// An entry SET TO ENTRY named: its program-name, and whether a CALL through it is dynamic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dynamic: bool,
}

/// SET TO ENTRY: the value the receivers take, the same each time the entry is named (C140).
pub fn entry(entries: &mut Vec<Entry>, name: &str, dynamic: bool, pos: Pos) -> R<u32> {
    let wanted = Entry { name: name.to_owned(), dynamic };
    let slot = match entries.iter().position(|e| *e == wanted) {
        Some(slot) => slot,
        None => {
            entries.push(wanted);
            entries.len() - 1
        }
    };
    u32::try_from(slot).ok().filter(|&s| s < ENTRY_SPAN).map(|s| ENTRY_TAG + s).ok_or_else(|| Abend::ironwork("SET TO ENTRY: more entries than a pointer value can tell apart", pos))
}

/// The entry a function-pointer's or procedure-pointer's value names, when SET TO ENTRY gave it.
pub fn entry_of(entries: &[Entry], value: u32) -> Option<&Entry> {
    value.checked_sub(ENTRY_TAG).filter(|&s| s < ENTRY_SPAN).and_then(|s| entries.get(s as usize))
}

/// SET UP BY or DOWN BY: each target in turn, by what reading it gives, moved by `by`.
pub fn up_down<P: Copy>(x: &mut impl Host<P>, by: i64, down: bool, targets: &[P], pos: Pos) -> R<()> {
    let step = if down { -by } else { by };
    for &target in targets {
        let dest = x.locate(target, false)?;
        match host::read(x, dest, pos)? {
            Val::Address(a) => {
                let moved = u32::try_from(a as i64 + step).map_err(|_| Abend::ironwork("a pointer moved below zero", pos))?;
                store::write(x.mem(), dest, &moved.to_be_bytes());
            }
            Val::Num(f) => {
                let arith = x.facts().options().arith;
                let next = f.add(Fixed::new(step as i128, Places::new(19, 0)), 0, arith).map_err(|_| Abend::ironwork("SET UP/DOWN", pos))?;
                x.store_fixed(dest, &next, pos)?;
            }
            _ => return Err(Abend::ironwork("SET UP BY and DOWN BY take an index, integer or pointer", pos)),
        }
    }
    Ok(())
}
