//! The checks of lir.md §12.1 a lowered program must pass: every id names an entry of its table,
//! the control-flow graph is closed, every op has its debug entry, and places carry SSRANGE checks
//! exactly when the program has SSRANGE.

use rt::cics::Handles;
use rt::lir::{
    Advance, Argument, Base, Binding, Bound, CallArg, CallTarget, Ccsid, Chars, Comparand, Compare, Cond, Const, Convert, ConvertTable, Count, DisplayItem, InitValue, Inspected, Expr, FileVerb, Flag, Func, HostPlace, IntExpr,
    GlobalAt, JsonValue, Marker, Markup, MethodName, MovePlan, Named, Op, Operand, ParseValue, Place, PlaceId, Program, RangeKind, Receiver, Replacement, ReportOp, RowCount, SenderCheck, SetTo, SortIo, SortPlan,
    SqlStatement, StartKey, StorePlan, SymId, Terminator, UpDown, UserArgument, XmlValue,
};
use rt::report::{FieldContent, GroupKind, Origin};
use rt::vocab::AcceptFrom;

type Check<'a, T> = &'a dyn Fn(T) -> Result<(), String>;

/// A CICS command's handles, each checked against its table.
struct Ids<'a> {
    place: Check<'a, PlaceId>,
    operand: &'a dyn Fn(&Operand) -> Result<(), String>,
    symbol: Check<'a, SymId>,
}

impl Handles<PlaceId, Operand, SymId> for Ids<'_> {
    type Place = ();
    type Value = ();
    type Text = ();
    type Error = String;

    fn place(&mut self, place: PlaceId) -> Result<(), String> {
        (self.place)(place)
    }

    fn value(&mut self, value: Operand) -> Result<(), String> {
        (self.operand)(&value)
    }

    fn text(&mut self, text: SymId) -> Result<(), String> {
        (self.symbol)(text)
    }
}

/// A class definition's data and methods are programs of their own, each checked as one.
pub fn verify(p: &Program) -> Result<(), String> {
    verify_program(p)?;
    let Some(class) = &p.services.class else { return Ok(()) };
    for part in class.factory.iter().chain(&class.object) {
        verify(&part.data).map_err(|e| format!("class data: {e}"))?;
    }
    for m in &class.methods {
        verify(&m.code).map_err(|e| format!("method {}: {e}", p.symbols.get(m.name as usize).map_or("", String::as_str)))?;
    }
    let symbol = |id: u32| if (id as usize) < p.symbols.len() { Ok(()) } else { Err(format!("symbol {id} of {}", p.symbols.len())) };
    symbol(class.external)?;
    symbol(class.parent)?;
    for m in &class.methods {
        symbol(m.name)?;
        m.params.iter().chain(&m.returns).try_for_each(|&s| symbol(s))?;
    }
    Ok(())
}

fn verify_program(p: &Program) -> Result<(), String> {
    let within = |what: &str, id: u32, len: usize| if (id as usize) < len { Ok(()) } else { Err(format!("{what} {id} of {len}")) };
    let block = |id| within("block", id, p.blocks.len());
    let place = |id| within("place", id, p.places.len());
    let expr = |id| within("expression", id, p.exprs.len());
    let cond = |id| within("condition", id, p.conds.len());
    let abend = |id| within("abend", id, p.abends.len());
    let range = |id: u32, kind: RangeKind| match p.ranges.get(id as usize) {
        Some(r) if r.kind == kind => Ok(()),
        Some(r) => Err(format!("range {id} is {:?} where {kind:?} is wanted", r.kind)),
        None => Err(format!("range {id} of {}", p.ranges.len())),
    };
    let places = |qs: &[u32]| qs.iter().try_for_each(|&q| place(q));
    let int = |e: &IntExpr| match e {
        IntExpr::Const(_) | IntExpr::Walk(_) => Ok(()),
        IntExpr::Item(q) => place(*q),
        IntExpr::Fixed { expr: e, prepass, .. } => expr(*e).and_then(|()| places(prepass)),
    };
    let operand = |o: &Operand| match *o {
        Operand::Load(q) | Operand::LengthOf(q) | Operand::AddressOf(q) => place(q),
        Operand::Const(c) => within("constant", c, p.consts.len()),
        Operand::Function(f) => within("function plan", f, p.plans.function.len()),
        Operand::UserFunction(f) => within("user-defined function invocation", f, p.services.user_functions.len()),
    };
    let comparand = |c: &Comparand| match c {
        Comparand::Operand(o) => operand(o),
        Comparand::Expr { expr: e, prepass, .. } => expr(*e).and_then(|()| places(prepass)),
    };
    let ssrange = p.options.ssrange;
    let sender = |from: &Operand, check: SenderCheck| match (from, check) {
        (_, SenderCheck::None) => operand(from),
        (Operand::Load(q), _) if p.options.options.numcheck.is_some() => place(*q),
        _ => Err(format!("a NUMCHECK test {check:?} of a sender that is not a data item, or without NUMCHECK")),
    };
    if let Some((offset, len)) = p.storage.parmcheck
        && (p.options.options.parmcheck.is_none() || offset.checked_add(len).is_none_or(|end| end > p.storage.size))
    {
        return Err(format!("a PARMCHECK buffer at {offset} for {len} in a slab of {}, or without PARMCHECK", p.storage.size));
    }

    let numcheck = p.options.options.numcheck;
    let lax = numcheck.and_then(|c| c.zon).is_some_and(|z| z.lax);
    for (k, q) in p.places.iter().enumerate() {
        let Place { moved, subscripts, odo, refmod, at, .. } = q;
        within("debug entry", *at, p.debug.positions.len())?;
        if q.numcheck.lax.is_some() && !lax || q.numcheck.removed && numcheck.is_none() {
            return Err(format!("place {k}: NUMCHECK facts {:?} its options do not read", q.numcheck));
        }
        for s in subscripts {
            int(&s.value)?;
        }
        // Under SSRANGE a subscripted place carries its table's range, and without SSRANGE none.
        if q.table.is_some() != (ssrange && !subscripts.is_empty()) {
            return Err(format!("place {k}: a table range without SSRANGE, or none with it"));
        }
        for o in moved.iter().chain(odo) {
            int(&o.object)?;
            if o.check != ssrange {
                return Err(format!("place {k}: an OCCURS DEPENDING ON check that disagrees with SSRANGE"));
            }
        }
        if let Some(r) = refmod {
            int(&r.start)?;
            r.length.as_ref().map_or(Ok(()), int)?;
            if r.check != ssrange {
                return Err(format!("place {k}: a reference-modification check that disagrees with SSRANGE"));
            }
        }
    }
    for e in &p.exprs {
        match e {
            Expr::Operand(o) => operand(o)?,
            Expr::Neg(a) => expr(*a)?,
            Expr::Bin(a, _, b) => {
                expr(*a)?;
                expr(*b)?;
            }
            Expr::Pow(a, n) => {
                expr(*a)?;
                int(n)?;
            }
        }
    }
    for c in &p.conds {
        match c {
            Cond::Rel { a, b, .. } => {
                comparand(a)?;
                comparand(b)?;
            }
            Cond::Class { place: q, .. } => place(*q)?,
            Cond::Sign { value, .. } => comparand(value)?,
            Cond::Name { subject, values, .. } => {
                place(*subject)?;
                for &(low, high) in values {
                    within("constant", low, p.consts.len())?;
                    high.map_or(Ok(()), |h| within("constant", h, p.consts.len()))?;
                }
            }
            Cond::Not(a) => cond(*a)?,
            Cond::And(a, b) | Cond::Or(a, b) => {
                cond(*a)?;
                cond(*b)?;
            }
            Cond::Counter(_) | Cond::Sql(_) => {}
            Cond::InTable { index, count } => {
                place(*index)?;
                if let Count::Odo(o) = count {
                    int(&o.object)?;
                }
            }
        }
    }
    for a in &p.abends {
        a.at.map_or(Ok(()), |at| within("debug entry", at, p.debug.positions.len()))?;
    }
    p.consts.iter().try_for_each(|c| if let Const::Refused(a) = c { abend(*a) } else { Ok(()) })?;
    for a in &p.plans.arith {
        places(&a.prepass)?;
        for s in &a.steps {
            place(s.target)?;
            expr(s.expr)?;
            places(&s.probe)?;
        }
        if let Some(r) = &a.remainder {
            place(r.target)?;
            expr(r.dividend)?;
            expr(r.divisor)?;
        }
    }
    let symbol = |id: u32| within("symbol", id, p.symbols.len());
    if p.plans.function.iter().any(|f| f.func == Func::WhenCompiled) != p.options.when_compiled.is_some() {
        return Err("a compile time without a WHEN-COMPILED plan, or a WHEN-COMPILED plan without one".into());
    }
    for f in &p.plans.function {
        for a in &f.args {
            match a {
                Argument::Value(c) => comparand(c)?,
                Argument::All { element, all } => {
                    place(*element)?;
                    let subscripts = p.places.get(*element as usize).map_or(0, |q| q.subscripts.len());
                    for (position, count) in all {
                        if *position as usize >= subscripts {
                            return Err(format!("an ALL subscript at {position} of place {element}, which has {subscripts}"));
                        }
                        if let Count::Odo(o) = count {
                            int(&o.object)?;
                            if o.check != ssrange {
                                return Err("an ALL subscript's OCCURS DEPENDING ON check that disagrees with SSRANGE".into());
                            }
                        }
                    }
                }
            }
        }
        f.integer.as_ref().map_or(Ok(()), int)?;
        if let Some(r) = &f.refmod {
            int(&r.start)?;
            r.length.as_ref().map_or(Ok(()), int)?;
            if r.check {
                return Err("a FUNCTION's reference modification with an SSRANGE check".into());
            }
        }
        f.arity.map_or(Ok(()), abend)?;
        within("debug entry", f.at, p.debug.positions.len())?;
    }
    for f in &p.services.files {
        symbol(f.name)?;
        symbol(f.assign)?;
        f.error.map_or(Ok(()), |r| range(r, RangeKind::UseProcedure))?;
        f.status.map_or(Ok(()), |(q, _)| place(q))?;
        f.depending.map_or(Ok(()), |d| place(d.item))?;
        f.assign_item.map_or(Ok(()), |a| place(a.place))?;
        if let Some(r) = &f.relative {
            place(r.place)?;
            int(&r.value)?;
        }
        if let Some(l) = &f.linage {
            int(&l.lines)?;
            [&l.footing, &l.top, &l.bottom].into_iter().flatten().try_for_each(int)?;
            l.counter.map_or(Ok(()), |(q, _)| place(q))?;
        }
    }
    verify_scope(p, &range)?;
    let declaratives = &p.services.declaratives;
    declaratives.modes.iter().flatten().try_for_each(|&r| range(r, RangeKind::UseProcedure))?;
    if let Some((offset, len)) = declaratives.debug_item
        && offset.checked_add(len).is_none_or(|end| end > p.storage.size)
    {
        return Err(format!("DEBUG-ITEM at {offset} for {len} in a slab of {}", p.storage.size));
    }
    for op in &p.services.file_ops {
        let Some(f) = p.services.files.get(op.file as usize) else { return Err(format!("file {} of {}", op.file, p.services.files.len())) };
        let keys = f.keys.as_ref().map_or(0, |k| k.alternates.len() + 1);
        match &op.verb {
            FileVerb::Open(_) | FileVerb::Close | FileVerb::CloseWith(_) | FileVerb::Delete => {}
            FileVerb::Read { into, key, .. } => {
                into.map_or(Ok(()), |(q, _)| place(q))?;
                if *key != 0 && usize::from(*key) >= keys {
                    return Err(format!("key {key} of a file with {keys}"));
                }
            }
            FileVerb::Write { record, from, advancing } => {
                place(*record)?;
                from.map_or(Ok(()), |m| sender(&m.from, m.check).and_then(|()| place(m.to)))?;
                if let Some(Advance::Lines { count, .. }) = advancing {
                    int(count)?;
                }
            }
            FileVerb::Rewrite { record, from } => {
                place(*record)?;
                from.map_or(Ok(()), |m| sender(&m.from, m.check).and_then(|()| place(m.to)))?;
            }
            FileVerb::Start { key, .. } => match key {
                StartKey::Named { key, .. } if usize::from(*key) >= keys => return Err(format!("key {key} of a file with {keys}")),
                StartKey::Relative(n) => int(n)?,
                _ => {}
            },
        }
    }
    let chars = |c: &Chars| match c {
        Chars::Literal(_) => Ok(()),
        Chars::Place(q) => place(*q),
        Chars::Value(o) => operand(o),
    };
    let bounds = |bs: &[Bound]| bs.iter().try_for_each(|b| chars(&b.value));
    let store = |s: &StorePlan| if let StorePlan::Refused(a) = s { abend(*a) } else { Ok(()) };
    let moved = |m: &MovePlan| if let MovePlan::Refused(a) = m { abend(*a) } else { Ok(()) };
    for plan in &p.plans.string {
        place(plan.into)?;
        plan.pointer.as_ref().map_or(Ok(()), |(q, s)| place(*q).and_then(|()| store(s)))?;
        for source in &plan.sources {
            chars(&source.chars)?;
            source.delimiter.as_ref().map_or(Ok(()), chars)?;
        }
    }
    for plan in &p.plans.unstring {
        place(plan.source)?;
        plan.pointer.as_ref().map_or(Ok(()), |(q, s)| place(*q).and_then(|()| store(s)))?;
        plan.delimiters.iter().try_for_each(|(_, d)| chars(d))?;
        for field in &plan.into {
            place(field.target)?;
            moved(&field.plan)?;
            if let Some(d) = &field.delimiter {
                place(d.target)?;
                moved(&d.found)?;
                moved(&d.none)?;
            }
            field.count.as_ref().map_or(Ok(()), |(q, s)| place(*q).and_then(|()| store(s)))?;
        }
        plan.tallying.as_ref().map_or(Ok(()), |(q, s)| place(*q).and_then(|()| store(&s.store)))?;
    }
    for plan in &p.plans.inspect {
        match &plan.target {
            Inspected::Item(q) => place(*q)?,
            Inspected::Value(o) => operand(o)?,
        }
        for phrase in plan.tallying.iter().chain(&plan.replacing) {
            phrase.pattern.as_ref().map_or(Ok(()), chars)?;
            if let Some(Replacement::Chars(c)) = &phrase.by {
                chars(c)?;
            }
            phrase.counter.as_ref().map_or(Ok(()), |(q, s)| place(*q).and_then(|()| store(&s.store)))?;
            bounds(&phrase.bounds)?;
        }
        if let Some(c) = &plan.converting {
            if let ConvertTable::Operands { from, to } = &c.table {
                chars(from)?;
                chars(to)?;
            }
            bounds(&c.bounds)?;
        }
    }
    for plan in &p.plans.search_all {
        place(plan.index)?;
        store(&plan.store)?;
        if let Count::Odo(o) = &plan.count {
            int(&o.object)?;
        }
        for key in &plan.keys {
            comparand(&key.key)?;
            comparand(&key.value)?;
            if let Compare::Refused(a) = key.how {
                abend(a)?;
            }
        }
    }
    for c in &p.services.calls {
        match &c.target {
            CallTarget::Named { name, .. } => symbol(*name)?,
            CallTarget::Dynamic(o) => operand(o)?,
            CallTarget::Pointer(q) | CallTarget::Entry(q) => place(*q)?,
        }
        for a in &c.args {
            match a {
                CallArg::Reference(q) => place(*q)?,
                CallArg::Content(ch) => chars(ch)?,
                CallArg::Value(o) => operand(o)?,
                CallArg::Omitted => {}
            }
            if matches!(c.target, CallTarget::Pointer(_)) && !matches!(a, CallArg::Value(_) | CallArg::Omitted) {
                return Err("a CALL through a pointer with an argument that is not a value".into());
            }
        }
        c.returning.map_or(Ok(()), place)?;
    }
    for i in &p.services.invokes {
        match i.receiver {
            Receiver::SelfRef | Receiver::Super => {}
            Receiver::Class { name, external } => {
                symbol(name)?;
                symbol(external)?;
            }
            Receiver::Object(q) => place(q)?,
        }
        match i.method {
            MethodName::New => {}
            MethodName::Named(s) => symbol(s)?,
            MethodName::Dynamic(q) => place(q)?,
        }
        for (o, java) in &i.args {
            operand(o)?;
            symbol(*java)?;
        }
        if let Some((q, java)) = i.returning {
            place(q)?;
            symbol(java)?;
        }
    }
    for u in &p.services.user_functions {
        symbol(u.name)?;
        symbol(u.external)?;
        for a in &u.args {
            match a {
                UserArgument::Reference(q) => place(*q)?,
                UserArgument::Value(c) => comparand(c)?,
            }
        }
        if let Some(r) = &u.refmod {
            int(&r.start)?;
            r.length.as_ref().map_or(Ok(()), int)?;
            if r.check {
                return Err("a user-defined function's reference modification with an SSRANGE check".into());
            }
        }
        within("debug entry", u.at, p.debug.positions.len())?;
    }
    if let Some(f) = &p.services.function {
        let record = |q: PlaceId| p.places.get(q as usize).map(|q| q.base);
        let using: Vec<_> = p.storage.using.iter().map(|&o| Some(Base::Linkage(o))).collect();
        if f.params.iter().map(|&q| record(q)).ne(using) || record(f.returning) != p.storage.returning.map(Base::Linkage) {
            return Err("a function definition whose places are not its USING and RETURNING records".into());
        }
    }
    for c in &p.services.cics {
        c.clone().map(&mut Ids { place: &place, operand: &operand, symbol: &symbol })?;
        c.command.labels().into_iter().try_for_each(|q| within("paragraph", q, p.paragraphs.len()))?;
    }
    let host = |hs: &[HostPlace]| {
        hs.iter().try_for_each(|h| {
            place(h.var)?;
            h.ty.as_ref().map_or_else(|&a| abend(a), |_| Ok(()))?;
            h.indicator.map_or(Ok(()), |(q, _)| place(q))
        })
    };
    let rows_host = |rows: &RowCount| match rows {
        RowCount::Host(h) => host(std::slice::from_ref(h)),
        RowCount::Implicit | RowCount::Constant(_) => Ok(()),
    };
    for e in &p.sql {
        symbol(e.verb)?;
        symbol(e.text)?;
        match &e.statement {
            SqlStatement::Query { inputs, into } => host(inputs).and_then(|()| host(into))?,
            SqlStatement::Change { inputs, current_of, .. } => host(inputs).and_then(|()| current_of.map_or(Ok(()), symbol))?,
            SqlStatement::Open { cursor, inputs } => symbol(*cursor).and_then(|()| host(inputs))?,
            SqlStatement::Fetch { cursor, into } => symbol(*cursor).and_then(|()| host(into))?,
            SqlStatement::Close { cursor: s } | SqlStatement::Unsupported(s) => symbol(*s)?,
            SqlStatement::Connect { what, location } => symbol(*what).and_then(|()| host(location))?,
            SqlStatement::Prepare { name, source: inputs } | SqlStatement::Execute { name, inputs } => symbol(*name).and_then(|()| host(inputs))?,
            SqlStatement::ExecuteImmediate { source } => host(source)?,
            SqlStatement::OpenPrepared { cursor, statement, inputs } => symbol(*cursor).and_then(|()| symbol(*statement)).and_then(|()| host(inputs))?,
            SqlStatement::Describe { name, descriptor, .. } | SqlStatement::ExecuteDescriptor { name, descriptor } | SqlStatement::FetchDescriptor { cursor: name, descriptor } => symbol(*name).and_then(|()| place(*descriptor))?,
            SqlStatement::PrepareInto { name, source, descriptor, .. } => symbol(*name).and_then(|()| host(source)).and_then(|()| place(*descriptor))?,
            SqlStatement::OpenDescriptor { cursor, statement, descriptor } => symbol(*cursor).and_then(|()| symbol(*statement)).and_then(|()| place(*descriptor))?,
            SqlStatement::FetchRowset { cursor, rows, into, .. } => {
                symbol(*cursor)?;
                rows_host(rows)?;
                host(&into.iter().map(|a| a.place.clone()).collect::<Vec<_>>())?
            }
            SqlStatement::InsertRows { inputs, rows, .. } => {
                rows_host(rows)?;
                host(&inputs.iter().map(|a| a.place.clone()).collect::<Vec<_>>())?
            }
            SqlStatement::Call { procedure, args } => symbol(*procedure).and_then(|()| host(args))?,
            SqlStatement::Commit | SqlStatement::Rollback | SqlStatement::Declaration => {}
        }
    }
    p.services.sqlca.fields.iter().try_for_each(|&(_, q, _)| place(q))?;
    let count = |c: &Count| match c {
        Count::Fixed(_) | Count::Temp(_) => Ok(()),
        Count::Odo(o) => int(&o.object),
    };
    let marker = |m: &Marker| match *m {
        Marker::Byte(_) => Ok(()),
        Marker::Condition(c) => cond(c),
        Marker::Refused(a) => abend(a),
    };
    let convert = |c: &Convert| if let Convert::Refused(a) = *c { abend(a) } else { Ok(()) };
    let constant = |c: u32| within("constant", c, p.consts.len());
    let set_to = |s: &SetTo| match *s {
        SetTo::Nothing => Ok(()),
        SetTo::Move { place: q, value, .. } => place(q).and_then(|()| constant(value)),
        SetTo::Refused(a) => abend(a),
    };
    let flag = |f: &Flag| match f {
        Flag::Set { on, off } => set_to(on).and_then(|()| set_to(off)),
        Flag::Literals { on, off } => constant(on.0).and_then(|()| constant(off.0)),
    };
    let ccsid = |c: &Ccsid| if let Ccsid::Operand(o) = c { operand(o) } else { Ok(()) };
    let members = |k: usize, members: &[u32], nodes: usize| match members.iter().find(|&&m| m as usize <= k || m as usize >= nodes) {
        Some(m) => Err(format!("markup node {k} holds node {m} of {nodes}")),
        None => Ok(()),
    };
    for m in &p.services.markup {
        match m {
            Markup::JsonGenerate(g) => {
                place(g.from)?;
                g.subscripts.iter().try_for_each(int)?;
                g.name.map_or(Ok(()), symbol)?;
                place(g.receiver)?;
                ccsid(&g.encoding)?;
                g.count.map_or(Ok(()), |(q, _)| place(q))?;
                place(g.code.0)?;
                for (k, n) in g.nodes.iter().enumerate() {
                    symbol(n.name)?;
                    n.occurs.as_ref().map_or(Ok(()), count)?;
                    if let Some((at, test)) = &n.indicator {
                        at.map_or_else(abend, place)?;
                        marker(test)?;
                    }
                    match &n.value {
                        JsonValue::Object { members: held, .. } => members(k, held, g.nodes.len())?,
                        JsonValue::Leaf(leaf) => {
                            leaf.boolean.as_ref().map_or(Ok(()), marker)?;
                            convert(&leaf.convert)?;
                        }
                    }
                }
            }
            Markup::XmlGenerate(g) => {
                place(g.receiver)?;
                ccsid(&g.encoding)?;
                g.namespace.iter().chain(&g.prefix).try_for_each(operand)?;
                place(g.from)?;
                g.subscripts.iter().try_for_each(int)?;
                g.count.map_or(Ok(()), |(q, _)| place(q))?;
                place(g.code.0)?;
                for (k, n) in g.nodes.iter().enumerate() {
                    symbol(n.name)?;
                    n.occurs.as_ref().map_or(Ok(()), count)?;
                    match &n.value {
                        XmlValue::Element { members: held } | XmlValue::Members { members: held } => members(k, held, g.nodes.len())?,
                        XmlValue::Leaf { convert: c, .. } => convert(c)?,
                    }
                }
            }
            Markup::XmlParse(x) => {
                place(x.document)?;
                x.encoding.as_ref().map_or(Ok(()), operand)?;
                range(x.procedure, RangeKind::Processing)?;
                place(x.event)?;
                place(x.code.0)?;
                place(x.information.0)?;
                int(&x.code_value)?;
            }
            Markup::JsonParse(j) => {
                place(j.source)?;
                ccsid(&j.encoding)?;
                place(j.into)?;
                j.subscripts.iter().try_for_each(int)?;
                place(j.code.0)?;
                place(j.status.0)?;
                for (k, n) in j.nodes.iter().enumerate() {
                    if let Named::Exactly(name) | Named::Folded(name) = n.name {
                        symbol(name)?;
                    }
                    n.occurs.as_ref().map_or(Ok(()), count)?;
                    if let Some(i) = &n.indicator {
                        i.place.map_or(Ok(()), |at| at.map_or_else(abend, place))?;
                        flag(&i.flag)?;
                    }
                    match &n.value {
                        ParseValue::Object { members: held } => members(k, held, j.nodes.len())?,
                        ParseValue::Leaf(leaf) => leaf.boolean.as_ref().map_or(Ok(()), flag)?,
                        ParseValue::Suppressed => {}
                    }
                }
            }
        }
    }
    let files = |k: u16| within("file", u32::from(k), p.services.files.len());
    for s in &p.services.sorts {
        match s {
            SortPlan::File(f) => {
                files(f.sd)?;
                for io in f.input.iter().chain(&f.output) {
                    match io {
                        SortIo::Files(ks) => ks.iter().try_for_each(|&k| files(k))?,
                        SortIo::Procedure(r) => range(*r, RangeKind::SortProcedure)?,
                    }
                }
                place(f.sort_return)?;
                place(f.sort_control)?;
            }
            SortPlan::Table(t) => {
                place(t.first)?;
                count(&t.count)?;
                symbol(t.name)?;
            }
        }
    }
    for r in &p.services.releases {
        place(r.record)?;
        r.file.map_or(Ok(()), files)?;
        r.from.as_ref().map_or(Ok(()), |m| sender(&m.from, m.check).and_then(|()| place(m.to)).and_then(|()| moved(&m.plan)))?;
        place(r.sort_return)?;
        symbol(r.name)?;
    }
    for r in &p.services.returns {
        r.file.map_or(Ok(()), files)?;
        r.into.as_ref().map_or(Ok(()), |(q, m)| place(*q).and_then(|()| moved(m)))?;
        place(r.sort_return)?;
        symbol(r.name)?;
    }
    let writer = &p.services.report;
    let item = |k: usize| within("report item", u32::try_from(k).unwrap_or(u32::MAX), p.places.len());
    writer.print_switch.map_or(Ok(()), item)?;
    for r in &writer.reports {
        within("report file", u32::try_from(r.file).unwrap_or(u32::MAX), p.services.files.len())?;
        r.code.map_or(Ok(()), constant)?;
        r.controls.iter().try_for_each(|c| place(c.reference))?;
        [r.page_counter, r.line_counter, r.state].into_iter().try_for_each(item)?;
        r.sums.iter().try_for_each(|s| item(s.total))?;
        let group = |g: usize| within("report group", u32::try_from(g).unwrap_or(u32::MAX), r.groups.len());
        let sum = |s: usize| within("SUM", u32::try_from(s).unwrap_or(u32::MAX), r.sums.len());
        [r.report_heading, r.page_heading, r.page_footing, r.report_footing].into_iter().flatten().try_for_each(group)?;
        r.control_headings.iter().chain(&r.control_footings).flatten().try_for_each(|&g| group(g))?;
        let origin = |o: &Origin| match o {
            Origin::Source(c) => comparand(c),
            Origin::Value(v) => constant(*v),
            Origin::Total(t) => sum(*t),
        };
        for st in &r.subtotals {
            sum(st.sum)?;
            comparand(&st.operand)?;
        }
        for g in &r.groups {
            for f in g.lines.iter().flat_map(|l| &l.fields).chain(&g.unprinted) {
                item(f.item)?;
                match &f.content {
                    FieldContent::Source(c) => comparand(c)?,
                    FieldContent::Value(v) => constant(*v)?,
                    FieldContent::Sum(s) => sum(*s)?,
                    FieldContent::Program => {}
                }
            }
            for (s, o) in g.cross.iter().chain(&g.rolls) {
                sum(*s)?;
                origin(o)?;
            }
            g.totals.iter().try_for_each(|&s| sum(s))?;
            g.declarative.map_or(Ok(()), |d| range(d, RangeKind::UseBeforeReporting))?;
        }
    }
    let report = |op: &ReportOp| {
        let report = |ri: u32| match writer.reports.get(ri as usize) {
            Some(r) => Ok(r),
            None => Err(format!("report {ri} of {}", writer.reports.len())),
        };
        match *op {
            ReportOp::Initiate(ri) | ReportOp::Terminate(ri) => report(ri).map(drop),
            ReportOp::Generate { report: ri, detail } => {
                let r = report(ri)?;
                match detail.map(|d| r.groups.get(d as usize).map(|g| g.kind)) {
                    None | Some(Some(GroupKind::Detail)) => Ok(()),
                    Some(_) => Err(format!("GENERATE of report {ri}'s group {detail:?}, which is not a DETAIL group")),
                }
            }
            ReportOp::Suppress => Ok(()),
        }
    };
    for e in &p.services.entries {
        symbol(e.name)?;
        within("paragraph", e.paragraph, p.paragraphs.len())?;
        block(e.block)?;
        e.using.iter().try_for_each(|&r| within("LINKAGE record", u32::from(r), p.storage.linkage.len()))?;
    }
    for d in &p.plans.display {
        for item in &d.items {
            match item {
                DisplayItem::Bytes(q) | DisplayItem::National(q) | DisplayItem::Digits { place: q, .. } => place(*q)?,
                DisplayItem::Refused { place: q, abend: a } => {
                    place(*q)?;
                    abend(*a)?;
                }
                DisplayItem::Text(t) => within("symbol", *t, p.symbols.len())?,
                DisplayItem::Value(o) => operand(o)?,
            }
        }
    }
    for f in p.plans.init.iter().flat_map(|i| &i.fields) {
        match f.value {
            InitValue::Default(_) => {}
            InitValue::Value(c) => within("constant", c, p.consts.len())?,
            InitValue::Replacing(o) => operand(&o)?,
        }
        match f.store {
            MovePlan::Refused(a) | MovePlan::Numeric { store: StorePlan::Refused(a), .. } => abend(a)?,
            _ => {}
        }
    }

    if p.debug.ops.len() != p.blocks.len() {
        return Err(format!("debug entries for {} blocks of {}", p.debug.ops.len(), p.blocks.len()));
    }
    if p.debug.statements.len() != p.blocks.len() {
        return Err(format!("statement starts for {} blocks of {}", p.debug.statements.len(), p.blocks.len()));
    }
    for (b, (blk, starts)) in p.blocks.iter().zip(&p.debug.statements).enumerate() {
        if starts.windows(2).any(|w| w[0].0 > w[1].0) || starts.last().is_some_and(|&(k, _)| k as usize > blk.ops.len()) {
            return Err(format!("block {b}: statement starts out of order or past its {} ops and terminator", blk.ops.len()));
        }
        starts.iter().try_for_each(|&(_, id)| within("debug entry", id, p.debug.positions.len()))?;
    }
    for (b, (blk, ids)) in p.blocks.iter().zip(&p.debug.ops).enumerate() {
        if ids.len() != blk.ops.len() + 1 {
            return Err(format!("block {b}: {} debug entries for {} ops and a terminator", ids.len(), blk.ops.len()));
        }
        ids.iter().try_for_each(|&id| within("debug entry", id, p.debug.positions.len()))?;
        // The number of blocks the Select after an op has, 0 when the op returns no arm.
        let arms = |op: &Op| match op {
            Op::Arith(a) if p.plans.arith.get(*a as usize).is_some_and(|plan| plan.handled) => 2,
            Op::Call(c) if p.services.calls.get(*c as usize).is_some_and(|plan| plan.on_exception || plan.not_on_exception) => 2,
            Op::Invoke(i) if p.services.invokes.get(*i as usize).is_some_and(|plan| plan.on_exception || plan.not_on_exception) => 2,
            Op::File(f) => p.services.file_ops.get(*f as usize).map_or(0, |op| op.arms()),
            Op::String(_) | Op::Unstring(_) | Op::SearchAll(_) | Op::Return(_) | Op::Accept { from: AcceptFrom::ArgumentValue, .. } => 2,
            Op::Markup(m) if p.services.markup.get(*m as usize).is_some_and(|x| x.phrases() != (false, false)) => 2,
            Op::ScreenAccept { handled: true, .. } => 2,
            _ => 0,
        };
        let armed = |op: &Op| arms(op) > 0;
        let last = blk.ops.len().saturating_sub(1);
        if blk.ops.iter().enumerate().any(|(k, op)| armed(op) && (k != last || !matches!(blk.end, Terminator::Select(_)))) {
            return Err(format!("block {b}: an op that returns an arm is not followed by its Select"));
        }
        for op in &blk.ops {
            match op {
                Op::Move { from, to, check, .. } => {
                    sender(from, *check)?;
                    place(*to)?;
                }
                Op::Set { from, to, .. } => {
                    operand(from)?;
                    place(*to)?;
                }
                Op::Initialize { target, plan } => {
                    place(*target)?;
                    within("INITIALIZE plan", *plan, p.plans.init.len())?;
                }
                Op::Arith(a) => within("arithmetic plan", *a, p.plans.arith.len())?,
                Op::Step { var, by, prepass, .. } => {
                    place(*var)?;
                    expr(*by)?;
                    places(prepass)?;
                }
                Op::SetTemp(_, n) | Op::ArgumentNumber(n) => int(n)?,
                Op::SetCount(_, o) if o.check != ssrange => return Err(format!("block {b}: a SEARCH count's check that disagrees with SSRANGE")),
                Op::SetCount(_, o) => int(&o.object)?,
                Op::Display(d) => within("DISPLAY plan", *d, p.plans.display.len())?,
                Op::ScreenDisplay { display, screen } => {
                    within("DISPLAY plan", *display, p.plans.display.len())?;
                    screen_ints(screen).try_for_each(int)?;
                }
                Op::ScreenAccept { inputs, .. } => {
                    for i in inputs {
                        place(i.target)?;
                        place(i.field)?;
                        position_ints(&i.at).try_for_each(int)?;
                    }
                }
                Op::Call(c) => within("CALL plan", *c, p.services.calls.len())?,
                Op::Cancel(o) => operand(o)?,
                Op::Invoke(i) => within("INVOKE plan", *i, p.services.invokes.len())?,
                Op::Alter { para, to } => {
                    within("paragraph", *para, p.paragraphs.len())?;
                    within("paragraph", *to, p.paragraphs.len())?;
                }
                Op::File(f) => within("file statement", *f, p.services.file_ops.len())?,
                Op::Markup(m) => within("JSON or XML statement", *m, p.services.markup.len())?,
                Op::Cics(c) => within("EXEC CICS command", *c, p.services.cics.len())?,
                Op::Sort(s) => within("SORT or MERGE", *s, p.services.sorts.len())?,
                Op::Release(r) => within("RELEASE", *r, p.services.releases.len())?,
                Op::Return(r) => within("RETURN", *r, p.services.returns.len())?,
                Op::Report(op) => report(op)?,
                Op::Sql(k) if *k == 0 || *k as usize > p.sql.len() => return Err(format!("block {b}: EXEC SQL ordinal {k} of {}", p.sql.len())),
                Op::Sql(_) => {}
                Op::SetAddress { records, address } => {
                    records.iter().try_for_each(|&r| within("LINKAGE record", u32::from(r), p.storage.linkage.len()))?;
                    operand(address)?;
                }
                Op::SetEntry { entry, targets } => {
                    operand(entry)?;
                    places(targets)?;
                }
                Op::SetUpDown { by, targets, .. } => {
                    int(by)?;
                    for (q, how) in targets {
                        place(*q)?;
                        if let UpDown::Refused(a) = how {
                            abend(*a)?;
                        }
                    }
                }
                Op::String(id) => within("STRING plan", *id, p.plans.string.len())?,
                Op::Unstring(id) => within("UNSTRING plan", *id, p.plans.unstring.len())?,
                Op::Inspect(id) => within("INSPECT plan", *id, p.plans.inspect.len())?,
                Op::SearchAll(id) => within("SEARCH ALL plan", *id, p.plans.search_all.len())?,
                Op::SetInt { target, value } => {
                    place(*target)?;
                    int(value)?;
                }
                Op::Accept { target, plan, .. } => {
                    place(*target)?;
                    moved(plan)?;
                }
                Op::DebugAlter { range: r, name, contents } => {
                    range(*r, RangeKind::Debugging)?;
                    symbol(*name)?;
                    symbol(*contents)?;
                }
                Op::Nest | Op::Unnest(_) | Op::DecTemp(_) | Op::EnterSegment(_) | Op::DebugLine(_) => {}
            }
        }
        match &blk.end {
            Terminator::Jump(t) => block(*t)?,
            Terminator::Branch { cond: c, then, otherwise } => {
                cond(*c)?;
                block(*then)?;
                block(*otherwise)?;
            }
            Terminator::Select(targets) => {
                targets.iter().try_for_each(|&t| block(t))?;
                if blk.ops.last().map_or(0, arms) != targets.len() {
                    return Err(format!("block {b}: a Select that does not follow an op with its phrases"));
                }
            }
            Terminator::AlteredGoTo { para, otherwise } => {
                within("paragraph", *para, p.paragraphs.len())?;
                block(*otherwise)?;
            }
            Terminator::ParagraphEnd { next } => {
                if *next as usize > p.paragraphs.len() {
                    return Err(format!("block {b}: paragraph end to {next} of {}", p.paragraphs.len()));
                }
            }
            Terminator::GoTo(t) => within("paragraph", *t, p.paragraphs.len())?,
            Terminator::Switch { value, targets, otherwise } => {
                int(value)?;
                targets.iter().try_for_each(|&t| within("paragraph", t, p.paragraphs.len()))?;
                block(*otherwise)?;
            }
            Terminator::PerformEnter { range: r, ret, resume } => {
                range(*r, RangeKind::Perform)?;
                block(*ret)?;
                if let Some(resume) = resume {
                    within("paragraph", resume.para, p.paragraphs.len())?;
                    block(resume.block)?;
                }
            }
            Terminator::Debug { range: r, name, next } => {
                range(*r, RangeKind::Debugging)?;
                symbol(*name)?;
                block(*next)?;
            }
            Terminator::ExitProgram { next } => block(*next)?,
            Terminator::End(_) => {}
            Terminator::Abend(a) => abend(*a)?,
        }
    }
    for (k, para) in p.paragraphs.iter().enumerate() {
        block(para.entry)?;
        if (para.section_end as usize) < k || para.section_end as usize >= p.paragraphs.len() {
            return Err(format!("paragraph {k}: section end {}", para.section_end));
        }
        para.abandoned.map_or(Ok(()), abend)?;
        if para.abandoned.is_some() != p.ranges.iter().any(|r| r.last as usize == k) {
            return Err(format!("paragraph {k}: an abandoned return point's abend where no range ends, or none where one does"));
        }
    }
    for r in &p.ranges {
        within("paragraph", r.first, p.paragraphs.len())?;
        within("paragraph", r.last, p.paragraphs.len())?;
    }
    if p.procedure_start as usize > p.paragraphs.len() {
        return Err(format!("procedure start {} of {} paragraphs", p.procedure_start, p.paragraphs.len()));
    }
    Ok(())
}

/// Every record, file, symbol and range the EXTERNAL and GLOBAL tables name, and a GLOBAL record
/// within its storage.
fn verify_scope(p: &Program, range: &dyn Fn(u32, RangeKind) -> Result<(), String>) -> Result<(), String> {
    let scope = &p.services.scope;
    let within = |what: &str, id: usize, len: usize| if id < len { Ok(()) } else { Err(format!("{what} {id} of {len}")) };
    let symbol = |id: SymId| within("symbol", id as usize, p.symbols.len());
    let record = |o: u16| within("LINKAGE record", usize::from(o), p.storage.linkage.len());
    let file = |k: u16| within("file", usize::from(k), p.services.files.len());
    scope.containers.iter().chain(&scope.callable).chain(&scope.hidden).try_for_each(|&s| symbol(s))?;
    let mut last = None;
    for &(o, ref binding) in &scope.records {
        record(o)?;
        if last.is_some_and(|l| l >= o) {
            return Err(format!("LINKAGE record {o} bound out of order"));
        }
        last = Some(o);
        match *binding {
            Binding::External { name, .. } => symbol(name)?,
            Binding::ExternalFile(k) => file(k)?,
            Binding::Global { program, name, .. } => symbol(program).and_then(|()| symbol(name))?,
        }
    }
    for f in &scope.files {
        file(f.file)?;
        f.declared_in.map_or(Ok(()), symbol)?;
        if !f.external && f.declared_in.is_none() {
            return Err(format!("file {} shared as neither EXTERNAL nor GLOBAL", f.file));
        }
    }
    for &(k, o) in &scope.areas {
        file(k)?;
        record(o)?;
    }
    for g in &scope.globals {
        symbol(g.name)?;
        match g.at {
            GlobalAt::Program(offset) => within("GLOBAL record offset", offset as usize, p.storage.image.len())?,
            GlobalAt::Local(offset) => within("GLOBAL LOCAL-STORAGE offset", offset as usize, p.storage.local_image.len())?,
            GlobalAt::Linkage(o) => record(o)?,
        }
    }
    for &(k, r) in &scope.global_files {
        file(k)?;
        range(r, RangeKind::UseProcedure)?;
    }
    scope.global_modes.iter().flatten().try_for_each(|&r| range(r, RangeKind::UseProcedure))
}

/// The integers a screen phrase evaluates.
fn screen_ints(screen: &rt::lir::ScreenPlan) -> impl Iterator<Item = &IntExpr> {
    position_ints(&screen.at)
}

/// The integers a screen position evaluates.
fn position_ints(at: &rt::lir::ScreenPosition) -> impl Iterator<Item = &IntExpr> {
    let (a, b, c) = match at {
        rt::lir::ScreenPosition::Cursor => (None, None, None),
        rt::lir::ScreenPosition::Combined(at) => (Some(at), None, None),
        rt::lir::ScreenPosition::LineColumn { line, column } => (None, line.as_ref(), column.as_ref()),
    };
    a.into_iter().chain(b).chain(c)
}
