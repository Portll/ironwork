#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Oracle {
    /// A bare-metal instruction run under Hercules settles it.
    Hercules,
    /// Only a program compiled by Enterprise COBOL, on the pinned target, settles it.
    EnterpriseCobol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Stated in IBM's documentation and checked against its text.
    Documented,
    /// Stated in IBM's documentation, but written here from memory of it: check against the manual.
    Recalled,
    /// Not stated anywhere; the documentation leaves it to the generated code.
    Chosen,
}

#[derive(Clone, Copy, Debug)]
pub struct Assumption {
    pub id: &'static str,
    pub claim: &'static str,
    pub basis: Basis,
    pub oracle: Oracle,
}

pub const HFP_EXTENDED_LOW_HALF: &str = "M1";
pub const HFP_FROM_FIXED_TRUNCATES: &str = "M2";
pub const INTERMEDIATE_TABLE: &str = "C1";
pub const TRUNC_OPT_IS_BINARY: &str = "C2";
pub const PFD_MOVES_BYTES: &str = "C3";
pub const PFD_COMPARES_LOGICALLY: &str = "C4";
pub const FLOAT_FROM_DECIMAL: &str = "C5";
pub const FLOAT_TO_DECIMAL: &str = "C6";
pub const FLOAT_NARROWING_TRUNCATES: &str = "C7";
pub const LE_MASKS_UNDERFLOW: &str = "C8";
pub const PREFERRED_RESULT_SIGNS: &str = "C9";
pub const ZONED_BY_PACK: &str = "C10";
pub const CCSID_TABLES: &str = "C11";
pub const WORKING_STORAGE_LAYOUT: &str = "C12";
pub const NOPFD_REPAIRS_UNSIGNED_INPUT: &str = "C13";
pub const DISPLAY_OF_NONDISPLAY_NUMERIC: &str = "C14";
pub const ACCEPT_AT_END: &str = "C15";
pub const CONTENT_LITERAL_ZONED: &str = "C16";
pub const KEYED_FILE_STATUS: &str = "C17";
pub const ALTERNATE_KEY_ORDER: &str = "C18";
pub const FAILED_READ_LOSES_POSITION: &str = "C19";
pub const START_COMPARES_SHORTER: &str = "C20";
pub const REWRITE_SHARED_ALTERNATE: &str = "C21";
pub const CICS_FRESH_STORAGE: &str = "C22";
pub const CICS_CALEN_RESTORED: &str = "C23";
pub const CICS_HANDLE_ABEND_CATCHES_CONDITIONS: &str = "C24";
pub const CICS_LENGTH_DEFAULTS_TO_INTO: &str = "C25";
pub const CICS_PROGRAM_CHECK_IS_ASRA: &str = "C26";
pub const CICS_BROWSE_SKIP: &str = "C27";

pub const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        id: HFP_EXTENDED_LOW_HALF,
        claim: "An extended HFP result's low half carries the high characteristic minus 14, modulo 128, and is all zero when the value is all zero bits (SA22-7832-14, HFP extended format, p. 18-4)",
        basis: Basis::Documented,
        oracle: Oracle::Hercules,
    },
    Assumption {
        id: HFP_FROM_FIXED_TRUNCATES,
        claim: "CONVERT FROM FIXED to HFP truncates the hexadecimal digits the precision cannot hold",
        basis: Basis::Recalled,
        oracle: Oracle::Hercules,
    },
    Assumption {
        id: INTERMEDIATE_TABLE,
        claim: "Intermediate results carry i and d places up to 30 digits (31 under ARITH(EXTEND)); beyond that, N-d and d when d <= dmax, else i and N-i when i+dmax <= N, else N-dmax and dmax; digits beyond are truncated",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: TRUNC_OPT_IS_BINARY,
        claim: "Under TRUNC(OPT) a binary receiver whose value exceeds its PICTURE is truncated at the halfword, fullword or doubleword, as under TRUNC(BIN)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PFD_MOVES_BYTES,
        claim: "Under NUMPROC(PFD) a MOVE between packed items of the same length and scale copies the bytes, so a non-preferred sign passes through; under NOPFD the receiver gets the preferred sign",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PFD_COMPARES_LOGICALLY,
        claim: "Under NUMPROC(PFD) two packed items of the same length and scale compare byte by byte, so X'1F' and X'1C' differ",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_FROM_DECIMAL,
        claim: "A fixed-point decimal m with s decimal places becomes HFP as CONVERT FROM FIXED of m, then an HFP divide by 10^s",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_TO_DECIMAL,
        claim: "An HFP value moved to a fixed-point receiver is its exact value truncated at the receiver's scale, or rounded half away from zero under ROUNDED",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_NARROWING_TRUNCATES,
        claim: "A floating-point intermediate stored into a narrower COMP-1 or COMP-2 keeps its high-order part (truncation), not LOAD ROUNDED",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_MASKS_UNDERFLOW,
        claim: "Language Environment runs COBOL with the HFP exponent-underflow and significance masks off, so both yield a true zero",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PREFERRED_RESULT_SIGNS,
        claim: "Arithmetic results take the preferred sign under either NUMPROC setting: C or D for a signed item, F for an unsigned one",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ZONED_BY_PACK,
        claim: "A zoned operand enters arithmetic as PACK leaves it: zones other than the sign's are discarded, so an embedded space is a zero digit",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CCSID_TABLES,
        claim: "The compiler's conversion to UTF-16 for each carried CCSID is ICU's ibm-* table for it, byte for byte",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: WORKING_STORAGE_LAYOUT,
        claim: "Each 01 and 77 item of WORKING-STORAGE starts on an 8-byte boundary in source order, and storage without a VALUE starts as X'00'",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NOPFD_REPAIRS_UNSIGNED_INPUT,
        claim: "Under NUMPROC(NOPFD) an unsigned packed or zoned operand's sign is forced to X'F' before use, so no sign makes it a data exception; under PFD it is used as it stands",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DISPLAY_OF_NONDISPLAY_NUMERIC,
        claim: "DISPLAY shows a packed or binary item as zoned digits of its PICTURE, with the sign overpunched on the last digit when the item is signed; a COMP-5 item, or any binary item under TRUNC(BIN), shows its whole binary value in 5, 10, or 19 (signed) or 20 digits for a halfword, fullword or doubleword",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ACCEPT_AT_END,
        claim: "ACCEPT from SYSIN at its end leaves the receiving item unchanged and the run continues",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CONTENT_LITERAL_ZONED,
        claim: "A numeric literal passed BY CONTENT arrives as zoned decimal of its own digits, the sign overpunched when negative",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: KEYED_FILE_STATUS,
        claim: "Indexed and relative files report 02 for a shared alternate key, 14 for a record number too long for the RELATIVE KEY on sequential READ, 21 for a sequential WRITE whose key is not above the last (equal included) or a REWRITE that changed the key, 22 duplicate, 23 not found, 24 a record number below 1 or too long for the RELATIVE KEY on WRITE, 43 REWRITE or DELETE with no READ just before, 46 READ NEXT with no next record, and 47, 48, 49 for the wrong open mode",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALTERNATE_KEY_ORDER,
        claim: "Records sharing an alternate key come back in the order they were written; a record REWRITTEN with a new alternate key goes after the others with it",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FAILED_READ_LOSES_POSITION,
        claim: "A random READ that finds no record leaves no next record, so READ NEXT then gives 46",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: START_COMPARES_SHORTER,
        claim: "START compares the key with its operand over the shorter of the two, left to right, as bytes",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_FRESH_STORAGE,
        claim: "A COBOL program reached by EXEC CICS LINK or XCTL starts with fresh WORKING-STORAGE each time, unlike one reached by CALL",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_CALEN_RESTORED,
        claim: "EIBCALEN is the linked program's COMMAREA length while it runs and the caller's again after the LINK returns",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_HANDLE_ABEND_CATCHES_CONDITIONS,
        claim: "HANDLE ABEND LABEL receives control when a condition nothing handles would abend the task (AEIx), and is cancelled by being taken",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_LENGTH_DEFAULTS_TO_INTO,
        claim: "READ, READNEXT, READPREV, READQ TS and READQ TD without LENGTH take the INTO item's length as the limit, so a longer record raises LENGERR",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_PROGRAM_CHECK_IS_ASRA,
        claim: "A program check (S0C4, S0C7 and the like) in a CICS task ends it with transaction abend ASRA",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_BROWSE_SKIP,
        claim: "READNEXT after the program changed RIDFLD to a key the browse is not at continues from the first record at or after the new RIDFLD (skip-sequential)",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REWRITE_SHARED_ALTERNATE,
        claim: "REWRITE gives 02 whenever another record shares one of the record's alternate keys that allow duplicates, whether or not that key changed",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
];

pub fn get(id: &str) -> &'static Assumption {
    ASSUMPTIONS.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("no assumption {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = ASSUMPTIONS.iter().map(|a| a.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ASSUMPTIONS.len());
    }
}
