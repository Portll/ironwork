
//! Plans (lir.md §7 and §9): what `Machine::assign`, `arithmetic`, `store_fixed_checked`,
//! `initialize` and `display` decide from kinds on each execution, decided once.

use super::data::{Side, Value, scale};
use super::{Lower, R, push, unsupported};
use numeric::precision::receiver_dec;
use numeric::{Numproc, Trunc};
use rt::lir::{
    self, ArithId, ArithPlan, ArithStep, DisplayId, DisplayItem, ExprId, FloatFrom, Image, InitField, InitId, InitPlan, Mode, MovePlan, NationalFrom,
    NumericFrom, PlaceId, RemainderPlan, StorePlan,
};
use rt::picture::Sym;
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{Expr, Figurative, Literal, Operand, Target};

impl Lower<'_> {
    /// The name a TRUNC(OPT) report gives a binary receiver.
    fn receiver_name(&mut self, item: Option<usize>) -> lir::SymId {
        let name = match item {
            Some(i) => self.layout.items[i].name.clone().unwrap_or_else(|| "FILLER".into()),
            None => "RETURN-CODE".into(),
        };
        self.sym(&name)
    }

    /// How `store_fixed_checked` and `store_value` store a number into a receiver of `kind`.
    pub(super) fn store_plan(&mut self, kind: Kind, item: Option<usize>) -> R<StorePlan> {
        Ok(match kind {
            Kind::Zoned { digits, scale, signed, sign } => StorePlan::Zoned { digits, scale, signed, sign },
            Kind::Packed { digits, scale, signed } => StorePlan::Packed { digits, scale, signed },
            Kind::Binary { digits, scale, signed, native } => StorePlan::Binary { digits, scale, signed, native, name: self.receiver_name(item) },
            Kind::NumericEdited { edit, digits, scale, blank_when_zero } => StorePlan::NumericEdited { edit, digits, scale, blank_when_zero },
            Kind::Float(p) => StorePlan::Float(p),
            Kind::Index => StorePlan::Index,
            _ => StorePlan::Refused(self.ironwork("a numeric value stored into a non-numeric item")?),
        })
    }

    /// `Machine::assign` of what `from` reads as into a receiver of kind `to`, whose layout item is
    /// `item`. A group move converts nothing: a group sender's bytes go to a numeric, floating-point
    /// or edited receiver as they are, and a group receiver takes a numeric, floating-point or
    /// pointer item's bytes as stored.
    pub(super) fn move_plan(&mut self, from: &Side, to: Kind, item: Option<usize>) -> R<MovePlan> {
        let refused = |lower: &mut Self, message: &str| lower.ironwork(message).map(MovePlan::Refused);
        let unconverted = matches!(to, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. });
        if from.src == Some(Kind::Group) && unconverted {
            return Ok(MovePlan::Alnum { image: Image::Bytes, justified: false });
        }
        if to == Kind::Group && from.src.is_some() && matches!(from.value, Value::Num(_) | Value::Float | Value::Address) {
            return Ok(MovePlan::Alnum { image: Image::Stored, justified: false });
        }
        if from.value == Value::Num(None) && matches!(to, Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. }) {
            return unsupported("a FUNCTION result whose digits are known only when it runs, moved to an alphanumeric item", syntax::Pos::default());
        }
        Ok(match to {
            Kind::Group | Kind::Alnum { .. } => match image(from) {
                Ok(image) => MovePlan::Alnum { image, justified: matches!(to, Kind::Alnum { justified: true }) },
                Err(message) => refused(self, message)?,
            },
            Kind::AlnumEdited { edit } => {
                let positions = self.layout.edits[edit as usize].iter().filter(|s| !matches!(s, Sym::Insert(_))).count() as u32;
                match image(from) {
                    Ok(image) => MovePlan::AlnumEdited { image, edit, positions },
                    Err(message) => refused(self, message)?,
                }
            }
            Kind::National => match from.value {
                Value::National => MovePlan::National(NationalFrom::Units),
                Value::Bytes => MovePlan::National(NationalFrom::Decoded),
                Value::Fig(_) => MovePlan::National(NationalFrom::Figurative),
                _ => refused(self, "this value cannot be moved to a national item")?,
            },
            Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => match from.value {
                Value::Address | Value::Fig(Figurative::Null) => MovePlan::Address,
                _ => refused(self, "a pointer takes an address: use SET ... TO ADDRESS OF or NULL")?,
            },
            Kind::Index => match from.value {
                Value::Num(_) => MovePlan::Index,
                _ => refused(self, "an index takes an occurrence number")?,
            },
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } => {
                let copy = matches!(to, Kind::Packed { digits, signed: true, .. } if digits > 0) && from.src == Some(to) && self.c.options.numproc == Numproc::Pfd;
                let numeric_from = match from.value {
                    Value::Num(_) if copy => NumericFrom::PackedCopy,
                    Value::Num(_) => NumericFrom::Value,
                    Value::Float => NumericFrom::Float,
                    Value::Fig(Figurative::Zero) => NumericFrom::Zero,
                    Value::Fig(_) | Value::All => NumericFrom::Fill,
                    Value::Bytes => match from.src {
                        Some(Kind::NumericEdited { edit, digits, scale, .. }) => NumericFrom::DeEdit { edit, digits, scale },
                        _ => NumericFrom::Zoned,
                    },
                    Value::National => return refused(self, "a national value cannot be moved to a numeric item"),
                    Value::Address => return refused(self, "a pointer cannot be moved to a numeric item"),
                };
                MovePlan::Numeric { from: numeric_from, store: self.store_plan(to, item)? }
            }
            Kind::Float(precision) => match from.value {
                Value::Float => MovePlan::Float { from: FloatFrom::Float, precision },
                Value::Num(_) => MovePlan::Float { from: FloatFrom::Fixed, precision },
                Value::Fig(Figurative::Zero) => MovePlan::Float { from: FloatFrom::Zero, precision },
                _ => refused(self, "this value cannot be moved to a floating-point item")?,
            },
        })
    }

    /// COMPUTE, ADD, SUBTRACT, MULTIPLY and DIVIDE as `Machine::arithmetic` runs them: dmax and each
    /// receiver's store fixed here, and the places its two locate passes reach kept in order.
    pub(super) fn arith_plan(&mut self, computations: &[(&Target, &Expr)], remainder: Option<&(Target, Expr, Expr)>, handled: bool, pos: Pos) -> R<ArithId> {
        let mut prepass = Vec::new();
        let mut dmax = 0;
        let sources = computations.iter().map(|&(t, e)| (t, e)).chain(remainder.map(|(t, dividend, _)| (t, dividend)));
        for (t, e) in sources {
            let target = self.place(&t.r, false)?;
            if !self.is_static(target) {
                prepass.push(target);
            }
            prepass.extend(self.dmax_places(e)?);
            dmax = dmax.max(receiver_dec(scale(self.kind_of(target)), t.rounded)).max(self.dmax(e)?);
        }
        let arith = self.c.options.arith;
        let mut float_receiver = false;
        for &(t, _) in computations {
            let target = self.place(&t.r, false)?;
            float_receiver |= matches!(self.kind_of(target), Kind::Float(_));
        }
        let mut lowered: Vec<(&Expr, ExprId)> = Vec::new();
        let mut steps = Vec::new();
        for &(t, e) in computations {
            let target = self.place(&t.r, false)?;
            // A COMP-1 or COMP-2 receiver makes every step floating point, and the walker then skips the float test.
            let (probe, float) = if float_receiver { (Vec::new(), true) } else { (self.float_probe(e)?, self.uses_float(e)?) };
            let mode = if float { Mode::Float(arith.float_intermediate()) } else { Mode::Fixed };
            let expr = match lowered.iter().find(|(seen, _)| std::ptr::eq(*seen, e)) {
                Some(&(_, id)) => id,
                None => {
                    let id = self.expr(e, pos)?;
                    lowered.push((e, id));
                    id
                }
            };
            let store = self.store_plan(self.kind_of(target), self.place_items[target as usize])?;
            steps.push(ArithStep { target, expr, mode, store, rounded: t.rounded, probe });
        }
        let remainder = match (remainder, steps.first().map(|s| s.target)) {
            (Some((t, dividend, divisor)), Some(quotient)) => {
                let target = self.place(&t.r, false)?;
                let (dividend, divisor) = (self.expr(dividend, pos)?, self.expr(divisor, pos)?);
                let store = self.store_plan(self.kind_of(target), self.place_items[target as usize])?;
                Some(RemainderPlan { target, dividend, divisor, quotient_scale: scale(self.kind_of(quotient)), store })
            }
            _ => None,
        };
        push(&mut self.plans.arith, ArithPlan { dmax, arith, prepass, steps, remainder, handled }, "arithmetic plans")
    }

    /// `Machine::initialize` unrolled: every elementary item the walk reaches, offset from the
    /// target's start, every occurrence listed.
    pub(super) fn init_plan(&mut self, target: PlaceId) -> R<InitId> {
        let mut fields = Vec::new();
        match self.place_items[target as usize] {
            None => {
                let store = self.store_plan(self.kind_of(target), None)?;
                fields.push(InitField { offset: 0, len: self.places[target as usize].len, value: Figurative::Zero, store: MovePlan::Numeric { from: NumericFrom::Zero, store } });
            }
            Some(item) => self.init_fields(item, 0, &mut fields)?,
        }
        push(&mut self.plans.init, InitPlan { fields }, "INITIALIZE plans")
    }

    fn init_fields(&mut self, index: usize, offset: u32, fields: &mut Vec<InitField>) -> R<()> {
        let layout = self.layout;
        let item = &layout.items[index];
        let (value, from) = match item.kind {
            Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => return Ok(()),
            Kind::Group => {
                for &c in &item.children {
                    let child = &layout.items[c];
                    if child.redefines.is_some() || child.name.is_none() {
                        continue;
                    }
                    for k in 0..child.occurs {
                        self.init_fields(c, offset + (child.offset - item.offset) + k * child.size, fields)?;
                    }
                }
                return Ok(());
            }
            Kind::Pointer => (Figurative::Null, Value::Address),
            Kind::Alnum { .. } | Kind::National => (Figurative::Space, Value::Fig(Figurative::Space)),
            _ => (Figurative::Zero, Value::Fig(Figurative::Zero)),
        };
        let store = self.move_plan(&Side { value: from, src: None, digits: 0 }, item.kind, Some(index))?;
        fields.push(InitField { offset, len: item.size, value, store });
        Ok(())
    }

    /// `Machine::display`: each item shown as its kind is, literals as their text.
    pub(super) fn display_plan(&mut self, items: &[Operand], no_advancing: bool, pos: Pos) -> R<DisplayId> {
        let mut shown = Vec::with_capacity(items.len());
        for op in items {
            shown.push(match op {
                Operand::Ref(r) => {
                    let place = self.place(r, false)?;
                    match self.kind_of(place) {
                        Kind::National => DisplayItem::National(place),
                        Kind::Packed { digits, signed, .. } => DisplayItem::Digits { place, digits, signed },
                        Kind::Binary { digits, signed, native, .. } => {
                            let whole = native || self.c.options.trunc == Trunc::Bin;
                            let digits = match self.places[place as usize].len {
                                _ if !whole => digits,
                                2 => 5,
                                4 => 10,
                                _ if signed => 19,
                                _ => 20,
                            };
                            DisplayItem::Digits { place, digits, signed }
                        }
                        Kind::Float(_) => DisplayItem::Refused { place, abend: self.ironwork("DISPLAY of a floating-point item is not supported yet")? },
                        Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => {
                            DisplayItem::Refused { place, abend: self.ironwork("DISPLAY of a pointer, index or object reference is not supported")? }
                        }
                        _ => DisplayItem::Bytes(place),
                    }
                }
                Operand::Literal(Literal::Number(t)) if self.program.environment.decimal_point_comma => DisplayItem::Text(self.sym(&t.replace('.', ","))),
                Operand::Literal(Literal::Number(t)) => DisplayItem::Text(self.sym(t)),
                Operand::Literal(lit) => {
                    let text = self.display_text(lit, pos)?;
                    DisplayItem::Text(self.sym(&text))
                }
                Operand::LengthOf(_) | Operand::AddressOf(_) | Operand::Function(_) => DisplayItem::Value(self.operand(op, pos)?.operand),
            });
        }
        push(&mut self.plans.display, lir::DisplayPlan { items: shown, no_advancing }, "DISPLAY plans")
    }

    /// What DISPLAY shows for a literal other than a number: its characters, a figurative
    /// constant's one character, an ALL literal once.
    fn display_text(&mut self, lit: &Literal, pos: Pos) -> R<String> {
        Ok(match lit {
            Literal::Alnum(s) => self.page.decode(&self.encode(s, pos)?),
            Literal::Hex(b) => self.page.decode(b),
            Literal::National(s) => String::from_utf16_lossy(&s.encode_utf16().collect::<Vec<_>>()),
            Literal::Number(t) => t.clone(),
            Literal::Figurative(f) => self.page.decode_byte(self.c.collating.figurative(*f)).to_string(),
            Literal::All(inner) => match &**inner {
                Literal::Alnum(_) | Literal::Hex(_) | Literal::Figurative(_) => self.display_text(inner, pos)?,
                _ => return unsupported("ALL with a literal that is not alphanumeric", pos),
            },
        })
    }
}

/// `alnum_image`: the bytes an alphanumeric receiver takes from a sender, or why it refuses them.
fn image(from: &Side) -> Result<Image, &'static str> {
    match from.value {
        Value::Bytes => Ok(Image::Bytes),
        Value::All => Ok(Image::All),
        Value::Fig(_) => Ok(Image::Figurative),
        Value::Num(Some(0)) => Ok(Image::Digits { digits: from.digits }),
        Value::National => Err("a national value cannot be moved to an alphanumeric item"),
        Value::Num(_) | Value::Float | Value::Address => Err("only an integer numeric value can be moved to an alphanumeric item"),
    }
}
