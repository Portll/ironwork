//! Signs of packed and zoned items under NUMPROC. Arithmetic results take the preferred sign under
//! either setting; the settings differ in whether a sign is repaired on the way through a MOVE and
//! whether a comparison is algebraic. See [`crate::assumptions::PFD_MOVES_BYTES`] and
//! [`crate::assumptions::PFD_COMPARES_LOGICALLY`].

use crate::options::Numproc;
use std::cmp::Ordering;
use zarch::check::ProgramCheck;
use zarch::decimal::{self, MINUS, PLUS, UNSIGNED};

pub const fn preferred(negative: bool, signed: bool) -> u8 {
    match (signed, negative) {
        (false, _) => UNSIGNED,
        (true, true) => MINUS,
        (true, false) => PLUS,
    }
}

/// A MOVE between packed items of the same length and scale.
pub fn move_packed(source: &[u8], receiver_signed: bool, numproc: Numproc) -> Vec<u8> {
    let mut out = source.to_vec();
    if numproc == Numproc::Nopfd {
        let last = out.last_mut().expect("a packed item is at least one byte");
        let negative = decimal::is_minus(*last & 0xF);
        *last = (*last & 0xF0) | preferred(negative, receiver_signed);
    }
    out
}

/// A comparison between packed items of the same length and scale.
pub fn compare_packed(a: &[u8], b: &[u8], numproc: Numproc) -> Result<Ordering, ProgramCheck> {
    match numproc {
        Numproc::Nopfd => Ok(match decimal::cp(a, b)?.0 {
            0 => Ordering::Equal,
            1 => Ordering::Less,
            _ => Ordering::Greater,
        }),
        Numproc::Pfd => Ok(a.cmp(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nopfd_repairs_the_sign_on_a_move_and_pfd_copies_it() {
        assert_eq!(move_packed(&[0x12, 0x3A], true, Numproc::Nopfd), [0x12, 0x3C]);
        assert_eq!(move_packed(&[0x12, 0x3B], true, Numproc::Nopfd), [0x12, 0x3D]);
        assert_eq!(move_packed(&[0x12, 0x3C], false, Numproc::Nopfd), [0x12, 0x3F]);
        assert_eq!(move_packed(&[0x12, 0x3A], true, Numproc::Pfd), [0x12, 0x3A]);
    }

    #[test]
    fn plus_signs_f_and_c_are_equal_only_under_nopfd() {
        assert_eq!(compare_packed(&[0x1F], &[0x1C], Numproc::Nopfd), Ok(Ordering::Equal));
        assert_eq!(compare_packed(&[0x1F], &[0x1C], Numproc::Pfd), Ok(Ordering::Greater));
    }

    #[test]
    fn nopfd_comparison_of_an_invalid_sign_is_a_data_exception() {
        assert_eq!(compare_packed(&[0x14], &[0x1C], Numproc::Nopfd), Err(ProgramCheck::Data));
    }
}
