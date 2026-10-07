//! What a program's generated code reads, computes and stores with where nothing watches the run
//! (`vm::Machine::watched`): the activation's storage and the VM's own count arithmetic, digit reads
//! and counted stores. Every function answers None where the VM's general path must decide, and
//! writes nothing then, so that generated code can run the op as the VM runs it instead.

pub use crate::count::{Number, aligned, count_mod, int_binop};
pub use crate::lir::Base;
pub use crate::storage::Kind;
pub use crate::vocab::{BinOp, SignClause, SignPosition};
pub use numeric::precision::Places;
pub use numeric::{Arith, Native, Options};
use crate::fixed::{MAX_DIGITS, places_of};
use crate::store;
use crate::unit::RETURN_CODE;
use std::cmp::Ordering;

/// An activation's storage as generated code reaches it.
pub struct Storage<'a> {
    pub mem: &'a mut [u8],
    pub program: usize,
    pub local: usize,
    pub linkage: &'a [Option<usize>],
    pub options: Options,
}

impl Storage<'_> {
    /// Where a place at `offset` from `base` begins, None where a LINKAGE record has no address or
    /// the place lies outside storage: `place::direct`'s address.
    #[inline]
    pub fn at(&self, base: Base, offset: u32, len: u32) -> Option<usize> {
        let start = match base {
            Base::Program => self.program,
            Base::Local => self.local,
            Base::ReturnCode => RETURN_CODE,
            Base::Linkage(record) => (*self.linkage.get(usize::from(record))?)?,
            _ => return None,
        };
        let at = start + offset as usize;
        (at + len as usize <= self.mem.len()).then_some(at)
    }

    /// Where an element of a table begins, each subscript a value and its stride, within the table
    /// `(displacement, extent)` where the place has one: the VM's quick place, None where a check
    /// fails.
    #[inline]
    pub fn element(&self, base: Base, offset: u32, len: u32, subscripts: &[(i64, u32)], table: Option<(u32, u32)>) -> Option<usize> {
        let composed: i64 = subscripts.iter().map(|&(value, stride)| crate::loc::subscript(value, stride)).sum();
        if let Some((displacement, extent)) = table {
            let from = i64::from(displacement) + composed;
            if from < 0 || from + i64::from(len) > i64::from(extent) {
                return None;
            }
        }
        let start = match base {
            Base::Program => self.program,
            Base::Local => self.local,
            Base::ReturnCode => RETURN_CODE,
            _ => return None,
        };
        let at = i64::try_from(start + offset as usize).ok()? + composed;
        (at >= 0 && at as usize + len as usize <= self.mem.len()).then_some(at as usize)
    }

    /// An item of no PICTURE P as a count: `store::read_digits`.
    #[inline]
    pub fn count(&self, at: usize, len: u32, kind: Kind) -> Option<Number> {
        let n = store::digits(&self.mem[at..at + len as usize], kind, || self.options)?;
        Some(Number::Int(n, places_of(kind)))
    }

    /// An item `store::read_integer` reads, of no PICTURE P.
    #[inline]
    pub fn integer(&self, at: usize, len: u32, kind: Kind) -> Option<i64> {
        match kind {
            Kind::Index | Kind::Binary { scale: 0, .. } | Kind::Packed { scale: 0, .. } | Kind::Zoned { scale: 0, .. } => store::digits(&self.mem[at..at + len as usize], kind, || self.options),
            _ => None,
        }
    }

    /// `store::store_count` of `n` into a binary, packed or zoned item of no PICTURE P, unless a size
    /// error and `keep` leave it as it is: whether it was one.
    #[inline]
    pub fn store(&mut self, at: usize, len: u32, kind: Kind, n: Number, rounded: bool, keep: bool) -> Option<bool> {
        let Number::Int(n, places) = n else { return None };
        let mut held = [0; MAX_DIGITS + 1];
        let (written, size_error) = store::count_bytes(kind, len as usize, (n, places), rounded, || self.options, &mut held)?.ok()?;
        if !(size_error && keep) {
            self.mem[at..at + written].copy_from_slice(&held[..written]);
        }
        Some(size_error)
    }

    /// `store::set_integer` of an index: its 4 bytes, None where the value does not fit them.
    #[inline]
    pub fn set_index(&mut self, at: usize, n: i64) -> Option<()> {
        let magnitude = i32::try_from(n.unsigned_abs()).ok()?;
        self.mem[at..at + 4].copy_from_slice(&(if n < 0 { -magnitude } else { magnitude }).to_be_bytes());
        Some(())
    }
}

/// Two counts compared as `store::compare` compares their values.
#[inline]
pub fn compare(x: Number, y: Number) -> Option<Ordering> {
    let (Number::Int(x, px), Number::Int(y, py)) = (x, y) else { return None };
    let (x, y) = aligned(i128::from(x), px.dec, i128::from(y), py.dec)?;
    Some(x.cmp(&y))
}

/// A literal's count, as generated code holds one.
#[inline]
pub const fn literal(n: i64, int: u32, dec: u32) -> Number {
    Number::Int(n, Places::new(int, dec))
}
