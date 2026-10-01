//! A FUNCTION's arguments, evaluated here; the functions themselves are `rt::intrinsic`'s.

use super::*;
use super::facts::Facts;
use rt::intrinsic::function::{self as intrinsic_function, Evaluator};
use rt::intrinsic;

/// A FUNCTION's arguments as written, which CHAR, NATIONAL-OF, INTEGER-OF-DATE, DATE-OF-INTEGER
/// and RANDOM evaluate again, and the run unit's clock and RANDOM state.
pub(super) struct Call<'m, 'p, 'u, 'w, 'f> {
    pub(super) machine: &'m mut Machine<'p, 'u, 'w>,
    pub(super) f: &'f FunctionCall,
}

impl<'p> Evaluator for Call<'_, 'p, '_, '_, '_> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.machine.facts()
    }

    fn integer(&mut self, k: usize, pos: Pos) -> R<i64> {
        self.machine.integer(&self.f.args[k], pos)
    }

    fn written(&self) -> usize {
        self.f.args.len()
    }

    fn now(&self) -> (i64, u32) {
        self.machine.unit.now()
    }

    fn compiled(&self) -> (i64, u32) {
        (self.machine.when_compiled.seconds, self.machine.when_compiled.hundredths)
    }

    fn random(&mut self) -> &mut Option<u32> {
        &mut self.machine.unit.random
    }

    fn currency(&self) -> String {
        self.machine.default_currency()
    }
}

impl<'p> Machine<'p, '_, '_> {
    /// Whether an expression holding `f` is evaluated in floating point.
    pub(super) fn is_floating_point(&mut self, f: &FunctionCall) -> R<bool> {
        if intrinsic::FLOATING_POINT.contains(&f.name.as_str()) {
            return Ok(true);
        }
        if intrinsic::MIXED.contains(&f.name.as_str()) {
            for a in &f.args {
                if self.uses_float(a)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The arguments' values; a table written with ALL subscripts gives one per element
    /// (Language Reference SC27-8713-03, pp. 501-502).
    pub(super) fn function_arguments(&mut self, f: &FunctionCall) -> R<Vec<Val>> {
        let mut out = Vec::with_capacity(f.args.len());
        for (i, a) in f.args.iter().enumerate() {
            match (f.all_subscripts.iter().find(|(k, _)| *k == i), a) {
                (Some((_, positions)), Expr::Operand(Operand::Ref(table))) => {
                    for element in self.all_elements(table, positions)? {
                        out.push(self.operand(&Operand::Ref(element), f.pos)?);
                    }
                }
                _ => out.push(self.expr_value(a, f.pos)?),
            }
        }
        Ok(out)
    }

    /// The elements ALL subscripts name, the rightmost ALL varying fastest; a dimension with
    /// OCCURS DEPENDING ON runs to its object's current value.
    fn all_elements(&mut self, table: &Ref, positions: &[usize]) -> R<Vec<Ref>> {
        let Resolved::Item(index) = self.resolve(table)? else {
            return Err(Abend::ironwork(format!("{} is a condition-name, not a table", table.name), table.pos));
        };
        let layout = self.layout;
        let mut dimensions = Vec::new();
        let mut at = Some(index);
        while let Some(i) = at {
            if layout.items[i].table {
                dimensions.push(i);
            }
            at = layout.items[i].parent;
        }
        dimensions.reverse();
        if dimensions.len() != table.subscripts.len() {
            return Err(Abend::ironwork(format!("{} takes {} subscripts, not {}", table.name, dimensions.len(), table.subscripts.len()), table.pos));
        }
        let mut counts = Vec::with_capacity(positions.len());
        for &p in positions {
            counts.push(self.occurrences(dimensions[p], table.pos)?);
        }
        let mut elements = Vec::new();
        if counts.contains(&0) {
            return Ok(elements);
        }
        let mut current = vec![1u32; positions.len()];
        loop {
            let mut element = table.clone();
            for (k, &p) in positions.iter().enumerate() {
                element.subscripts[p] = Expr::Operand(Operand::Literal(Literal::Number(current[k].to_string())));
            }
            elements.push(element);
            let mut k = positions.len();
            loop {
                if k == 0 {
                    return Ok(elements);
                }
                k -= 1;
                if current[k] < counts[k] {
                    current[k] += 1;
                    break;
                }
                current[k] = 1;
            }
        }
    }

    /// HEX-OF, BIT-OF and BYTE-LENGTH, which read an argument's bytes as stored, so that invalid
    /// data in a numeric item is shown rather than ending the run.
    pub(super) fn storage_function(&mut self, f: &FunctionCall) -> R<Option<Val>> {
        if !matches!(f.name.as_str(), "HEX-OF" | "BIT-OF" | "BYTE-LENGTH") {
            return Ok(None);
        }
        if f.args.len() != 1 {
            return Err(Abend::ironwork(format!("FUNCTION {} takes one argument", f.name), f.pos));
        }
        let bytes = self.stored_bytes(&f.args[0], f.pos)?;
        intrinsic_function::storage(&self.facts(), &f.name, &bytes, f.pos).map(Some)
    }

    /// An argument's bytes: an item's storage, or a value as DISPLAY would hold it.
    fn stored_bytes(&mut self, e: &Expr, pos: Pos) -> R<Vec<u8>> {
        if let Expr::Operand(Operand::Ref(r)) = e {
            let loc = self.locate(r)?;
            return Ok(self.bytes(loc).to_vec());
        }
        let val = self.expr_value(e, pos)?;
        intrinsic_function::stored_bytes(&self.facts(), val, pos)
    }
}
