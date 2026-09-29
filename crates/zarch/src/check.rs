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

impl From<std::cmp::Ordering> for Cc {
    fn from(ordering: std::cmp::Ordering) -> Self {
        Self(match ordering {
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Greater => 2,
        })
    }
}

impl Cc {
    /// The comparison a code 0, 1 or 2 stands for.
    pub fn ordering(self) -> Option<std::cmp::Ordering> {
        match self.0 {
            0 => Some(std::cmp::Ordering::Equal),
            1 => Some(std::cmp::Ordering::Less),
            2 => Some(std::cmp::Ordering::Greater),
            _ => None,
        }
    }
}

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
    /// From the PSW mask nibble: 8 fixed-point overflow, 4 decimal overflow, 2 HFP exponent
    /// underflow, 1 significance.
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            fixed_point_overflow: bits & 8 != 0,
            decimal_overflow: bits & 4 != 0,
            hfp_exponent_underflow: bits & 2 != 0,
            hfp_significance: bits & 1 != 0,
        }
    }

    pub const fn bits(self) -> u8 {
        (self.fixed_point_overflow as u8) << 3
            | (self.decimal_overflow as u8) << 2
            | (self.hfp_exponent_underflow as u8) << 1
            | self.hfp_significance as u8
    }

    pub fn decimal(self, cc: Cc) -> Result<Cc, ProgramCheck> {
        if cc.0 == 3 && self.decimal_overflow { Err(ProgramCheck::DecimalOverflow) } else { Ok(cc) }
    }
}
