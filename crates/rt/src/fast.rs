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
            let (shift, pad) = (64 - 8 * len, if n == 8 { 0 } else { HIGH << (8 * len) });
            let word = |at: usize| match self.mem.get(at..at + 8) {
                Some(b) => (u64::from_be_bytes(b.try_into().unwrap()) >> shift) | pad,
                None => {
                    let mut w = [0xF0; 8];
                    w[8 - n..].copy_from_slice(&self.mem[at..at + n]);
                    u64::from_be_bytes(w)
                }
            };
            let digits = |w: u64| w & HIGH == HIGH && ((w & !HIGH) + 0x0606_0606_0606_0606) & HIGH == 0;
            let (x, y) = (word(a), word(b));
            return (digits(x) && digits(y)).then(|| x.cmp(&y));
        }
        let (x, y) = (&self.mem[a..a + n], &self.mem[b..b + n]);
        x.iter().chain(y).all(|&c| (0xF0..=0xF9).contains(&c)).then(|| x.cmp(y))
    }

    /// A linear SEARCH whose one WHEN tests an unsigned zoned key of `len` bytes in each occurrence
    /// for equality with the zoned item at `key`, from occurrence `from` to `count`, occurrence 1's
    /// key at `first` and each next `stride` bytes on: where the search stops, at the first key
    /// equal to the item's or one past `count`, and whether its WHEN holds there. Every key it
    /// passes is compared by its bytes as `zoned_order` compares them; None where one of them, or
    /// the item, is not every byte a digit in the F zone, which the VM's own steps decide, or where
    /// the index's 4 bytes at `index` lie within the item or the keys, which its steps would move.
    // Out of line, the loop keeps its constants in registers that a large caller's code would spill.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub fn scan_zoned(&self, index: usize, first: usize, stride: usize, from: i64, count: i64, key: usize, len: u32) -> Option<(i64, bool)> {
        const HIGH: u64 = 0xF0F0_F0F0_F0F0_F0F0;
        const SIX: u64 = 0x0606_0606_0606_0606;
        let n = len as usize;
        if !(1..=8).contains(&n) || stride == 0 || !(1..=count).contains(&from) || self.options.invdata.is_some_and(|i| i.cleansign) {
            return None;
        }
        let apart = |at: usize, end: usize| index + 4 <= at || end <= index;
        if !apart(key, key + n) || !apart(first, first + (count - 1) as usize * stride + n) {
            return None;
        }
        // A key's bytes as the low bytes of a little-endian word, the rest FF: the word holds every
        // byte a digit in the F zone where each byte of both `w` and `w + six` is in the F zone, for
        // adding 6 to a digit above 9 carries out of its byte and clears that zone.
        let rest = if n == 8 { 0 } else { u64::MAX << (8 * len) };
        let six = SIX & !rest;
        let word = |at: usize| {
            let mut w = [0xFF; 8];
            w[..n].copy_from_slice(&self.mem[at..at + n]);
            u64::from_le_bytes(w)
        };
        let k = word(key);
        let (mut zones, mut sums) = (k, k.wrapping_add(six));
        let start = first + (from - 1) as usize * stride;
        let last = first + (count - 1) as usize * stride;
        let mut at = start;
        let mut found = false;
        if let Some(table) = self.mem.get(..last + 8) {
            // Two keys a step; the step where either equals the item's ends the loop, and the
            // single steps below take it on from the first of them.
            while at + stride <= last {
                let pair = &table[at..at + stride + 8];
                let w0 = u64::from_le_bytes(pair[..8].try_into().unwrap()) | rest;
                let w1 = u64::from_le_bytes(pair[stride..].try_into().unwrap()) | rest;
                if w0 == k || w1 == k {
                    break;
                }
                zones &= w0 & w1;
                sums &= w0.wrapping_add(six) & w1.wrapping_add(six);
                at += 2 * stride;
            }
        }
        while at <= last {
            let w = word(at);
            zones &= w;
            sums &= w.wrapping_add(six);
            if w == k {
                found = true;
                break;
            }
            at += stride;
        }
        (zones & sums & HIGH == HIGH).then_some((from + ((at - start) / stride) as i64, found))
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

    #[test]
    fn a_scan_stops_where_the_searchs_steps_stop() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let (mut scanned, mut found) = (0, 0);
        for _ in 0..20_000 {
            let len = 1 + (next() % 8) as u32;
            let stride = len as usize + (next() % 6) as usize;
            let count = 1 + (next() % 40) as i64;
            let first = len as usize + 4;
            let mut mem: Vec<u8> = (0..first + count as usize * stride + (next() % 3) as usize).map(|_| 0xF0 | (next() % 10) as u8).collect();
            for _ in 0..next() % 2 {
                let at = (next() as usize) % mem.len();
                mem[at] = if next() % 2 == 0 { (next() & 0xFF) as u8 } else { 0xC0 | (next() % 10) as u8 };
            }
            if next() % 2 == 0 {
                let ix = (next() % count as u64) as usize;
                let (k, e) = mem.split_at_mut(first);
                k[..len as usize].copy_from_slice(&e[ix * stride..ix * stride + len as usize]);
            }
            let from = (next() % (count as u64 + 2)) as i64;
            let options = Options::default();
            let s = Storage { mem: &mut mem, program: 0, local: 0, linkage: &[], options: &options };
            let Some((ix, hit)) = s.scan_zoned(len as usize, first, stride, from, count, 0, len) else { continue };
            assert!(s.scan_zoned(len as usize - 1, first, stride, from, count, 0, len).is_none() && s.scan_zoned(first, first, stride, from, count, 0, len).is_none());
            scanned += 1;
            found += usize::from(hit);
            let mut step = from;
            let stepped = loop {
                if !(1..=count).contains(&step) {
                    break false;
                }
                let at = first + (step - 1) as usize * stride;
                if s.zoned_order(at, 0, len).expect("a key the scan passed compares by its bytes").is_eq() {
                    break true;
                }
                step += 1;
            };
            assert_eq!((ix, hit), (step, stepped), "{:02X?} len {len} stride {stride} from {from} count {count}", s.mem);
        }
        assert!(scanned > 8_000 && found > 2_000, "{scanned} scans, {found} found");
    }
}
