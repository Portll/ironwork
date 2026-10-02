//! STRING, UNSTRING and INSPECT (lir.md §9.1) over the executor's handles, in the walker's order of
//! locates, reads and stores. The inputs are `lir::text`'s plans with each receiver's MOVE, store
//! and step plan left to its `Loc`'s kind, which `store` decides by.

use crate::abend::Abend;
use crate::host::{self, Host, Values};
use crate::lir::{Bound, Chars, ConvertTable, Converting, Replacement, StringSource};
use crate::storage::{Loc, Val};
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
/// after the last, and each source is read after the ones before it are stored.
pub fn string<P: Copy, O>(x: &mut impl Values<P, O>, into: P, pointer: Option<P>, sources: &[StringSource<P, O>], pos: Pos) -> R<bool> {
    let dest = x.locate(into, true)?;
    let mut at = match pointer {
        Some(p) => x.integer(p, pos)?,
        None => 1,
    };
    let len = dest.len as i64;
    let mut overflow = at < 1 || at > len;
    if !overflow {
        'sources: for source in sources {
            let bytes = chars(x, &source.chars, pos)?;
            let delimiter = match &source.delimiter {
                None => None,
                Some(d) => Some(chars(x, d, pos)?),
            };
            for b in strings::delimited(&bytes, delimiter.as_deref()) {
                if at > len {
                    overflow = true;
                    break 'sources;
                }
                x.mem()[dest.offset + at as usize - 1] = b;
                host::mark(x, dest.offset + at as usize - 1, 1);
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
    let len = source.len() as i64;
    let mut at = match pointer {
        Some(p) => x.integer(p, pos)?,
        None => 1,
    };
    let mut found = Vec::with_capacity(delimiters.len());
    for (all, d) in delimiters {
        found.push((*all, chars(x, d, pos)?));
    }
    let delimiters = found;
    let mut overflow = at < 1 || at > len;
    let mut fields = 0i64;
    if !overflow {
        for field in into {
            if at > len {
                break;
            }
            let start = at as usize - 1;
            let dest = x.locate(field.target, true)?;
            let (end, matched) = if delimiters.is_empty() {
                ((start + dest.len).min(source.len()), None)
            } else {
                match strings::next_delimiter(&source, start, &delimiters) {
                    Some((at, k)) => (at, Some(k)),
                    None => (source.len(), None),
                }
            };
            x.assign(dest, Val::Bytes(source[start..end].to_vec()), None, pos)?;
            let delimiter = matched.map(|k| delimiters[k].1.clone());
            if let Some(p) = field.delimiter {
                let d = x.locate(p, true)?;
                x.assign(d, delimiter.clone().map_or(Val::Fig(Figurative::Space), Val::Bytes), None, pos)?;
            }
            if let Some(p) = field.count {
                host::set_integer(x, p, (end - start) as i64, pos)?;
            }
            let next = match matched {
                Some(k) => strings::past_delimiter(&source, end, &delimiters[k].1, delimiters[k].0),
                None => end,
            };
            at = next as i64 + 1;
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

/// INSPECT: TALLYING counts over the item as it is, then REPLACING and CONVERTING change it.
pub fn inspect<P: Copy, O>(
    x: &mut impl Values<P, O>,
    target: P,
    tallying: &[InspectPhrase<P, O>],
    replacing: &[InspectPhrase<P, O>],
    converting: Option<&Converting<P, O>>,
    pos: Pos,
) -> R<()> {
    let loc = x.locate(target, false)?;
    let mut data = store::bytes(x.mem(), loc).to_vec();
    count(x, &mut data, 1, tallying, pos)?;
    let mut changes = phrases(x, &data, 1, replacing, pos)?;
    if let Some(c) = converting {
        let pairs = match &c.table {
            ConvertTable::Built(pairs) => pairs.clone(),
            ConvertTable::Operands { from, to } => {
                let (from, to) = (chars(x, from, pos)?, chars(x, to, pos)?);
                if from.len() != to.len() {
                    return Err(Abend::ironwork("CONVERTING needs operands of the same length", pos));
                }
                let mut pairs: Vec<(u8, u8)> = Vec::new();
                for (f, t) in from.into_iter().zip(to) {
                    if !pairs.iter().any(|&(seen, _)| seen == f) {
                        pairs.push((f, t));
                    }
                }
                pairs
            }
        };
        let (start, end) = region(x, &data, 1, &c.bounds, pos)?;
        for (f, t) in pairs {
            changes.push(Phrase { mode: InspectMode::All, pattern: vec![f], by: Some(vec![t]), start, end });
        }
    }
    strings::inspect(&mut data, 1, &changes);
    host::write(x, loc, &data);
    Ok(())
}

/// INSPECT TALLYING of a function's value, evaluated once before the phrases' operands. A national
/// value's character positions are two bytes, and a figurative constant is one national character
/// (assumption C191).
pub fn tally<P: Copy, O>(x: &mut impl Values<P, O>, subject: &O, tallying: &[InspectPhrase<P, O>], pos: Pos) -> R<()> {
    let val = x.value(subject, pos)?;
    let unit = if matches!(val, Val::National(_)) { 2 } else { 1 };
    let mut data = store::natural_bytes(&x.facts(), val, pos)?;
    count(x, &mut data, unit, tallying, pos)
}

fn count<P: Copy, O>(x: &mut impl Values<P, O>, data: &mut [u8], unit: usize, tallying: &[InspectPhrase<P, O>], pos: Pos) -> R<()> {
    let tallied = phrases(x, data, unit, tallying, pos)?;
    let counts = strings::inspect(data, unit, &tallied);
    for (phrase, count) in tallying.iter().zip(counts) {
        let Some(counter) = phrase.counter else { continue };
        let dest = x.locate(counter, false)?;
        add_count(x, dest, count, "a TALLYING counter must be numeric", pos)?;
    }
    Ok(())
}

/// `chars`, where a figurative constant is a national character when a position is two bytes.
fn chars_in<P: Copy, O>(x: &mut impl Values<P, O>, c: &Chars<P, O>, unit: usize, pos: Pos) -> R<Vec<u8>> {
    match c {
        Chars::Value(o) if unit == 2 => match x.value(o, pos)? {
            Val::Fig(f) => Ok(store::figurative_unit(f, x.facts().options().quote).to_be_bytes().to_vec()),
            val => store::natural_bytes(&x.facts(), val, pos),
        },
        c => chars(x, c, pos),
    }
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
fn region<P: Copy, O>(x: &mut impl Values<P, O>, data: &[u8], unit: usize, bounds: &[Bound<P, O>], pos: Pos) -> R<(usize, usize)> {
    let (mut before, mut after) = (None, None);
    for b in bounds {
        let v = chars_in(x, &b.value, unit, pos)?;
        if b.after { after = Some(v) } else { before = Some(v) }
    }
    Ok(strings::region(data, unit, before.as_deref(), after.as_deref()))
}

fn phrases<P: Copy, O>(x: &mut impl Values<P, O>, data: &[u8], unit: usize, phrases: &[InspectPhrase<P, O>], pos: Pos) -> R<Vec<Phrase>> {
    let mut out = Vec::new();
    for p in phrases {
        let pattern = match &p.pattern {
            Some(c) => chars_in(x, c, unit, pos)?,
            None => Vec::new(),
        };
        let len = pattern.len().max(1);
        let by = match &p.by {
            Some(Replacement::Fill(b)) => Some(vec![*b; len]),
            Some(Replacement::Chars(c)) => Some(chars(x, c, pos)?),
            None => None,
        };
        if by.as_ref().is_some_and(|b| b.len() != len) {
            return Err(Abend::ironwork("a REPLACING value must be as long as what it replaces", pos));
        }
        let (start, end) = region(x, data, unit, &p.bounds, pos)?;
        out.push(Phrase { mode: p.mode, pattern, by, start, end });
    }
    Ok(out)
}
