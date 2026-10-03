//! STRING, UNSTRING and INSPECT (lir.md §9.1) over the executor's handles, in the walker's order of
//! locates, reads and stores. The inputs are `lir::text`'s plans with each receiver's MOVE, store
//! and step plan left to its `Loc`'s kind, which `store` decides by.

use crate::abend::Abend;
use crate::host::{self, Host, Values};
use crate::lir::{Bound, Chars, ConvertTable, Converting, Replacement, StringSource};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::strings::{self, Phrase};
use crate::vocab::{Figurative, InspectMode, Pos};
use numeric::precision::{Fixed, Places};

type R<T> = Result<T, Abend>;

/// `lir::UnstringInto`: the receiver, DELIMITER IN and COUNT IN.
#[derive(Clone, Copy, Debug)]
pub struct UnstringField<P> {
    pub target: P,
    pub delimiter: Option<P>,
    pub count: Option<P>,
}

/// `lir::InspectPhrase`, TALLYING's counter a place.
#[derive(Clone, Debug)]
pub struct InspectPhrase<P, O> {
    pub mode: InspectMode,
    pub pattern: Option<Chars<P, O>>,
    pub by: Option<Replacement<P, O>>,
    pub counter: Option<P>,
    pub bounds: Vec<Bound<P, O>>,
}

/// What a character position is: a byte, a national character (a UTF-16 unit) or a DBCS
/// character, two bytes each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Units {
    Bytes,
    National,
    Dbcs,
}

impl Units {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::National => Self::National,
            Kind::Dbcs { .. } => Self::Dbcs,
            _ => Self::Bytes,
        }
    }

    fn of_value(val: &Val) -> Self {
        match val {
            Val::National(_) => Self::National,
            Val::Dbcs(_) => Self::Dbcs,
            _ => Self::Bytes,
        }
    }

    fn size(self) -> usize {
        if self == Self::Bytes { 1 } else { 2 }
    }

    /// Characters of these units as the value a receiver takes.
    fn value(self, bytes: Vec<u8>) -> Val {
        match self {
            Self::Bytes => Val::Bytes(bytes),
            Self::National => Val::National(bytes),
            Self::Dbcs => Val::Dbcs(bytes),
        }
    }
}

/// An operand's characters: an item's storage, or a value as `store::natural_bytes` gives it.
pub fn chars<P: Copy, O>(x: &mut impl Values<P, O>, c: &Chars<P, O>, pos: Pos) -> R<Vec<u8>> {
    match c {
        Chars::Literal(bytes) => Ok(bytes.clone()),
        Chars::Place(p) => {
            let loc = x.locate(*p, false)?;
            Ok(store::bytes(x.mem(), loc).to_vec())
        }
        Chars::Value(o) => {
            let val = x.value(o, pos)?;
            store::natural_bytes(&x.facts(), val, pos)
        }
    }
}

/// STRING: returns whether it overflowed. The pointer is read before the first source and stored
/// after the last, and each source is read after the ones before it are stored. The pointer counts
/// the receiver's character positions, two bytes each when it is national or DBCS, and each
/// sender gives characters of the receiver's usage (Language Reference SC27-8713-03, pp. 459-460).
pub fn string<P: Copy, O>(x: &mut impl Values<P, O>, into: P, pointer: Option<P>, sources: &[StringSource<P, O>], pos: Pos) -> R<bool> {
    let dest = x.locate(into, true)?;
    let units = Units::of(dest.kind);
    let unit = units.size();
    let mut at = match pointer {
        Some(p) => x.integer(p, pos)?,
        None => 1,
    };
    let len = (dest.len / unit) as i64;
    let mut overflow = at < 1 || at > len;
    if !overflow {
        'sources: for source in sources {
            let bytes = chars_in(x, &source.chars, units, pos)?;
            let delimiter = match &source.delimiter {
                None => None,
                Some(d) => Some(chars_in(x, d, units, pos)?),
            };
            for character in strings::delimited(&bytes, delimiter.as_deref(), unit).chunks(unit) {
                if at > len {
                    overflow = true;
                    break 'sources;
                }
                let offset = dest.offset + (at as usize - 1) * unit;
                x.mem()[offset..offset + character.len()].copy_from_slice(character);
                host::mark(x, offset, character.len());
                at += 1;
            }
        }
    }
    if let Some(p) = pointer {
        host::set_integer(x, p, at, pos)?;
    }
    Ok(overflow)
}

/// UNSTRING: returns whether it overflowed. `delimiters` are the DELIMITED BY operands, each true
/// for ALL; `tallying` is the TALLYING IN item.
pub fn unstring<P: Copy, O>(
    x: &mut impl Values<P, O>,
    source: P,
    pointer: Option<P>,
    delimiters: &[(bool, Chars<P, O>)],
    into: &[UnstringField<P>],
    tallying: Option<P>,
    pos: Pos,
) -> R<bool> {
    let source_loc = x.locate(source, false)?;
    let source = store::bytes(x.mem(), source_loc).to_vec();
    let units = Units::of(source_loc.kind);
    let unit = units.size();
    let len = (source.len() / unit) as i64;
    let mut at = match pointer {
        Some(p) => x.integer(p, pos)?,
        None => 1,
    };
    let mut found = Vec::with_capacity(delimiters.len());
    for (all, d) in delimiters {
        found.push((*all, chars_in(x, d, units, pos)?));
    }
    let delimiters = found;
    let mut overflow = at < 1 || at > len;
    let mut fields = 0i64;
    if !overflow {
        for field in into {
            if at > len {
                break;
            }
            let start = (at as usize - 1) * unit;
            let dest = x.locate(field.target, true)?;
            let (end, matched) = if delimiters.is_empty() {
                ((start + dest.len).min(source.len()), None)
            } else {
                match strings::next_delimiter(&source, start, &delimiters, unit) {
                    Some((at, k)) => (at, Some(k)),
                    None => (source.len(), None),
                }
            };
            x.assign(dest, units.value(source[start..end].to_vec()), None, pos)?;
            let delimiter = matched.map(|k| delimiters[k].1.clone());
            if let Some(p) = field.delimiter {
                let d = x.locate(p, true)?;
                x.assign(d, delimiter.clone().map_or(Val::Fig(Figurative::Space), |d| units.value(d)), None, pos)?;
            }
            if let Some(p) = field.count {
                host::set_integer(x, p, ((end - start) / unit) as i64, pos)?;
            }
            let next = match matched {
                Some(k) => strings::past_delimiter(&source, end, &delimiters[k].1, delimiters[k].0),
                None => end,
            };
            at = (next / unit) as i64 + 1;
            fields += 1;
        }
        overflow = at <= len;
    }
    if let Some(p) = pointer {
        host::set_integer(x, p, at, pos)?;
    }
    if let Some(p) = tallying {
        let dest = x.locate(p, false)?;
        add_count(x, dest, fields, "TALLYING IN needs a numeric item", pos)?;
    }
    Ok(overflow)
}

/// INSPECT: TALLYING counts over the item as it is, then REPLACING and CONVERTING change it. A
/// national or DBCS item's character positions are two bytes (assumption C230).
pub fn inspect<P: Copy, O>(
    x: &mut impl Values<P, O>,
    target: P,
    tallying: &[InspectPhrase<P, O>],
    replacing: &[InspectPhrase<P, O>],
    converting: Option<&Converting<P, O>>,
    pos: Pos,
) -> R<()> {
    let loc = x.locate(target, false)?;
    let units = Units::of(loc.kind);
    let unit = units.size();
    let mut data = store::bytes(x.mem(), loc).to_vec();
    count(x, &mut data, units, tallying, pos)?;
    let mut changes = phrases(x, &data, units, replacing, pos)?;
    if let Some(c) = converting {
        let pairs = match &c.table {
            ConvertTable::Built(pairs) if units == Units::Bytes => pairs.iter().map(|&(f, t)| (vec![f], vec![t])).collect(),
            ConvertTable::Built(_) => return Err(Abend::ironwork("a CONVERTING table of single bytes cannot convert national or DBCS characters", pos)),
            ConvertTable::Operands { from, to } => {
                let (from, to) = (chars_in(x, from, units, pos)?, chars_in(x, to, units, pos)?);
                if from.len() != to.len() {
                    return Err(Abend::ironwork("CONVERTING needs operands of the same length", pos));
                }
                let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
                for (f, t) in from.chunks(unit).zip(to.chunks(unit)) {
                    if !pairs.iter().any(|(seen, _)| seen == f) {
                        pairs.push((f.to_vec(), t.to_vec()));
                    }
                }
                pairs
            }
        };
        let (start, end) = region(x, &data, units, &c.bounds, pos)?;
        for (pattern, by) in pairs {
            changes.push(Phrase { mode: InspectMode::All, pattern, by: Some(by), start, end });
        }
    }
    strings::inspect(&mut data, unit, &changes);
    host::write(x, loc, &data);
    Ok(())
}

/// INSPECT TALLYING of a function's value, evaluated once before the phrases' operands. A national
/// value's character positions are two bytes, and a figurative constant is one national character
/// (assumption C191).
pub fn tally<P: Copy, O>(x: &mut impl Values<P, O>, subject: &O, tallying: &[InspectPhrase<P, O>], pos: Pos) -> R<()> {
    let val = x.value(subject, pos)?;
    let units = Units::of_value(&val);
    let mut data = store::natural_bytes(&x.facts(), val, pos)?;
    count(x, &mut data, units, tallying, pos)
}

fn count<P: Copy, O>(x: &mut impl Values<P, O>, data: &mut [u8], units: Units, tallying: &[InspectPhrase<P, O>], pos: Pos) -> R<()> {
    let tallied = phrases(x, data, units, tallying, pos)?;
    let counts = strings::inspect(data, units.size(), &tallied);
    for (phrase, count) in tallying.iter().zip(counts) {
        let Some(counter) = phrase.counter else { continue };
        let dest = x.locate(counter, false)?;
        add_count(x, dest, count, "a TALLYING counter must be numeric", pos)?;
    }
    Ok(())
}

/// `chars`, where a value is characters of `units`.
fn chars_in<P: Copy, O>(x: &mut impl Values<P, O>, c: &Chars<P, O>, units: Units, pos: Pos) -> R<Vec<u8>> {
    match c {
        Chars::Value(o) if units != Units::Bytes => {
            let val = x.value(o, pos)?;
            in_units(x, val, units, pos)
        }
        c => chars(x, c, pos),
    }
}

/// A value as characters of `units`: national as [`national`] makes it, DBCS as its bytes with a
/// figurative constant one DBCS character of two of its bytes, SPACE's the DBCS space.
fn in_units<P: Copy, O>(x: &impl Values<P, O>, val: Val, units: Units, pos: Pos) -> R<Vec<u8>> {
    match (units, val) {
        (Units::National, val) => national(x, val, pos),
        (Units::Dbcs, Val::Fig(f)) => Ok(vec![x.facts().figurative(f); 2]),
        (_, val) => store::natural_bytes(&x.facts(), val, pos),
    }
}

/// A value as national characters: a figurative constant is one (assumption C191), and any other
/// value that is not national is converted as MOVE converts it (C232).
fn national<P: Copy, O>(x: &impl Values<P, O>, val: Val, pos: Pos) -> R<Vec<u8>> {
    let facts = x.facts();
    Ok(match val {
        Val::National(b) | Val::AllNational(b) => b,
        Val::Fig(f) => store::figurative_unit(f, facts.options().quote).to_be_bytes().to_vec(),
        val => facts.page().decode(&store::natural_bytes(&facts, val, pos)?).encode_utf16().flat_map(u16::to_be_bytes).collect(),
    })
}

/// A REPLACING BY value's characters, which fill what they replace when it is a figurative constant.
struct Substitute {
    chars: Vec<u8>,
    figurative: bool,
}

/// REPLACING's BY value, `len` bytes when it is a figurative constant.
fn substitution<P: Copy, O>(x: &mut impl Values<P, O>, by: &Replacement<P, O>, units: Units, len: usize, pos: Pos) -> R<Vec<u8>> {
    let s = match by {
        Replacement::Fill(b) if units == Units::Bytes => Substitute { chars: vec![*b], figurative: true },
        Replacement::Fill(b) if units == Units::Dbcs => Substitute { chars: vec![*b; 2], figurative: true },
        Replacement::Fill(_) => return Err(Abend::ironwork("a REPLACING byte cannot replace national characters", pos)),
        Replacement::Chars(Chars::Value(o)) => {
            let val = x.value(o, pos)?;
            let figurative = matches!(val, Val::Fig(_));
            Substitute { chars: in_units(x, val, units, pos)?, figurative }
        }
        Replacement::Chars(c) => Substitute { chars: chars(x, c, pos)?, figurative: false },
    };
    Ok(if s.figurative { s.chars.repeat(len / units.size()) } else { s.chars })
}

/// TALLYING's add: `n` added to a numeric item, stored with no size error.
fn add_count<P: Copy>(x: &mut impl Host<P>, dest: Loc, n: i64, not_numeric: &str, pos: Pos) -> R<()> {
    let Val::Num(current) = host::read(x, dest, pos)? else {
        return Err(Abend::ironwork(not_numeric, pos));
    };
    let arith = x.facts().options().arith;
    let total = current.add(Fixed::new(n as i128, Places::new(19, 0)), 0, arith).map_err(|_| Abend::ironwork("TALLYING", pos))?;
    x.store_fixed(dest, &total, pos)
}

/// The part of `data` BEFORE and AFTER INITIAL leave a phrase: the last of each applies.
fn region<P: Copy, O>(x: &mut impl Values<P, O>, data: &[u8], units: Units, bounds: &[Bound<P, O>], pos: Pos) -> R<(usize, usize)> {
    let (mut before, mut after) = (None, None);
    for b in bounds {
        let v = chars_in(x, &b.value, units, pos)?;
        if b.after { after = Some(v) } else { before = Some(v) }
    }
    Ok(strings::region(data, units.size(), before.as_deref(), after.as_deref()))
}

fn phrases<P: Copy, O>(x: &mut impl Values<P, O>, data: &[u8], units: Units, phrases: &[InspectPhrase<P, O>], pos: Pos) -> R<Vec<Phrase>> {
    let mut out = Vec::new();
    for p in phrases {
        let pattern = match &p.pattern {
            Some(c) => chars_in(x, c, units, pos)?,
            None => Vec::new(),
        };
        let len = if pattern.is_empty() { units.size() } else { pattern.len() };
        let by = match &p.by {
            Some(by) => Some(substitution(x, by, units, len, pos)?),
            None => None,
        };
        if by.as_ref().is_some_and(|b| b.len() != len) {
            return Err(Abend::ironwork("a REPLACING value must be as long as what it replaces", pos));
        }
        let (start, end) = region(x, data, units, &p.bounds, pos)?;
        out.push(Phrase { mode: p.mode, pattern, by, start, end });
    }
    Ok(out)
}
