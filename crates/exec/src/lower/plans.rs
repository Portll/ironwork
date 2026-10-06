
//! Plans (lir.md §7 and §9): what `Machine::assign`, `arithmetic`, `store_fixed_checked`,
//! `initialize` and `display` decide from kinds on each execution, decided once.

use super::data::{Side, Value, Within, scale};
use super::{Lower, R, push, unsupported};
use numeric::precision::Dmax;
use numeric::{Dialect, Numproc, Switched, Trunc};
use crate::machine::{divided_exponent, value_kind};
use rt::lir::{
    self, ArithId, ArithPlan, ArithStep, DisplayId, DisplayItem, ExprId, FloatFrom, Image, InitField, InitId, InitPlan, InitValue, Mode, MovePlan,
    NationalFrom, NumericFrom, PlaceId, RemainderPlan, StorePlan,
};
use rt::picture::Sym;
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{DataCategory, Expr, Figurative, InitialValue, InitializeWith, Literal, Operand, Target};

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
        let unconverted = matches!(to, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } | Kind::Dbcs { .. });
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
                Value::Dbcs => MovePlan::National(NationalFrom::Dbcs),
                Value::Fig(_) => MovePlan::National(NationalFrom::Figurative),
                _ => refused(self, "this value cannot be moved to a national item")?,
            },
            Kind::Dbcs { justified, edit } => match from.value {
                Value::Dbcs | Value::Fig(Figurative::Space) | Value::All => MovePlan::Dbcs { justified, edit },
                _ => refused(self, "only DBCS data or SPACE can be moved to a DBCS item")?,
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
                    Value::Dbcs => return refused(self, "a DBCS value cannot be moved to a numeric item"),
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
    pub(super) fn arith_plan(&mut self, computations: &[(&Target, &Expr)], remainder: Option<&(Target, Expr, Expr)>, handled: bool, per_receiver: bool, pos: Pos) -> R<ArithId> {
        let mut prepass = Vec::new();
        let mut places = Dmax::default();
        let sources = computations.iter().map(|&(t, e)| (t, e)).chain(remainder.map(|(t, dividend, _)| (t, dividend)));
        for (Target { r, rounded }, e) in sources {
            let target = self.place(r, false)?;
            if !self.is_static(target) {
                prepass.push(target);
            }
            prepass.extend(self.dmax_places(e)?);
            places = places.max(Dmax::receiver(scale(self.kind_of(target)), *rounded, self.c.options.extra_place())).with(self.dmax(e)?);
        }
        let dmax = places.last;
        let arith = self.c.options.arith;
        let mut float_receiver = false;
        for &(t, _) in computations {
            let target = self.place(&t.r, false)?;
            float_receiver |= matches!(self.kind_of(target), Kind::Float(_));
        }
        let mut lowered: Vec<(&Expr, ExprId)> = Vec::new();
        let mut steps = Vec::new();
        for &(Target { r, rounded }, e) in computations {
            let target = self.place(r, false)?;
            // A COMP-1 or COMP-2 receiver makes every step floating point, and the walker then skips the float test.
            let (probe, float) = if float_receiver { (Vec::new(), true) } else { (self.float_probe(e)?, self.uses_float(e)? || (divided_exponent(e) && dmax > 0)) };
            let mode = if float { Mode::Float(arith.float_intermediate()) } else { Mode::Fixed };
            let expr = match lowered.iter().find(|(seen, _)| std::ptr::eq(*seen, e)) {
                Some(&(_, id)) => id,
                None => {
                    let id = self.expr_within(e, pos, Within::of(mode, dmax))?;
                    lowered.push((e, id));
                    id
                }
            };
            let store = self.store_plan(self.kind_of(target), self.place_items[target as usize])?;
            steps.push(ArithStep { target, expr, mode, store, rounded: *rounded, probe });
        }
        let remainder = match (remainder, steps.first().map(|s| s.target)) {
            // The walker stores the remainder unrounded.
            (Some((Target { r, rounded: _ }, dividend, divisor)), Some(quotient)) => {
                let target = self.place(r, false)?;
                let (dividend, divisor) = (self.expr_within(dividend, pos, Within::Fixed(dmax))?, self.expr_within(divisor, pos, Within::Fixed(dmax))?);
                let store = self.store_plan(self.kind_of(target), self.place_items[target as usize])?;
                Some(RemainderPlan { target, dividend, divisor, quotient_scale: scale(self.kind_of(quotient)), store })
            }
            _ => None,
        };
        push(&mut self.plans.arith, ArithPlan { dmax, arith, prepass, steps, remainder, handled, per_receiver, inner_dmax: places.inner }, "arithmetic plans")
    }

    /// `Machine::initialize` unrolled: every elementary item the walk reaches with `with`'s FILLER,
    /// offset from the target's start, every occurrence listed, each with what its phrases send it.
    /// A reference-modified target is one field, SPACE or REPLACING's operand by its category; a
    /// target that is no data item's, RETURN-CODE, takes zeros or REPLACING NUMERIC's operand.
    pub(super) fn init_plan(&mut self, target: PlaceId, with: &InitializeWith, pos: Pos) -> R<InitId> {
        let mut fields = Vec::new();
        let place = &self.places[target as usize];
        let (kind, len, refmod) = (place.kind, place.len, place.refmod.is_some());
        let item = self.place_items[target as usize];
        let category = match (item, refmod) {
            (_, true) => self.layout.refmod_category(item, kind),
            (Some(item), false) => {
                self.init_fields(item, with, pos, &mut fields)?;
                return push(&mut self.plans.init, InitPlan { fields }, "INITIALIZE plans");
            }
            (None, false) => DataCategory::Numeric,
        };
        match with.initial_value(Some(category), false) {
            Some(InitialValue::Replacing(by)) => {
                let by = self.operand(by, pos)?;
                let store = self.move_plan(&by.side, kind, None)?;
                fields.push(InitField { offset: 0, len, value: InitValue::Replacing(by.operand), store, scaling: 0 });
            }
            Some(_) if refmod => {
                let store = self.move_plan(&Side { value: Value::Fig(Figurative::Space), src: None, digits: 0 }, kind, None)?;
                fields.push(InitField { offset: 0, len, value: InitValue::Default(Figurative::Space), store, scaling: 0 });
            }
            Some(_) => {
                let store = MovePlan::Numeric { from: NumericFrom::Zero, store: self.store_plan(kind, None)? };
                fields.push(InitField { offset: 0, len, value: InitValue::Default(Figurative::Zero), store, scaling: 0 });
            }
            None => {}
        }
        push(&mut self.plans.init, InitPlan { fields }, "INITIALIZE plans")
    }

    fn init_fields(&mut self, index: usize, with: &InitializeWith, pos: Pos, fields: &mut Vec<InitField>) -> R<()> {
        // `initial_value` reads VALUE, REPLACING and DEFAULT, for the walker as for lowering.
        let InitializeWith { filler, value: _, replacing: _, default: _ } = with;
        let layout = self.layout;
        for (i, at) in layout.initialize_receivers(index, *filler) {
            let item = &layout.items[i];
            let (value, side, kind) = match (with.initial_value(layout.category(i), item.value.is_some()), &item.value) {
                (None, _) => continue,
                (Some(InitialValue::Value), Some(literal)) => {
                    let (constant, side) = self.literal_const(literal, pos)?;
                    (InitValue::Value(constant), side, value_kind(item.kind, literal))
                }
                (Some(InitialValue::Replacing(by)), _) => {
                    let by = self.operand(by, pos)?;
                    (InitValue::Replacing(by.operand), by.side, item.kind)
                }
                (Some(_), _) => {
                    let (fill, from) = match item.kind {
                        Kind::Pointer => (Figurative::Null, Value::Address),
                        Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::National | Kind::Dbcs { .. } => (Figurative::Space, Value::Fig(Figurative::Space)),
                        _ => (Figurative::Zero, Value::Fig(Figurative::Zero)),
                    };
                    (InitValue::Default(fill), Side { value: from, src: None, digits: 0 }, item.kind)
                }
            };
            let store = self.move_plan(&side, kind, Some(i))?;
            fields.push(InitField { offset: at, len: item.size, value, store, scaling: item.scaling });
        }
        Ok(())
    }

    /// `Machine::display`: each item shown as its kind is, literals as their text, national data
    /// converted only `upon_console`.
    pub(super) fn display_plan(&mut self, items: &[Operand], upon_console: bool, no_advancing: bool, pos: Pos) -> R<DisplayId> {
        let mut shown = Vec::with_capacity(items.len());
        for op in items {
            shown.push(match op {
                Operand::Ref(r) => {
                    let place = self.place(r, false)?;
                    match self.kind_of(place) {
                        Kind::National if upon_console => DisplayItem::National(place),
                        Kind::National => DisplayItem::Bytes(place),
                        Kind::Packed { digits, signed, .. } => DisplayItem::Digits { place, digits, signed },
                        Kind::Binary { digits, signed, native, .. } => {
                            let whole = native.is_native() || self.c.options.trunc == Trunc::Bin;
                            let digits = match self.places[place as usize].len {
                                len if self.c.options.dialect_of(Switched::DisplayOfNondisplayNumeric) == Dialect::Gnucobol => rt::display::whole_binary_digits(len as usize) as u32,
                                _ if !whole => digits,
                                1 => 3,
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
                Operand::Literal(Literal::Number(t)) => {
                    let point = if self.program.environment.decimal_point_comma { ',' } else { '.' };
                    DisplayItem::Text(self.sym(&rt::display::literal(t, point, self.c.options.dialect_of(Switched::DecimalCommaDisplayLiteral))))
                }
                Operand::Literal(lit) if self.unencodable(lit).is_some() => DisplayItem::Value(self.operand(op, pos)?.operand),
                Operand::Literal(lit) => {
                    let text = self.display_text(lit, upon_console, pos)?;
                    DisplayItem::Text(self.sym(&text))
                }
                Operand::LengthOf(_) | Operand::AddressOf(_) | Operand::Function(_) => {
                    let lowered = self.operand(op, pos)?;
                    match lowered.side.value {
                        Value::National if upon_console => DisplayItem::Value(self.display_of(lowered.operand, pos)?),
                        _ => DisplayItem::Value(lowered.operand),
                    }
                }
            });
        }
        push(&mut self.plans.display, lir::DisplayPlan { items: shown, no_advancing }, "DISPLAY plans")
    }

    /// FUNCTION DISPLAY-OF of a national value, which `rt::display::national` converts as it does.
    fn display_of(&mut self, national: lir::Operand, pos: Pos) -> R<lir::Operand> {
        let at = self.at(pos);
        let args = vec![lir::Argument::Value(lir::Comparand::Operand(national))];
        let plan = lir::FunctionPlan { func: lir::Func::DisplayOf, args, integer: None, side: None, refmod: None, arity: None, at };
        Ok(lir::Operand::Function(push(&mut self.plans.function, plan, "FUNCTION plans")?))
    }

    /// What DISPLAY shows for a literal other than a number: its characters, a figurative
    /// constant's one character, an ALL literal once; a national literal as `rt::display::national`
    /// writes it.
    fn display_text(&mut self, lit: &Literal, upon_console: bool, pos: Pos) -> R<String> {
        Ok(match lit {
            Literal::Alnum(s) => self.page.decode(&self.encode(s, pos)?),
            Literal::Hex(b) => self.page.decode(b),
            Literal::National(s) => rt::display::national(self.page, &s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<_>>(), upon_console),
            Literal::Dbcs(s) => self.page.decode_dbcs(&self.dbcs(s, pos)?),
            Literal::Number(t) => t.clone(),
            Literal::Figurative(f) => self.page.decode_byte(self.c.collating.figurative(*f)).to_string(),
            Literal::All(inner) => match &**inner {
                Literal::Alnum(_) | Literal::Hex(_) | Literal::National(_) | Literal::Figurative(_) => self.display_text(inner, upon_console, pos)?,
                _ => return unsupported("ALL with a literal that is not alphanumeric or national", pos),
            },
        })
    }
}

/// `alnum_image`: the bytes an alphanumeric receiver takes from a sender, or why it refuses them.
fn image(from: &Side) -> Result<Image, &'static str> {
    match from.value {
        Value::Bytes | Value::Dbcs => Ok(Image::Bytes),
        Value::All => Ok(Image::All),
        Value::Fig(_) => Ok(Image::Figurative),
        Value::Num(Some(0)) => Ok(Image::Digits { digits: from.digits }),
        Value::National => Err("a national value cannot be moved to an alphanumeric item"),
        Value::Num(_) | Value::Float | Value::Address => Err("only an integer numeric value can be moved to an alphanumeric item"),
    }
}
