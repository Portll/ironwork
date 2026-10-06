use crate::options::{Options, Trunc, TruncCheck};
use zarch::wide::U256;

/// Whether a binary item's value is limited by its bytes rather than its PICTURE, and how many
/// bytes it has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Native {
    /// USAGE BINARY, COMP or COMP-4: two, four or eight bytes as the PICTURE's digits need, the
    /// value truncated as TRUNC says.
    #[default]
    No,
    /// COMP-5: the same sizes, never truncated to the PICTURE.
    Comp5,
    /// GnuCOBOL's and Micro Focus's BINARY-CHAR under `--compliance extended`: one byte, never
    /// truncated to its three digits.
    BinaryChar,
}

impl Native {
    pub const fn is_native(self) -> bool {
        !matches!(self, Native::No)
    }
}

/// A USAGE BINARY, COMP or COMP-4 item, COMP-5 or BINARY-CHAR as `native` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binary {
    pub digits: u8,
    pub signed: bool,
    pub native: Native,
}

impl Binary {
    pub const fn bytes(self) -> usize {
        match (self.native, self.digits) {
            (Native::BinaryChar, _) => 1,
            (_, 0..=4) => 2,
            (_, 5..=9) => 4,
            _ => 8,
        }
    }

    pub fn load(self, bytes: &[u8]) -> i128 {
        assert_eq!(bytes.len(), self.bytes());
        let raw = bytes.iter().fold(0u128, |acc, &b| acc << 8 | b as u128);
        let bits = 8 * bytes.len() as u32;
        if self.signed && raw >> (bits - 1) == 1 { raw as i128 - (1i128 << bits) } else { raw as i128 }
    }

    fn decimal_truncation(self, value: i128) -> i128 {
        let cap = if self.digits <= 38 { U256::pow10(u32::from(self.digits)).lo } else { 10u128.pow(u32::from(self.digits)) };
        let magnitude = value.unsigned_abs();
        let kept = (if magnitude < cap { magnitude } else { magnitude % cap }) as i128;
        if value < 0 { -kept } else { kept }
    }

    fn binary_truncation(self, value: i128) -> i128 {
        let bits = 8 * self.bytes() as u32;
        let raw = (value as u128) & ((1u128 << bits) - 1);
        if self.signed && raw >> (bits - 1) == 1 { raw as i128 - (1i128 << bits) } else { raw as i128 }
    }
}

/// A TRUNC(OPT) store whose value exceeds the PICTURE, where decimal and binary truncation give
/// different results and the program depends on which one the generated code happens to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TruncOptDivergence {
    pub value: i128,
    pub decimal: i128,
    pub binary: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub bytes: Vec<u8>,
    pub value: i128,
    pub divergence: Option<TruncOptDivergence>,
}

/// Stores `value`, already scaled to the item's decimal places. An unsigned item takes the absolute
/// value. See [`crate::assumptions::TRUNC_OPT_IS_BINARY`] for what TRUNC(OPT) keeps.
pub fn store(item: Binary, value: i128, options: &Options) -> Stored {
    let (kept, divergence) = kept(item, value, options);
    Stored { bytes: kept.to_be_bytes()[16 - item.bytes()..].to_vec(), value: kept, divergence }
}

/// The value `store` keeps, whose low-order `item.bytes()` bytes it stores, and its divergence.
pub fn kept(item: Binary, value: i128, options: &Options) -> (i128, Option<TruncOptDivergence>) {
    let value = if item.signed { value } else { value.unsigned_abs() as i128 };
    let decimal = item.decimal_truncation(value);
    let binary = item.binary_truncation(value);
    match (item.native.is_native(), options.trunc) {
        (true, _) | (false, Trunc::Bin) => (binary, None),
        (false, Trunc::Std) => (decimal, None),
        (false, Trunc::Opt) => {
            let report = decimal != binary && options.trunc_check == TruncCheck::Report;
            (binary, report.then_some(TruncOptDivergence { value, decimal, binary }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;

    const HALFWORD: Binary = Binary { digits: 4, signed: false, native: Native::No };
    const SIGNED_HALFWORD: Binary = Binary { digits: 4, signed: true, native: Native::No };

    fn with(trunc: Trunc) -> Options {
        Options { trunc, ..Options::default() }
    }

    #[test]
    fn sizes_follow_the_picture() {
        let size = |digits| Binary { digits, signed: true, native: Native::No }.bytes();
        assert_eq!([size(1), size(4), size(5), size(9), size(10), size(18)], [2, 2, 4, 4, 8, 8]);
    }

    #[test]
    fn std_truncates_to_the_picture() {
        let s = store(HALFWORD, 12345, &with(Trunc::Std));
        assert_eq!((s.value, s.bytes.as_slice()), (2345, [0x09, 0x29].as_slice()));
        assert_eq!(store(SIGNED_HALFWORD, -12345, &with(Trunc::Std)).value, -2345);
    }

    #[test]
    fn bin_truncates_to_the_halfword() {
        assert_eq!(store(HALFWORD, 12345, &with(Trunc::Bin)).value, 12345);
        assert_eq!(store(HALFWORD, 70000, &with(Trunc::Bin)).value, 70000 - 65536);
        let s = store(SIGNED_HALFWORD, 40000, &with(Trunc::Bin));
        assert_eq!((s.value, s.bytes.as_slice()), (40000 - 65536, [0x9C, 0x40].as_slice()));
    }

    #[test]
    fn comp5_ignores_trunc() {
        let native = Binary { native: Native::Comp5, ..HALFWORD };
        assert_eq!(store(native, 12345, &with(Trunc::Std)).value, 12345);
    }

    #[test]
    fn binary_char_is_one_byte_truncated_to_it() {
        let byte = |signed| Binary { digits: 3, signed, native: Native::BinaryChar };
        assert_eq!(byte(true).bytes(), 1);
        let s = store(byte(true), -1, &with(Trunc::Std));
        assert_eq!((s.value, s.bytes.as_slice()), (-1, [0xFF].as_slice()));
        assert_eq!(store(byte(false), 300, &with(Trunc::Std)).value, 44);
        assert_eq!(store(byte(true), 200, &with(Trunc::Std)).value, -56);
        assert_eq!(byte(true).load(&[0x80]), -128);
    }

    #[test]
    fn unsigned_items_take_the_absolute_value() {
        assert_eq!(store(HALFWORD, -42, &with(Trunc::Std)).value, 42);
    }

    #[test]
    fn opt_reports_a_store_where_decimal_and_binary_truncation_disagree() {
        let s = store(HALFWORD, 12345, &with(Trunc::Opt));
        assert_eq!(s.divergence, Some(TruncOptDivergence { value: 12345, decimal: 2345, binary: 12345 }));
        assert_eq!(store(HALFWORD, 9999, &with(Trunc::Opt)).divergence, None);
    }

    #[test]
    fn silent_stores_the_same_value_without_a_report() {
        let mut options = with(Trunc::Opt);
        options.apply_flag("-silent").unwrap();
        let s = store(HALFWORD, 12345, &options);
        assert_eq!((s.value, s.divergence), (12345, None));
    }

    #[test]
    fn load_reads_twos_complement() {
        assert_eq!(SIGNED_HALFWORD.load(&[0xFF, 0xFE]), -2);
        assert_eq!(HALFWORD.load(&[0xFF, 0xFE]), 65534);
    }
}
