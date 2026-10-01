//! Storage and MOVE: an item's bytes and value by its `Loc`, numeric stores with their size-error
//! and TRUNC(OPT) rules, MOVE, and the comparisons and class tests of conditions.

use crate::abend::Abend;
use crate::codec;
use crate::edit;
use crate::fixed::{MAX_DIGITS, align, compare_fixed, fixed, places_of, pow10, scaled_down, scaled_up, zoned_digits};
use crate::lir::{ByteClass, SignTest};
use crate::picture::Sym;
use crate::storage::{Kind, Loc, Val};
use crate::unit::{Loader, RunUnit};
use crate::vocab::{Figurative, Pos, SignClause, SignPosition};
use numeric::binary::{self, Binary};
use numeric::precision::{Fixed, Places};
use numeric::{Numproc, Options, Quote, Trunc, float, sign};
use std::cmp::Ordering;
use zarch::check::ProgramMask;
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
}

pub fn bytes(mem: &[u8], loc: Loc) -> &[u8] {
    &mem[loc.offset..loc.offset + loc.len]
}

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
        Val::Bytes(b) | Val::All(b) | Val::National(b) => b,
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
            write(&mut unit.mem, loc, &h.to_bytes());
            Ok(false)
        }
        (Kind::Float(p), Val::Num(f)) => {
            let h = float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?;
            write(&mut unit.mem, loc, &h.to_bytes());
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
    write(&mut unit.mem, loc, &bytes);
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
        write(&mut unit.mem, dest, &out);
        return Ok(());
    }
    let page = facts.page();
    match dest.kind {
        Kind::Group | Kind::Alnum { .. } => {
            let justified = matches!(dest.kind, Kind::Alnum { justified: true });
            let image = match (dest.kind, src) {
                (Kind::Group, Some(s)) if matches!(val, Val::Num(_) | Val::Float(_) | Val::Address(_)) => bytes(&unit.mem, s).to_vec(),
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
            write(&mut unit.mem, dest, &out);
        }
        Kind::National => {
            let units: Vec<u16> = match val {
                Val::National(b) => b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect(),
                Val::Bytes(b) => page.decode(&b).encode_utf16().collect(),
                Val::Fig(f) => vec![figurative_unit(f, facts.options().quote); dest.len / 2],
                _ => return Err(Abend::ironwork("this value cannot be moved to a national item", pos)),
            };
            let mut out: Vec<u8> = units.iter().take(dest.len / 2).flat_map(|u| u.to_be_bytes()).collect();
            while out.len() < dest.len {
                out.extend_from_slice(&0x0020u16.to_be_bytes());
            }
            write(&mut unit.mem, dest, &out);
        }
        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => match val {
            Val::Address(a) => write(&mut unit.mem, dest, &a.to_be_bytes()),
            Val::Fig(Figurative::Null) => write(&mut unit.mem, dest, &[0; 4]),
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
            write(&mut unit.mem, dest, &out);
        }
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } => match val {
            Val::Num(f) => {
                if let (Some(s), Kind::Packed { digits, scale, signed: true }, Numproc::Pfd) = (src, dest.kind, facts.options().numproc)
                    && s.kind == dest.kind
                    && scaling(facts, s) == scaling(facts, dest)
                    && digits > 0
                    && scale == places_of(s.kind).dec
                {
                    let copied = sign::move_packed(bytes(&unit.mem, s), true, Numproc::Pfd);
                    write(&mut unit.mem, dest, &copied);
                } else {
                    store_fixed(facts, unit, dest, &f, false, pos)?;
                }
            }
            Val::Float(h) => {
                let (f, _) = float::to_receiver(h, places(facts, dest));
                store_fixed(facts, unit, dest, &f, false, pos)?;
            }
            Val::Fig(Figurative::Zero) => store_fixed(facts, unit, dest, &Fixed::new(0, Places::new(1, 0)), false, pos)?,
            Val::Fig(f) => write(&mut unit.mem, dest, &vec![facts.figurative(f); dest.len]),
            Val::All(b) => {
                let fill: Vec<u8> = b.iter().copied().cycle().take(dest.len).collect();
                write(&mut unit.mem, dest, &fill);
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
            Val::National(_) => return Err(Abend::ironwork("a national value cannot be moved to a numeric item", pos)),
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
            write(&mut unit.mem, dest, &h.to_bytes());
        }
    }
    Ok(())
}

/// The bytes an alphanumeric receiver of `len` gets from `val`.
pub fn alnum_image(facts: &dyn ProgramFacts, val: &Val, src: Option<Loc>, len: usize, pos: Pos) -> R<Vec<u8>> {
    Ok(match val {
        Val::Bytes(b) => b.clone(),
        Val::All(b) => b.iter().copied().cycle().take(len.max(b.len())).collect(),
        Val::Fig(f) => vec![facts.figurative(*f); len],
        Val::Num(f) if f.places.dec == 0 => {
            let digits = src.and_then(|s| s.kind.digits_scale().map(|(d, _)| d + scaling(facts, s))).unwrap_or(f.places.total());
            zoned_digits(f.magnitude.to_u128().unwrap_or(0), digits as usize, decimal::UNSIGNED)
        }
        Val::National(_) => return Err(Abend::ironwork("a national value cannot be moved to an alphanumeric item", pos)),
        _ => return Err(Abend::ironwork("only an integer numeric value can be moved to an alphanumeric item", pos)),
    })
}

/// NUMERIC or ALPHABETIC, tested on an item's bytes.
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
    let len = image.len().max(image_len(&other.0, other.1));
    let y = match other.1.and_then(|l| compared_zoned_bytes(facts, mem, l)) {
        Some(bytes) => bytes,
        None => alnum_image(facts, &other.0, other.1, len, pos)?,
    };
    let o = ebcdic::compare_alphanumeric(image, &y, facts.collation());
    Ok(if zoned_first { o } else { o.reverse() })
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

/// National values compared unit by unit, the shorter padded with national spaces.
pub fn compare_national(a: &[u8], b: &[u8]) -> Ordering {
    let unit = |s: &[u8], i: usize| if i + 1 < s.len() { u16::from_be_bytes([s[i], s[i + 1]]) } else { 0x0020 };
    let len = a.len().max(b.len());
    (0..len).step_by(2).map(|i| unit(a, i).cmp(&unit(b, i))).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
}
