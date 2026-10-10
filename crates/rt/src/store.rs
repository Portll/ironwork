//! Storage and MOVE: an item's bytes and value by its `Loc`, numeric stores with their size-error
//! and TRUNC(OPT) rules, MOVE, and the comparisons and class tests of conditions.

use crate::abend::{Abend, AbendCode};
use crate::codec;
use crate::edit;
use crate::fixed::{MAX_DIGITS, align, compare_fixed, fixed, places_of, pow10, scaled_down, scaled_up, zoned_digits, zoned_digits_into};
use crate::lir::{ByteClass, SenderCheck, SignTest};
use crate::picture::Sym;
use crate::storage::{Kind, Loc, Val};
use crate::unit::{Loader, RunUnit};
use crate::vocab::{Figurative, Pos, SignClause, SignPosition};
use numeric::binary::{self, Binary};
use numeric::precision::{Fixed, Places};
use numeric::{Native, Numproc, Options, Quote, Trunc, float, sign};
use std::cmp::Ordering;
use zarch::check::{ProgramCheck, ProgramMask};
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::{self, CodePage, Collation};
use zarch::hfp::{Hfp, Precision};
use zarch::wide::U256;

type R<T> = Result<T, Abend>;

/// What these semantics read of the running program beyond a `Loc`: its options, code page and
/// collating sequence, its edited PICTUREs, and its items by the index a `Loc` carries.
pub trait ProgramFacts {
    fn options(&self) -> Options;
    fn page(&self) -> &'static CodePage;
    /// A figurative constant's character: HIGH-VALUE and LOW-VALUE are the collating sequence's.
    fn figurative(&self, f: Figurative) -> u8;
    fn collation(&self) -> &Collation;
    /// A character's ordinal position in the collating sequence, from 1, which FUNCTION ORD gives.
    fn ordinal(&self, byte: u8) -> u16;
    /// The character at an ordinal position, which FUNCTION CHAR gives.
    fn character(&self, ordinal: i64) -> Option<u8>;
    /// How many characters the collating sequence orders.
    fn characters(&self) -> usize;
    fn decimal_point(&self) -> char;
    /// An edited PICTURE's symbols, and the currency sign it shows.
    fn edit(&self, edit: u32) -> (&[Sym], &str);
    /// PICTURE P positions to the right of an item's digits.
    fn scaling(&self, item: usize) -> u32;
    /// The item a TRUNC(OPT) report names.
    fn item_name(&self, item: usize) -> String;
    /// What NUMCHECK(ZON(LAX)) tolerates in a zoned item because of the item it redefines.
    fn lax_redefinition(&self, _item: usize) -> Option<LaxRedefinition> {
        None
    }
    /// Whether the compiler removed NUMCHECK's test of `item` where the reference at `pos` reads
    /// it, having found the test always fails.
    fn numcheck_removed(&self, _item: usize, _pos: Pos) -> bool {
        false
    }
}

/// The two redefinitions NUMCHECK(ZON(LAX)) tolerates (Programming Guide SC27-8714-03, pp. 390-391).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaxRedefinition {
    /// An unsigned item whose last byte is the last of a signed trailing-overpunch level-01 or
    /// level-77 item it redefines: tested as signed.
    Signed,
    /// A zoned item starting where a level-01 or level-77 numeric-edited item it redefines starts:
    /// this many of its leading bytes may hold spaces, those over the edited item's leading Z
    /// positions.
    LeadingSpaces(u32),
}

pub fn bytes(mem: &[u8], loc: Loc) -> &[u8] {
    &mem[loc.offset..loc.offset + loc.len]
}

/// A write taint does not see: only for operations that mark the run unfollowed
/// (`RunUnit::unfollowed`); any other goes through `RunUnit::write`.
pub fn write(mem: &mut [u8], loc: Loc, bytes: &[u8]) {
    mem[loc.offset..loc.offset + loc.len].copy_from_slice(bytes);
}

/// PICTURE scaling positions to the right of a numeric item's digits.
pub fn scaling(facts: &dyn ProgramFacts, loc: Loc) -> u32 {
    match loc.kind {
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } => facts.scaling(loc.item),
        _ => 0,
    }
}

/// The places of the value a numeric item holds, its scaling positions included.
pub fn places(facts: &dyn ProgramFacts, loc: Loc) -> Places {
    let places = places_of(loc.kind);
    Places::new(places.int + scaling(facts, loc), places.dec)
}

pub fn read(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos) -> R<Val> {
    let value = read_stored(facts, mem, loc, pos)?;
    Ok(match value {
        Val::Num(f) => Val::Num(scaled_up(f, scaling(facts, loc))),
        other => other,
    })
}

/// The value `read` gives an integer item, an index or an unscaled binary, packed or zoned item,
/// when it reads without an abend and fits an `i64`; None where it must be read as `read` reads it.
pub fn read_integer(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc) -> Option<i64> {
    match loc.kind {
        Kind::Index | Kind::Binary { scale: 0, .. } | Kind::Packed { scale: 0, .. } | Kind::Zoned { scale: 0, .. } => read_digits(facts, mem, loc),
        _ => None,
    }
}

/// The digits an index or a binary, packed or zoned item of any scale holds, as a signed count of
/// its last decimal place: the value `read` gives times ten to the scale, where it reads without an
/// abend, has no PICTURE P and fits an `i64`.
pub fn read_digits(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc) -> Option<i64> {
    if !matches!(loc.kind, Kind::Index) && facts.scaling(loc.item) != 0 {
        return None;
    }
    digits(bytes(mem, loc), loc.kind, || facts.options())
}

/// `read_digits` of an item with no PICTURE P whose bytes are `bytes`, the compile options read
/// only for a packed or zoned item.
#[inline(always)]
pub fn digits(bytes: &[u8], kind: Kind, options: impl FnOnce() -> Options) -> Option<i64> {
    let decimal = |d: Decimal| i64::try_from(d.magnitude).ok().map(|m| if d.negative { -m } else { m });
    match kind {
        Kind::Index => Some(i64::from(i32::from_be_bytes(bytes.try_into().ok()?))),
        Kind::Binary { digits, signed, native, .. } => i64::try_from(Binary { digits: digits as u8, signed, native }.load(bytes)).ok(),
        Kind::Packed { signed, .. } | Kind::Zoned { signed, .. } => {
            let options = options();
            if options.invdata.is_some_and(|i| i.cleansign) {
                return None;
            }
            let read = match kind {
                Kind::Zoned { sign, .. } => codec::zoned(bytes, signed, sign, options.numproc),
                _ => codec::packed(bytes, signed, options.numproc),
            };
            read.ok().and_then(decimal)
        }
        _ => None,
    }
}

/// An item's value as its digits hold it, before any scaling positions to their right.
pub fn read_stored(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos) -> R<Val> {
    let options = facts.options();
    let cleaned = options.invdata.is_some_and(|i| i.cleansign).then(|| sign_cleaned(bytes(mem, loc), loc.kind)).flatten();
    let bytes = cleaned.as_deref().unwrap_or(bytes(mem, loc));
    let places = places_of(loc.kind);
    Ok(match loc.kind {
        Kind::Group | Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => Val::Bytes(bytes.to_vec()),
        Kind::National => Val::National(bytes.to_vec()),
        Kind::Dbcs { .. } => Val::Dbcs(bytes.to_vec()),
        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => Val::Address(u32::from_be_bytes(bytes.try_into().unwrap())),
        Kind::Index => Val::Num(Fixed::new(i32::from_be_bytes(bytes.try_into().unwrap()) as i128, Places::new(9, 0))),
        Kind::Float(p) => Val::Float(Hfp::from_bytes(p, bytes)),
        Kind::Binary { digits, signed, native, .. } => Val::Num(Fixed::new(Binary { digits: digits as u8, signed, native }.load(bytes), places)),
        Kind::Packed { signed, .. } => {
            let d = codec::packed(bytes, signed, options.numproc).map_err(|c| Abend::check(c, pos))?;
            Val::Num(fixed(d.negative, U256::from_u128(d.magnitude), places))
        }
        Kind::Zoned { signed, sign, .. } => Val::Num(zoned_value(options.numproc, bytes, signed, sign, places, pos)?),
    })
}

/// INVDATA(CLEANSIGN): a zoned or packed item whose sign half-byte is not a sign code (0 to 9) is
/// read with that half-byte made F, positive (assumption C222); None when there is nothing to
/// clean. A separate sign is a character, not a half-byte, and is left alone.
fn sign_cleaned(bytes: &[u8], kind: Kind) -> Option<Vec<u8>> {
    let (at, high) = match kind {
        Kind::Packed { .. } => (bytes.len().checked_sub(1)?, false),
        Kind::Zoned { sign: Some(SignClause { separate: true, .. }), .. } => return None,
        Kind::Zoned { sign: Some(SignClause { position: SignPosition::Leading, .. }), .. } => (0, true),
        Kind::Zoned { .. } => (bytes.len().checked_sub(1)?, true),
        _ => return None,
    };
    let half = if high { bytes[at] >> 4 } else { bytes[at] & 0x0F };
    if half > 9 {
        return None;
    }
    let mut cleaned = bytes.to_vec();
    cleaned[at] |= if high { 0xF0 } else { 0x0F };
    Some(cleaned)
}

/// A zoned operand enters arithmetic through PACK, which keeps only the sign's zone.
pub fn zoned_value(numproc: Numproc, bytes: &[u8], signed: bool, sign: Option<SignClause>, places: Places, pos: Pos) -> R<Fixed> {
    let d = codec::zoned(bytes, signed, sign, numproc).map_err(|c| Abend::check(c, pos))?;
    Ok(fixed(d.negative, U256::from_u128(d.magnitude), places))
}

/// A value as STRING, UNSTRING and INSPECT see an operand that is not an item: its bytes, a
/// figurative constant as one character, a numeric literal as its digits.
pub fn natural_bytes(facts: &dyn ProgramFacts, val: Val, pos: Pos) -> R<Vec<u8>> {
    Ok(match val {
        Val::Bytes(b) | Val::All(b) | Val::National(b) | Val::AllNational(b) | Val::Dbcs(b) => b,
        Val::Fig(f) => vec![facts.figurative(f)],
        Val::Num(f) => zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED),
        _ => return Err(Abend::ironwork("this operand has no characters to work on", pos)),
    })
}

pub fn set_integer<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, value: i64, pos: Pos) -> R<()> {
    if matches!(dest.kind, Kind::Index)
        && let Ok(magnitude) = i32::try_from(value.unsigned_abs())
    {
        unit.write(dest.offset, &(if value < 0 { -magnitude } else { magnitude }).to_be_bytes());
        return Ok(());
    }
    store_fixed(facts, unit, dest, &Fixed::new(value as i128, Places::new(19, 0)), false, pos)
}

/// Stores an arithmetic result; returns whether it was a size error.
pub fn store_value<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
    match (loc.kind, value) {
        (Kind::Float(p), Val::Float(h)) => {
            let h = if h.precision.digits() > p.digits() { float::narrow_rounded(h, p).map_err(|c| Abend::check(c, pos))? } else { h.lengthen(p) };
            unit.write(loc.offset, &h.to_bytes());
            Ok(false)
        }
        (Kind::Float(p), Val::Num(f)) => {
            let h = float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?;
            unit.write(loc.offset, &h.to_bytes());
            Ok(false)
        }
        (_, Val::Float(h)) => {
            let (f, overflow) = float::to_receiver(h, places(facts, loc));
            if overflow && keep_on_size_error {
                return Ok(true);
            }
            Ok(store_fixed_checked(facts, unit, loc, &f, false, keep_on_size_error, pos)? || overflow)
        }
        (_, Val::Num(f)) => store_fixed_checked(facts, unit, loc, &f, rounded, keep_on_size_error, pos),
        _ => Err(Abend::ironwork("a non-numeric arithmetic result", pos)),
    }
}

pub fn store_fixed<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, value: &Fixed, rounded: bool, pos: Pos) -> R<()> {
    store_fixed_checked(facts, unit, loc, value, rounded, false, pos).map(|_| ())
}

/// Stores a value into a numeric item; returns whether it was a size error, and with
/// `keep_on_size_error` leaves the item unchanged on one.
pub fn store_fixed_checked<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, value: &Fixed, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
    let (image, size_error) = match integer_image(facts, loc, value) {
        Some(stored) => stored,
        None => image_of(facts, &mut *unit.err, loc, value, rounded, pos)?,
    };
    if size_error && keep_on_size_error {
        return Ok(true);
    }
    unit.write(loc.offset, image.bytes());
    Ok(size_error)
}

/// What `image_of` gives an integer below 2^64 stored into an unscaled binary or zoned item with no
/// PICTURE scaling, where TRUNC(OPT) has nothing to report; None for any other store.
fn integer_image(facts: &dyn ProgramFacts, loc: Loc, value: &Fixed) -> Option<(Image, bool)> {
    if value.places.dec != 0 || value.magnitude.hi != 0 || value.magnitude.lo > u128::from(u64::MAX) {
        return None;
    }
    let m = value.magnitude.lo;
    match loc.kind {
        Kind::Binary { digits, scale: 0, signed, native } if facts.scaling(loc.item) == 0 => {
            let options = facts.options();
            let v = if value.negative { -(m as i128) } else { m as i128 };
            let item = Binary { digits: digits as u8, signed, native };
            let (kept, divergence) = binary::kept(item, v, &options);
            if divergence.is_some() {
                return None;
            }
            let bits = 8 * item.bytes() as u32;
            let binary_range = if signed { v >= -(1i128 << (bits - 1)) && v < (1i128 << (bits - 1)) } else { (0..(1i128 << bits)).contains(&v.abs()) };
            let exceeds = if native.is_native() || options.trunc == Trunc::Bin { !binary_range } else { v.unsigned_abs() >= if digits <= 38 { pow10(digits).lo } else { 10u128.pow(digits) } };
            Some((Image::of(&kept.to_be_bytes()[16 - item.bytes()..]), exceeds))
        }
        Kind::Zoned { digits, scale: 0, signed, sign } if digits <= 38 && facts.scaling(loc.item) == 0 => {
            let cap = pow10(digits).lo;
            let kept = if m < cap { m } else { m % cap };
            let mut out = Image::zeroed(digits as usize + usize::from(sign.is_some_and(|s| s.separate)));
            zoned_image_into(out.bytes_mut(), kept, signed, signed && value.negative && kept != 0, sign);
            Some((out, m >= cap))
        }
        _ => None,
    }
}

/// `store_fixed_checked` of a value held as `n` counts of the last of `places`' decimal places into a
/// binary, packed or zoned item with no PICTURE P; None for any other store, one past 128 bits, or
/// a TRUNC(OPT) store that reports, which takes the value as a `Fixed`.
pub fn store_count<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, count: (i64, Places), rounded: bool, keep_on_size_error: bool, pos: Pos) -> Option<R<bool>> {
    let mut held = [0; MAX_DIGITS + 1];
    let (len, size_error) = match count_image(facts, loc, count, rounded, &mut held, pos)? {
        Ok(stored) => stored,
        Err(abend) => return Some(Err(abend)),
    };
    if size_error && keep_on_size_error {
        return Some(Ok(true));
    }
    unit.write(loc.offset, &held[..len]);
    Some(Ok(size_error))
}

/// What `image_of` writes for `store_count`'s value and receiver, written over the start of `out`:
/// its length, and whether it is a size error.
fn count_image(facts: &dyn ProgramFacts, loc: Loc, count: (i64, Places), rounded: bool, out: &mut [u8; MAX_DIGITS + 1], pos: Pos) -> Option<R<(usize, bool)>> {
    if facts.scaling(loc.item) != 0 {
        return None;
    }
    Some(count_bytes(loc.kind, loc.len, count, rounded, || facts.options(), out)?.map_err(|c| Abend::check(c, pos)))
}

/// `count_image` of a binary, packed or zoned item of `len` bytes with no PICTURE P, the compile
/// options read only for a binary item; None where it must be stored as a `Fixed`.
#[inline]
pub fn count_bytes(kind: Kind, len: usize, (n, places): (i64, Places), rounded: bool, options: impl FnOnce() -> Options, out: &mut [u8; MAX_DIGITS + 1]) -> Option<Result<(usize, bool), ProgramCheck>> {
    let (Kind::Binary { digits, scale, .. } | Kind::Packed { digits, scale, .. } | Kind::Zoned { digits, scale, .. }) = kind else { return None };
    let magnitude = u128::from(n.unsigned_abs());
    let m = if scale >= places.dec {
        magnitude.checked_mul(10u128.checked_pow(scale - places.dec)?)?
    } else {
        let d = 10u128.checked_pow(places.dec - scale)?;
        let (q, r) = (magnitude / d, magnitude % d);
        q + u128::from(rounded && r >= d / 2)
    };
    let negative = n < 0;
    Some(Ok(match kind {
        Kind::Packed { signed, .. } => {
            let cap = 10u128.checked_pow(digits)?;
            let kept = if m < cap { m } else { m % cap };
            let image = out.get_mut(..len).filter(|image| image.len() <= 16)?;
            if let Err(c) = decimal::encode(image, Decimal { negative: signed && negative && kept != 0, magnitude: kept }) {
                return Some(Err(c));
            }
            if !signed {
                *image.last_mut().unwrap() |= 0x0F;
            }
            (len, m >= cap)
        }
        Kind::Zoned { signed, sign, .. } => {
            let cap = 10u128.checked_pow(digits)?;
            let kept = if m < cap { m } else { m % cap };
            let len = digits as usize + usize::from(sign.is_some_and(|s| s.separate));
            zoned_image_into(out.get_mut(..len)?, kept, signed, signed && negative && kept != 0, sign);
            (len, m >= cap)
        }
        Kind::Binary { signed, native, .. } => {
            let magnitude = i128::try_from(m).ok()?;
            let v = if negative { -magnitude } else { magnitude };
            let item = Binary { digits: digits as u8, signed, native };
            let options = options();
            let (kept, divergence) = binary::kept(item, v, &options);
            if divergence.is_some() {
                return None;
            }
            let bits = 8 * item.bytes() as u32;
            let binary_range = if signed { v >= -(1i128 << (bits - 1)) && v < (1i128 << (bits - 1)) } else { (0..(1i128 << bits)).contains(&v.abs()) };
            let exceeds = if native.is_native() || options.trunc == Trunc::Bin { !binary_range } else { v.unsigned_abs() >= 10u128.checked_pow(digits)? };
            out[..item.bytes()].copy_from_slice(&kept.to_be_bytes()[16 - item.bytes()..]);
            (item.bytes(), exceeds)
        }
        _ => return None,
    }))
}

/// The bytes a store of `value` into `loc` writes, and whether it is a size error, with a TRUNC(OPT)
/// report written to `err`.
fn image_of(facts: &dyn ProgramFacts, err: &mut dyn std::io::Write, loc: Loc, value: &Fixed, rounded: bool, pos: Pos) -> R<(Image, bool)> {
    let beyond = || Abend::ironwork("a value wider than 256 bits", pos);
    let value = &scaled_down(*value, scaling(facts, loc));
    let options = facts.options();
    Ok(match loc.kind {
        Kind::Zoned { digits, scale, signed, sign } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let negative = signed && value.negative && kept != 0;
            let mut out = Image::zeroed(digits as usize + usize::from(sign.is_some_and(|s| s.separate)));
            zoned_image_into(out.bytes_mut(), kept, signed, negative, sign);
            (out, m >= cap)
        }
        Kind::Packed { digits, scale, signed } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let mut out = Image::zeroed(loc.len);
            decimal::encode(out.bytes_mut(), Decimal { negative: signed && value.negative && kept != 0, magnitude: kept }).map_err(|c| Abend::check(c, pos))?;
            if !signed {
                *out.bytes_mut().last_mut().unwrap() |= 0x0F;
            }
            (out, m >= cap)
        }
        Kind::Binary { digits, scale, signed, native } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let magnitude = m.to_u128().and_then(|m| i128::try_from(m).ok()).ok_or_else(beyond)?;
            let v = if value.negative { -magnitude } else { magnitude };
            let item = Binary { digits: digits as u8, signed, native };
            let (kept, divergence) = binary::kept(item, v, &options);
            if let Some(d) = divergence {
                let name = facts.item_name(loc.item);
                let _ = writeln!(
                    err,
                    "ironwork: {pos}: TRUNC(OPT) store of {} into {name} PIC {}9({digits}) BINARY: the PICTURE keeps {}, the binary field {}; {} was stored (-silent stops these reports)",
                    d.value,
                    if signed { "S" } else { "" },
                    d.decimal,
                    d.binary,
                    d.binary
                );
            }
            let bits = 8 * item.bytes() as u32;
            let binary_range = if signed { v >= -(1i128 << (bits - 1)) && v < (1i128 << (bits - 1)) } else { (0..(1i128 << bits)).contains(&v.abs()) };
            let exceeds = if native.is_native() || options.trunc == Trunc::Bin { !binary_range } else { v.unsigned_abs() >= if digits <= 38 { pow10(digits).lo } else { 10u128.pow(digits) } };
            (Image::of(&kept.to_be_bytes()[16 - item.bytes()..]), exceeds)
        }
        Kind::Float(p) => (Image::Grown(float::from_fixed(*value, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?.to_bytes()), false),
        Kind::Index => {
            let whole = align(value, 0, false).and_then(|m| m.to_u128()).and_then(|m| i32::try_from(m).ok()).ok_or_else(beyond)?;
            (Image::of(&(if value.negative { -whole } else { whole }).to_be_bytes()), false)
        }
        Kind::NumericEdited { edit, digits, scale, blank_when_zero } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let (syms, currency) = facts.edit(edit);
            let text = edit::numeric(syms, digits, value.negative && kept != 0, kept, blank_when_zero, facts.decimal_point(), currency);
            (Image::Grown(facts.page().encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?), m >= cap)
        }
        _ => return Err(Abend::ironwork("a numeric value stored into a non-numeric item", pos)),
    })
}

/// The bytes a numeric store writes, held without an allocation when they fit.
enum Image {
    Held([u8; MAX_DIGITS + 1], usize),
    Grown(Vec<u8>),
}

impl Image {
    fn zeroed(len: usize) -> Self {
        if len <= MAX_DIGITS + 1 { Self::Held([0; MAX_DIGITS + 1], len) } else { Self::Grown(vec![0; len]) }
    }

    fn of(bytes: &[u8]) -> Self {
        let mut image = Self::zeroed(bytes.len());
        image.bytes_mut().copy_from_slice(bytes);
        image
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Self::Held(held, len) => &held[..*len],
            Self::Grown(grown) => grown,
        }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Held(held, len) => &mut held[..*len],
            Self::Grown(grown) => grown,
        }
    }
}

/// A zoned item's bytes for a magnitude, with its sign as the SIGN clause places it.
pub fn zoned_image(magnitude: u128, digits: u32, signed: bool, negative: bool, sign: Option<SignClause>) -> Vec<u8> {
    let mut out = vec![0; digits as usize + usize::from(sign.is_some_and(|s| s.separate))];
    zoned_image_into(&mut out, magnitude, signed, negative, sign);
    out
}

/// `zoned_image` written over `out`, which has as many bytes.
fn zoned_image_into(out: &mut [u8], magnitude: u128, signed: bool, negative: bool, sign: Option<SignClause>) {
    let zone = match (signed, negative) {
        (false, _) => decimal::UNSIGNED,
        (true, true) => decimal::MINUS,
        (true, false) => decimal::PLUS,
    };
    match sign {
        Some(SignClause { separate: true, position }) => {
            let s = if negative { 0x60 } else { 0x4E };
            let (sign_byte, body) = if position == SignPosition::Leading { out.split_first_mut().unwrap() } else { out.split_last_mut().unwrap() };
            *sign_byte = s;
            zoned_digits_into(body, magnitude, decimal::UNSIGNED);
        }
        Some(SignClause { separate: false, position: SignPosition::Leading }) => {
            zoned_digits_into(out, magnitude, decimal::UNSIGNED);
            out[0] = (zone << 4) | (out[0] & 0x0F);
        }
        _ => zoned_digits_into(out, magnitude, zone),
    }
}

pub fn figurative_unit(f: Figurative, quote: Quote) -> u16 {
    match f {
        Figurative::Zero => 0x0030,
        Figurative::Space => 0x0020,
        Figurative::HighValue => 0xFFFF,
        Figurative::LowValue => 0x0000,
        Figurative::Quote => quote.unit(),
        Figurative::Null => 0,
    }
}

/// MOVE, and VALUE at start-up, into one receiving item. `src` is the sending item, when there is
/// one.
pub fn assign<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
    if let Some(s) = src
        && matches!(s.kind, Kind::Group)
        && matches!(dest.kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } | Kind::Dbcs { .. })
    {
        // A group move converts nothing (Language Reference SC27-8713-03, p. 410); single-byte
        // spaces pad a DBCS receiver, two of them a DBCS space (p. 214).
        let mut out = vec![ebcdic::SPACE; dest.len];
        let n = s.len.min(dest.len);
        out[..n].copy_from_slice(&bytes(&unit.mem, s)[..n]);
        unit.write(dest.offset, &out);
        return Ok(());
    }
    match dest.kind {
        Kind::Group | Kind::Alnum { .. } => {
            let justified = matches!(dest.kind, Kind::Alnum { justified: true });
            let image = match (dest.kind, src) {
                (Kind::Group, Some(s)) if matches!(val, Val::Num(_) | Val::Float(_) | Val::Address(_)) || is_decimal(s.kind) => bytes(&unit.mem, s).to_vec(),
                _ => alnum_image(facts, &val, src, dest.len, pos)?,
            };
            let mut out = vec![ebcdic::SPACE; dest.len];
            if justified && image.len() < dest.len {
                out[dest.len - image.len()..].copy_from_slice(&image);
            } else if justified {
                out.copy_from_slice(&image[image.len() - dest.len..]);
            } else {
                let n = image.len().min(dest.len);
                out[..n].copy_from_slice(&image[..n]);
            }
            unit.write(dest.offset, &out);
        }
        Kind::National => {
            let units: Vec<u16> = match val {
                Val::National(b) => b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect(),
                Val::AllNational(b) => b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).cycle().take(dest.len / 2).collect(),
                Val::Bytes(b) => facts.page().decode(&b).encode_utf16().collect(),
                Val::Dbcs(b) => facts.page().decode_dbcs(&b).encode_utf16().collect(),
                Val::Fig(f) => vec![figurative_unit(f, facts.options().quote); dest.len / 2],
                _ => return Err(Abend::ironwork("this value cannot be moved to a national item", pos)),
            };
            let mut out: Vec<u8> = units.iter().take(dest.len / 2).flat_map(|u| u.to_be_bytes()).collect();
            while out.len() < dest.len {
                out.extend_from_slice(&0x0020u16.to_be_bytes());
            }
            unit.write(dest.offset, &out);
        }
        Kind::Dbcs { justified, edit } => {
            let image = match val {
                Val::Dbcs(b) => b,
                Val::Fig(Figurative::Space) => Vec::new(),
                Val::All(b) if !b.is_empty() => b.iter().copied().cycle().take(dest.len).collect(),
                _ => return Err(Abend::ironwork("only DBCS data or SPACE can be moved to a DBCS item", pos)),
            };
            let out = match edit {
                Some(edit) => edit::dbcs(facts.edit(edit).0, &image),
                None => {
                    let mut out = vec![ebcdic::SPACE; dest.len];
                    let n = image.len().min(dest.len);
                    if justified {
                        out[dest.len - n..].copy_from_slice(&image[image.len() - n..]);
                    } else {
                        out[..n].copy_from_slice(&image[..n]);
                    }
                    out
                }
            };
            unit.write(dest.offset, &out);
        }
        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => match val {
            Val::Address(a) => unit.write(dest.offset, &a.to_be_bytes()),
            Val::Fig(Figurative::Null) => unit.write(dest.offset, &[0; 4]),
            _ => return Err(Abend::ironwork("a pointer takes an address: use SET ... TO ADDRESS OF or NULL", pos)),
        },
        Kind::Index => match val {
            Val::Num(f) => store_fixed(facts, unit, dest, &f, false, pos)?,
            _ => return Err(Abend::ironwork("an index takes an occurrence number", pos)),
        },
        Kind::AlnumEdited { edit } => {
            let (syms, _) = facts.edit(edit);
            let positions = syms.iter().filter(|s| !matches!(s, Sym::Insert(_))).count();
            let image = alnum_image(facts, &val, src, positions, pos)?;
            let page = facts.page();
            let out = edit::alphanumeric(syms, &image, ebcdic::SPACE, |c| page.encode_char(c).unwrap_or(ebcdic::SPACE));
            unit.write(dest.offset, &out);
        }
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } => match val {
            Val::Num(f) => {
                if let Some(s) = src
                    && packed_copy(facts, s, dest)
                {
                    let copied = sign::move_packed(bytes(&unit.mem, s), true, Numproc::Pfd);
                    unit.write(dest.offset, &copied);
                } else {
                    store_fixed(facts, unit, dest, &f, false, pos)?;
                }
            }
            Val::Float(h) => {
                let (f, _) = float::to_receiver(h, places(facts, dest));
                store_fixed(facts, unit, dest, &f, false, pos)?;
            }
            Val::Fig(Figurative::Zero) => store_fixed(facts, unit, dest, &Fixed::new(0, Places::new(1, 0)), false, pos)?,
            Val::Fig(f) => unit.write(dest.offset, &vec![facts.figurative(f); dest.len]),
            Val::All(b) => {
                let fill: Vec<u8> = b.iter().copied().cycle().take(dest.len).collect();
                unit.write(dest.offset, &fill);
            }
            Val::Bytes(b) if let Some(s) = src
                && is_decimal(s.kind)
                && is_decimal(dest.kind) =>
            {
                carry_digits(facts, unit, dest, s, &b, pos)?;
            }
            Val::Bytes(b) if facts.options().emulates_cobc()
                && !src.is_some_and(|s| is_decimal(s.kind) || matches!(s.kind, Kind::NumericEdited { .. }))
                && scaling(facts, dest) == 0
                && let Some((digits, scale)) = dest.kind.digits_scale() =>
            {
                let v = cobc_characters_value(&facts.page().decode(&b), digits, scale, facts.decimal_point());
                store_fixed(facts, unit, dest, &v, false, pos)?;
            }
            Val::Bytes(b) if integer_digits(facts, dest).is_some() && !matches!(src.map(|s| s.kind), Some(Kind::NumericEdited { .. })) => {
                move_digit_halves(facts, unit, dest, &b, pos)?;
            }
            Val::Bytes(b) => {
                let v = match src.map(|s| (s, s.kind)) {
                    Some((s, Kind::NumericEdited { edit, .. })) => {
                        let (syms, currency) = facts.edit(edit);
                        let (negative, magnitude) = edit::de_edit(syms, &facts.page().decode(&b), currency);
                        scaled_up(fixed(negative, U256::from_u128(magnitude), places_of(s.kind)), scaling(facts, s))
                    }
                    _ => {
                        let digits = &b[b.len().saturating_sub(MAX_DIGITS)..];
                        zoned_value(facts.options().numproc, digits, false, None, Places::new(digits.len() as u32, 0), pos)?
                    }
                };
                store_fixed(facts, unit, dest, &v, false, pos)?;
            }
            Val::National(_) | Val::AllNational(_) => return Err(Abend::ironwork("a national value cannot be moved to a numeric item", pos)),
            Val::Dbcs(_) => return Err(Abend::ironwork("a DBCS value cannot be moved to a numeric item", pos)),
            Val::Address(_) => return Err(Abend::ironwork("a pointer cannot be moved to a numeric item", pos)),
        },
        Kind::Float(p) => {
            let h = match val {
                Val::Float(h) if h.precision.digits() > p.digits() => float::narrow_rounded(h, p).map_err(|c| Abend::check(c, pos))?,
                Val::Float(h) => h.lengthen(p),
                Val::Num(f) => float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?,
                Val::Fig(Figurative::Zero) => Hfp::zero(p),
                _ => return Err(Abend::ironwork("this value cannot be moved to a floating-point item", pos)),
            };
            unit.write(dest.offset, &h.to_bytes());
        }
    }
    Ok(())
}

/// The digits of a zoned or packed integer item without P scaling.
fn integer_digits(facts: &dyn ProgramFacts, loc: Loc) -> Option<usize> {
    match loc.kind {
        Kind::Zoned { digits, scale: 0, .. } | Kind::Packed { digits, scale: 0, .. } if scaling(facts, loc) == 0 => Some(digits as usize),
        _ => None,
    }
}

fn is_decimal(kind: Kind) -> bool {
    matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. })
}

/// NUMPROC(PFD), a signed packed item moved to one of the same kind and scaling: the bytes are copied.
fn packed_copy(facts: &dyn ProgramFacts, src: Loc, dest: Loc) -> bool {
    matches!(dest.kind, Kind::Packed { digits, signed: true, .. } if digits > 0)
        && src.kind == dest.kind
        && scaling(facts, src) == scaling(facts, dest)
        && facts.options().numproc == Numproc::Pfd
}

/// Whether a MOVE from a zoned or packed sender compiles only to instructions that check no digit
/// or sign: a byte copy, PACK, UNPK, and the OI that makes a sign F. A packed sender to another packed
/// shape takes ZAP or SRP, a binary receiver CVB, a numeric-edited one ED, and those check
/// (assumption C260).
fn moved_unchecked(facts: &dyn ProgramFacts, src: Loc, dest: Loc) -> bool {
    match (src.kind, dest.kind) {
        (Kind::Packed { .. }, Kind::Packed { .. }) => packed_copy(facts, src, dest),
        (Kind::Zoned { .. } | Kind::Packed { .. }, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Group) => true,
        (Kind::Zoned { .. } | Kind::Packed { .. }, Kind::Alnum { .. } | Kind::AlnumEdited { .. }) => places_of(src.kind).dec == 0,
        _ => false,
    }
}

/// A MOVE's sending item read as a number; but where the MOVE checks nothing (C260) and its digits
/// or sign are not decimal, its bytes as stored, after INVDATA(CLEANSIGN), which `assign` carries
/// to the receiver unchecked.
pub fn move_sender(facts: &dyn ProgramFacts, mem: &[u8], src: Loc, dest: Loc, pos: Pos) -> R<Val> {
    if facts.options().emulates_cobc() && holds_characters(src, dest, bytes(mem, src)) {
        return Ok(Val::Bytes(bytes(mem, src).to_vec()));
    }
    match read(facts, mem, src, pos) {
        Err(Abend { code: AbendCode::Check(ProgramCheck::Data), .. }) if moved_unchecked(facts, src, dest) => {
            let stored = bytes(mem, src);
            let cleaned = facts.options().invdata.is_some_and(|i| i.cleansign).then(|| sign_cleaned(stored, src.kind)).flatten();
            Ok(Val::Bytes(cleaned.unwrap_or_else(|| stored.to_vec())))
        }
        value => value,
    }
}

/// Whether a zoned sender with no separate sign, moved to a zoned receiver with none, holds a byte
/// that is no digit character, or in its sign position no overpunched digit: cobc, which reads
/// characters, copies it.
pub fn holds_characters(src: Loc, dest: Loc, b: &[u8]) -> bool {
    let in_place = |kind: Kind| matches!(kind, Kind::Zoned { sign: None | Some(SignClause { separate: false, .. }), .. });
    let Kind::Zoned { signed, sign, .. } = src.kind else { return false };
    if !in_place(src.kind) || !in_place(dest.kind) || b.is_empty() {
        return false;
    }
    let sign_at = if matches!(sign, Some(SignClause { position: SignPosition::Leading, .. })) { 0 } else { b.len() - 1 };
    b.iter().enumerate().any(|(i, &x)| if signed && i == sign_at { x >> 4 < 0xA || x & 0x0F > 9 } else { x >> 4 != 0x0F || x & 0x0F > 9 })
}

/// A zoned or packed item's digits as half-bytes, most significant first, and its sign half-byte
/// when signed; SIGN SEPARATE gives D for '-' and C for any other character.
fn digit_halves(kind: Kind, b: &[u8]) -> (Vec<u8>, Option<u8>) {
    match kind {
        Kind::Packed { digits, signed, .. } => {
            let nibbles: Vec<u8> = b.iter().flat_map(|x| [x >> 4, x & 0x0F]).collect();
            let (sign, body) = nibbles.split_last().expect("a packed item is at least one byte");
            (body[body.len().saturating_sub(digits as usize)..].to_vec(), signed.then_some(*sign))
        }
        Kind::Zoned { sign: Some(SignClause { separate: true, position }), .. } => {
            let (s, body) = if position == SignPosition::Leading { (b[0], &b[1..]) } else { (b[b.len() - 1], &b[..b.len() - 1]) };
            (body.iter().map(|x| x & 0x0F).collect(), Some(if s == 0x60 { 0x0D } else { 0x0C }))
        }
        Kind::Zoned { signed, sign, .. } => {
            let at = if matches!(sign, Some(SignClause { position: SignPosition::Leading, .. })) { 0 } else { b.len() - 1 };
            (b.iter().map(|x| x & 0x0F).collect(), signed.then(|| b[at] >> 4))
        }
        _ => (Vec::new(), None),
    }
}

/// A zoned or packed sender whose digits or sign are not decimal, moved to a zoned or packed
/// receiver as PACK and UNPK move it: each receiver digit takes the sender's digit of the same
/// power of ten, zero where there is none, and the data exception comes where the receiver is next
/// read as a number (C260). Compiled for GnuCOBOL, a zoned receiver with no separate sign takes the
/// bytes of a zoned sender with none as cobc moves them (cob_move_display_to_display, libcob/move.c):
/// the sign byte read as [`cobc_sign_byte`] reads it, '0' for a space and where there is no byte,
/// any other byte as stored, and a negative sign overpunched on a digit.
fn carry_digits<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, src: Loc, sender: &[u8], pos: Pos) -> R<()> {
    if packed_copy(facts, src, dest) {
        unit.write(dest.offset, sender);
        return Ok(());
    }
    let power = |loc: Loc| i64::from(scaling(facts, loc)) - i64::from(places_of(loc.kind).dec);
    let in_place = |kind: Kind| matches!(kind, Kind::Zoned { sign: None | Some(SignClause { separate: false, .. }), .. });
    if facts.options().emulates_cobc() && in_place(src.kind) && in_place(dest.kind) {
        let (m, n) = (sender.len() as i64, dest.len as i64);
        let shift = m - n + power(src) - power(dest);
        let zero = facts.page().encode_char('0').unwrap_or(0xF0);
        let sign_at = |kind: Kind, len: usize| if matches!(kind, Kind::Zoned { sign: Some(SignClause { position: SignPosition::Leading, .. }), .. }) { 0 } else { len - 1 };
        let mut sender = sender.to_vec();
        let mut negative = false;
        if matches!(src.kind, Kind::Zoned { signed: true, .. }) && !sender.is_empty() {
            let at = sign_at(src.kind, sender.len());
            (sender[at], negative) = cobc_sign_byte(sender[at]);
        }
        let mut copied: Vec<u8> = (0..n)
            .map(|j| usize::try_from(j + shift).ok().and_then(|i| sender.get(i)).copied().filter(|&b| b != ebcdic::SPACE && b != 0).unwrap_or(zero))
            .collect();
        if negative && matches!(dest.kind, Kind::Zoned { signed: true, .. }) && !copied.is_empty() {
            let at = sign_at(dest.kind, copied.len());
            if (0xF0..=0xF9).contains(&copied[at]) {
                copied[at] = copied[at] & 0x0F | 0xD0;
            }
        }
        unit.write(dest.offset, &copied);
        return Ok(());
    }
    let (halves, sign) = digit_halves(src.kind, sender);
    let (m, n) = (halves.len() as i64, dest.kind.digits_scale().map_or(0, |(d, _)| d) as i64);
    let shift = m - n + power(src) - power(dest);
    let aligned: Vec<u8> = (0..n).map(|j| usize::try_from(j + shift).ok().and_then(|i| halves.get(i)).copied().unwrap_or(0)).collect();
    store_digit_halves(facts, unit, dest, &aligned, sign, pos)
}

/// Characters moved to a number of `digits` with `scale` decimal places as cobc moves them
/// (libcob/move.c, cob_move_alphanum_to_display): spaces before one sign skipped, the digits
/// aligned on the decimal point and the leftmost dropped, then spaces and the digit separator
/// passed over until the receiver is filled. Another character met before then, or a second
/// decimal point, gives zero.
pub fn cobc_characters_value(text: &str, digits: u32, scale: u32, point: char) -> Fixed {
    let separator = if point == ',' { '.' } else { ',' };
    let zero = Fixed::new(0, Places::new(digits, scale));
    let chars: Vec<char> = text.chars().collect();
    let mut at = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(chars.len());
    let negative = chars.get(at) == Some(&'-');
    if matches!(chars.get(at), Some('+' | '-')) {
        at += 1;
    }
    let before = chars[at..].iter().take_while(|&&c| c != point).filter(|c| c.is_ascii_digit()).count();
    let integer = (digits - scale) as usize;
    let mut skip = before.saturating_sub(integer);
    while skip > 0 && at < chars.len() {
        skip -= usize::from(chars[at].is_ascii_digit());
        at += 1;
    }
    let mut out = vec![0u8; digits as usize];
    let (mut slot, mut points) = (integer.saturating_sub(before), 0);
    while at < chars.len() && slot < out.len() {
        match chars[at] {
            c if c.is_ascii_digit() => {
                out[slot] = c as u8 - b'0';
                slot += 1;
            }
            c if c == point => {
                points += 1;
                if points > 1 {
                    return zero;
                }
            }
            c if c.is_whitespace() || c == separator => {}
            _ => return zero,
        }
        at += 1;
    }
    let magnitude = out.iter().fold(0u128, |v, &d| v * 10 + u128::from(d));
    fixed(negative && magnitude != 0, U256::from_u128(magnitude), Places::new(digits, scale))
}

/// An alphanumeric sender moved to a zoned or packed integer: the low half of each of its last bytes
/// as a digit, zeros to the left, stored positive and unchecked. The byte copy, PACK and UNPK such a
/// MOVE compiles to check no digit, so a non-digit is a data exception only where the item is next
/// read as a number (assumption C240).
fn move_digit_halves<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, sender: &[u8], pos: Pos) -> R<()> {
    let n = integer_digits(facts, dest).unwrap_or(0);
    let halves: Vec<u8> = (0..n).map(|i| (sender.len() + i).checked_sub(n).map_or(0, |k| sender[k] & 0x0F)).collect();
    store_digit_halves(facts, unit, dest, &halves, None, pos)
}

/// One digit half-byte per receiver digit, and the sender's sign half-byte: the value of the
/// decimal digits is stored, then each half above 9 is put back in its digit's place, and a sign
/// half that is a digit in a signed receiver's sign place. An unsigned receiver's sign is F.
fn store_digit_halves<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, halves: &[u8], sign: Option<u8>, pos: Pos) -> R<()> {
    let n = halves.len();
    let value = halves.iter().fold(0u128, |v, &h| v * 10 + if h > 9 { 0 } else { u128::from(h) });
    let magnitude = U256::from_u128(value).checked_mul(pow10(scaling(facts, dest))).unwrap_or_default();
    let negative = sign.is_some_and(|s| s > 9 && decimal::is_minus(s));
    store_fixed(facts, unit, dest, &fixed(negative, magnitude, places(facts, dest)), false, pos)?;
    let sign_digit = sign.filter(|&s| s <= 9);
    if halves.iter().all(|&h| h <= 9) && sign_digit.is_none() {
        return Ok(());
    }
    let mut out = bytes(&unit.mem, dest).to_vec();
    for (i, &h) in halves.iter().enumerate() {
        let (at, high) = match dest.kind {
            Kind::Packed { .. } => {
                let nibble = 2 * out.len() - 1 - n + i;
                (nibble / 2, nibble.is_multiple_of(2))
            }
            Kind::Zoned { sign: Some(SignClause { separate: true, position: SignPosition::Leading }), .. } => (i + 1, false),
            _ => (i, false),
        };
        out[at] = if high { (out[at] & 0x0F) | (h << 4) } else { (out[at] & 0xF0) | h };
    }
    if let Some(s) = sign_digit {
        let last = out.len() - 1;
        match dest.kind {
            Kind::Packed { signed: true, .. } => out[last] = (out[last] & 0xF0) | s,
            Kind::Zoned { signed: true, sign: Some(SignClause { separate: true, .. }), .. } => {}
            Kind::Zoned { signed: true, sign: Some(SignClause { position: SignPosition::Leading, .. }), .. } => out[0] = (out[0] & 0x0F) | (s << 4),
            Kind::Zoned { signed: true, .. } => out[last] = (out[last] & 0x0F) | (s << 4),
            _ => {}
        }
    }
    unit.write(dest.offset, &out);
    Ok(())
}

/// What an alphanumeric receiver gets from a zoned or packed integer whose digits or sign are not
/// decimal: a zoned sender's digit bytes as stored, an overpunched sign's zone made F; a packed
/// sender's digits unpacked with F zones; a zero for each P (C260).
fn unchecked_digit_bytes(facts: &dyn ProgramFacts, src: Loc, b: &[u8]) -> Vec<u8> {
    let mut out = match src.kind {
        Kind::Zoned { sign: Some(SignClause { separate: true, position }), .. } => {
            if position == SignPosition::Leading { b[1..].to_vec() } else { b[..b.len() - 1].to_vec() }
        }
        Kind::Zoned { signed: true, sign, .. } => {
            let mut digits = b.to_vec();
            let at = if matches!(sign, Some(SignClause { position: SignPosition::Leading, .. })) { 0 } else { digits.len() - 1 };
            digits[at] |= 0xF0;
            digits
        }
        Kind::Zoned { .. } => b.to_vec(),
        _ => digit_halves(src.kind, b).0.iter().map(|h| 0xF0 | h).collect(),
    };
    out.resize(out.len() + scaling(facts, src) as usize, 0xF0);
    out
}

/// The bytes an alphanumeric receiver of `len` gets from `val`.
pub fn alnum_image(facts: &dyn ProgramFacts, val: &Val, src: Option<Loc>, len: usize, pos: Pos) -> R<Vec<u8>> {
    Ok(match val {
        Val::Bytes(b) => match src {
            Some(s) if is_decimal(s.kind) => unchecked_digit_bytes(facts, s, b),
            _ => b.clone(),
        },
        Val::All(b) => b.iter().copied().cycle().take(len.max(b.len())).collect(),
        Val::Fig(f) => vec![facts.figurative(*f); len],
        Val::Num(f) if f.places.dec == 0 => {
            let digits = src.and_then(|s| s.kind.digits_scale().map(|(d, _)| d + scaling(facts, s))).unwrap_or(f.places.total());
            zoned_digits(f.magnitude.to_u128().unwrap_or(0), digits as usize, decimal::UNSIGNED)
        }
        Val::Dbcs(b) => b.clone(),
        Val::National(_) | Val::AllNational(_) => return Err(Abend::ironwork("a national value cannot be moved to an alphanumeric item", pos)),
        _ => return Err(Abend::ironwork("only an integer numeric value can be moved to an alphanumeric item", pos)),
    })
}

/// What NUMCHECK finds wrong with a sending item's data, or None (see [`numcheck_fault_in`]).
pub fn numcheck_fault(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, as_integer: bool) -> Option<&'static str> {
    numcheck_fault_in(&facts.options(), loc.kind, bytes(mem, loc), facts.lax_redefinition(loc.item), as_integer)
}

/// What NUMCHECK finds wrong with `stored`, an item of `kind`, or None: a zoned or packed item that
/// is not NUMERIC, its sign half-byte cleaned first under INVDATA(CLEANSIGN), and under ZON(LAX) a
/// zoned item's `lax` redefinition tolerated; an alphanumeric item moved to a numeric one
/// (`as_integer`) that is not an unsigned integer's digits; or a binary item holding more digits
/// than its PICTURE. COMP-5 is not checked, nor binary under TRUNC(BIN) with BIN(NOTRUNCBIN)
/// (Programming Guide SC27-8714-03, pp. 388-391).
pub fn numcheck_fault_in(options: &Options, kind: Kind, stored: &[u8], lax: Option<LaxRedefinition>, as_integer: bool) -> Option<&'static str> {
    if !numcheck_tests(options, kind, as_integer) {
        return None;
    }
    let check = options.numcheck?;
    let cleaned = options.invdata.is_some_and(|i| i.cleansign).then(|| sign_cleaned(stored, kind)).flatten();
    let b = cleaned.as_deref().unwrap_or(stored);
    let lax = lax.filter(|_| check.zon.is_some_and(|z| z.lax));
    let spaces = match lax {
        Some(LaxRedefinition::LeadingSpaces(n)) => n as usize,
        _ => 0,
    };
    let digit = |x: &u8| (0xF0..=0xF9).contains(x);
    let digits = |from: usize, bytes: &[u8]| bytes.iter().enumerate().all(|(i, x)| digit(x) || from + i < spaces && *x == ebcdic::SPACE);
    let overpunch = |at: usize, x: u8| matches!(x >> 4, 0xC | 0xD | 0xF) && x & 0x0F <= 9 || at < spaces && x == ebcdic::SPACE;
    let valid = match kind {
        Kind::Zoned { signed, sign, .. } => {
            let last = b.len() - 1;
            match (signed || lax == Some(LaxRedefinition::Signed), sign) {
                (false, _) => digits(0, b),
                (true, Some(SignClause { separate: true, position })) => {
                    let (s, rest) = if position == SignPosition::Leading { (b[0], &b[1..]) } else { (b[last], &b[..last]) };
                    matches!(s, 0x4E | 0x60) && rest.iter().all(digit)
                }
                (true, Some(SignClause { position: SignPosition::Leading, .. })) => overpunch(0, b[0]) && digits(1, &b[1..]),
                (true, _) => overpunch(last, b[last]) && digits(0, &b[..last]),
            }
        }
        Kind::Group | Kind::Alnum { .. } => b.iter().all(digit),
        Kind::Packed { digits, signed, .. } => {
            let spare_clear = digits % 2 == 1 || b[0] >> 4 == 0;
            decimal::tp(b).is_ok_and(|cc| cc.0 == 0) && (signed || b[b.len() - 1] & 0x0F == 0x0F) && spare_clear
        }
        Kind::Binary { digits, signed, native, .. } => {
            let raw = Binary { digits: digits as u8, signed, native }.load(b);
            raw.unsigned_abs() < 10u128.pow(digits)
        }
        _ => true,
    };
    (!valid).then_some(match kind {
        Kind::Binary { .. } => "has more digits than its PICTURE allows",
        _ => "is not NUMERIC",
    })
}

/// Whether NUMCHECK, under `options`, tests the data of an item of `kind`, which `as_integer`
/// extends to an alphanumeric or group item; `numcheck_fault` tests only these.
pub fn numcheck_tests(options: &Options, kind: Kind, as_integer: bool) -> bool {
    let Some(check) = options.numcheck else { return false };
    match kind {
        Kind::Zoned { .. } => check.zon.is_some(),
        Kind::Group | Kind::Alnum { .. } => as_integer && check.zon.is_some(),
        Kind::Packed { .. } => check.pac,
        Kind::Binary { native: Native::No, .. } => check.bin.is_some_and(|c| c.truncbin || options.trunc != Trunc::Bin),
        _ => false,
    }
}

/// NUMCHECK's run-time check of a sending item: under MSG a warning on the error stream, with the
/// item, its bytes in hexadecimal, the line and the program, and the statement runs; under ABD a
/// terminating message, U4038 (assumptions [`numeric::assumptions::NUMCHECK_SENDERS`] and
/// [`numeric::assumptions::NUMCHECK_MESSAGE`]).
pub fn numcheck<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, as_integer: bool, program: &str, pos: Pos) -> R<()> {
    if facts.numcheck_removed(loc.item, pos) {
        return Ok(());
    }
    let Some(why) = numcheck_fault(facts, &unit.mem, loc, as_integer) else { return Ok(()) };
    let (name, value) = (facts.item_name(loc.item), crate::digest::hex(bytes(&unit.mem, loc)).to_ascii_uppercase());
    let detail = format!("{name} X'{value}' in program {program} {why}");
    let line = pos.line;
    let binary = matches!(loc.kind, Kind::Binary { .. });
    let abd = facts.options().numcheck.is_some_and(|c| c.abd);
    let failed = if binary {
        "was invalid. The value exceeded the number of digits in the data definition, and failed the SIZE ERROR test generated by the NUMCHECK(BIN) compiler option."
    } else {
        "failed the NUMERIC class test or contained a value larger than the PICTURE clause as detected by the NUMCHECK compiler option."
    };
    let message = match (abd, binary) {
        (true, false) => format!("IGZ0278S The contents of data item {name} at the time of reference on line {line} {failed} ({detail})"),
        (true, true) => format!("IGZ0315S The contents of data item {name} at the time of reference on line {line} {failed} ({detail})"),
        (false, false) => format!("IGZ0279W The value X'{value}' of data item {name} at the time of reference on line {line} in program {program} {failed} ({detail})"),
        (false, true) => format!("IGZ0316W The value X'{value}' of data item {name} at the time of reference on line {line} in program {program} {failed} ({detail})"),
    };
    if abd {
        return Err(Abend { code: crate::abend::AbendCode::user(4038), message, pos, file: None });
    }
    let _ = writeln!(unit.err, "ironwork: {pos}: {message}; the statement runs");
    Ok(())
}

/// What NUMCHECK tests of a MOVE's sending item of kind `sender` moved to a receiver of kind
/// `receiver`: an alphanumeric or group sender to a numeric receiver as an unsigned integer's
/// digits, any other as `numcheck` tests an item, and under ZON(LAX) a zoned sender to a zoned,
/// alphanumeric or group receiver not at all (Programming Guide SC27-8714-03, pp. 388-391).
pub fn move_check(options: &Options, sender: Kind, receiver: Kind) -> SenderCheck {
    let Some(check) = options.numcheck else { return SenderCheck::None };
    let lax = check.zon.is_some_and(|z| z.lax);
    if lax && matches!(sender, Kind::Zoned { .. }) && matches!(receiver, Kind::Zoned { .. } | Kind::Alnum { .. } | Kind::Group) {
        return SenderCheck::None;
    }
    let receiver_numeric = matches!(receiver, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. });
    if receiver_numeric && matches!(sender, Kind::Alnum { .. } | Kind::Group) { SenderCheck::Integer } else { SenderCheck::Item }
}

/// NUMCHECK's test of a MOVE's sending item as `check` names it.
pub fn numcheck_sender<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, loc: Loc, check: SenderCheck, program: &str, pos: Pos) -> R<()> {
    match check {
        SenderCheck::None => Ok(()),
        SenderCheck::Item => numcheck(facts, unit, loc, false, program, pos),
        SenderCheck::Integer => numcheck(facts, unit, loc, true, program, pos),
    }
}

/// ZON(NOALPHNUM): NUMCHECK leaves an item compared with an alphanumeric operand untested
/// (Programming Guide SC27-8714-03, pp. 389-390).
pub fn noalphnum(options: &Options) -> bool {
    options.numcheck.and_then(|c| c.zon).is_some_and(|z| !z.alphnum)
}

/// Whether an item of `kind` is a nonnumeric operand of a comparison: one that ZON(NOALPHNUM)
/// spares the other operand's test against, and that a zoned integer is compared with by its bytes.
pub fn nonnumeric(kind: Kind) -> bool {
    matches!(kind, Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. })
}

/// NUMERIC, ALPHABETIC, ALPHABETIC-LOWER or ALPHABETIC-UPPER, tested on an item's bytes.
pub fn byte_class(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, test: ByteClass) -> bool {
    let bytes = bytes(mem, loc);
    match test {
        ByteClass::Packed { signed } => decimal::tp(bytes).is_ok_and(|cc| cc.0 == 0) && (signed || bytes.last().is_some_and(|b| b & 0x0F == 0x0F)),
        ByteClass::Zoned { signed } => bytes.iter().enumerate().all(|(i, &b)| {
            let zone_ok = if i + 1 == bytes.len() && signed { matches!(b >> 4, 0xC | 0xD | 0xF) } else { b >> 4 == 0xF };
            zone_ok && b & 0x0F <= 9
        }),
        ByteClass::Digits => bytes.iter().all(|b| (0xF0..=0xF9).contains(b)),
        ByteClass::Alphabetic => bytes.iter().all(|&b| b == ebcdic::SPACE || facts.page().decode_byte(b).is_ascii_alphabetic()),
        ByteClass::AlphabeticLower => bytes.iter().all(|&b| b == ebcdic::SPACE || facts.page().decode_byte(b).is_ascii_lowercase()),
        ByteClass::AlphabeticUpper => bytes.iter().all(|&b| b == ebcdic::SPACE || facts.page().decode_byte(b).is_ascii_uppercase()),
        ByteClass::Dbcs | ByteClass::Kanji => {
            let first = if test == ByteClass::Kanji { 0x41..=0x7E } else { 0x41..=0xFE };
            bytes.len().is_multiple_of(2) && bytes.as_chunks::<2>().0.iter().all(|c| *c == [ebcdic::SPACE; 2] || first.contains(&c[0]) && (0x41..=0xFE).contains(&c[1]))
        }
        ByteClass::Set { bits } => bytes.iter().all(|&b| bits[usize::from(b / 8)] >> (b % 8) & 1 == 1),
    }
}

/// A DBCS literal's bytes, two for each character, in the program's code page, which must be one
/// of the mixed pages; a single-byte page has no DBCS characters (assumption
/// [`numeric::assumptions::DBCS_UNDER_SINGLE_BYTE_PAGE`]).
pub fn dbcs_literal(page: &CodePage, text: &str) -> Result<Vec<u8>, String> {
    let dbcs = page.dbcs().ok_or_else(|| format!("CODEPAGE({}) is a single-byte page with no DBCS characters: a DBCS literal needs one of the mixed pages DBCS programs compile with", page.ccsid))?;
    dbcs.encode(text, page.ccsid).map_err(|e| e.to_string())
}

/// POSITIVE, NEGATIVE or ZERO, tested on a value.
pub fn sign_test(val: Val, test: SignTest, pos: Pos) -> R<bool> {
    let v = match val {
        Val::Num(f) => f,
        Val::Float(h) => float::to_fixed(h, Places::new(31, 0), false).0,
        _ => return Err(Abend::ironwork("a sign condition on a non-numeric operand", pos)),
    };
    Ok(match test {
        SignTest::Positive => !v.negative && !v.magnitude.is_zero(),
        SignTest::Negative => v.negative,
        SignTest::Zero => v.magnitude.is_zero(),
    })
}

/// Compares two operands, each a value and, for an item, its `Loc`: packed items under
/// NUMPROC(PFD) as bytes, pointers as addresses, numbers algebraically, national as UTF-16, and
/// anything else as alphanumeric in the program's collating sequence.
pub fn compare(facts: &dyn ProgramFacts, mem: &[u8], a: (Val, Option<Loc>), b: (Val, Option<Loc>), pos: Pos) -> R<Ordering> {
    let ((va, la), (vb, lb)) = (a, b);
    if let (Some(x), Some(y)) = (la, lb)
        && let (Kind::Packed { .. }, true, Numproc::Pfd) = (x.kind, x.kind == y.kind && scaling(facts, x) == scaling(facts, y), facts.options().numproc)
    {
        return sign::compare_packed(bytes(mem, x), bytes(mem, y), Numproc::Pfd).map_err(|c| Abend::check(c, pos));
    }
    let address = |v: &Val| match v {
        Val::Address(a) => Some(*a),
        Val::Fig(Figurative::Null) => Some(0),
        _ => None,
    };
    if matches!(va, Val::Address(_)) || matches!(vb, Val::Address(_)) {
        return match (address(&va), address(&vb)) {
            (Some(x), Some(y)) => Ok(x.cmp(&y)),
            _ => Err(Abend::ironwork("a pointer compared with something other than a pointer or NULL", pos)),
        };
    }
    let numeric = |v: &Val| matches!(v, Val::Num(_) | Val::Float(_));
    match (&va, &vb) {
        (Val::Float(_), _) | (_, Val::Float(_)) if (numeric(&va) || matches!(va, Val::Fig(Figurative::Zero))) && (numeric(&vb) || matches!(vb, Val::Fig(Figurative::Zero))) => {
            let to_float = |v: &Val| -> R<Hfp> {
                Ok(match v {
                    Val::Float(h) => h.lengthen(Precision::Extended),
                    Val::Num(f) => float::from_fixed(*f, Precision::Extended, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?,
                    _ => Hfp::zero(Precision::Extended),
                })
            };
            Ok(to_float(&va)?.compare(to_float(&vb)?))
        }
        (Val::Num(x), Val::Num(y)) => Ok(compare_fixed(x, y)),
        (Val::Num(x), Val::Fig(Figurative::Zero)) => Ok(compare_fixed(x, &Fixed::new(0, Places::new(1, 0)))),
        (Val::Fig(Figurative::Zero), Val::Num(y)) => Ok(compare_fixed(&Fixed::new(0, Places::new(1, 0)), y)),
        (Val::National(x), Val::National(y)) => Ok(compare_national(x, y)),
        (Val::National(x), Val::AllNational(y)) => Ok(compare_national(x, &repeated(y, x.len()))),
        (Val::AllNational(x), Val::National(y)) => Ok(compare_national(&repeated(x, y.len()), y)),
        (Val::Dbcs(_), Val::National(_)) | (Val::National(_), Val::Dbcs(_)) => {
            let national = |v: &Val| match v {
                Val::Dbcs(b) => facts.page().decode_dbcs(b).encode_utf16().flat_map(u16::to_be_bytes).collect(),
                Val::National(b) => b.clone(),
                _ => Vec::new(),
            };
            Ok(compare_national(&national(&va), &national(&vb)))
        }
        (Val::Dbcs(_), _) | (_, Val::Dbcs(_)) if [&va, &vb].into_iter().all(|v| matches!(v, Val::Dbcs(_) | Val::Fig(Figurative::Space) | Val::All(_))) => {
            let len = [&va, &vb].into_iter().map(|v| if let Val::Dbcs(b) = v { b.len() } else { 0 }).max().unwrap_or(0);
            let dbcs = |v: &Val| -> Vec<u8> {
                match v {
                    Val::Dbcs(b) => b.clone(),
                    Val::All(b) if !b.is_empty() => b.iter().copied().cycle().take(len).collect(),
                    _ => Vec::new(),
                }
            };
            // DBCS comparisons ignore the collating sequence (Language Reference SC27-8713-03, pp. 79, 277).
            Ok(ebcdic::compare_alphanumeric(&dbcs(&va), &dbcs(&vb), &ebcdic::Collation::Native))
        }
        _ => {
            let (va, la) = stored_digits(facts, mem, va, la, pos)?;
            let (vb, lb) = stored_digits(facts, mem, vb, lb, pos)?;
            let len = compared_len((&va, la), (&vb, lb));
            let x = all_cut(&va, alnum_image(facts, &va, la, len, pos)?, len);
            let y = all_cut(&vb, alnum_image(facts, &vb, lb, len, pos)?, len);
            Ok(ebcdic::compare_alphanumeric(&x, &y, facts.collation()))
        }
    }
}

/// The bytes a zoned integer holds, as a comparison with a nonnumeric operand reads them: as a MOVE
/// to an alphanumeric item of its size leaves them, so a sign it overpunches is removed under ZWB
/// and kept under NOZWB, and a separate sign is left out (assumption C221); for cobc, a sign byte
/// holding a space is kept and one holding no digit reads as 0. None for an operand the comparison
/// reads as a number first: not zoned, not an integer, or scaled.
pub fn compared_zoned_bytes(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc) -> Option<Vec<u8>> {
    let Kind::Zoned { scale: 0, signed, sign, .. } = loc.kind else { return None };
    if scaling(facts, loc) > 0 {
        return None;
    }
    let mut image = bytes(mem, loc).to_vec();
    let leading = matches!(sign, Some(SignClause { position: SignPosition::Leading, .. }));
    match sign {
        Some(SignClause { separate: true, position: SignPosition::Leading }) => {
            image.remove(0);
        }
        Some(SignClause { separate: true, position: SignPosition::Trailing }) => {
            image.pop();
        }
        _ if !signed => {}
        _ if facts.options().emulates_cobc() => {
            let b = if leading { image.first_mut()? } else { image.last_mut()? };
            *b = cobc_sign_byte(*b).0;
        }
        _ if !facts.options().zwb => {}
        _ if leading => image[0] |= 0xF0,
        _ => *image.last_mut()? |= 0xF0,
    }
    Some(image)
}

/// A zoned item's sign byte as libcob reads it (cob_real_get_sign, libcob/common.c): a digit or a
/// space as it stands, an overpunched digit as the digit, any other character as 0; and whether
/// the sign is negative.
pub fn cobc_sign_byte(b: u8) -> (u8, bool) {
    match b {
        0xF0..=0xF9 | ebcdic::SPACE => (b, false),
        _ if b >> 4 >= 0xA && b & 0x0F <= 9 => (b | 0xF0, matches!(b >> 4, 0xB | 0xD)),
        _ => (0xF0, false),
    }
}

/// `image`, from [`compared_zoned_bytes`], compared as alphanumeric with `other`, whose own bytes
/// are taken the same way when it is a zoned integer too; `zoned_first` is whether the zoned item
/// is the comparison's first operand.
pub fn compare_zoned_bytes(facts: &dyn ProgramFacts, mem: &[u8], image: &[u8], other: (Val, Option<Loc>), zoned_first: bool, pos: Pos) -> R<Ordering> {
    let other = match other {
        (v, None) if zero(&v) => (Val::Fig(Figurative::Zero), None),
        other => other,
    };
    let len = match other.0 {
        Val::All(_) if other.1.is_none() => image.len(),
        _ => image.len().max(image_len(&other.0, other.1)),
    };
    let y = match other.1.and_then(|l| compared_zoned_bytes(facts, mem, l)) {
        Some(bytes) => bytes,
        None => all_cut(&other.0, alnum_image(facts, &other.0, other.1, len, pos)?, len),
    };
    let o = ebcdic::compare_alphanumeric(image, &y, facts.collation());
    Ok(if zoned_first { o } else { o.reverse() })
}

/// Whether a comparand is zero as a constant gives it: ZERO, or a number of value zero, which a
/// comparison by bytes reads as ZERO's zeros (assumption C262).
pub fn zero(v: &Val) -> bool {
    match v {
        Val::Fig(Figurative::Zero) => true,
        Val::Num(f) => f.magnitude == U256::ZERO,
        _ => false,
    }
}

/// A numeric operand compared with a nonnumeric one is its digits, scaling positions ignored
/// (Language Reference SC27-8713-03, p. 211).
pub fn stored_digits(facts: &dyn ProgramFacts, mem: &[u8], v: Val, loc: Option<Loc>, pos: Pos) -> R<(Val, Option<Loc>)> {
    match loc {
        Some(l) if matches!(v, Val::Num(_)) && scaling(facts, l) > 0 => Ok((read_stored(facts, mem, l, pos)?, None)),
        _ => Ok((v, loc)),
    }
}

/// How many characters two operands are compared over as alphanumeric: an ALL literal compared with
/// a data item has the item's length (Language Reference SC27-8713-03, p. 17), and otherwise the
/// shorter operand is padded to the longer's.
fn compared_len(a: (&Val, Option<Loc>), b: (&Val, Option<Loc>)) -> usize {
    match (a, b) {
        ((Val::All(_), None), (other, Some(item))) | ((other, Some(item)), (Val::All(_), None)) => image_len(other, Some(item)),
        _ => image_len(a.0, a.1).max(image_len(b.0, b.1)),
    }
}

/// An ALL literal's image cut to `len` characters.
fn all_cut(v: &Val, mut image: Vec<u8>, len: usize) -> Vec<u8> {
    if matches!(v, Val::All(_)) {
        image.truncate(len);
    }
    image
}

/// How many characters an operand has when compared as alphanumeric.
pub fn image_len(v: &Val, loc: Option<Loc>) -> usize {
    match (v, loc) {
        (_, Some(l)) if !l.kind.is_numeric() => l.len,
        (Val::Bytes(b) | Val::All(b) | Val::Dbcs(b), _) => b.len(),
        (Val::Num(f), Some(l)) => l.kind.digits_scale().map_or(f.places.total(), |(d, _)| d) as usize,
        (Val::Num(f), None) => f.places.total() as usize,
        _ => 1,
    }
}

/// An ALL literal's `units` repeated to `len` bytes, or once where `len` is shorter.
fn repeated(units: &[u8], len: usize) -> Vec<u8> {
    units.iter().copied().cycle().take(len.max(units.len())).collect()
}

/// National values compared unit by unit, the shorter padded with national spaces.
pub fn compare_national(a: &[u8], b: &[u8]) -> Ordering {
    let unit = |s: &[u8], i: usize| if i + 1 < s.len() { u16::from_be_bytes([s[i], s[i + 1]]) } else { 0x0020 };
    let len = a.len().max(b.len());
    (0..len).step_by(2).map(|i| unit(a, i).cmp(&unit(b, i))).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use numeric::options::Invdata;

    struct Facts {
        options: Options,
        scaling: u32,
        collation: Collation,
    }

    impl ProgramFacts for Facts {
        fn options(&self) -> Options {
            self.options
        }
        fn page(&self) -> &'static CodePage {
            self.options.code_page()
        }
        fn figurative(&self, _: Figurative) -> u8 {
            0
        }
        fn collation(&self) -> &Collation {
            &self.collation
        }
        fn ordinal(&self, byte: u8) -> u16 {
            u16::from(byte) + 1
        }
        fn character(&self, _: i64) -> Option<u8> {
            None
        }
        fn characters(&self) -> usize {
            256
        }
        fn decimal_point(&self) -> char {
            '.'
        }
        fn edit(&self, _: u32) -> (&[Sym], &str) {
            (&[], "")
        }
        fn scaling(&self, _: usize) -> u32 {
            self.scaling
        }
        fn item_name(&self, _: usize) -> String {
            String::new()
        }
    }

    /// Every numeric kind with an integer or scaled PICTURE, and the bytes each takes.
    fn kinds() -> Vec<(Kind, usize)> {
        let mut kinds = vec![(Kind::Index, 4)];
        for signed in [false, true] {
            for scale in [0, 2] {
                for digits in [1, 4, 5, 9, 10, 18] {
                    for native in [Native::No, Native::Comp5] {
                        kinds.push((Kind::Binary { digits, scale, signed, native }, Binary { digits: digits as u8, signed, native }.bytes()));
                    }
                }
                kinds.push((Kind::Binary { digits: 3, scale, signed, native: Native::BinaryChar }, 1));
                for digits in [1, 2, 7, 18, 19, 31] {
                    kinds.push((Kind::Packed { digits, scale, signed }, digits as usize / 2 + 1));
                    for sign in [None, Some((SignPosition::Leading, false)), Some((SignPosition::Leading, true)), Some((SignPosition::Trailing, true))] {
                        let sign = sign.map(|(position, separate)| SignClause { position, separate });
                        kinds.push((Kind::Zoned { digits, scale, signed, sign }, digits as usize + usize::from(sign.is_some_and(|s| s.separate))));
                    }
                }
            }
        }
        kinds
    }

    #[test]
    fn an_integer_reads_as_its_value_is_read() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8
        };
        let invdata = [None, Some(Invdata { forcenumcmp: false, cleansign: true }), Some(Invdata { forcenumcmp: false, cleansign: false })];
        let mut fast = 0;
        for (kind, len) in kinds() {
            for _ in 0..200 {
                let mem: Vec<u8> = (0..len)
                    .map(|_| match next() % 8 {
                        0 => next(),
                        1 => 0xC0 | (next() % 10),
                        2 => 0xD0 | (next() % 10),
                        3 => 0x4E,
                        4 => 0x60,
                        5 => ((next() % 10) << 4) | 0x0C,
                        _ => 0xF0 | (next() % 10),
                    })
                    .collect();
                let loc = Loc { offset: 0, len, kind, item: 0 };
                for numproc in [Numproc::Nopfd, Numproc::Pfd] {
                    for invdata in invdata {
                        for scaling in [0, 2] {
                            let facts = Facts { options: Options { numproc, invdata, ..Options::default() }, scaling, collation: Collation::Native };
                            let Some(n) = read_integer(&facts, &mem, loc) else { continue };
                            fast += 1;
                            let Ok(Val::Num(v)) = read(&facts, &mem, loc, Pos::default()) else { panic!("{kind:?} {mem:02X?} reads as no number") };
                            let whole = align(&v, 0, false).and_then(|m| m.to_u128()).and_then(|m| i64::try_from(m).ok()).map(|m| if v.negative { -m } else { m });
                            assert_eq!(Some(n), whole, "{kind:?} {mem:02X?}");
                            assert_eq!(v.places.dec, 0, "{kind:?}");
                        }
                    }
                }
            }
        }
        assert!(fast > 20_000, "only {fast} reads took the integer path");
    }

    #[test]
    fn digits_read_as_the_value_is_read() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            (state.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as u8
        };
        let invdata = [None, Some(Invdata { forcenumcmp: false, cleansign: true }), Some(Invdata { forcenumcmp: false, cleansign: false })];
        let mut fast = 0;
        for (kind, len) in kinds() {
            for _ in 0..200 {
                let mem: Vec<u8> = (0..len)
                    .map(|_| match next() % 8 {
                        0 => next(),
                        1 => 0xC0 | (next() % 10),
                        2 => 0xD0 | (next() % 10),
                        3 => 0x4E,
                        4 => 0x60,
                        5 => ((next() % 10) << 4) | 0x0C,
                        _ => 0xF0 | (next() % 10),
                    })
                    .collect();
                let loc = Loc { offset: 0, len, kind, item: 0 };
                for numproc in [Numproc::Nopfd, Numproc::Pfd] {
                    for invdata in invdata {
                        for scaling in [0, 2] {
                            let facts = Facts { options: Options { numproc, invdata, ..Options::default() }, scaling, collation: Collation::Native };
                            let Some(n) = read_digits(&facts, &mem, loc) else { continue };
                            fast += 1;
                            let Ok(Val::Num(v)) = read(&facts, &mem, loc, Pos::default()) else { panic!("{kind:?} {mem:02X?} reads as no number") };
                            assert_eq!(Fixed::new(i128::from(n), places_of(kind)), v, "{kind:?} {mem:02X?}");
                        }
                    }
                }
            }
        }
        assert!(fast > 40_000, "only {fast} reads took the digits path");
    }

    #[test]
    fn a_count_stores_as_any_fixed_value_stores() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let mut fast = 0;
        for (kind, len) in kinds() {
            let loc = Loc { offset: 0, len, kind, item: 0 };
            for _ in 0..150 {
                let bits = next() % 64;
                let n = (next() >> (63 - bits)) as i64;
                let n = if next() % 2 == 0 { n.wrapping_neg() } else { n };
                let places = Places::new(1 + (next() % 20) as u32, (next() % 10) as u32);
                let rounded = next() % 2 == 0;
                for trunc in [Trunc::Std, Trunc::Bin, Trunc::Opt] {
                    for scaling in [0, 1] {
                        let facts = Facts { options: Options { trunc, ..Options::default() }, scaling, collation: Collation::Native };
                        let mut out = [0xEE; MAX_DIGITS + 1];
                        let Some(image) = count_image(&facts, loc, (n, places), rounded, &mut out, Pos::default()) else { continue };
                        fast += 1;
                        let value = Fixed::new(i128::from(n), places);
                        let mut report = Vec::new();
                        let general = image_of(&facts, &mut report, loc, &value, rounded, Pos::default());
                        assert!(report.is_empty(), "{value:?} into {kind:?} under {trunc:?} reports");
                        match (image, general) {
                            (Ok((len, size_error)), Ok((general, general_size_error))) => {
                                assert_eq!((&out[..len], size_error), (general.bytes(), general_size_error), "{value:?} into {kind:?} under {trunc:?}, rounded {rounded}");
                            }
                            (Err(a), Err(b)) => assert_eq!(a, b),
                            (a, b) => panic!("{value:?} into {kind:?}: {a:?} where image_of gives {:?}", b.map(|(i, e)| (i.bytes().to_vec(), e))),
                        }
                    }
                }
            }
        }
        assert!(fast > 60_000, "only {fast} stores took the count path");
    }

    #[test]
    fn an_integer_stores_as_any_fixed_value_stores() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        };
        let mut kinds = Vec::new();
        for signed in [false, true] {
            for digits in [1, 4, 5, 9, 10, 18] {
                for native in [Native::No, Native::Comp5] {
                    kinds.push(Kind::Binary { digits, scale: 0, signed, native });
                }
            }
            kinds.push(Kind::Binary { digits: 3, scale: 0, signed, native: Native::BinaryChar });
            for digits in [1, 6, 15, 18, 19, 31] {
                for sign in [None, Some((SignPosition::Trailing, false)), Some((SignPosition::Leading, false)), Some((SignPosition::Leading, true)), Some((SignPosition::Trailing, true))] {
                    kinds.push(Kind::Zoned { digits, scale: 0, signed, sign: sign.map(|(position, separate)| SignClause { position, separate }) });
                }
            }
        }
        let mut fast = 0;
        for kind in kinds {
            for _ in 0..300 {
                let bits = next() % 64;
                let magnitude = u128::from(next() >> (63 - bits));
                let value = fixed(next() % 2 == 0, U256::from_u128(magnitude), Places::new(1 + (next() % 30) as u32, 0));
                let loc = Loc { offset: 0, len: 0, kind, item: 0 };
                for trunc in [Trunc::Std, Trunc::Bin, Trunc::Opt] {
                    for scaling in [0, 1] {
                        let options = Options { trunc, ..Options::default() };
                        let facts = Facts { options, scaling, collation: Collation::Native };
                        let Some((image, size_error)) = integer_image(&facts, loc, &value) else { continue };
                        fast += 1;
                        let mut report = Vec::new();
                        let (general, general_size_error) = image_of(&facts, &mut report, loc, &value, next() % 2 == 0, Pos::default()).unwrap();
                        assert_eq!((image.bytes(), size_error), (general.bytes(), general_size_error), "{value:?} into {kind:?} under {trunc:?}");
                        assert!(report.is_empty(), "{value:?} into {kind:?} under {trunc:?} reports");
                    }
                }
            }
        }
        assert!(fast > 30_000, "only {fast} stores took the integer path");
    }
}
