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
use numeric::binary::Binary;
use numeric::{Numproc, Trunc};
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
        let bytes = &self.mem[at..at + len as usize];
        if let Kind::Packed { signed, .. } = kind
            && len <= 9
            && !self.options.invdata.is_some_and(|i| i.cleansign)
        {
            return packed_count(bytes, signed || self.options.numproc != Numproc::Nopfd);
        }
        store::digits(bytes, kind, || *self.options)
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
    #[inline(always)]
    pub fn store(&mut self, at: usize, len: u32, kind: Kind, (n, places): (i64, Places), rounded: bool, keep: bool) -> Option<bool> {
        let fits = match kind {
            Kind::Packed { digits, scale, .. } => Some((digits, scale)),
            Kind::Zoned { digits, scale, sign: None, .. } if len == digits => Some((digits, scale)),
            Kind::Binary { digits, scale, signed, native: Native::No } if digits <= 18 && self.options.trunc == Trunc::Std && len as usize == Binary { digits: digits as u8, signed, native: Native::No }.bytes() => Some((digits, scale)),
            _ => None,
        };
        if let Some((digits, scale)) = fits
            && let Some((kept, size_error)) = kept(n.unsigned_abs(), places.dec, scale, digits, rounded)
        {
            if !(size_error && keep) {
                let negative = n < 0 && kept != 0;
                let item = self.mem.get_mut(at..at + len as usize)?;
                match kind {
                    Kind::Packed { signed, .. } => packed_into(item, kept, if !signed { 0xF } else if negative { 0xD } else { 0xC }),
                    Kind::Zoned { signed, .. } => zoned_into(item, kept, if !signed { 0xF } else if negative { 0xD } else { 0xC }),
                    Kind::Binary { signed, .. } => {
                        let v = if signed && n < 0 { -(kept as i64) } else { kept as i64 };
                        item.copy_from_slice(&v.to_be_bytes()[8 - item.len()..]);
                    }
                    _ => unreachable!(),
                }
            }
            return Some(size_error);
        }
        self.store_bytes(at, len, kind, (n, places), rounded, keep)
    }

    /// `store`, through `store::count_bytes` whatever the item.
    #[inline(never)]
    fn store_bytes(&mut self, at: usize, len: u32, kind: Kind, (n, places): (i64, Places), rounded: bool, keep: bool) -> Option<bool> {
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

/// A count's magnitude `m` at `dec` decimal places held at an item's `scale`, truncated or rounded,
/// and the digits of it that `digits` keep, with whether it lost any: `store::count_bytes`'s for a
/// value whose digits fit a `u64`, None for another.
#[inline(always)]
fn kept(m: u64, dec: u32, scale: u32, digits: u32, rounded: bool) -> Option<(u64, bool)> {
    let m = if scale >= dec {
        m.checked_mul(*POW10.get((scale - dec) as usize)?)?
    } else {
        let d = *POW10.get((dec - scale) as usize)?;
        m / d + u64::from(rounded && m % d >= d / 2)
    };
    let cap = *POW10.get(digits as usize)?;
    Some(if m < cap { (m, false) } else { (m % cap, true) })
}

/// Ten to each power a `u64` holds.
const POW10: [u64; 20] = {
    let mut table = [1; 20];
    let mut k = 1;
    while k < table.len() {
        table[k] = table[k - 1] * 10;
        k += 1;
    }
    table
};

/// Each number below 100 as two packed digits.
const BCD: [u8; 100] = {
    let mut table = [0; 100];
    let mut k = 0;
    while k < 100 {
        table[k] = ((k / 10) << 4 | k % 10) as u8;
        k += 1;
    }
    table
};

/// A packed item of 1 to 9 bytes as `codec::packed` reads it, its sign nibble taken as F where
/// `signed` is false: None where a digit is above 9 or the sign is not one.
#[inline(always)]
fn packed_count(bytes: &[u8], signed: bool) -> Option<i64> {
    const LOW: u64 = 0x0F0F_0F0F_0F0F_0F0F;
    let (&last, body) = bytes.split_last()?;
    let sign = if signed { last & 0xF } else { 0xF };
    if sign < 0xA || last >> 4 > 9 {
        return None;
    }
    let w = body.iter().fold(0u64, |w, &b| w << 8 | u64::from(b));
    let (lo, hi) = (w & LOW, w >> 4 & LOW);
    if ((lo + 0x0606_0606_0606_0606) | (hi + 0x0606_0606_0606_0606)) & !LOW != 0 {
        return None;
    }
    // Each byte's two digits as 0 to 99, then pairs of bytes, of 16-bit and of 32-bit lanes joined.
    let pairs = hi * 10 + lo;
    let fours = (pairs >> 8 & 0x00FF_00FF_00FF_00FF) * 100 + (pairs & 0x00FF_00FF_00FF_00FF);
    let eights = (fours >> 16 & 0x0000_FFFF_0000_FFFF) * 10_000 + (fours & 0x0000_FFFF_0000_FFFF);
    let m = ((eights >> 32) * 100_000_000 + (eights & 0xFFFF_FFFF)) * 10 + u64::from(last >> 4);
    let m = m as i64;
    Some(if sign == 0xB || sign == 0xD { -m } else { m })
}

/// `decimal::encode`'s bytes for `m`, its sign nibble `sign`.
#[inline(always)]
fn packed_into(item: &mut [u8], m: u64, sign: u8) {
    let (last, body) = item.split_last_mut().unwrap();
    *last = ((m % 10) as u8) << 4 | sign;
    let mut m = m / 10;
    for b in body.iter_mut().rev() {
        *b = BCD[(m % 100) as usize];
        m /= 100;
    }
}

/// `fixed::zoned_digits_into`'s bytes for `m`, the last byte's zone `zone`.
#[inline(always)]
fn zoned_into(item: &mut [u8], m: u64, zone: u8) {
    let mut m = m;
    for b in item.iter_mut().rev() {
        *b = 0xF0 | (m % 10) as u8;
        m /= 10;
    }
    let last = item.last_mut().unwrap();
    *last = zone << 4 | (*last & 0x0F);
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
    fn packed_bytes_read_as_store_reads_them() {
        let mut state = 0x6A09_E667_F3BC_C908u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let mut read = 0;
        for _ in 0..200_000 {
            let len = 1 + (next() % 9) as u32;
            let signed = next() % 2 == 0;
            let kind = Kind::Packed { digits: 2 * len - 1, scale: (next() % u64::from(2 * len)) as u32, signed };
            let mut mem: Vec<u8> = (0..len).map(|_| match next() % 12 {
                0 => (next() & 0xFF) as u8,
                _ => ((next() % 10) << 4 | (next() % 10)) as u8,
            }).collect();
            let sign = [0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 0x3][(next() % 7) as usize];
            *mem.last_mut().unwrap() = (*mem.last().unwrap() & 0xF0) | sign;
            for numproc in [Numproc::Nopfd, Numproc::Pfd] {
                let options = Options { numproc, ..Options::default() };
                let s = Storage { mem: &mut mem, program: 0, local: 0, linkage: &[], options: &options };
                let fast = s.digits(0, len, kind);
                read += usize::from(fast.is_some());
                assert_eq!(fast, store::digits(&s.mem[..], kind, || options), "{:02X?} as {kind:?} under {numproc:?}", s.mem);
            }
        }
        assert!(read > 150_000, "only {read} reads");
    }

    #[test]
    fn a_count_stores_as_count_bytes_stores_it() {
        let mut state = 0xD1B5_4A32_D192_ED03u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        for _ in 0..300_000 {
            let options = Options { trunc: [Trunc::Std, Trunc::Opt, Trunc::Bin][(next() % 3) as usize], ..Options::default() };
            let digits = 1 + (next() % 20) as u32;
            let scale = (next() % u64::from(digits + 1)) as u32;
            let signed = next() % 2 == 0;
            let (kind, len) = match next() % 3 {
                0 => (Kind::Packed { digits, scale, signed }, digits / 2 + 1),
                1 => (Kind::Zoned { digits, scale, signed, sign: None }, digits),
                _ => {
                    let digits = digits.min(18);
                    let scale = scale.min(digits);
                    (Kind::Binary { digits, scale, signed, native: Native::No }, Binary { digits: digits as u8, signed, native: Native::No }.bytes() as u32)
                }
            };
            let wide = next() % 19;
            let n = (next() % 10u64.pow(wide as u32 + 1).max(1)) as i64 * if next() % 3 == 0 { -1 } else { 1 };
            let dec = (next() % 20) as u32;
            let places = Places::new(19u32.saturating_sub(dec).max(1), dec);
            let (rounded, keep) = (next() % 2 == 0, next() % 2 == 0);
            let mut fast = vec![0xEE; len as usize + 2];
            let mut slow = fast.clone();
            let a = Storage { mem: &mut fast, program: 0, local: 0, linkage: &[], options: &options }.store(1, len, kind, (n, places), rounded, keep);
            let b = Storage { mem: &mut slow, program: 0, local: 0, linkage: &[], options: &options }.store_bytes(1, len, kind, (n, places), rounded, keep);
            assert_eq!((a, &fast), (b, &slow), "{n} at {places:?} into {kind:?} rounded {rounded} keep {keep}");
        }
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
