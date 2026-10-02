//! Places, operands, constants and arithmetic expressions (lir.md §5 and §6).

use super::{Lower, R, is_static, push, unsupported};
use crate::layout::Resolved;
use crate::machine::literal_fixed;
use numeric::precision::Fixed;
use rt::lir::{self, Comparand, ConstId, ExprId, IntExpr, Mode, PlaceId};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{BinOp, Expr, Figurative, Literal, Operand, Ref};
use zarch::wide::U256;

/// What the walker reads from an operand (`Machine::read`, `literal_value`), which decides the
/// MOVE and comparison plans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Bytes,
    All,
    Fig(Figurative),
    /// A fixed-point number with its decimal places, None for an expression's result.
    Num(Option<u32>),
    Float,
    National,
    Address,
}

/// An operand as MOVE and comparison see it: its value, the sender's kind where the walker keeps
/// its location, and the digits an integer shows as alphanumeric.
#[derive(Clone, Copy, Debug)]
pub(super) struct Side {
    pub value: Value,
    pub src: Option<Kind>,
    pub digits: u32,
}

pub(super) struct Lowered {
    pub operand: lir::Operand,
    pub side: Side,
}

pub(super) fn value_of(kind: Kind) -> Value {
    match kind {
        Kind::Group | Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => Value::Bytes,
        Kind::National => Value::National,
        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => Value::Address,
        Kind::Index => Value::Num(Some(0)),
        Kind::Float(_) => Value::Float,
        Kind::Binary { scale, .. } | Kind::Packed { scale, .. } | Kind::Zoned { scale, .. } => Value::Num(Some(scale)),
    }
}

pub(super) fn scale(kind: Kind) -> u32 {
    kind.digits_scale().map_or(0, |(_, s)| s)
}

fn integer_kind(kind: Kind) -> bool {
    match kind {
        Kind::Zoned { scale, .. } | Kind::Packed { scale, .. } | Kind::Binary { scale, .. } => scale == 0,
        Kind::Index => true,
        _ => false,
    }
}

/// The integer part, as `Machine::integer` truncates it; None past 64 bits, where the walker abends.
fn whole(f: &Fixed) -> Option<i64> {
    let m = f.magnitude.div_rem(U256::pow10(f.places.dec)).0.to_u128().and_then(|m| i64::try_from(m).ok())?;
    Some(if f.negative { -m } else { m })
}

/// The references `Machine::dmax` locates: every operand but divisors and exponents.
pub(super) fn dmax_refs<'e>(e: &'e Expr, out: &mut Vec<&'e Ref>) {
    match e {
        Expr::Operand(Operand::Ref(r)) => out.push(r),
        Expr::Operand(_) => {}
        Expr::Neg(inner) => dmax_refs(inner, out),
        Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => dmax_refs(a, out),
        Expr::Bin(a, _, b) => {
            dmax_refs(a, out);
            dmax_refs(b, out);
        }
    }
}

impl Lower<'_> {
    pub(super) fn kind_of(&self, place: PlaceId) -> Kind {
        self.places[place as usize].kind
    }

    pub(super) fn is_static(&self, place: PlaceId) -> bool {
        is_static(&self.places[place as usize])
    }

    /// A data reference as `Machine::locate_as` resolves it. `receiving` keeps a group that holds
    /// the object of its own OCCURS DEPENDING ON at its maximum length.
    pub(super) fn place(&mut self, r: &Ref, receiving: bool) -> R<PlaceId> {
        let key = format!("{receiving} {r:?}");
        if let Some(&id) = self.place_ids.get(&key) {
            return Ok(id);
        }
        let id = self.new_place(r, receiving)?;
        self.place_ids.insert(key, id);
        Ok(id)
    }

    fn new_place(&mut self, r: &Ref, receiving: bool) -> R<PlaceId> {
        let layout = self.layout;
        // `oo_register`: SELF's cell and JNIENVPTR's, whole, whatever reference modification says.
        if r.qualifiers.is_empty() && r.subscripts.is_empty() && matches!(r.name.as_str(), "SELF" | "JNIENVPTR") && layout.resolve(&r.name, &[], r.pos).is_err() {
            let (base, kind) = if r.name == "SELF" { (lir::Base::SelfRef, Kind::ObjectReference) } else { (lir::Base::JniEnv, Kind::Pointer) };
            let place = lir::Place { base, offset: 0, len: 4, kind, subscripts: Vec::new(), odo: None, refmod: None, name: self.sym(&r.name), at: self.at(r.pos) };
            return self.push_place(place, None);
        }
        if compile::markup::xml_register(layout, r) {
            let place = self.xml_register(r)?;
            return self.push_place(place, None);
        }
        if r.name == "RETURN-CODE" && r.qualifiers.is_empty() && !layout.items.iter().any(|i| i.name.as_deref() == Some("RETURN-CODE")) {
            let place = lir::Place {
                base: lir::Base::ReturnCode,
                offset: 0,
                len: 2,
                kind: Kind::Binary { digits: 4, scale: 0, signed: true, native: false },
                subscripts: Vec::new(),
                odo: None,
                refmod: None,
                name: self.sym(&r.name),
                at: self.at(r.pos),
            };
            return self.push_place(place, None);
        }
        match layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(index)) => self.item_place(index, r, receiving),
            Ok(Resolved::Condition(_)) => unsupported("a condition-name used as data", r.pos),
            Err(_) => unsupported("a data name the walker resolves only when it runs", r.pos),
        }
    }

    /// Item `index`, referenced with `r`'s subscripts and reference modification, without looking
    /// its name up again.
    pub(super) fn item_place(&mut self, index: usize, r: &Ref, receiving: bool) -> R<PlaceId> {
        let layout = self.layout;
        let item = &layout.items[index];
        if r.subscripts.len() != item.dims.len() {
            return unsupported("a reference with the wrong number of subscripts", r.pos);
        }
        let ssrange = self.c.ssrange;
        let base = match item.linkage {
            Some(record) => lir::Base::Linkage(record),
            None if item.local => lir::Base::Local,
            None => lir::Base::Program,
        };
        let mut subscripts = Vec::with_capacity(item.dims.len());
        for (&(stride, count), sub) in item.dims.iter().zip(&r.subscripts) {
            subscripts.push(lir::Subscript { stride, value: self.int_expr(sub, r.pos)?, check: ssrange.then_some(count) });
        }
        if !item.moved_by.is_empty() || item.odo.iter().any(|&t| layout.items[t].followed) {
            return unsupported("an item that follows an OCCURS DEPENDING ON table in its record, or a group holding such a table and what follows it", r.pos);
        }
        let mut odo = None;
        if let Some(&t) = item.odo.first()
            && !(receiving && r.refmod.is_none() && self.object_within(t, index)?)
        {
            let table = &layout.items[t];
            let Some(object) = &table.depending_on else { return unsupported("an OCCURS DEPENDING ON table without its object", r.pos) };
            let object = self.int_expr(&Expr::Operand(Operand::Ref(object.clone())), r.pos)?;
            odo = Some(lir::Odo { object, max: table.occurs, element: table.size, check: ssrange });
        }
        let (kind, refmod) = match &r.refmod {
            None => (item.kind, None),
            Some(rm) => {
                let start = self.int_expr(&rm.start, r.pos)?;
                let length = match &rm.length {
                    Some(l) => Some(self.int_expr(l, r.pos)?),
                    None => None,
                };
                let kind = if item.kind == Kind::National { Kind::National } else { Kind::Alnum { justified: false } };
                (kind, Some(lir::RefMod { start, length, check: ssrange }))
            }
        };
        let place = lir::Place { base, offset: item.offset, len: item.size, kind, subscripts, odo, refmod, name: self.sym(&r.name), at: self.at(r.pos) };
        self.push_place(place, Some(index))
    }

    pub(super) fn push_place(&mut self, place: lir::Place, item: Option<usize>) -> R<PlaceId> {
        let id = push(&mut self.places, place, "places")?;
        self.place_items.push(item);
        Ok(id)
    }

    /// Whether table `t`'s OCCURS DEPENDING ON object lies within item `group`.
    fn object_within(&self, t: usize, group: usize) -> R<bool> {
        let layout = self.layout;
        let Some(object) = &layout.items[t].depending_on else { return Ok(false) };
        let mut at = match layout.resolve(&object.name, &object.qualifiers, object.pos) {
            Ok(Resolved::Item(i)) => i,
            Ok(Resolved::Condition(_)) => return Ok(false),
            Err(_) => return unsupported("an OCCURS DEPENDING ON object the walker resolves only when it runs", object.pos),
        };
        loop {
            if at == group {
                return Ok(true);
            }
            match layout.items[at].parent {
                Some(p) => at = p,
                None => return Ok(false),
            }
        }
    }

    /// A subscript, bound, count or exponent, as `Machine::integer` evaluates it.
    pub(super) fn int_expr(&mut self, e: &Expr, pos: Pos) -> R<IntExpr> {
        match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => {
                if let Some(n) = literal_fixed(t).as_ref().and_then(whole) {
                    return Ok(IntExpr::Const(n));
                }
            }
            Expr::Operand(Operand::Literal(Literal::Figurative(Figurative::Zero))) => return Ok(IntExpr::Const(0)),
            Expr::Operand(Operand::Ref(r)) => {
                let p = self.place(r, false)?;
                if integer_kind(self.kind_of(p)) {
                    return Ok(IntExpr::Item(p));
                }
            }
            _ => {}
        }
        let dmax = self.dmax(e)?;
        let prepass = self.dmax_places(e)?;
        Ok(IntExpr::Fixed { expr: self.expr(e, pos)?, dmax, prepass })
    }

    /// An expression other than an operand, as `Machine::expr_value` evaluates it: the float test
    /// locates operands up to the first floating-point one, then a fixed-point expression's dmax pass
    /// locates its operands again.
    pub(super) fn computed(&mut self, e: &Expr, pos: Pos) -> R<Comparand> {
        let mut prepass = self.float_probe(e)?;
        let (mode, dmax) = if self.uses_float(e)? {
            (Mode::Float(self.c.options.arith.float_intermediate()), 0)
        } else {
            prepass.extend(self.dmax_places(e)?);
            (Mode::Fixed, self.dmax(e)?)
        };
        Ok(Comparand::Expr { expr: self.expr(e, pos)?, dmax, mode, prepass })
    }

    /// The places `Machine::dmax` locates, in its order, static ones left out.
    pub(super) fn dmax_places(&mut self, e: &Expr) -> R<Vec<PlaceId>> {
        let mut refs = Vec::new();
        dmax_refs(e, &mut refs);
        let mut places = Vec::new();
        for r in refs {
            let p = self.place(r, false)?;
            if !self.is_static(p) {
                places.push(p);
            }
        }
        Ok(places)
    }

    /// The walker's `dmax`: the most decimal places among the operands, divisors and exponents aside.
    pub(super) fn dmax(&mut self, e: &Expr) -> R<u32> {
        Ok(match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).map_or(0, |f| f.places.dec),
            Expr::Operand(Operand::Ref(r)) => {
                let p = self.place(r, false)?;
                scale(self.kind_of(p))
            }
            Expr::Operand(_) => 0,
            Expr::Neg(inner) => self.dmax(inner)?,
            Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => self.dmax(a)?,
            Expr::Bin(a, _, b) => self.dmax(a)?.max(self.dmax(b)?),
        })
    }

    /// The walker's `uses_float`: a floating-point item among the operands, a floating-point
    /// function, or a function of `rt::intrinsic::MIXED` with an argument that is one.
    pub(super) fn uses_float(&mut self, e: &Expr) -> R<bool> {
        self.probe(e, &mut Vec::new())
    }

    /// The places the walker's float test locates, in its order, static ones left out.
    pub(super) fn float_probe(&mut self, e: &Expr) -> R<Vec<PlaceId>> {
        let mut places = Vec::new();
        self.probe(e, &mut places)?;
        Ok(places)
    }

    /// `uses_float`: each operand is located left to right until a floating-point one; a
    /// function's arguments are tested only for the functions of `rt::intrinsic::MIXED`.
    fn probe(&mut self, e: &Expr, located: &mut Vec<PlaceId>) -> R<bool> {
        Ok(match e {
            Expr::Operand(Operand::Ref(r)) => {
                let p = self.place(r, false)?;
                if !self.is_static(p) {
                    located.push(p);
                }
                matches!(self.kind_of(p), Kind::Float(_))
            }
            Expr::Operand(Operand::Function(f)) => {
                let name = f.name.as_str();
                if rt::intrinsic::FLOATING_POINT.contains(&name) {
                    return Ok(true);
                }
                if rt::intrinsic::MIXED.contains(&name) {
                    for a in &f.args {
                        if self.probe(a, located)? {
                            return Ok(true);
                        }
                    }
                }
                false
            }
            Expr::Operand(_) => false,
            Expr::Neg(inner) => self.probe(inner, located)?,
            Expr::Bin(a, _, b) => self.probe(a, located)? || self.probe(b, located)?,
        })
    }

    pub(super) fn expr(&mut self, e: &Expr, pos: Pos) -> R<ExprId> {
        let node = match e {
            Expr::Operand(op) => lir::Expr::Operand(self.operand(op, pos)?.operand),
            Expr::Neg(inner) => lir::Expr::Neg(self.expr(inner, pos)?),
            Expr::Bin(a, BinOp::Pow, b) => {
                let base = self.expr(a, pos)?;
                lir::Expr::Pow(base, self.int_expr(b, pos)?)
            }
            Expr::Bin(a, op, b) => {
                let x = self.expr(a, pos)?;
                lir::Expr::Bin(x, *op, self.expr(b, pos)?)
            }
        };
        push(&mut self.exprs, node, "expressions")
    }

    pub(super) fn operand(&mut self, op: &Operand, pos: Pos) -> R<Lowered> {
        match op {
            Operand::Ref(r) => {
                let p = self.place(r, false)?;
                let kind = self.kind_of(p);
                let digits = kind.digits_scale().map_or(0, |(d, _)| d);
                Ok(Lowered { operand: lir::Operand::Load(p), side: Side { value: value_of(kind), src: Some(kind), digits } })
            }
            Operand::Literal(lit) => self.literal(lit, pos),
            Operand::LengthOf(r) => {
                let p = self.place(r, false)?;
                Ok(Lowered { operand: lir::Operand::LengthOf(p), side: Side { value: Value::Num(Some(0)), src: None, digits: 9 } })
            }
            Operand::AddressOf(r) => {
                let p = self.place(r, false)?;
                Ok(Lowered { operand: lir::Operand::AddressOf(p), side: Side { value: Value::Address, src: None, digits: 0 } })
            }
            Operand::Function(f) => {
                let (id, side) = self.function(f)?;
                Ok(Lowered { operand: lir::Operand::Function(id), side })
            }
        }
    }

    pub(super) fn literal(&mut self, lit: &Literal, pos: Pos) -> R<Lowered> {
        let (id, side) = self.literal_const(lit, pos)?;
        Ok(Lowered { operand: lir::Operand::Const(id), side })
    }

    /// A literal converted once, as `literal_value` converts it on every use.
    pub(super) fn literal_const(&mut self, lit: &Literal, pos: Pos) -> R<(ConstId, Side)> {
        let (constant, value, digits) = match lit {
            Literal::Alnum(s) => (lir::Const::Bytes(self.encode(s, pos)?), Value::Bytes, 0),
            Literal::Hex(b) => (lir::Const::Bytes(b.clone()), Value::Bytes, 0),
            Literal::National(s) => (lir::Const::National(s.encode_utf16().flat_map(u16::to_be_bytes).collect()), Value::National, 0),
            Literal::Number(t) => match literal_fixed(t) {
                Some(f) => (lir::Const::Number(f), Value::Num(Some(f.places.dec)), f.places.total()),
                None => return unsupported("a numeric literal of more than 31 digits", pos),
            },
            Literal::Figurative(f) => (lir::Const::Figurative(*f), Value::Fig(*f), 0),
            Literal::All(inner) => match &**inner {
                Literal::Alnum(s) => (lir::Const::All(self.encode(s, pos)?), Value::All, 0),
                Literal::Hex(b) => (lir::Const::All(b.clone()), Value::All, 0),
                Literal::Figurative(f) => (lir::Const::Figurative(*f), Value::Fig(*f), 0),
                _ => return unsupported("ALL with a literal that is not alphanumeric", pos),
            },
        };
        Ok((self.constant(constant)?, Side { value, src: None, digits }))
    }

    pub(super) fn encode(&self, text: &str, pos: Pos) -> R<Vec<u8>> {
        self.page.encode(text).or_else(|_| unsupported("a literal the code page cannot encode", pos))
    }

    pub(super) fn constant(&mut self, c: lir::Const) -> R<ConstId> {
        let key = format!("{c:?}");
        if let Some(&id) = self.const_ids.get(&key) {
            return Ok(id);
        }
        let id = push(&mut self.consts, c, "constants")?;
        self.const_ids.insert(key, id);
        Ok(id)
    }
}
