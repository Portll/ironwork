//! DISPLAY (lir.md §9.1): each item shown as its kind or value shows, then the line written to
//! standard output.

use crate::abend::{Abend, AbendCode, Signal};
use crate::fixed::{pow10, zoned_digits};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{Pos, SignClause, SignPosition};
use numeric::{DispSign, Trunc};
use std::io::Write;
use zarch::decimal;

type R<T> = Result<T, Abend>;

/// A data item, by its kind: packed and binary items as their last digits, COMP-5 and TRUNC(BIN)
/// binary items as every digit their halfword, fullword or doubleword holds, a negative value's
/// sign overpunched on the last. Under DISPSIGN(SEP) a signed binary, packed or overpunched zoned
/// item shows its sign, + or -, before its digits (Programming Guide SC27-8714-03, pp. 362-363,
/// Table 48; assumption C213).
pub fn place(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos) -> R<String> {
    let separate = facts.options().dispsign == DispSign::Sep;
    Ok(match loc.kind {
        Kind::National => utf16_text(store::bytes(mem, loc)),
        Kind::Packed { digits, signed, .. } | Kind::Binary { digits, signed, .. } => {
            let Val::Num(f) = store::read_stored(facts, mem, loc, pos)? else { unreachable!() };
            let zone = if signed && !separate && f.negative { decimal::MINUS } else { decimal::UNSIGNED };
            let whole = match loc.kind {
                Kind::Binary { native, .. } => native || facts.options().trunc == Trunc::Bin,
                _ => false,
            };
            let shown = if whole {
                let width = match loc.len {
                    2 => 5,
                    4 => 10,
                    _ if signed => 19,
                    _ => 20,
                };
                zoned_digits(f.magnitude.to_u128().unwrap_or(0), width, zone)
            } else {
                zoned_digits(f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0), digits as usize, zone)
            };
            facts.page().decode(&if signed && separate { sign_first(f.negative, shown) } else { shown })
        }
        Kind::Zoned { signed: true, sign, .. } if separate && !sign.is_some_and(|s| s.separate) => {
            let mut shown = store::bytes(mem, loc).to_vec();
            let at = if sign == Some(SignClause { position: SignPosition::Leading, separate: false }) { 0 } else { shown.len() - 1 };
            let negative = matches!(shown[at] >> 4, 0xB | 0xD);
            shown[at] |= 0xF0;
            facts.page().decode(&sign_first(negative, shown))
        }
        Kind::Float(_) => return Err(Abend::ironwork("DISPLAY of a floating-point item is not supported yet", pos)),
        Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => {
            return Err(Abend::ironwork("DISPLAY of a pointer, index or object reference is not supported", pos));
        }
        _ => facts.page().decode(store::bytes(mem, loc)),
    })
}

/// Zoned digits after a separate sign, as DISPSIGN(SEP) shows a signed item.
fn sign_first(negative: bool, digits: Vec<u8>) -> Vec<u8> {
    let mut shown = vec![if negative { 0x60 } else { 0x4E }];
    shown.extend(digits);
    shown
}

/// A numeric literal as written, its decimal point the program's.
pub fn number(written: &str, facts: &dyn ProgramFacts) -> String {
    written.replace('.', &facts.decimal_point().to_string())
}

/// A literal, figurative constant, FUNCTION, LENGTH OF or ADDRESS OF, by its value.
pub fn value(facts: &dyn ProgramFacts, val: Val, pos: Pos) -> R<String> {
    Ok(match val {
        Val::Bytes(b) | Val::All(b) => facts.page().decode(&b),
        Val::National(b) | Val::AllNational(b) => utf16_text(&b),
        Val::Fig(f) => facts.page().decode_byte(facts.figurative(f)).to_string(),
        Val::Num(f) => facts.page().decode(&zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED)),
        Val::Float(_) => return Err(Abend::ironwork("DISPLAY of a floating-point value is not supported yet", pos)),
        Val::Address(_) => return Err(Abend::ironwork("DISPLAY of a pointer is not supported", pos)),
    })
}

/// The line, and a newline unless NO ADVANCING.
pub fn write(out: &mut dyn Write, text: &str, no_advancing: bool, pos: Pos) -> R<()> {
    let result = if no_advancing { write!(out, "{text}") } else { writeln!(out, "{text}") };
    result.map_err(|e| match e.kind() {
        std::io::ErrorKind::BrokenPipe => Abend { code: AbendCode::Signal(Signal::ClosedOutput), message: "standard output closed".into(), pos, file: None },
        _ => Abend::ironwork(format!("DISPLAY: {e}"), pos),
    })
}

/// Big-endian UTF-16, an unpaired surrogate shown as U+FFFD.
pub fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}
