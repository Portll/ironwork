//! Storage and MOVE: an item's bytes and value by its `Loc`, numeric stores with their size-error
//! and TRUNC(OPT) rules, MOVE, and the comparisons and class tests of conditions.

use crate::abend::{Abend, AbendCode};
use crate::codec;
use crate::edit;
use crate::fixed::{MAX_DIGITS, align, compare_fixed, fixed, places_of, pow10, scaled_down, scaled_up, zoned_digits};
use crate::lir::{ByteClass, SenderCheck, SignTest};
use crate::picture::Sym;
use crate::storage::{Kind, Loc, Val};
use crate::unit::{Loader, RunUnit};
use crate::vocab::{Figurative, Pos, SignClause, SignPosition};
use numeric::binary::{self, Binary};
use numeric::precision::{Fixed, Places};
use numeric::{Numproc, Options, Quote, Trunc, float, sign};
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

/// An item's value as its digits hold it, before any scaling positions to their right.
pub fn read_stored(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos) -> R<Val> {
    let cleaned = facts.options().invdata.is_some_and(|i| i.cleansign).then(|| sign_cleaned(bytes(mem, loc), loc.kind)).flatten();
    let bytes = cleaned.as_deref().unwrap_or(bytes(mem, loc));
    let places = places_of(loc.kind);
    Ok(match loc.kind {
        Kind::Group | Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => Val::Bytes(bytes.to_vec()),
        Kind::National => Val::National(bytes.to_vec()),
        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => Val::Address(u32::from_be_bytes(bytes.try_into().unwrap())),
        Kind::Index => Val::Num(Fixed::new(i32::from_be_bytes(bytes.try_into().unwrap()) as i128, Places::new(9, 0))),
        Kind::Float(p) => Val::Float(Hfp::from_bytes(p, bytes)),
        Kind::Binary { digits, signed, native, .. } => Val::Num(Fixed::new(Binary { digits: digits as u8, signed, native }.load(bytes), places)),
        Kind::Packed { signed, .. } => {
            let d = codec::packed(bytes, signed, facts.options().numproc).map_err(|c| Abend::check(c, pos))?;
            Val::Num(fixed(d.negative, U256::from_u128(d.magnitude), places))
        }
        Kind::Zoned { signed, sign, .. } => Val::Num(zoned_value(facts.options().numproc, bytes, signed, sign, places, pos)?),
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
        Val::Bytes(b) | Val::All(b) | Val::National(b) | Val::AllNational(b) => b,
        Val::Fig(f) => vec![facts.figurative(f)],
        Val::Num(f) => zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED),
        _ => return Err(Abend::ironwork("this operand has no characters to work on", pos)),
    })
}

pub fn set_integer<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, value: i64, pos: Pos) -> R<()> {
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
    let beyond = || Abend::ironwork("a value wider than 256 bits", pos);
    let value = &scaled_down(*value, scaling(facts, loc));
    let options = facts.options();
    let (bytes, size_error) = match loc.kind {
        Kind::Zoned { digits, scale, signed, sign } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let negative = signed && value.negative && kept != 0;
            (zoned_image(kept, digits, signed, negative, sign), m >= cap)
        }
        Kind::Packed { digits, scale, signed } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let mut out = vec![0u8; loc.len];
            decimal::encode(&mut out, Decimal { negative: signed && value.negative && kept != 0, magnitude: kept }).map_err(|c| Abend::check(c, pos))?;
            if !signed {
                *out.last_mut().unwrap() |= 0x0F;
            }
            (out, m >= cap)
        }
        Kind::Binary { digits, scale, signed, native } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let magnitude = m.to_u128().and_then(|m| i128::try_from(m).ok()).ok_or_else(beyond)?;
            let v = if value.negative { -magnitude } else { magnitude };
            let item = Binary { digits: digits as u8, signed, native };
            let stored = binary::store(item, v, &options);
            if let Some(d) = stored.divergence {
                let name = facts.item_name(loc.item);
                let _ = writeln!(
                    unit.err,
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
            let exceeds = if native || options.trunc == Trunc::Bin { !binary_range } else { v.unsigned_abs() >= 10u128.pow(digits) };
            (stored.bytes, exceeds)
        }
        Kind::Float(p) => (float::from_fixed(*value, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?.to_bytes(), false),
        Kind::Index => {
            let whole = align(value, 0, false).and_then(|m| m.to_u128()).and_then(|m| i32::try_from(m).ok()).ok_or_else(beyond)?;
            ((if value.negative { -whole } else { whole }).to_be_bytes().to_vec(), false)
        }
        Kind::NumericEdited { edit, digits, scale, blank_when_zero } => {
            let m = align(value, scale, rounded).ok_or_else(beyond)?;
            let cap = pow10(digits);
            let kept = m.div_rem(cap).1.to_u128().unwrap();
            let (syms, currency) = facts.edit(edit);
            let text = edit::numeric(syms, digits, value.negative && kept != 0, kept, blank_when_zero, facts.decimal_point(), currency);
            (facts.page().encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?, m >= cap)
        }
        _ => return Err(Abend::ironwork("a numeric value stored into a non-numeric item", pos)),
    };
    if size_error && keep_on_size_error {
        return Ok(true);
    }
    unit.write(loc.offset, &bytes);
    Ok(size_error)
}

/// A zoned item's bytes for a magnitude, with its sign as the SIGN clause places it.
pub fn zoned_image(magnitude: u128, digits: u32, signed: bool, negative: bool, sign: Option<SignClause>) -> Vec<u8> {
    let zone = match (signed, negative) {
        (false, _) => decimal::UNSIGNED,
        (true, true) => decimal::MINUS,
        (true, false) => decimal::PLUS,
    };
    match sign {
        Some(SignClause { separate: true, position }) => {
            let body = zoned_digits(magnitude, digits as usize, decimal::UNSIGNED);
            let s = if negative { 0x60 } else { 0x4E };
            if position == SignPosition::Leading { [&[s][..], &body].concat() } else { [&body[..], &[s]].concat() }
        }
        Some(SignClause { separate: false, position: SignPosition::Leading }) => {
            let mut body = zoned_digits(magnitude, digits as usize, decimal::UNSIGNED);
            body[0] = (zone << 4) | (body[0] & 0x0F);
            body
        }
        _ => zoned_digits(magnitude, digits as usize, zone),
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
        && s.kind == Kind::Group
        && matches!(dest.kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. })
    {
        // A group move converts nothing (Language Reference SC27-8713-03, p. 410).
        let mut out = vec![ebcdic::SPACE; dest.len];
        let n = s.len.min(dest.len);
        out[..n].copy_from_slice(&bytes(&unit.mem, s)[..n]);
        unit.write(dest.offset, &out);
        return Ok(());
    }
    let page = facts.page();
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
                Val::Bytes(b) => page.decode(&b).encode_utf16().collect(),
                Val::Fig(f) => vec![figurative_unit(f, facts.options().quote); dest.len / 2],
                _ => return Err(Abend::ironwork("this value cannot be moved to a national item", pos)),
            };
            let mut out: Vec<u8> = units.iter().take(dest.len / 2).flat_map(|u| u.to_be_bytes()).collect();
            while out.len() < dest.len {
                out.extend_from_slice(&0x0020u16.to_be_bytes());
            }
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
            Val::Bytes(b) if integer_digits(facts, dest).is_some() && !matches!(src.map(|s| s.kind), Some(Kind::NumericEdited { .. })) => {
                move_digit_halves(facts, unit, dest, &b, pos)?;
            }
            Val::Bytes(b) => {
                let v = match src.map(|s| (s, s.kind)) {
                    Some((s, Kind::NumericEdited { edit, .. })) => {
                        let (syms, currency) = facts.edit(edit);
                        let (negative, magnitude) = edit::de_edit(syms, &page.decode(&b), currency);
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
    match read(facts, mem, src, pos) {
        Err(Abend { code: AbendCode::Check(ProgramCheck::Data), .. }) if moved_unchecked(facts, src, dest) => {
            let stored = bytes(mem, src);
            let cleaned = facts.options().invdata.is_some_and(|i| i.cleansign).then(|| sign_cleaned(stored, src.kind)).flatten();
            Ok(Val::Bytes(cleaned.unwrap_or_else(|| stored.to_vec())))
        }
        value => value,
    }
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
/// read as a number (C260).
fn carry_digits<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, src: Loc, sender: &[u8], pos: Pos) -> R<()> {
    if packed_copy(facts, src, dest) {
        unit.write(dest.offset, sender);
        return Ok(());
    }
    let (halves, sign) = digit_halves(src.kind, sender);
    let power = |loc: Loc| i64::from(scaling(facts, loc)) - i64::from(places_of(loc.kind).dec);
    let (m, n) = (halves.len() as i64, dest.kind.digits_scale().map_or(0, |(d, _)| d) as i64);
    let shift = m - n + power(src) - power(dest);
    let aligned: Vec<u8> = (0..n).map(|j| usize::try_from(j + shift).ok().and_then(|i| halves.get(i)).copied().unwrap_or(0)).collect();
    store_digit_halves(facts, unit, dest, &aligned, sign, pos)
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
        Kind::Binary { digits, signed, .. } => {
            let raw = Binary { digits: digits as u8, signed, native: true }.load(b);
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
        Kind::Binary { native: false, .. } => check.bin.is_some_and(|c| c.truncbin || options.trunc != Trunc::Bin),
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
    let message = format!("NUMCHECK: {} X'{}' in program {program} {why}", facts.item_name(loc.item), crate::digest::hex(bytes(&unit.mem, loc)).to_ascii_uppercase());
    if facts.options().numcheck.is_some_and(|c| c.abd) {
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
    }
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
        _ => {
            let (va, la) = stored_digits(facts, mem, va, la, pos)?;
            let (vb, lb) = stored_digits(facts, mem, vb, lb, pos)?;
            let len = image_len(&va, la).max(image_len(&vb, lb));
            let x = alnum_image(facts, &va, la, len, pos)?;
            let y = alnum_image(facts, &vb, lb, len, pos)?;
            Ok(ebcdic::compare_alphanumeric(&x, &y, facts.collation()))
        }
    }
}

/// The bytes a zoned integer holds, as a comparison with a nonnumeric operand reads them: as a MOVE
/// to an alphanumeric item of its size leaves them, so a sign it overpunches is removed under ZWB
/// and kept under NOZWB, and a separate sign is left out (assumption C221). None for an operand the
/// comparison reads as a number first: not zoned, not an integer, or scaled.
pub fn compared_zoned_bytes(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc) -> Option<Vec<u8>> {
    let Kind::Zoned { scale: 0, signed, sign, .. } = loc.kind else { return None };
    if scaling(facts, loc) > 0 {
        return None;
    }
    let mut image = bytes(mem, loc).to_vec();
    match sign {
        Some(SignClause { separate: true, position: SignPosition::Leading }) => {
            image.remove(0);
        }
        Some(SignClause { separate: true, position: SignPosition::Trailing }) => {
            image.pop();
        }
        _ if !signed || !facts.options().zwb => {}
        Some(SignClause { position: SignPosition::Leading, .. }) => image[0] |= 0xF0,
        _ => *image.last_mut()? |= 0xF0,
    }
    Some(image)
}

/// `image`, from [`compared_zoned_bytes`], compared as alphanumeric with `other`, whose own bytes
/// are taken the same way when it is a zoned integer too; `zoned_first` is whether the zoned item
/// is the comparison's first operand.
pub fn compare_zoned_bytes(facts: &dyn ProgramFacts, mem: &[u8], image: &[u8], other: (Val, Option<Loc>), zoned_first: bool, pos: Pos) -> R<Ordering> {
    let other = match other {
        (v, None) if zero(&v) => (Val::Fig(Figurative::Zero), None),
        other => other,
    };
    let len = image.len().max(image_len(&other.0, other.1));
    let y = match other.1.and_then(|l| compared_zoned_bytes(facts, mem, l)) {
        Some(bytes) => bytes,
        None => alnum_image(facts, &other.0, other.1, len, pos)?,
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

/// How many characters an operand has when compared as alphanumeric.
pub fn image_len(v: &Val, loc: Option<Loc>) -> usize {
    match (v, loc) {
        (_, Some(l)) if !l.kind.is_numeric() => l.len,
        (Val::Bytes(b) | Val::All(b), _) => b.len(),
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
