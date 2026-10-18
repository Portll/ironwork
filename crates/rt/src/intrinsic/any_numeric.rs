//! What an ANY NUMERIC parameter is, read from the field its CALL passed: GnuCOBOL's descriptor
//! gives the callee the argument's class, digits, scale, sign and usage (assumption C510).

use crate::abend::Abend;
use crate::display;
use crate::lir::ByteClass;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::Pos;
use numeric::precision::{Fixed, Places};

type R<T> = Result<T, Abend>;

/// The function `name` of the argument `field` stands for: its value, 1 or 0 for whether the class
/// test NUMERIC holds, or the text DISPLAY shows.
pub fn evaluate(facts: &dyn ProgramFacts, name: &str, field: Option<(Kind, Vec<u8>)>, pos: Pos) -> R<Val> {
    let Some((kind, bytes)) = field else {
        return Err(refusal("the argument was omitted, not passed, or passed BY VALUE", pos));
    };
    let loc = Loc { offset: 0, len: bytes.len(), kind, item: 0 };
    match name {
        "ARGUMENT NUMERIC CLASS" => Ok(Val::Num(Fixed::new(i128::from(numeric(facts, &bytes, loc)), Places::new(1, 0)))),
        "ARGUMENT NUMERIC DISPLAY" => {
            let text = display::place(facts, &bytes, loc, pos, false)?;
            Ok(Val::Bytes(facts.page().encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?))
        }
        _ => value(facts, &bytes, loc, pos),
    }
}

/// Whether the class test NUMERIC holds for the argument: its digits and sign are valid for a
/// numeric usage, every byte of an alphanumeric one or a group is a digit, and no edited or
/// floating-point item is numeric, as cobc's class test finds.
fn numeric(facts: &dyn ProgramFacts, bytes: &[u8], loc: Loc) -> bool {
    let test = match loc.kind {
        Kind::Packed { signed, .. } => ByteClass::Packed { signed },
        Kind::Zoned { signed, sign: None, .. } => ByteClass::Zoned { signed },
        Kind::Binary { digits, native: numeric::Native::Comp6, .. } => ByteClass::Comp6 { digits },
        Kind::Zoned { .. } | Kind::Binary { .. } | Kind::Index => return true,
        Kind::Group | Kind::Alnum { .. } => ByteClass::Digits,
        _ => return false,
    };
    store::byte_class(facts, bytes, loc, test)
}

fn value(facts: &dyn ProgramFacts, bytes: &[u8], loc: Loc, pos: Pos) -> R<Val> {
    let why = match loc.kind {
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Index => return store::read_stored(facts, bytes, loc, pos),
        Kind::Alnum { .. } if bytes.iter().all(|b| (0xF0..=0xF9).contains(b)) && !bytes.is_empty() && bytes.len() <= 31 => {
            let digits = facts.page().decode(bytes);
            return Ok(Val::Num(Fixed::new(digits.parse().unwrap_or(0), Places::new(bytes.len() as u32, 0))));
        }
        Kind::Group | Kind::Alnum { .. } => "alphanumeric",
        Kind::NumericEdited { .. } => "numeric edited",
        Kind::Float(_) => "floating-point",
        _ => "not numeric",
    };
    Err(refusal(&format!("the argument is {why}"), pos))
}

fn refusal(why: &str, pos: Pos) -> Abend {
    crate::refusal::IWR0096.abend(format_args!("reading ANY NUMERIC: {why}"), pos)
}
