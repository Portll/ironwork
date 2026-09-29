use crate::options::{Options, Trunc, TruncCheck};

/// A USAGE BINARY, COMP or COMP-4 item, or COMP-5 when `native`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binary {
    pub digits: u8,
    pub signed: bool,
    pub native: bool,
}

impl Binary {
    pub const fn bytes(self) -> usize {
        match self.digits {
            0..=4 => 2,
            5..=9 => 4,
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
        let kept = (value.unsigned_abs() % 10u128.pow(self.digits as u32)) as i128;
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
    let value = if item.signed { value } else { value.unsigned_abs() as i128 };
    let decimal = item.decimal_truncation(value);
    let binary = item.binary_truncation(value);
    let (kept, divergence) = match (item.native, options.trunc) {
        (true, _) | (false, Trunc::Bin) => (binary, None),
        (false, Trunc::Std) => (decimal, None),
        (false, Trunc::Opt) => {
            let report = decimal != binary && options.trunc_check == TruncCheck::Report;
            (binary, report.then_some(TruncOptDivergence { value, decimal, binary }))
        }
    };
    Stored { bytes: kept.to_be_bytes()[16 - item.bytes()..].to_vec(), value: kept, divergence }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;

    const HALFWORD: Binary = Binary { digits: 4, signed: false, native: false };
    const SIGNED_HALFWORD: Binary = Binary { digits: 4, signed: true, native: false };

    fn with(trunc: Trunc) -> Options {
        Options { trunc, ..Options::default() }
    }

    #[test]
    fn sizes_follow_the_picture() {
        let size = |digits| Binary { digits, signed: true, native: false }.bytes();
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
        let native = Binary { native: true, ..HALFWORD };
        assert_eq!(store(native, 12345, &with(Trunc::Std)).value, 12345);
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
