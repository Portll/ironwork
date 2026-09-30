//! What a statement's semantics ask of the executor running it (semantics-library.md C6): a
//! handle's `Loc` or value, evaluated where the walker would locate or evaluate it, and the stores.
//! `P` is the executor's handle to a data item and `O` to any other operand: the LIR's `PlaceId`
//! and `Operand` in the VM, the walker's own references in the interpreter.

use crate::abend::Abend;
use crate::storage::{Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::Pos;
use numeric::precision::{Fixed, Places};

type R<T> = Result<T, Abend>;

pub trait Host<P: Copy> {
    type Facts: ProgramFacts;
    fn facts(&self) -> Self::Facts;
    fn mem(&mut self) -> &mut [u8];
    /// With `receiving`, a group holding the object of its own OCCURS DEPENDING ON is its maximum length.
    fn locate(&mut self, place: P, receiving: bool) -> R<Loc>;
    /// The place's value as a subscript takes it.
    fn integer(&mut self, place: P, pos: Pos) -> R<i64>;
    /// MOVE into `dest`; `src` is the sender's storage when it has any.
    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()>;
    /// A numeric store, not ROUNDED, with no size error.
    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<()>;
}

pub trait Values<P: Copy, O>: Host<P> {
    fn value(&mut self, operand: &O, pos: Pos) -> R<Val>;
}

pub fn read<P: Copy>(x: &mut impl Host<P>, loc: Loc, pos: Pos) -> R<Val> {
    store::read(&x.facts(), x.mem(), loc, pos)
}

/// Locates `place` again and stores a whole number in it.
pub fn set_integer<P: Copy>(x: &mut impl Host<P>, place: P, value: i64, pos: Pos) -> R<()> {
    let dest = x.locate(place, false)?;
    x.store_fixed(dest, &Fixed::new(value as i128, Places::new(19, 0)), pos)
}
