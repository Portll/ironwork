//! What a program's generated code reads, computes and stores with where nothing watches the run
//! (`vm::Machine::watched`): the activation's storage and the VM's own count arithmetic, digit reads
//! and counted stores. Every function answers None where the VM's general path must decide, and
//! writes nothing then, so that generated code can run the op as the VM runs it instead.

pub use crate::count::{binop, modulo, negated, order};
pub use crate::lir::Base;
pub use crate::storage::Kind;
pub use crate::vocab::{BinOp, SignClause, SignPosition};
pub use numeric::precision::Places;
pub use numeric::{Arith, Native, Options};
use crate::fixed::MAX_DIGITS;
use crate::store;
use crate::unit::RETURN_CODE;
use std::cmp::Ordering;

/// How generated code leaves a loop of blocks it runs whole: to another block, at a branch whose
/// condition its fast path did not decide, or at an op whose fast path declined.
pub enum Leave {
    To(u32),
    End(u32),
    Op(u32, usize),
}

/// An activation's storage as generated code reaches it.
pub struct Storage<'a> {
    pub mem: &'a mut [u8],
    pub program: usize,
    pub local: usize,
    pub linkage: &'a [Option<usize>],
    pub options: &'a Options,
}

impl Storage<'_> {
    /// Where a place at `offset` from `base` begins, None where a LINKAGE record has no address or
    /// the place lies outside storage: `place::direct`'s address.
    #[inline(always)]
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
    #[inline(always)]
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

    /// An item of no PICTURE P as a count of its last decimal place, `places_of` its kind:
    /// `store::read_digits`.
    #[inline(always)]
    pub fn digits(&self, at: usize, len: u32, kind: Kind) -> Option<i64> {
        store::digits(&self.mem[at..at + len as usize], kind, || *self.options)
    }

    /// An item `store::read_integer` reads, of no PICTURE P.
    #[inline(always)]
    pub fn integer(&self, at: usize, len: u32, kind: Kind) -> Option<i64> {
        match kind {
            Kind::Index | Kind::Binary { scale: 0, .. } | Kind::Packed { scale: 0, .. } | Kind::Zoned { scale: 0, .. } => store::digits(&self.mem[at..at + len as usize], kind, || *self.options),
            _ => None,
        }
    }

    /// `store::store_count` of `n`, a count at `places`, into a binary, packed or zoned item of no
    /// PICTURE P, unless a size error and `keep` leave it as it is: whether it was one.
    #[inline]
    pub fn store(&mut self, at: usize, len: u32, kind: Kind, (n, places): (i64, Places), rounded: bool, keep: bool) -> Option<bool> {
        let mut held = [0; MAX_DIGITS + 1];
        let (written, size_error) = store::count_bytes(kind, len as usize, (n, places), rounded, || *self.options, &mut held)?.ok()?;
        if !(size_error && keep) {
            self.mem[at..at + written].copy_from_slice(&held[..written]);
        }
        Some(size_error)
    }

    /// `store::set_integer` of an index: its 4 bytes, None where the value does not fit them.
    #[inline(always)]
    pub fn set_index(&mut self, at: usize, n: i64) -> Option<()> {
        let magnitude = i32::try_from(n.unsigned_abs()).ok()?;
        self.mem[at..at + 4].copy_from_slice(&(if n < 0 { -magnitude } else { magnitude }).to_be_bytes());
        Some(())
    }
}

impl Storage<'_> {
    /// Two unsigned zoned items of `len` bytes, the same scale and no SIGN clause, compared by their
    /// bytes where every byte is a digit in the F zone: the order of their values, which reading
    /// them gives. None where a byte is not, or INVDATA(CLEANSIGN) reads them otherwise.
    #[inline(always)]
    pub fn zoned_order(&self, a: usize, b: usize, len: u32) -> Option<Ordering> {
        if self.options.invdata.is_some_and(|i| i.cleansign) {
            return None;
        }
        let n = len as usize;
        if n <= 8 {
            // Eight bytes as one word, F0 before a shorter item's: every byte a digit in the F zone
            // when each high nibble is F and adding 6 to each low nibble carries into none.
            const HIGH: u64 = 0xF0F0_F0F0_F0F0_F0F0;
            let word = |at: usize| {
                let mut w = [0xF0; 8];
                w[8 - n..].copy_from_slice(&self.mem[at..at + n]);
                u64::from_be_bytes(w)
            };
            let digits = |w: u64| w & HIGH == HIGH && ((w & !HIGH) + 0x0606_0606_0606_0606) & HIGH == 0;
            let (x, y) = (word(a), word(b));
            return (digits(x) && digits(y)).then(|| x.cmp(&y));
        }
        let (x, y) = (&self.mem[a..a + n], &self.mem[b..b + n]);
        x.iter().chain(y).all(|&c| (0xF0..=0xF9).contains(&c)).then(|| x.cmp(y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numeric::Numproc;

    #[test]
    fn zoned_bytes_order_as_their_values_do() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        };
        let mut fast = 0;
        for _ in 0..20_000 {
            let digits = 1 + (next() % 18) as u32;
            let scale = (next() % u64::from(digits + 1)) as u32;
            let kind = Kind::Zoned { digits, scale, signed: false, sign: None };
            let len = digits as usize;
            let mut byte = || match next() % 20 {
                0 => (next() & 0xFF) as u8,
                1 => 0xC0 | (next() % 10) as u8,
                _ => 0xF0 | (next() % 10) as u8,
            };
            let mut mem: Vec<u8> = (0..2 * len).map(|_| byte()).collect();
            if next() % 4 == 0 {
                let (a, b) = mem.split_at_mut(len);
                b.copy_from_slice(a);
            }
            for numproc in [Numproc::Nopfd, Numproc::Pfd] {
                let options = Options { numproc, ..Options::default() };
                let s = Storage { mem: &mut mem, program: 0, local: 0, linkage: &[], options: &options };
                let Some(order_by_bytes) = s.zoned_order(0, len, digits) else { continue };
                fast += 1;
                let values = order(s.digits(0, digits, kind).unwrap(), scale, s.digits(len, digits, kind).unwrap(), scale);
                assert_eq!(Some(order_by_bytes), values, "{:02X?} at {kind:?} under {numproc:?}", &mem[..]);
            }
        }
        assert!(fast > 10_000, "only {fast} comparisons by bytes");
    }
}
