//! Codecs for the types the LIR borrows from `rt`, `numeric` and `zarch`, with load-module.md's tags.

use crate::module::ModuleError;
use crate::module::codec::{Decode, Encode, Reader, Writer};
use crate::picture::Sym;
use crate::storage::Kind;
use crate::vocab::{Figurative, SignClause, SignPosition};
use crate::{codec_enum, codec_struct};
use numeric::precision::{Fixed, Places};
use numeric::{Arith, Numproc, Options, SortKeys, Trunc, TruncCheck};
use zarch::check::ProgramCheck;
use zarch::ebcdic::CodePage;
use zarch::hfp::Precision;
use zarch::wide::U256;

codec_enum!(Kind {
    Group = 0,
    Alnum { justified } = 1,
    National = 2,
    Zoned { digits, scale, signed, sign } = 3,
    Packed { digits, scale, signed } = 4,
    Binary { digits, scale, signed, native } = 5,
    Float(precision) = 6,
    NumericEdited { edit, digits, scale, blank_when_zero } = 7,
    AlnumEdited { edit } = 8,
    Pointer = 9,
    Index = 10,
    ObjectReference = 11,
    ProgramPointer = 12,
});
codec_struct!(SignClause { position, separate });
codec_enum!(SignPosition { Leading = 0, Trailing = 1 });
codec_enum!(Figurative { Zero = 0, Space = 1, HighValue = 2, LowValue = 3, Quote = 4, Null = 5 });
codec_enum!(Sym {
    Nine = 0,
    Z = 1,
    Star = 2,
    FloatLead(c) = 3,
    Float(c) = 4,
    Sign(c) = 5,
    Currency = 6,
    Cr = 7,
    Db = 8,
    Point = 9,
    Implied = 10,
    Insert(c) = 11,
    Char = 12,
});
codec_enum!(Precision { Short = 0, Long = 1, Extended = 2 });
codec_enum!(ProgramCheck {
    Specification = 0,
    Data = 1,
    FixedPointOverflow = 2,
    FixedPointDivide = 3,
    DecimalOverflow = 4,
    DecimalDivide = 5,
    HfpExponentOverflow = 6,
    HfpExponentUnderflow = 7,
    HfpSignificance = 8,
    HfpDivide = 9,
});

codec_struct!(Options {
    arith, trunc, numproc, codepage, trunc_check, fastsrt, sort_keys, adv, thread, dll, rent, dbcs,
} check options_valid);
codec_enum!(Arith { Compat = 0, Extend = 1 });
codec_enum!(Trunc { Std = 0, Opt = 1, Bin = 2 });
codec_enum!(Numproc { Nopfd = 0, Pfd = 1 });
codec_enum!(TruncCheck { Report = 0, Silent = 1 });
codec_enum!(SortKeys { Dfsort = 0, Strict = 1 });

/// `Options::code_page` panics on a CCSID the tables do not carry.
fn options_valid(options: &Options) -> Result<(), String> {
    match CodePage::by_ccsid(options.codepage) {
        Some(_) => Ok(()),
        None => Err(format!("CODEPAGE({}) is not a page the tables carry", options.codepage)),
    }
}

codec_struct!(Fixed { negative, magnitude, places });
codec_struct!(Places { int, dec });

/// Four 64-bit limbs, low first.
impl Encode for U256 {
    fn encode(&self, w: &mut Writer) {
        [self.lo as u64, (self.lo >> 64) as u64, self.hi as u64, (self.hi >> 64) as u64].encode(w);
    }
}

impl Decode for U256 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let [a, b, c, d] = <[u64; 4]>::decode(r)?;
        let limbs = |low: u64, high: u64| u128::from(low) | u128::from(high) << 64;
        Ok(U256 { hi: limbs(c, d), lo: limbs(a, b) })
    }
}
