//! The checks of lir.md §12.1 a lowered program must pass: every id names an entry of its table,
//! the control-flow graph is closed, every op has its debug entry, and places carry SSRANGE checks
//! exactly when the program has SSRANGE.

use rt::lir::{
    Advance, Bound, CallArg, CallTarget, Chars, Comparand, Compare, Cond, ConvertTable, Count, DisplayItem, Expr, FileVerb, IntExpr, MethodName, MovePlan, Op, Operand,
    Place, Program, RangeKind, Receiver, Replacement, StartKey, StorePlan, Terminator, UpDown,
};

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
        IntExpr::Const(_) => Ok(()),
        IntExpr::Item(q) => place(*q),
        IntExpr::Fixed { expr: e, prepass, .. } => expr(*e).and_then(|()| places(prepass)),
    };
    let operand = |o: &Operand| match *o {
        Operand::Load(q) | Operand::LengthOf(q) | Operand::AddressOf(q) => place(q),
        Operand::Const(c) => within("constant", c, p.consts.len()),
        Operand::Function(f) => within("function plan", f, p.plans.function.len()),
    };
    let comparand = |c: &Comparand| match c {
        Comparand::Operand(o) => operand(o),
        Comparand::Expr { expr: e, prepass, .. } => expr(*e).and_then(|()| places(prepass)),
    };
    let ssrange = p.options.ssrange;

    for (k, q) in p.places.iter().enumerate() {
        let Place { subscripts, odo, refmod, at, .. } = q;
        within("debug entry", *at, p.debug.positions.len())?;
        for s in subscripts {
            int(&s.value)?;
            if s.check.is_some() != ssrange {
                return Err(format!("place {k}: a subscript check without SSRANGE, or none with it"));
            }
        }
        if let Some(o) = odo {
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
    for f in &p.plans.function {
        f.args.iter().try_for_each(comparand)?;
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
                from.map_or(Ok(()), |m| operand(&m.from).and_then(|()| place(m.to)))?;
                if let Some(Advance::Lines { count, .. }) = advancing {
                    int(count)?;
                }
            }
            FileVerb::Rewrite { record, from } => {
                place(*record)?;
                from.map_or(Ok(()), |m| operand(&m.from).and_then(|()| place(m.to)))?;
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
        place(plan.target)?;
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
            CallTarget::Pointer(q) => place(*q)?,
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

    if p.debug.ops.len() != p.blocks.len() {
        return Err(format!("debug entries for {} blocks of {}", p.debug.ops.len(), p.blocks.len()));
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
            Op::String(_) | Op::Unstring(_) | Op::SearchAll(_) => 2,
            _ => 0,
        };
        let armed = |op: &Op| arms(op) > 0;
        let last = blk.ops.len().saturating_sub(1);
        if blk.ops.iter().enumerate().any(|(k, op)| armed(op) && (k != last || !matches!(blk.end, Terminator::Select(_)))) {
            return Err(format!("block {b}: an op that returns an arm is not followed by its Select"));
        }
        for op in &blk.ops {
            match op {
                Op::Move { from, to, .. } => {
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
                Op::SetTemp(_, n) => int(n)?,
                Op::Display(d) => within("DISPLAY plan", *d, p.plans.display.len())?,
                Op::Call(c) => within("CALL plan", *c, p.services.calls.len())?,
                Op::Cancel(o) => operand(o)?,
                Op::Invoke(i) => within("INVOKE plan", *i, p.services.invokes.len())?,
                Op::Alter { para, to } => {
                    within("paragraph", *para, p.paragraphs.len())?;
                    within("paragraph", *to, p.paragraphs.len())?;
                }
                Op::File(f) => within("file statement", *f, p.services.file_ops.len())?,
                Op::SetAddress { records, address } => {
                    records.iter().try_for_each(|&r| within("LINKAGE record", u32::from(r), p.storage.linkage.len()))?;
                    operand(address)?;
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
                other => return Err(format!("block {b}: {other:?} is outside this slice")),
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
