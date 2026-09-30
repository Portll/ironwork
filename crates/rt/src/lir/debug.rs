//! The debug table (lir.md §10), with positions encoded as differences (load-module.md §9).

use super::{DebugId, SymId};
use crate::module::ModuleError;
use crate::module::codec::{Decode, Encode, Reader, Writer};
use crate::vocab::Pos;

/// `positions` maps a DebugId to its position; `ops` holds, per block, one per op and one for the terminator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Debug {
    pub sources: Vec<SymId>,
    pub positions: Vec<Pos>,
    pub ops: Vec<Vec<DebugId>>,
}

impl Encode for Debug {
    fn encode(&self, w: &mut Writer) {
        self.sources.encode(w);
        w.count(self.positions.len());
        let mut last = Pos::default();
        for &pos in &self.positions {
            w.zigzag(i64::from(pos.file) - i64::from(last.file));
            w.zigzag(i64::from(pos.line) - i64::from(last.line));
            w.zigzag(i64::from(pos.col) - i64::from(last.col));
            last = pos;
        }
        self.ops.encode(w);
    }
}

impl Decode for Debug {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let sources = Vec::decode(r)?;
        let count = r.count()?;
        let mut positions = Vec::with_capacity(count);
        let mut last = Pos::default();
        for _ in 0..count {
            let pos = Pos {
                file: moved(r, "file", last.file)?,
                line: moved(r, "line", last.line)?,
                col: moved(r, "column", last.col)?,
            };
            positions.push(pos);
            last = pos;
        }
        let ops = Vec::decode(r)?;
        Ok(Self { sources, positions, ops })
    }
}

fn moved<T: Into<i64> + TryFrom<i64>>(r: &mut Reader<'_>, what: &str, from: T) -> Result<T, ModuleError> {
    let at = r.position();
    let delta = r.zigzag()?;
    let value = from.into().checked_add(delta);
    value
        .and_then(|v| T::try_from(v).ok())
        .ok_or_else(|| r.malformed(at, format!("a {what} of {}", value.map_or_else(|| "over 64 bits".to_owned(), |v| v.to_string()))))
}
