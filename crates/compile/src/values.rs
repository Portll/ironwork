//! VALUE clauses: a program's storage as they leave it, applied in a run unit or built as the
//! initial image a lowered program carries.

use crate::Compiled;
use crate::facts::Facts;
use crate::layout::Kind;
use rt::abend::Abend;
use rt::files::Dds;
use rt::loc;
use rt::oo::ClassCode;
use rt::storage::{Loc, Val, literal_fixed};
use rt::store;
use rt::unit::{Clock, FoundClass, LoadError, LoadedProgram, Loader, RunUnit};
use std::rc::Rc;
use syntax::Pos;
use syntax::ast::Literal;
use zarch::ebcdic::CodePage;

type R<T> = Result<T, Abend>;

/// Applies VALUE clauses over storage at `base`: to WORKING-STORAGE and file records, or to
/// LOCAL-STORAGE.
pub fn initialize<H, L: Loader<H>>(c: &Compiled, unit: &mut RunUnit<'_, H, L>, base: usize, local: bool) -> R<()> {
    let facts = Facts::of(c);
    for (index, item) in c.layout.items.iter().enumerate() {
        let Some(value) = item.value.as_ref().filter(|_| item.linkage.is_none() && item.local == local) else { continue };
        let occurrences: u32 = item.dims.iter().map(|&(_, n)| n).product::<u32>().max(1);
        let kind = value_kind(item.kind, value);
        for k in 0..occurrences {
            let offset = base + item.offset as usize + loc::occurrence_offset(&item.dims, k);
            let loc = Loc { offset, len: item.size as usize, kind, item: index };
            let val = literal_value(facts.page, value, item.pos)?;
            store::assign(&facts, unit, loc, val, None, item.pos)?;
        }
    }
    Ok(())
}

/// A literal's value, its text in the program's code page.
pub fn literal_value(page: &CodePage, lit: &Literal, pos: Pos) -> R<Val> {
    Ok(match lit {
        Literal::Alnum(s) => Val::Bytes(page.encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?),
        Literal::Hex(b) => Val::Bytes(b.clone()),
        Literal::National(s) => Val::National(s.encode_utf16().flat_map(u16::to_be_bytes).collect()),
        Literal::Dbcs(s) => Val::Dbcs(store::dbcs_literal(page, s).map_err(|m| Abend::ironwork(m, pos))?),
        Literal::Number(t) => Val::Num(literal_fixed(t).ok_or_else(|| Abend::ironwork(format!("the literal {t} has more than 31 digits"), pos))?),
        Literal::Figurative(f) => Val::Fig(*f),
        Literal::All(inner) => match literal_value(page, inner, pos)? {
            Val::Bytes(b) | Val::Dbcs(b) => Val::All(b),
            Val::National(n) => Val::AllNational(n),
            Val::Fig(f) => Val::Fig(f),
            _ => return Err(Abend::ironwork("ALL takes an alphanumeric or national literal", pos)),
        },
    })
}

/// The kind a VALUE clause's literal is placed as: editing is ignored, so an alphanumeric VALUE fills
/// a numeric-edited or alphanumeric-edited item as alphanumeric data (Language Reference p. 246).
pub fn value_kind(kind: Kind, value: &Literal) -> Kind {
    match (kind, value) {
        (Kind::NumericEdited { .. } | Kind::AlnumEdited { .. }, Literal::Alnum(_) | Literal::Figurative(_) | Literal::All(_)) => Kind::Alnum { justified: false },
        (kind, _) => kind,
    }
}

/// A program's storage and LOCAL-STORAGE as its VALUE clauses leave them on a first activation,
/// the lines they reported, and the abend that stopped them, if any.
pub struct Initial {
    pub image: Vec<u8>,
    pub local_image: Vec<u8>,
    pub reports: Vec<String>,
    pub abend: Option<Abend>,
}

/// The storage a first activation of `c` starts with, built in a run unit of its own.
pub fn initial(c: &Compiled) -> Initial {
    let (size, local) = (c.layout.size as usize, c.layout.local_size as usize);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (image, local_image, abend) = {
        let mut unit = RunUnit::new(Scratch, Dds::default(), None, Clock::Fixed(0, 0), &mut out, &mut err);
        let me = unit.add_named(None, c.program.id.to_ascii_uppercase(), c.program.files.len(), size);
        let base = unit.programs[me].base;
        let abend = first_activation(c, &mut unit, base, local).err();
        let image = unit.mem[base..base + size].to_vec();
        let local_image = if local > 0 { unit.mem[unit.mem.len() - local..].to_vec() } else { Vec::new() };
        (image, local_image, abend)
    };
    let reports = String::from_utf8_lossy(&err).lines().map(str::to_owned).collect();
    Initial { image, local_image, reports, abend }
}

/// LOCAL-STORAGE's VALUEs, in storage pushed after the program's, then the program's own, as an
/// activation applies them.
fn first_activation(c: &Compiled, unit: &mut RunUnit<'_, (), Scratch>, base: usize, local: usize) -> R<()> {
    if local > 0 {
        let at = unit.push_temporary(&vec![0; local]);
        initialize(c, unit, at, true)?;
    }
    initialize(c, unit, base, false)
}

/// The loader of a run unit no CALL runs in: it finds nothing.
struct Scratch;

impl Loader<()> for Scratch {
    fn program(&mut self, _: &str) -> Result<LoadedProgram<()>, LoadError> {
        Err(LoadError::NotFound)
    }

    fn holder(&self, _: &str) -> Option<String> {
        None
    }

    fn entry(_: &(), _: &str) -> Option<usize> {
        None
    }

    fn shape(_: &()) -> (usize, usize) {
        (0, 0)
    }

    fn nested(_: &()) -> &[String] {
        &[]
    }

    fn facts(_: &()) -> numeric::governs::Facts {
        numeric::governs::Facts::default()
    }

    fn source(_: &(), _: usize) -> Option<String> {
        None
    }

    fn class(&mut self, _: &str) -> Result<Option<FoundClass<Rc<ClassCode<()>>>>, String> {
        Ok(None)
    }

    fn mapset(&mut self, _: &str) -> Option<Result<rt::bms::Mapset, String>> {
        None
    }
}
