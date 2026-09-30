//! SET (lir.md §9.1): the `SetAddress` and `SetUpDown` ops, and the pointer rule of SET TO, which
//! is otherwise a MOVE. SET TO TRUE and TO FALSE are MOVEs the executor resolves.

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
