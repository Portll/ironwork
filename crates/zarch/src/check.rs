use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProgramCheck {
    Specification,
    Data,
    FixedPointOverflow,
    FixedPointDivide,
    DecimalOverflow,
    DecimalDivide,
    HfpExponentOverflow,
    HfpExponentUnderflow,
    HfpSignificance,
    HfpDivide,
}

impl ProgramCheck {
    pub const fn interruption_code(self) -> u8 {
        match self {
            Self::Specification => 0x06,
            Self::Data => 0x07,
            Self::FixedPointOverflow => 0x08,
            Self::FixedPointDivide => 0x09,
            Self::DecimalOverflow => 0x0A,
            Self::DecimalDivide => 0x0B,
            Self::HfpExponentOverflow => 0x0C,
            Self::HfpExponentUnderflow => 0x0D,
            Self::HfpSignificance => 0x0E,
            Self::HfpDivide => 0x0F,
        }
    }

    /// The system completion code a batch step ends with when nothing handles the check.
    pub fn abend(self) -> String {
        format!("S0C{:X}", self.interruption_code())
    }
}

impl fmt::Display for ProgramCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({})", self, self.abend())
    }
}

impl std::error::Error for ProgramCheck {}

/// Condition code: 0 zero or equal, 1 negative or low, 2 positive or high, 3 overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cc(pub u8);

/// The four PSW program-mask bits. A masked-off condition completes the instruction without an
/// interruption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgramMask {
    pub fixed_point_overflow: bool,
    pub decimal_overflow: bool,
    pub hfp_exponent_underflow: bool,
    pub hfp_significance: bool,
}

impl ProgramMask {
    pub fn decimal(self, cc: Cc) -> Result<Cc, ProgramCheck> {
        if cc.0 == 3 && self.decimal_overflow { Err(ProgramCheck::DecimalOverflow) } else { Ok(cc) }
    }
}
