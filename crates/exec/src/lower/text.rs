//! STRING, UNSTRING and INSPECT (lir.md §9.1): the plans `rt::text` runs, with each receiver's MOVE,
//! store and step plan decided from its kind, where the walker leaves them to its `Loc`.

use super::data::{Side, Value};
use super::{Lower, R, push, unsupported};
use crate::machine::literal_fixed;
use rt::fixed::zoned_digits;
use rt::lir::{
    Bound, Chars, ConvertTable, Converting, DelimiterIn, InspectId, InspectPhrase, InspectPlan, Inspected, PlaceId, Replacement, StepPlan,
    StorePlan, StringId, StringPlan, StringSource, UnstringId, UnstringInto, UnstringPlan,
};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{self, Delimiter, Figurative, Inspect, Literal, Operand, Ref, StringStmt, Unstring};
use zarch::decimal;

impl Lower<'_> {
    pub(super) fn string_plan(&mut self, st: &StringStmt, pos: Pos) -> R<StringId> {
        let into = self.place(&st.into, true)?;
        let pointer = self.pointer(st.pointer.as_ref())?;
        let mut sources = Vec::with_capacity(st.sources.len());
        for (op, delimiter) in &st.sources {
            let chars = self.chars(op, pos)?;
            let delimiter = match delimiter {
                Delimiter::Size => None,
                Delimiter::By(d) => Some(self.chars(d, pos)?),
            };
            sources.push(StringSource { chars, delimiter });
        }
        push(&mut self.plans.string, StringPlan { into, pointer, sources }, "STRING plans")
    }

    pub(super) fn unstring_plan(&mut self, u: &Unstring, pos: Pos) -> R<UnstringId> {
        let source = self.place(&u.source, false)?;
        let pointer = self.pointer(u.pointer.as_ref())?;
        let mut delimiters = Vec::with_capacity(u.delimiters.len());
        for (all, d) in &u.delimiters {
            delimiters.push((*all, self.chars(d, pos)?));
        }
        let bytes = Side { value: Value::Bytes, src: None, digits: 0 };
        let space = Side { value: Value::Fig(Figurative::Space), src: None, digits: 0 };
        let mut into = Vec::with_capacity(u.into.len());
        for field in &u.into {
            let target = self.place(&field.target, true)?;
            let plan = self.move_plan(&bytes, self.kind_of(target), self.place_items[target as usize])?;
            let delimiter = match &field.delimiter_in {
                None => None,
                Some(r) => {
                    let target = self.place(r, true)?;
                    let (kind, item) = (self.kind_of(target), self.place_items[target as usize]);
                    Some(DelimiterIn { target, found: self.move_plan(&bytes, kind, item)?, none: self.move_plan(&space, kind, item)? })
                }
            };
            let count = match &field.count_in {
                None => None,
                Some(r) => Some(self.integer_store(r)?),
            };
            into.push(UnstringInto { target, plan, delimiter, count });
        }
        let tallying = match &u.tallying {
            None => None,
            Some(r) => {
                let place = self.place(r, false)?;
                Some((place, self.count_plan(place, "TALLYING IN needs a numeric item")?))
            }
        };
        push(&mut self.plans.unstring, UnstringPlan { source, pointer, delimiters, into, tallying }, "UNSTRING plans")
    }

    /// CONVERTING's table is built here when both operands are literals of the same length and the
    /// item is not national; else the operands are kept and read, and their lengths compared, when
    /// the statement runs. A function's value is tallied alone, as the walker's `rt::text::tally`
    /// ignores REPLACING and CONVERTING there.
    pub(super) fn inspect_plan(&mut self, i: &Inspect, pos: Pos) -> R<InspectId> {
        let target = match &i.target {
            Operand::Ref(r) => Inspected::Item(self.place(r, false)?),
            op => Inspected::Value(self.operand(op, pos)?.operand),
        };
        let valued = match target {
            Inspected::Value(_) => true,
            Inspected::Item(place) => self.kind_of(place) == Kind::National,
        };
        let mut tallying = Vec::with_capacity(i.tallying.len());
        for p in &i.tallying {
            tallying.push(self.inspect_phrase(p, valued, pos)?);
        }
        if let Inspected::Value(_) = target {
            return push(&mut self.plans.inspect, InspectPlan { target, tallying, replacing: Vec::new(), converting: None }, "INSPECT plans");
        }
        let mut replacing = Vec::with_capacity(i.replacing.len());
        for p in &i.replacing {
            replacing.push(self.inspect_phrase(p, valued, pos)?);
        }
        let converting = match &i.converting {
            None => None,
            Some((from, to, bounds)) => {
                let table = match (self.inspect_chars(from, valued, pos)?, self.inspect_chars(to, valued, pos)?) {
                    (Chars::Literal(from), Chars::Literal(to)) if from.len() == to.len() => {
                        let mut pairs: Vec<(u8, u8)> = Vec::new();
                        for (f, t) in from.into_iter().zip(to) {
                            if !pairs.iter().any(|&(seen, _)| seen == f) {
                                pairs.push((f, t));
                            }
                        }
                        ConvertTable::Built(pairs)
                    }
                    (from, to) => ConvertTable::Operands { from, to },
                };
                Some(Converting { table, bounds: self.bounds(bounds, valued, pos)? })
            }
        };
        push(&mut self.plans.inspect, InspectPlan { target, tallying, replacing, converting }, "INSPECT plans")
    }

    /// `valued`: what is inspected may be national, a national item or function value, whose
    /// character positions are two bytes, so its literals stay values for `rt::text` to read as
    /// national characters, a figurative constant as one.
    fn inspect_phrase(&mut self, p: &ast::InspectPhrase, valued: bool, pos: Pos) -> R<InspectPhrase> {
        let pattern = p.pattern.as_ref().map(|op| self.inspect_chars(op, valued, pos)).transpose()?;
        let by = match &p.by {
            None => None,
            Some(Operand::Literal(Literal::Figurative(f))) if !valued => Some(Replacement::Fill(self.c.collating.figurative(*f))),
            Some(op) => Some(Replacement::Chars(self.inspect_chars(op, valued, pos)?)),
        };
        let counter = match &p.counter {
            None => None,
            Some(r) => {
                let place = self.place(r, false)?;
                Some((place, self.count_plan(place, "a TALLYING counter must be numeric")?))
            }
        };
        Ok(InspectPhrase { mode: p.mode, pattern, by, counter, bounds: self.bounds(&p.bounds, valued, pos)? })
    }

    fn bounds(&mut self, bounds: &[ast::Bound], valued: bool, pos: Pos) -> R<Vec<Bound>> {
        bounds.iter().map(|b| Ok(Bound { after: b.after, value: self.inspect_chars(&b.value, valued, pos)? })).collect()
    }

    /// `chars`, a literal kept as a value when `valued`.
    fn inspect_chars(&mut self, op: &Operand, valued: bool, pos: Pos) -> R<Chars> {
        match op {
            Operand::Literal(_) if valued => Ok(Chars::Value(self.operand(op, pos)?.operand)),
            _ => self.chars(op, pos),
        }
    }

    /// WITH POINTER: read as an integer, and stored as `set_integer` stores.
    fn pointer(&mut self, r: Option<&Ref>) -> R<Option<(PlaceId, StorePlan)>> {
        r.map(|r| self.integer_store(r)).transpose()
    }

    /// `set_integer`'s store into `r`, located as it locates it.
    fn integer_store(&mut self, r: &Ref) -> R<(PlaceId, StorePlan)> {
        let place = self.place(r, false)?;
        Ok((place, self.store_plan(self.kind_of(place), self.place_items[place as usize])?))
    }

    /// `add_count`: an item that does not read as a fixed-point number abends with `message`.
    fn count_plan(&mut self, place: PlaceId, message: &str) -> R<StepPlan> {
        let kind = self.kind_of(place);
        let store = match kind {
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Index => self.store_plan(kind, self.place_items[place as usize])?,
            _ => StorePlan::Refused(self.ironwork(message)?),
        };
        Ok(StepPlan { dmax: 0, store })
    }

    /// An operand as `facts::chars` gives it: an item's storage, a literal's `natural_bytes`, or any
    /// other operand's value.
    fn chars(&mut self, op: &Operand, pos: Pos) -> R<Chars> {
        Ok(match op {
            Operand::Ref(r) => Chars::Place(self.place(r, false)?),
            Operand::Literal(lit) => Chars::Literal(self.natural_bytes(lit, pos)?),
            _ => Chars::Value(self.operand(op, pos)?.operand),
        })
    }

    /// `store::natural_bytes` of what `literal_value` makes of a literal.
    fn natural_bytes(&mut self, lit: &Literal, pos: Pos) -> R<Vec<u8>> {
        Ok(match lit {
            Literal::Alnum(s) => self.encode(s, pos)?,
            Literal::Hex(b) => b.clone(),
            Literal::National(s) => s.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            Literal::Number(t) => match literal_fixed(t) {
                Some(f) => zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED),
                None => return unsupported("a numeric literal of more than 31 digits", pos),
            },
            Literal::Figurative(f) => vec![self.c.collating.figurative(*f)],
            Literal::All(inner) => match &**inner {
                Literal::Alnum(_) | Literal::Hex(_) | Literal::Figurative(_) => self.natural_bytes(inner, pos)?,
                _ => return unsupported("ALL with a literal that is not alphanumeric", pos),
            },
        })
    }
}
