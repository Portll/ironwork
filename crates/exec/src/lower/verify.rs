//! The checks of lir.md §12.1 a lowered program must pass: every id names an entry of its table,
//! the control-flow graph is closed, every op has its debug entry, and places carry SSRANGE checks
//! exactly when the program has SSRANGE.

use rt::lir::{Comparand, Cond, DisplayItem, Expr, IntExpr, Op, Operand, Place, Program, Terminator};

pub fn verify(p: &Program) -> Result<(), String> {
    let within = |what: &str, id: u32, len: usize| if (id as usize) < len { Ok(()) } else { Err(format!("{what} {id} of {len}")) };
    let block = |id| within("block", id, p.blocks.len());
    let place = |id| within("place", id, p.places.len());
    let expr = |id| within("expression", id, p.exprs.len());
    let cond = |id| within("condition", id, p.conds.len());
    let abend = |id| within("abend", id, p.abends.len());
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
            Cond::InTable { index, .. } => place(*index)?,
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
                Op::Nest | Op::Unnest(_) | Op::DecTemp(_) => {}
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
            Terminator::Select(arms) => {
                arms.iter().try_for_each(|&t| block(t))?;
                let handled = match blk.ops.last() {
                    Some(Op::Arith(a)) => p.plans.arith.get(*a as usize).is_some_and(|plan| plan.handled),
                    _ => false,
                };
                if !handled || arms.len() != 2 {
                    return Err(format!("block {b}: a Select that does not follow an arithmetic op with SIZE ERROR"));
                }
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
            Terminator::PerformEnter { range, ret } => {
                within("range", *range, p.ranges.len())?;
                block(*ret)?;
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
