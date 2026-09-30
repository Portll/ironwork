#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Oracle {
    /// A bare-metal instruction run under Hercules settles it.
    Hercules,
    /// Only a program compiled by Enterprise COBOL, on the pinned target, settles it.
    EnterpriseCobol,
    /// Only a program run against Db2 for z/OS settles it.
    Db2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Stated in IBM's documentation and checked against its text.
    Documented,
    /// Stated in IBM's documentation, but written here from memory of it: check against the manual.
    Recalled,
    /// Not stated anywhere; the documentation leaves it to the generated code.
    Chosen,
    /// Seen on a related system the claim names, which is not the oracle, and not contradicted by the
    /// oracle's documentation.
    Observed,
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
pub const BMS_RECEIVE_NULLS: &str = "C28";
pub const BMS_INPUT_JUSTIFY: &str = "C29";
pub const BMS_EXTENDED_ORDER: &str = "C30";
pub const BMS_CONSTANTS_UNVERIFIED: &str = "C31";
pub const BMS_SEND_DATA_CHOICE: &str = "C32";
pub const CICS_INITIAL_AID: &str = "C33";
pub const LONG_ZONED_BY_PACKS: &str = "C34";
pub const ASCII_COLLATION: &str = "C35";
pub const TABLE_SORT_COLLATION: &str = "C36";
pub const ALPHABET_LITERALS: &str = "C37";
pub const LE_SERVICE_AFTER_PROGRAMS: &str = "L1";
pub const LE_ARGUMENTS_BY_ADDRESS: &str = "L2";
pub const LE_FEEDBACK_TOKEN: &str = "L3";
pub const LE_FEEDBACK_NO_INSTANCE_INFO: &str = "L4";
pub const LE_OMITTED_FC_ABENDS: &str = "L5";
pub const LE_CEE3ABD: &str = "L6";
pub const LE_LOCAL_TIME_IS_UTC: &str = "L7";
pub const LE_SECONDS_HFP: &str = "L8";
pub const LE_PICTURE_OUTPUT: &str = "L9";
pub const LE_PICTURE_INPUT: &str = "L10";
pub const LE_CENTURY_WINDOW: &str = "L11";
pub const LE_RETURN_CODE_UNCHANGED: &str = "L12";
pub const LE_MESSAGE_AND_DUMP_FILES: &str = "L13";
pub const LE_HEAP: &str = "L14";
pub const LE_UNDER_CICS: &str = "L15";
pub const LE_CEEIGZCT: &str = "L16";
pub const LE_CEEIGZCT_DISAGREEMENTS: &str = "L17";
pub const LE_SHORT_ARGUMENT_LIST: &str = "L18";
pub const REPORT_WRITER_PRECOMPILER: &str = "RW1";
pub const REPORT_TOTALS_BEFORE_PAGE_FIT: &str = "RW2";
pub const REPORT_SOURCE_SUM_CORRELATION: &str = "RW3";
pub const REPORT_PAGE_REGION_DEFAULTS: &str = "RW4";
pub const REPORT_LINE_WRITES: &str = "RW5";
pub const REPORT_CARRIAGE_CONTROL: &str = "RW6";
pub const REPORT_RECORD_LENGTH: &str = "RW7";
pub const REPORT_SUM_OVERFLOW: &str = "RW8";
pub const REPORT_SOURCE_OVERFLOW: &str = "RW9";
pub const REPORT_SUPPRESS_PRINTING: &str = "RW10";
pub const REPORT_NEW_PAGES: &str = "RW11";
pub const REPORT_OUT_OF_ORDER: &str = "RW12";
pub const REPORT_CONTROL_AREA: &str = "RW13";
pub const OBJECT_REFERENCE_VALUE: &str = "J1";
pub const LOCAL_REFERENCES_EXPIRE: &str = "J2";
pub const OBJECTS_NEVER_FREED: &str = "J3";
pub const INSTANCE_DATA_START: &str = "J4";
pub const FACTORY_DATA_START: &str = "J5";
pub const METHOD_LOOKUP: &str = "J6";
pub const METHOD_NAME_ITEM: &str = "J7";
pub const INVOKE_NULL: &str = "J8";
pub const NO_METHOD_ABEND: &str = "J9";
pub const FACTORY_SELF: &str = "J10";
pub const CLASS_SEARCH: &str = "J11";
pub const INVOKE_KEEPS_RETURN_CODE: &str = "J12";
pub const OO_OPTIONS_REQUIRED: &str = "J13";
pub const CHAR_FROM_DISPLAY: &str = "J14";
pub const LOCAL_REFERENCES_OUTSIDE_METHODS: &str = "J15";
pub const SELF_IS_LOCAL: &str = "J16";
pub const EXPIRED_REFERENCE_ABENDS: &str = "J17";
pub const LOCAL_FRAMES: &str = "J18";
pub const OO_OPTIONS_SEVERITY: &str = "J19";
pub const REFERENCES_KEPT: &str = "J20";
pub const SQL_COMMIT_AT_NORMAL_END: &str = "SQ1";
pub const SQL_WHENEVER_ORDER: &str = "SQ2";
pub const SQL_TRUNCATED_INDICATOR: &str = "SQ3";
pub const SQL_POSTGRES_ERRORS: &str = "SQ4";
pub const SQL_DIALECT_REWRITES: &str = "SQ5";
pub const SQL_INTO_WITHOUT_COLONS: &str = "SQ7";
pub const SQL_DECLARATIONS_CROSS_NESTED_PROGRAMS: &str = "SQ8";
pub const SQL_DOUBLE_TO_HFP_TRUNCATES: &str = "SQ9";
pub const SQL_ZONED_IS_DECIMAL: &str = "SQ10";
pub const SQL_ISO_DATETIME: &str = "SQ11";
pub const SQL_TRAILING_BLANKS_SENT: &str = "SQ12";
pub const SQL_FETCH_ROW_COUNT: &str = "SQ13";
pub const SORT_EQUAL_KEYS_IN_ORDER: &str = "S1";
pub const MERGE_EQUAL_KEYS_BY_FILE: &str = "S2";
pub const MERGE_OUT_OF_SEQUENCE_FAILS: &str = "S3";
pub const SORT_FILE_FAILURE: &str = "S4";
pub const SORT_DECIMAL_KEYS: &str = "S5";
pub const SORT_RECORD_LENGTHS: &str = "S6";
pub const SORT_RETURN_STOPS: &str = "S7";
pub const RETURN_AFTER_END: &str = "S8";
pub const SORT_KEY_INVALID_DIGITS: &str = "S9";
pub const SORT_NEGATIVE_ZERO: &str = "S10";
pub const FASTSRT_FILES: &str = "S11";
pub const FASTSRT_STATUS: &str = "S12";
pub const FASTSRT_FAILURE: &str = "S13";
pub const SAME_AREA_VSAM: &str = "S14";
pub const FASTSRT_PRINT_RECORDS: &str = "S15";
pub const FASTSRT_RECORD_LENGTHS: &str = "S16";
pub const FASTSRT_ADV_PRINT: &str = "S17";
pub const PRINT_CONTROL_CHARACTER: &str = "C40";
pub const PRINT_SPACING_RECORDS: &str = "C41";
pub const PRINT_CONTROL_RUN_TIME: &str = "C42";
pub const TEXT_PRINT_LINES: &str = "C43";
pub const COMPILER_SEVERITIES: &str = "C44";
pub const REFUSALS_ARE_SEVERE: &str = "C45";
pub const REFUSED_FROM_S: &str = "C46";
pub const NOCOMPILE: &str = "C47";
pub const COMMENT_ENTRY_EXTENT: &str = "C80";
pub const COMMENT_ENTRY_HEADERS: &str = "C81";
pub const COMMENT_ENTRY_REMARKS: &str = "C82";
pub const CONTINUED_LITERAL_QUOTES: &str = "C83";
pub const COPY_SEARCH_ROUNDS: &str = "C85";
pub const COPY_NOT_THE_PROGRAM: &str = "C86";
pub const COPY_LITERAL_AS_WRITTEN: &str = "C87";
pub const COPY_DOUBLED_PERIOD: &str = "C88";
pub const LINAGE_COUNTER: &str = "C70";
pub const LINAGE_PAGE_MOVEMENT: &str = "C71";
pub const LINAGE_END_OF_PAGE: &str = "C72";
pub const LINAGE_VALUES: &str = "C73";
pub const LINAGE_EXTEND: &str = "C74";
pub const LINAGE_COUNTER_BETWEEN_WRITES: &str = "C75";
pub const PRINT_FILE_UPDATE: &str = "C76";
pub const ENTRY_IN_SEQUENCE: &str = "C50";
pub const ENTRY_CALLS: &str = "C51";
pub const ALTERED_GO_TO_RESET: &str = "C52";
pub const DISPLAY_STREAM: &str = "C53";
pub const RANDOM_GENERATOR: &str = "C54";
pub const ZERO_DIVISOR_CHECK: &str = "C55";
pub const ERROR_DECLARATIVE_MODE: &str = "C60";
pub const ERROR_DECLARATIVE_STATUSES: &str = "C61";
pub const SORT_FILE_DECLARATIVE: &str = "C62";
pub const DEBUG_RUNTIME_OPTION: &str = "C63";
pub const DEBUG_LINE_NUMBER: &str = "C64";
pub const DEBUG_LINE_STATEMENT: &str = "C65";
pub const DEBUG_CONTENTS_LENGTH: &str = "C66";
pub const DEBUG_NAME_FORM: &str = "C67";
pub const DEBUGGING_SECTION_REFERENCES: &str = "C68";
pub const GLOBAL_DECLARATIVES: &str = "C69";
pub const SYNC_SUBORDINATE_GROUP: &str = "C90";
pub const SYNC_ONLY_WHEN_WRITTEN: &str = "C91";
pub const SYNC_SLACK_OWNER: &str = "C92";
pub const SYNC_REDEFINES_REFUSED: &str = "C93";
pub const DECIMAL_COMMA_SEPARATOR: &str = "C94";
pub const DECIMAL_COMMA_DISPLAY_LITERAL: &str = "C95";
pub const ARITH_DIGIT_LIMITS: &str = "C96";
pub const MULTIPLE_RESULTS: &str = "C97";
pub const ALTER_DEBUGGING: &str = "C98";
pub const PERFORM_RETURN_POINTS: &str = "C99";
pub const FLOAT_FUNCTION_ARGUMENTS: &str = "C100";
pub const FLOAT_FUNCTION_ROUNDING: &str = "C110";
pub const FLOATING_POINT_FUNCTIONS: &str = "C111";
pub const FUNCTION_DOMAIN: &str = "C112";
pub const NUMVAL_TEST_RULES: &str = "C113";
pub const FUNCTION_CLOCK: &str = "C114";
pub const UUID4_SOURCE: &str = "C115";
pub const FORMATTED_DATETIME_RULES: &str = "C116";
pub const ROUNDED_EXTRA_PLACE: &str = "C101";
pub const CURRENCY_SIGNS: &str = "C102";
pub const JSON_GENERATE_RULES: &str = "C117";
pub const XML_PARSE_RULES: &str = "C118";
pub const XML_GENERATE_RULES: &str = "C119";
pub const JSON_PARSE_RULES: &str = "C170";
pub const CORRESPONDING_PAIRS: &str = "C130";
pub const CORRESPONDING_CHOICES: &str = "C131";
pub const NUMPROC_MIG_WARNS: &str = "C120";
pub const INVALID_OPTION_DISCARDED: &str = "C121";
pub const OPTIONS_WITHOUT_EFFECT: &str = "C122";
pub const NON_COBOL_CHARACTERS: &str = "C123";
pub const NO_PROGRAM_END: &str = "C124";
pub const USE_WITHOUT_PARAGRAPH: &str = "C125";

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
        id: BMS_RECEIVE_NULLS,
        claim: "RECEIVE MAP sets the input map to nulls, then fills only the fields the operator modified; a field erased to empty gets F = X'80' and L = 0",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_INPUT_JUSTIFY,
        claim: "Input data lands left-justified and blank-filled unless JUSTIFY says otherwise, and a NUM field right-justified and zero-filled, the defaults IBM documents for JUSTIFY",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_EXTENDED_ORDER,
        claim: "A field's extended attribute bytes in the symbolic map follow its A byte in the order COLOR, PS, HILIGHT, VALIDN, OUTLINE, SOSI, TRANSP",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_CONSTANTS_UNVERIFIED,
        claim: "DFHNULL is X'00', and DFHBMPEM, DFHBMPNL, DFHBMPFF and DFHBMPCR are X'19', X'15', X'0C' and X'0D'",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_SEND_DATA_CHOICE,
        claim: "SEND MAP without MAPONLY or DATAONLY sends a field's symbolic data when its first byte is not X'00', else the map's INITIAL, and a non-null A byte replaces ATTRB",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_INITIAL_AID,
        claim: "A task started by terminal input sees that input's AID in EIBAID before any RECEIVE",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REWRITE_SHARED_ALTERNATE,
        claim: "REWRITE gives 02 whenever another record shares one of the record's alternate keys that allow duplicates, whether or not that key changed",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_SERVICE_AFTER_PROGRAMS,
        claim: "A CALL that finds no program of its name in the run unit or its program libraries reaches the Language Environment callable service of that name, as a link-edit that finds the name in no user library resolves it from SCEELKED; a user program of the same name comes first",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_ARGUMENTS_BY_ADDRESS,
        claim: "A callable service reads and stores each parameter at its argument's address for the length SA38-0683-60 gives it (fullword, halfword-prefixed string, 8-byte COMP-2, 17- or 80-byte string, 12-byte feedback code), whatever the argument's own length; an OMITTED parameter other than fc is a protection exception (S0C4), and a store past the end of the run unit's storage is lost",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_FEEDBACK_TOKEN,
        claim: "A feedback code is 12 bytes: severity and message number as halfwords, a byte of case 1, severity and control 001 (X'59' at severity 3), the facility CEE in EBCDIC, then the instance-specific word; twelve zero bytes are CEE000, success (SA38-0683-60, _FEEDBACK in Table 20; so CEE2EB, severity 3 and message 2507, is X'000309CB59C3C5C5')",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_FEEDBACK_NO_INSTANCE_INFO,
        claim: "The instance-specific word of every feedback code the provided services return is zero",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_OMITTED_FC_ABENDS,
        claim: "With fc OMITTED a failing service signals its condition (SA38-0683-60, Invoking callable services); nothing handles it, so one of severity 2 or more ends the run as the default ABTERMENC(ABEND) does, with user abend U4038, and one of severity 1 lets the run continue",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_CEE3ABD,
        claim: "CEE3ABD ends the run with user abend abcode modulo 4096, the ABEND macro's user completion code, to which SA38-0683-60 says abcode passes unchecked; every clean-up value ends it alike: open files are closed as at any ironwork abend, and neither a CEEDUMP nor a system dump is written",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_LOCAL_TIME_IS_UTC,
        claim: "Local time is UTC: CEELOCT and CEEGMT read the clock that ACCEPT FROM DATE and TIME and CURRENT-DATE read, and CEEGMTO reports a zero offset, as CURRENT-DATE reports +0000",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_SECONDS_HFP,
        claim: "Lilian seconds are the whole number of milliseconds converted to long HFP and divided by 1000, truncating as FLOAT_FROM_DECIMAL does, so a MOVE of CEESECS's result to a decimal item can show a millisecond less; seconds given to CEEDATM are taken to the nearest millisecond",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_PICTURE_OUTPUT,
        claim: "CEEDATE and CEEDATM write the terms of SA38-0683-60 Table 34 and copy anything else as it stands, with English month and day names; MM after an hour term is minutes (Table 28); CEEDATE writes time terms as zero and AP as AM (Table 27, where Table 35 says blank); the eras <JJJJ>, <CCCC> and YYY are not provided and give CEE2EM; a null or blank picture is the COUNTRY(US) default of Table 33",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_PICTURE_INPUT,
        claim: "CEEDAYS and CEESECS read a numeric term followed by a delimiter as up to its width of digits (6/2/88 for MM/DD/YY) and any other as exactly its width, leading blanks allowed; a delimiter takes one character, whatever it is; a month name is three or more of its letters; input that ends before the date is complete is CEE2EB, before the time is complete, zeros; a non-digit is CEE2EO (CEE2ET for CEESECS)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_CENTURY_WINDOW,
        claim: "A two-digit year falls in the hundred years starting 80 years before the current year (SA38-0683-60, CEEDAYS and CEESECS); CEESCEN, which moves the window, is not provided",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_RETURN_CODE_UNCHANGED,
        claim: "A CALL of a callable service leaves RETURN-CODE as it was; SA38-0683-60 leaves register 15 undefined on return",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_MESSAGE_AND_DUMP_FILES,
        claim: "CEEMOUT writes to DD SYSOUT, MSGFILE's default ddname, and CEE3DMP to DD CEEDUMP or the ddname FNAME gives, as UTF-8 text lines, the run's first write replacing the file, or to standard error without the DD; CEE3DMP writes its title, date and time, options and the active programs, not storage or control blocks",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_HEAP,
        claim: "CEEGTST gives zeroed run-unit storage on a doubleword from heap 0 only, refuses a request above 256 MiB with CEE0PD, and keeps the storage until the run ends; CEEFRST marks it free, and it is not reused",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_UNDER_CICS,
        claim: "In a CICS task, CEE3ABD ends the task as EXEC CICS ABEND CANCEL would, with abcode modulo 4096 as a four-digit decimal ABCODE, so HANDLE ABEND does not catch it; CEEMOUT and CEE3DMP write each line as one item on transient data queue CESE, as MSGFILE and any DD are ignored under CICS (SA38-0683-60), without the terminal, transaction and time prefix CICS may put on a CESE record",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_WRITER_PRECOMPILER,
        claim: "Enterprise COBOL takes the REPORT SECTION, the FD REPORT clause, INITIATE, GENERATE, TERMINATE, PAGE-COUNTER, LINE-COUNTER, PRINT-SWITCH and USE BEFORE REPORTING only through the COBOL Report Writer Precompiler, 5798-DYR (Migration Guide GC27-8715-04, pp. 69-70); a Report Writer program runs as the precompiler's generated COBOL runs, with the precompiler as supplied, option OSVS on (SC26-4301-04, 1.1.3)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_TOTALS_BEFORE_PAGE_FIT,
        claim: "Under OSVS a report group's totalling (cross-footing, subtotalling, rolling forward) comes before its USE BEFORE REPORTING procedure and its page-fit test, so a group that forces a new page is already in the totals the PAGE FOOTING shows (SC26-4301-04, 4.2.4)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_SOURCE_SUM_CORRELATION,
        claim: "Under OSVS a SUM of an item outside the REPORT SECTION that a DETAIL group has as a SOURCE is added only when such a DETAIL is generated, and once for each such DETAIL on GENERATE report-name; UPON names the DETAILs outright; any other such operand is added on every GENERATE (SC26-4301-04, 3.23.5 and 4.2.3)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_PAGE_REGION_DEFAULTS,
        claim: "With no FOOTING, LAST CONTROL FOOTING is LAST DETAIL when that is written, else the line before the PAGE FOOTING (for a relative PAGE FOOTING, the line that puts its last line on PAGE LIMIT), else PAGE LIMIT; the standard would take PAGE LIMIT. With no LAST DETAIL it is FOOTING; a PAGE LIMIT below either is raised to it (SC26-4301-04, 2.9.3 and message RW-031)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_LINE_WRITES,
        claim: "Each report line is one WRITE AFTER ADVANCING: the first line of a page AFTER ADVANCING PAGE when it is line 1, otherwise after a record of spaces, with no CODE, written AFTER ADVANCING PAGE; every other line after the distance from the last; a report with no PAGE LIMIT never skips to a new page. The manual shows this for its file handlers and says the direct output also writes line 1 at the top of the page (SC26-4301-04, 5.3.8 and 6.3)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_CARRIAGE_CONTROL,
        claim: "A report file is written AFTER ADVANCING, so each report record carries an ASA control character (PRINT_CONTROL_CHARACTER): under ADV a byte before the record, under NOADV the record's first byte, which the precompiler leaves for it (SC26-4301-04, 2.2.3 rules 7 to 9, 2.7.2 rule 3). The CODE comes after the control character, as 5.3.2 says of the PRNT handler, which prints as no handler does, though 2.5.3 rule 2 puts it before",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_RECORD_LENGTH,
        claim: "A report file whose FD has no RECORD CONTAINS has records as long as the longest line of its reports, rounded up to a multiple of 4, plus the CODE and the control character (SC26-4301-04, 2.2.3 rule 7), the FD's record holding the control character only under NOADV (rule 8, 2.7.2 rule 3), so that under ADV a RECORD CONTAINS length is the line and CODE alone; under RECORDING MODE V each record ends after its last printed field (rule 9)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_SUM_OVERFLOW,
        claim: "A SUM total is a signed binary item with the integer and decimal places of its SUM entry, widened to those of a REPORT SECTION item it totals, and packed decimal beyond 18 digits (SC26-4301-04, 3.23.4); an addition that would overflow it is not made and run-time error 11 is logged, as 2.8.3 says for SUM OVERFLOW STANDARD, though 3.23.8 says the field then prints blank",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_SOURCE_OVERFLOW,
        claim: "A SOURCE arithmetic expression that overflows its field or divides by zero leaves the field blank and logs run-time error 10, OVERFLOW PROCEDURE IS STANDARD being the default (SC26-4301-04, 2.8.3)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_SUPPRESS_PRINTING,
        claim: "SUPPRESS PRINTING, or PRINT-SWITCH left non-zero by a USE BEFORE REPORTING procedure, stops the group's lines, page-fit test and NEXT GROUP, but its totals are still reset; 4.5.3 and 4.7.3 of SC26-4301-04 say so, while 4.2.4 step 9 says no further action is taken",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_NEW_PAGES,
        claim: "LINE ... NEXT PAGE skips to a new page only when a body group is already on the page; a REPORT FOOTING on a page of its own gets no PAGE HEADING or PAGE FOOTING; a page holding the REPORT HEADING alone gets no PAGE FOOTING (SC26-4301-04, 3.24.3)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_OUT_OF_ORDER,
        claim: "GENERATE for a report not initiated logs run-time error 14 and initiates it (SC26-4301-04, message RW-142); INITIATE of an active report starts it afresh; TERMINATE of an inactive report does nothing",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_CONTROL_AREA,
        claim: "PAGE-COUNTER, LINE-COUNTER and PRINT-SWITCH are S9(9) COMP items (PAGE-COUNTER's PICTURE is in SC26-4301-04, 3.15.2); each report's control area, fields and totals are WORKING-STORAGE after the program's own items, so CANCEL and IS INITIAL start the report afresh",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OBJECT_REFERENCE_VALUE,
        claim: "An object reference is four bytes, as under LP(32): zero for NULL, otherwise the number of the local or global reference it holds, never reused in the run unit; each NEW, RETURNING value, argument received, SELF and reference a JNI service makes is a new number, so two references to one object can differ byte for byte, and = compares the objects they identify (Language Reference SC27-8713-03, p. 282)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LOCAL_REFERENCES_EXPIRE,
        claim: "The object references a method receives as arguments, gets back as INVOKE RETURNING values or from JNI services, and makes with INVOKE ... NEW are local references, valid until the method returns, whatever kind the invoked method returned; NewGlobalRef makes a global reference, valid until DeleteGlobalRef, and DeleteLocalRef frees a local reference at once (Programming Guide SC27-8714-03, pp. 702-703 and 721-723; Language Reference SC27-8713-03, p. 365). SET copies a reference and converts nothing (Language Reference p. 451), so the Guide's 'use a SET statement to convert' (p. 722) is read as the CALL of NewGlobalRef it shows on p. 702, and a local reference kept in OBJECT, FACTORY or method WORKING-STORAGE expires, which p. 722 calls an error",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OBJECTS_NEVER_FREED,
        claim: "Objects are never freed, and a run unit that creates more than 1,000,000 of them abends, where Java's garbage collector reclaims objects no longer referred to",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSTANCE_DATA_START,
        claim: "INVOKE class NEW gives each COBOL class in the new object's hierarchy its own copy of its OBJECT WORKING-STORAGE, set to X'00' and then to its VALUE clauses; IBM documents only the VALUE clauses",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FACTORY_DATA_START,
        claim: "A class's FACTORY WORKING-STORAGE is set to X'00' and then to its VALUE clauses when the run unit first uses the class, and its one copy serves every INVOKE of the class's factory methods, through a subclass too",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: METHOD_LOOKUP,
        claim: "INVOKE selects the method by its name, case kept, and the Java types of its arguments and RETURNING item (void without one), a universal object reference counting as java.lang.Object; the search starts at the class of the object itself, not the class its reference is typed with, and goes up the INHERITS chain; SUPER starts at the parent of the class that defines the running method",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: METHOD_NAME_ITEM,
        claim: "A method name held in a data item is the item's content without its trailing spaces",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INVOKE_NULL,
        claim: "INVOKE on a NULL object reference, or on four bytes that name no object, ends the run with an abend; IBM leaves the result undefined",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NO_METHOD_ABEND,
        claim: "An INVOKE without ON EXCEPTION that finds no method raises IBM's severity-3 Language Environment condition, which ends the run with abend U4038",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FACTORY_SELF,
        claim: "SELF in a factory method is the factory object of the class that defines the method, and INVOKE on a reference to a factory object runs factory methods",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CLASS_SEARCH,
        claim: "A class is a COBOL class when a class definition with its external name is among the programs read or in the program libraries, as a member named with its simple name or its full name with periods as underscores, as IBM names the class's DLL; any other class is a Java class",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INVOKE_KEEPS_RETURN_CODE,
        claim: "INVOKE leaves the invoking program's RETURN-CODE as it was (Language Reference, INVOKE statement, RETURNING phrase)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OO_OPTIONS_REQUIRED,
        claim: "A class definition, and a program with INVOKE or an object reference, is compiled with THREAD, DLL, RENT and DBCS (Programming Guide SC27-8714-03, pp. 291, 295, 363, 588, 591, 694; Language Reference SC27-8713-03, p. 89), and NORENT with THREAD or DLL is a conflict IBM resolves as RENT (Guide pp. 344-345). Under THREAD a program is RECURSIVE ('an error will occur', Guide p. 591; Language Reference p. 103), and INITIAL, nested programs, SORT of a file and MERGE are diagnosed as errors (Guide p. 418; Language Reference pp. 85, 103, 400, 452); a table SORT is allowed (p. 453), and a method is recursive without it (p. 94). The options are the CBL and PROCESS cards' over IBM's defaults NOTHREAD, NODLL, RENT and DBCS",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CHAR_FROM_DISPLAY,
        claim: "A one-byte reference modification of a display item passed to INVOKE becomes a Java char through the program's code page",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_EQUAL_KEYS_IN_ORDER,
        claim: "Records with equal keys leave a SORT in the order they entered it whether or not WITH DUPLICATES IN ORDER is written, as under DFSORT's EQUALS; a table SORT keeps equal elements in their order too",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MERGE_EQUAL_KEYS_BY_FILE,
        claim: "Records with equal keys leave a MERGE in the order of the USING files, and each file's in its own order",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MERGE_OUT_OF_SEQUENCE_FAILS,
        claim: "A MERGE input file whose records are out of the merge order makes the MERGE fail with SORT-RETURN 16 before any record is output, as DFSORT's ICE068A ends a merge",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_FILE_FAILURE,
        claim: "An I/O failure on a USING or GIVING file with a FILE STATUS sets the status and makes the SORT or MERGE fail with SORT-RETURN 16, leaving the rest undone; without a FILE STATUS it ends the run as the same failure on OPEN, READ, WRITE or CLOSE does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_DECIMAL_KEYS,
        claim: "A file SORT or MERGE compares a zoned or packed key as DFSORT compares ZD, PD, CLO, CSL and CST fields (z/OS DFSORT Application Programming Guide SC23-6878-50, Appendix C, pp. 825-828): sign nibbles F, E, C, A, 8, 6, 4, 2 and 0 are positive and D, B, 9, 7, 5, 3 and 1 negative, a separate sign is negative only when it is '-', the zones of the other digits are ignored, and no key is a data exception; a binary key compares at its full width (BI, FI) and a floating-point one by value (FL). -strict-sort-keys reads the key as the program would, so an invalid one is S0C7; a table SORT always compares as a relation condition does",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_RECORD_LENGTHS,
        claim: "A record shorter than a fixed-length SD or GIVING record is padded with spaces and a longer one cut to that length; a variable-length record that ends inside a key makes the SORT or MERGE fail with SORT-RETURN 16, as DFSORT does without VLSHRT",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_RETURN_STOPS,
        claim: "SORT-RETURN is 0 when a SORT or MERGE starts; 16 moved to it in an input or output procedure stops the operation at the next RELEASE or RETURN, which does nothing, or when the input procedure ends",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: RETURN_AFTER_END,
        claim: "A RETURN after the AT END condition takes the AT END phrase again",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_KEY_INVALID_DIGITS,
        claim: "A digit nibble A to F in a zoned or packed sort key collates above 9 in its place; DFSORT does not say where such a key collates",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_NEGATIVE_ZERO,
        claim: "A zoned or packed sort key of -0 collates before +0 in ascending order, as under DFSORT's SZERO=YES (Installation and Customization SC23-6881-70, p. 99), the IBM-supplied default",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FASTSRT_FILES,
        claim: "Under FASTSRT DFSORT does the I/O of a SORT's only USING file and its only GIVING file, except a MERGE's, a line-sequential or variable-length relative file, a GIVING file whose FD has LINAGE (p. 233), a print file under ADV unless --fastsrt-adv-print=include, one whose records differ from the SD's in format (fixed or variable) or largest length, and a GIVING file that is also the USING file; COBOL does the rest as under NOFASTSRT (Programming Guide SC27-8714-03, pp. 232-233, 369). The Guide does not name a print file under ADV. By default, --fastsrt-adv-print=exclude, it is left to COBOL, because ADV adds a byte to its record length for the printer control character (p. 346), which the DD's LRECL counts (p. 185): p. 233 wants the SD's and the FD's largest records the same length and p. 232 the DD to match the FD, so with the FD's record as long as the SD's the data set's records are a byte longer than DFSORT's, and with it a byte shorter the FD's and the SD's differ. --fastsrt-adv-print=include reads p. 233 as the FD's length alone and gives the file to DFSORT, as FASTSRT_ADV_PRINT says. Under NOADV the character is inside the FD's record and the lengths rule alone applies",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FASTSRT_STATUS,
        claim: "A file whose I/O DFSORT does keeps the FILE STATUS it had through the SORT, and a GIVING relative file's RELATIVE KEY is not set (Programming Guide SC27-8714-03, pp. 232-233)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FASTSRT_FAILURE,
        claim: "A failure on a file whose I/O DFSORT does, including an empty VSAM input file, fails the SORT with SORT-RETURN 16 and the run goes on, with or without a FILE STATUS",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SAME_AREA_VSAM,
        claim: "SAME AREA makes the VSAM (indexed and relative) files it names share one record area, as SAME RECORD AREA does, and is documentation for the others (Language Reference SC27-8713-03, p. 156)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LONG_ZONED_BY_PACKS,
        claim: "A zoned item of more than 16 digits, too long for one PACK (SA22-7832-14: each operand at most 16 bytes), enters arithmetic through two or three PACKs, the high-order part first and each lower PACK overwriting the byte the part above it ended in, so it packs as ZONED_BY_PACK says one PACK would: every zone but the sign's discarded and no digit checked until the arithmetic uses it. An alphanumeric sender longer than 31 characters moved to a numeric item is packed from its rightmost 31, the most any receiver holds",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ASCII_COLLATION,
        claim: "STANDARD-1 and STANDARD-2 put the characters of 7-bit ASCII in its order (Language Reference SC27-8713-03, Table 82, p. 754; Programming Guide SC27-8714-03, p. 7), each found in the program's code page; the characters that are not 7-bit ASCII follow them in EBCDIC order, as the characters an ALPHABET literal leaves out do",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: TABLE_SORT_COLLATION,
        claim: "A table SORT without a COLLATING SEQUENCE phrase orders alphanumeric keys in EBCDIC, as the SORT statement's format 2 rules say (SC27-8713-03, p. 450), though its rules for the phrase in both formats put the PROGRAM COLLATING SEQUENCE in the phrase's place (p. 451)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALPHABET_LITERALS,
        claim: "Among an ALPHABET clause's literals HIGH-VALUE, LOW-VALUE, SPACE, ZERO and QUOTE are the EBCDIC characters X'FF', X'00', X'40', X'F0' and X'7F', whatever the alphabet makes HIGH-VALUE and LOW-VALUE, and a numeric literal n is the character at ordinal n of EBCDIC, whose ordinals SC27-8713-03 points to (p. 128)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_CEEIGZCT,
        claim: "A COPY CEEIGZCT that no library answers gives a level-88 condition name for each of the 723 conditions whose entry in the Language Environment Runtime Messages (SA38-0686-60, chapter 1) shows a symbolic feedback code, severity from its message's I, W, E, S or C; the name is CEE and the message number in base 32 (Programming Guide SA38-0682-60, CEEBLDTX :msgname.), the value the token's first 8 bytes as LE_FEEDBACK_TOKEN lays them out (Programming Reference SA38-0683-60, CEENCOD), and CEE000 is all zeros (SA38-0682-60, testing a condition token for success); it is meant to follow the 8-byte group the token starts with (SA38-0682-60, Figure 76). It is written from those manuals alone, not from IBM's CEEIGZCT",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_CEEIGZCT_DISAGREEMENTS,
        claim: "Where the manuals disagree CEEIGZCT follows the message: its severity letter over a service's table in SA38-0683-60 (CEE07V, CEE317, CEE35S, CEE36V to CEE374), and its number over the code SA38-0686-60 prints beside it (CEE0356C shows CEE0BA, CEE5722I CEE5IP, CEE5771S CEE5KC); the eight messages printed with no code (CEE3252E, CEE3257E to CEE3259E, CEE3596S, CEE3796I to CEE3798I) have no name",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_SHORT_ARGUMENT_LIST,
        claim: "A CALL of a service with fewer arguments than its syntax lists, CEE3ABD with no USING among them, ends the run with ironwork's own abend, not a modelled one: IBM calls a short list invalid with unpredictable results (SA38-0683-60, General usage notes for callable services) and says nothing of register 1 at a CALL without USING, whose own CALL and CEEPCALL macros leave it unaltered when no parameter is coded (MVS Assembler Services Reference SA22-7606-13, CALL; SA38-0682-60, CEEPCALL); the service then reads its arguments through whatever register 1 and the storage past the list hold, so neither S0C4 nor any other result follows. A missing fc is not taken as OMITTED, nor a missing clean-up as none",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_CONTROL_CHARACTER,
        claim: "A sequential file that a WRITE with ADVANCING in the program names, or whose FD has LINAGE, is a print file: every record written to it carries a printer control character, a WRITE without ADVANCING being AFTER ADVANCING 1 LINE; ASA characters when every WRITE ... ADVANCING of the file says AFTER, machine codes when any says BEFORE (Language Reference SC27-8713-03, p. 479). Under ADV, the default, the character is a byte before the record; under NOADV it is the record's own first byte; a LINAGE file is ADV whatever the option (p. 480; Programming Guide SC27-8714-03, pp. 178-179, 346). ASA ' ', '0' and '-' space 1 to 3 lines before printing, '+' none, '1' to '9' and 'A' to 'C' skip to channels 1 to 12, PAGE and C01 being channel 1, except that PAGE moves a LINAGE file's paper in lines (LINAGE_PAGE_MOVEMENT), and CSP '+'; machine codes print then space, X'01', X'09', X'11', X'19', or skip, X'89' + 8(n-1) for channel n; AFP-5A is X'5A' (z/OS DFSMS Macro Instructions for Data Sets SC23-6852-60, pp. 397-398; Language Reference pp. 126-127, 483). SPECIAL-NAMES of a program apply to the programs it contains (p. 13)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_SPACING_RECORDS,
        claim: "A movement one control character cannot give is made with records that only move the paper, each as long as the line's record and blank after its control character: in an ASA file AFTER ADVANCING n lines above 3 is preceded by (n-1)/3 records with '-', the line taking the rest; in a machine-code file BEFORE ADVANCING n above 3 prints with X'19' and is followed by records spacing the rest without printing, three lines at a time (X'1B', then X'0B' or X'13'), and AFTER ADVANCING is such records, or X'8B' + 8(n-1) for channel n, then the line with X'01'",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_CONTROL_RUN_TIME,
        claim: "Under NOADV the control character is stored in the first byte of the record area, where the program sees it after the WRITE; an ADVANCING count below zero spaces no lines; under ADV a READ of a print file in the program that writes it skips the added byte",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: TEXT_PRINT_LINES,
        claim: "A text DD shows a print file's records without the control character, which it reads as line spacing, as the POSIX asa utility does: ' ', '0' and '-' put the line one, two or three lines below the last, the DD's first line starting on its first; '+' and X'01' overprint, after a carriage return when both lines show something; a skip to channel 1 is a form feed before the line; a skip to channels 2 to 12, or AFP-5A page mode data, is one line, a text DD having no forms control buffer; a machine code moves the paper after its line. Any other file's records are a line each",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LOCAL_REFERENCES_OUTSIDE_METHODS,
        claim: "A local reference made by a program that is not a method belongs to the method running when it runs, freed when that method returns; with no method running it belongs to the run unit and stays valid until the run ends, as the JNI keeps a thread's local references outside any native method, and the Guide asks for NewGlobalRef only 'if the client code is within a method' (Programming Guide SC27-8714-03, p. 703)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SELF_IS_LOCAL,
        claim: "SELF is a local reference of the method's own frame, made with the invocation, as the JNI passes a native method its object, and it expires when the method returns; the Guide's list of local references names arguments, RETURNING values, JNI results and NEW only (Programming Guide SC27-8714-03, p. 721)",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: EXPIRED_REFERENCE_ABENDS,
        claim: "Using a reference after it was freed ends the run with abend IRONWORK naming the item that holds it, how the reference was made and where it was freed: INVOKE on it, passing it to INVOKE or a JNI service, a method's RETURNING it, and comparing it with another object reference or SELF use it; SET, MOVE and CALL between programs copy its bytes without looking, and a comparison with NULL tests only its bytes. On z/OS a freed reference's slot is reused, and the Guide says only that 'an error occurs' (Programming Guide SC27-8714-03, p. 722)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LOCAL_FRAMES,
        claim: "The JNI reference services run as the JNI specification defines them: NewLocalRef and NewGlobalRef make a new reference to the object, NULL for NULL; DeleteLocalRef and DeleteGlobalRef free one and ignore NULL, and one given the other kind of reference ends the run; IsSameObject compares objects; GetObjectRefType answers 0 for NULL, 1 for local and 2 for global; PushLocalFrame opens a frame whose local references PopLocalFrame frees, giving back a local reference in the frame below to its argument's object, and EnsureLocalCapacity succeeds. PopLocalFrame with no frame of PushLocalFrame's open ends the run, and a method's return frees the frames it left open. The Guide documents NewGlobalRef, DeleteGlobalRef and DeleteLocalRef only (Programming Guide SC27-8714-03, pp. 722-723)",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OO_OPTIONS_SEVERITY,
        claim: "IBM names no message for object-oriented syntax compiled without THREAD, DLL, RENT or DBCS, nor for THREAD without RECURSIVE, which it calls an error (Messages and Codes SC27-4648-02, p. v, lists only some messages), so the severity of each rule of J13 is chosen. A missing option is a warning (W, return code 4), and the program runs as if compiled with it. NORENT with THREAD or DLL is a warning too: IBM forces RENT and 'generates an error message' (Programming Guide SC27-8714-03, p. 344), and the message it gives for an option dropped in conflict resolution is W, IGYOS4020-W, return code 4, in Enterprise COBOL job output quoted in the corpus (Delvoie_Mainframe, A5 instructions). THREAD without RECURSIVE, and INITIAL, a nested program, SORT of a file or MERGE under THREAD, are errors (S), as IBM diagnoses them as errors (J13). A program that reaches Java through JNIENVPTR alone, with no INVOKE or object reference, is held to none of them, as IBM builds its Bank-of-Z IBTRAN with DLL and without THREAD",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REFERENCES_KEPT,
        claim: "Every reference made is kept for the rest of the run, so that an expired one can say where it expired, and a run unit that makes more than 8,388,608 of them abends; the JVM reuses a freed reference's slot",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SQL_COMMIT_AT_NORMAL_END,
        claim: "A batch run unit commits at a normal end and rolls back at an abend: \"In all Db2 environments, the normal termination of a process is an implicit commit operation\" (Db2 12 for z/OS SQL Reference, COMMIT). Db2 for Linux rolls back instead",
        basis: Basis::Documented,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_WHENEVER_ORDER,
        claim: "WHENEVER tests SQLERROR (SQLCODE < 0), NOT FOUND (100) and SQLWARNING (SQLWARN0 W, or > 0 and not 100); the precompiler tests warning before not-found, and the three exclude one another (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_TRUNCATED_INDICATOR,
        claim: "A string cut to fit its host variable sets the indicator to its original length, SQLWARN0 and SQLWARN1, and SQLSTATE 01004; trailing blanks cut from a CHAR count (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_POSTGRES_ERRORS,
        claim: "PostgreSQL's SQLSTATEs map to Db2 SQLCODEs as sql-runtime.md §9 tabulates; Db2 for Linux agrees except -433 for -404 and -801 for -802, where the table keeps z/OS's documented codes (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_DIALECT_REWRITES,
        claim: "Db2 SQL is rewritten for PostgreSQL by the table in sql-runtime.md §9, and any other text runs unchanged",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_INTO_WITHOUT_COLONS,
        claim: "A name in an INTO list written without its colon is a host variable, as older precompilers read it",
        basis: Basis::Recalled,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_DECLARATIONS_CROSS_NESTED_PROGRAMS,
        claim: "WHENEVER and cursor declarations carry on in listing order across nested programs, as the precompiler reads the source in order",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_DOUBLE_TO_HFP_TRUNCATES,
        claim: "An IEEE double stored into COMP-1 or COMP-2 drops the low-order bits that do not fit, rather than rounding",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_ZONED_IS_DECIMAL,
        claim: "A zoned DISPLAY host variable without SIGN SEPARATE is DECIMAL to Db2, as SIGN LEADING SEPARATE is",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_ISO_DATETIME,
        claim: "Dates and times reach character host variables as YYYY-MM-DD, HH.MM.SS and YYYY-MM-DD-HH.MM.SS.NNNNNN, DSNHDECP's DATE(ISO) and TIME(ISO); the forms are as seen under DATETIME(ISO), and the default is an installation's (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_TRAILING_BLANKS_SENT,
        claim: "Character inputs are sent with their trailing blanks, since Db2 compares strings as if blank-padded (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQL_FETCH_ROW_COUNT,
        claim: "A single-row FETCH that returns a row sets SQLERRD(3) to 1; Db2 for z/OS documents SQLERRD(3) for a rowset FETCH only (Db2 12.1.5 for Linux, tools/db2-probe)",
        basis: Basis::Observed,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: FASTSRT_PRINT_RECORDS,
        claim: "Under FASTSRT DFSORT, not COBOL, does the I/O of the USING and GIVING files (Programming Guide SC27-8714-03, p. 232), and DFSORT adds a printer control character only to the lines of an OUTFIL report (DFSORT Application Programming Guide SC23-6878-50, p. 225), which needs control statements ironwork does not read. So DFSORT writes a GIVING print file's records as the SD holds them, with no control character, where COBOL writes each as a WRITE without phrases (Language Reference SC27-8713-03, p. 453), which for a print file is AFTER ADVANCING 1 LINE (Programming Guide pp. 178-179), the character over the record's first byte under NOADV; and DFSORT reads a USING print file's records as its data set holds them, where COBOL's READ skips the byte ADV adds (PRINT_CONTROL_RUN_TIME). Under NOADV the two read a USING print file alike",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FASTSRT_RECORD_LENGTHS,
        claim: "DFSORT takes the length of the records it sorts from the SORTIN data set, not from the RECORD statement (DFSORT Application Programming Guide SC23-6878-50, p. 420). A fixed-length record shorter than SORTOUT's LRECL is padded on the right with X'00', and a longer one cut on the right, with message ICE171I and return code 0 (pp. 14-15, 200, 209) under PAD=RC0 and TRUNC=RC0, the IBM-supplied defaults (DFSORT Installation and Customization SC23-6881-70, pp. 91, 102); padding needs the Blockset technique and a sort or copy, DFSORT checks neither padding nor truncation without both a SORTIN and a SORTOUT data set (pp. 200, 209), and it neither pads nor cuts the records an E15 or E35 exit returns (p. 15). A variable-length record longer than SORTOUT's LRECL ends DFSORT with ICE217A under NOVLLONG (p. 210), the IBM-supplied default (Installation and Customization p. 103; DFSORT Messages, Codes and Diagnosis SC23-6879-50, p. 76)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FASTSRT_ADV_PRINT,
        claim: "With --fastsrt-adv-print=include a print file under ADV is FASTSRT's as any other file is, and DFSORT meets its data set's records, a byte longer than the FD's. A USING file's data set stands as DFSORT's SORTIN and a GIVING file's as its SORTOUT, as the Programming Guide implies by keeping DFSORT's SORTIN and SORTOUT options from a FASTSRT program (SC27-8714-03, p. 232) without saying so, and COBOL gives DFSORT each key's place in the SD's record; so, by FASTSRT_PRINT_RECORDS and FASTSRT_RECORD_LENGTHS, a USING print file's records keep the control character as their first byte, each key being read a byte before where the FD has it, and a GIVING file's fixed-length records are padded with X'00' or cut to its data set's length. With no USING data set of DFSORT's own, as with an INPUT PROCEDURE or a USING file COBOL reads, a GIVING print file's longer fixed-length records fail the SORT before its input phase, as ICE043A reason 9 says of fixed-length output records longer than the input's (DFSORT Messages, Codes and Diagnosis SC23-6879-50, pp. 29-30). COBOL takes a record from DFSORT, for an OUTPUT PROCEDURE or a GIVING file it writes, at the SD's length at most, and a VSAM GIVING file takes it at its own. A text DD holds no control character, so on it a print file's records are the FD's length, and each record DFSORT writes is a line",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMPILER_SEVERITIES,
        claim: "A compiler message has one of five severities, each with a return code: I (informational) 0, the program runs correctly; W (warning) 4, a possible error; E (error) 8, an error the compiler attempted to correct; S (severe) 12, one it could not, and the program should not be run; U (unrecoverable) 16, the compilation ended. A compilation's return code is generally the highest of its messages' (Programming Guide SC27-8714-03, Table 38, p. 282), and the letter ends each message's identifier, as in IGYPS2121-S (p. 281). ironwork check exits with that return code, 0 when there is no message",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REFUSALS_ARE_SEVERE,
        claim: "Every refusal ironwork made before its messages had severities is S, return code 12, and none is E: IBM's E is an error the compiler corrects, still producing object code under the default NOCOMPILE(S) (Programming Guide SC27-8714-03, pp. 282, 355), and ironwork corrects nothing, so a program it refuses has no code to run. Where IBM documents a lower severity ironwork now follows it: NUMPROC(MIG) is W with the default NUMPROC, an invalid suboption is E with the option discarded, and a non-COBOL character is IGYLI0163-E (C120 onward). U is not used: ironwork's reader stops at the first syntax error, which IBM reports and reads past",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REFUSED_FROM_S,
        claim: "ironwork run and cics run a program whose compile's return code is 0, 4 or 8, printing its messages first, and refuse one at 12 or more, exiting with that return code, as IBM's IGYWCLG procedure runs one: its GO step is bypassed only when 8 is less than the compile step's return code, COND=((8,LT,COBOL),(4,LT,LKED)) (Programming Guide SC27-8714-03, pp. 259-260), and the default NOCOMPILE(S) produces object code after E-level messages, stopping it at the first S-level one (p. 355). A card's COMPILE or NOCOMPILE, or -warnings-block, moves the refusal (NOCOMPILE). No message of ironwork's is E yet (REFUSALS_ARE_SEVERE)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NOCOMPILE,
        claim: "COMPILE, abbreviated C, produces object code whatever the messages; NOCOMPILE(W), NOCOMPILE(E) or NOCOMPILE(S), abbreviated NOC, stops it at the first message of that severity or higher, NOCOMPILE(S) being the default; and NOCOMPILE alone is a syntax check with no object code (Programming Guide SC27-8714-03, p. 355). ironwork reads them from a CBL or PROCESS card, the last one given winning (p. 344): run and cics refuse a program with a message at or above the level, under NOCOMPILE whatever its messages, and under COMPILE from S as under NOCOMPILE(S): COMPILE's object code after an S-level message runs with results IBM calls unpredictable (p. 355), and IGYWCLG's COND bypasses its GO step above 8 whatever the object code (REFUSED_FROM_S). A program ironwork cannot parse or lay out is refused under any of them. -warnings-block is ironwork's command-line NOCOMPILE(W), and a card's COMPILE or NOCOMPILE wins over it, as options on a PROCESS or CBL statement take precedence over the compiler invocation's (p. 273). None of them changes the return code, the highest of the messages' (p. 282), so check exits as it would without them. IBM has no option that turns a warning into an error: FLAG(x,y) chooses only which messages are listed (pp. 369-370), and a MSGEXIT user exit of the EXIT option can raise a W or I message to any severity up to S, one message at a time, which changes the return code (pp. 836-837)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMMENT_ENTRY_EXTENT,
        claim: "The comment-entry of AUTHOR, INSTALLATION, DATE-WRITTEN, DATE-COMPILED or SECURITY is any characters at all, written in Area B on one or more lines and never in Area A (Language Reference SC27-8713-03, p. 117), and a COPY or REPLACE in it, or where it can appear, is part of it (pp. 700, 708). So a comment-entry runs from its paragraph header's period to the next line, not a comment or blank line, with a character in Area A, whatever the lines between hold; NIST's OBNC1M tests this with a whole program written in Area B inside a SECURITY comment-entry",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMMENT_ENTRY_HEADERS,
        claim: "ironwork takes a line in an IDENTIFICATION DIVISION as a comment-entry paragraph's header when its first word is the paragraph's name and a period follows, in whatever column the name starts, though a paragraph header belongs in Area A (Language Reference SC27-8713-03, p. 55); with no period the paragraph's text is read as program text. A line with a hyphen in column 7 inside a comment-entry, which p. 117 does not permit, is taken as more of the comment-entry rather than refused: the manual does not say how severe Enterprise COBOL's message for either is",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMMENT_ENTRY_REMARKS,
        claim: "Enterprise COBOL has no REMARKS paragraph: the Language Reference SC27-8713-03 names neither the paragraph (pp. 101, 117) nor the word anywhere. ironwork reads a REMARKS paragraph in an IDENTIFICATION DIVISION as it reads a comment-entry paragraph, as OS/VS COBOL did, where Enterprise COBOL presumably refuses the program",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CONTINUED_LITERAL_QUOTES,
        claim: "A continuation line of an alphanumeric or national literal left open at column 72 has a hyphen in column 7 and a quotation mark as its first nonblank character, and the literal resumes after that mark. When a literal's closing quotation mark is in column 72 and the continuation line starts with two, the pair stands for one quotation mark in one literal; otherwise, a quotation mark that starts a continuation line after a closed literal starts a second literal (Language Reference SC27-8713-03, p. 58). The rules hold for apostrophes alike, and ironwork gives the pair precedence where both could apply",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COPY_SEARCH_ROUNDS,
        claim: "A COPY member is looked for in three rounds, each through every copy library in order (the program's own directory, then each -I directory) before the next begins: as NAME.cpy, .CPY, .copy and .COPY; then as .cbl, .CBL, .cob and .COB; then as the name alone; each extension with the name as written, then upper-cased, then lower-cased. In one z/OS UNIX directory IBM tries .cpy, .CPY, .cbl, .CBL, .cob and .COB, each with the name upper- and lower-cased (Language Reference SC27-8713-03, p. 705), and the name alone last (Programming Guide SC27-8714-03, p. 440), searching the current directory, the -I directories and SYSLIB's in turn (LR p. 705, PG p. 441); it does not say whether an earlier directory's .cbl comes before a later one's .cpy. A compile from JCL searches SYSLIB's data sets, then COPYLOC's (LR p. 705), which hold copybooks and not the library the program is read from, so the rounds take a copybook in any library before a program source. .copy is not IBM's, and the name as written, which IBM folds to upper case (PG p. 440), comes first, so that on a file system that ignores case a message names the member as the program spells it; the order differs from IBM's only where two files' names differ only in case",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COPY_NOT_THE_PROGRAM,
        claim: "The file being compiled is never a COPY member of its own compilation, at any depth of nesting: a compile from JCL reads the program from SYSIN and looks for members in SYSLIB and COPYLOC (Language Reference SC27-8713-03, p. 705), so a member named as the program comes from another file or is not found. The program's own directory is a copy library only as ironwork's stand-in for the current directory cob2 searches first (p. 705). Any other member a chain of COPY statements reaches while it is still being copied is refused, as a nested COPY cannot cause recursion (p. 697)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COPY_LITERAL_AS_WRITTEN,
        claim: "A text-name written as a literal is looked for as written first, through every library, and then with extensions as a user-defined word is (COPY_SEARCH_ROUNDS). IBM takes a literal as the file name, relative path or absolute path it spells (Programming Guide SC27-8714-03, p. 440) and adds extensions only to a name that is not a literal (Language Reference SC27-8713-03, p. 705); the extensions are ironwork's, for sources written for compilers that add them",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COPY_DOUBLED_PERIOD,
        claim: "COPY X.. is refused, naming X.: a COPY statement ends with a separator period (Language Reference SC27-8713-03, p. 697), which is a period followed by a space (pp. 49-50), so the name is X., not X. A text-name or library-name for a data set holds only letters, digits and hyphens (p. 696); in z/OS UNIX directories any COBOL character may appear (p. 697), and X. would then name X..cpy and the like, not X's file. The manuals do not say whether Enterprise COBOL instead reads X and a separator period, leaving the second period in the text, as GnuCOBOL 3.2 does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_COUNTER,
        claim: "LINAGE IS n LINES [WITH FOOTING AT f] [LINES AT TOP t] [LINES AT BOTTOM b] describes a logical page of t + n + b lines, each page following the last with no spacing; TOP and BOTTOM default to 0, and each value is an unsigned integer, at most 99,999,999, or an unsigned integer data item (Language Reference SC27-8713-03, pp. 189-190, 747). OPEN OUTPUT or EXTEND takes all four for the first page, and a WRITE ... ADVANCING PAGE or a page overflow takes the data items' values again for the next page (p. 190). LINAGE-COUNTER, one for each LINAGE file and qualified by its file-name when there are two, has the PICTURE and USAGE of the page body's data item, or is binary with as many digits as its integer; OPEN sets it to 1, it is the line of the page body the printer is at, and no statement may change it (pp. 23-24, 70). A WRITE adds its ADVANCING lines to it, 1 without ADVANCING; a WRITE that would take it past the page body puts its line on the next page's first line, after the printer moves there (AFTER) or before (BEFORE), as ADVANCING PAGE does, and sets it to 1 (pp. 474-475). END-OF-PAGE, which needs LINAGE, runs once the line is written when LINAGE-COUNTER has reached the footing line or the page overflowed, only an overflow counting when FOOTING is not given, and NOT END-OF-PAGE otherwise, neither after a WRITE that failed (p. 475; Programming Guide SC27-8714-03, p. 178). LINAGE takes effect only for a file opened OUTPUT or EXTEND (p. 189), and an SD's does nothing (p. 190)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_PAGE_MOVEMENT,
        claim: "A WRITE to a LINAGE file moves the paper in lines, never by a skip to a channel, the logical page not being the printer's form (Language Reference SC27-8713-03, p. 190). When OPEN ends the printer is at the first page's first line, t lines of top margin above line 1 of its page body, so the first WRITE moves those t lines besides its own; a WRITE that starts a new page, by overflow or ADVANCING PAGE, moves the lines left in the page body, the bottom margin, the next page's top margin and one line more, to that page's line 1. The lines are written as any WRITE ... ADVANCING writes them, as control characters and spacing records (PRINT_CONTROL_CHARACTER, PRINT_SPACING_RECORDS), so the margins are blank lines, and a text DD shows them as line feeds (TEXT_PRINT_LINES). OPEN and CLOSE write nothing: a file closed with no WRITE is empty, and the last page is not spaced out",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_END_OF_PAGE,
        claim: "The end-of-page condition is judged by LINAGE-COUNTER once the WRITE is done, as the Language Reference words it (SC27-8713-03, p. 475): a WRITE ... ADVANCING PAGE, which leaves LINAGE-COUNTER at 1, raises it only when the next page's FOOTING is 1, whatever line it printed on, and a WRITE ... ADVANCING 0 LINES in the footing area raises it again",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_VALUES,
        claim: "A page body under 1 line, a footing line outside the page body or a margin below 0 that data items give at OPEN or at a new page ends the run with ironwork's own abend naming the file: the Language Reference states the rule (SC27-8713-03, p. 189) but no file status for breaking it (Table 34, pp. 300-303), and the Language Environment Runtime Messages (SA38-0686-60) have no message for it. Integers that break it are compile errors",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_EXTEND,
        claim: "OPEN EXTEND of a LINAGE file starts a new logical page as OPEN OUTPUT does, top margin first and LINAGE-COUNTER at 1 (Language Reference SC27-8713-03, p. 24), whatever line the data set's last page ended on",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LINAGE_COUNTER_BETWEEN_WRITES,
        claim: "LINAGE-COUNTER is zero until the file is first opened and keeps its value after CLOSE; a WRITE that fails leaves it and the page where they were; OPEN INPUT and I-O set it to 1 as OPEN OUTPUT does, since the Language Reference says so of any OPEN (SC27-8713-03, p. 24)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_FILE_UPDATE,
        claim: "A print file opened I-O under ADV holds the control character a byte before each FD record, as it was written (PRINT_CONTROL_CHARACTER): READ skips the byte as PRINT_CONTROL_RUN_TIME says, REWRITE writes the record behind the byte it was read with, REWRITE having no ADVANCING phrase, and WRITE, with or without ADVANCING, fails with file status 48, a sequential file's WRITE needing OUTPUT or EXTEND (Language Reference SC27-8713-03, pp. 471, 476; Table 34, p. 302); LINAGE has no effect on it (p. 189). IBM does not say what REWRITE does with the byte",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ENTRY_IN_SEQUENCE,
        claim: "Control that reaches an ENTRY statement in the program's own sequence passes it as it passes CONTINUE: nothing is bound and no storage changes. The Language Reference says only where a CALL of the entry begins, at the first executable statement after it (SC27-8713-03, pp. 339-340)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ENTRY_CALLS,
        claim: "A CALL of an ENTRY name begins at the statement after the ENTRY and binds the entry's USING list alone, so a LINKAGE item only the PROCEDURE DIVISION USING names has no address (Language Reference SC27-8713-03, pp. 264, 320, 339-340). A static CALL, of a literal under NODYNAM, enters the one copy of the program that its PROGRAM-ID enters, in its last-used state (Programming Guide SC27-8714-03, pp. 548, 553-554); a dynamic CALL, of an identifier or of a literal under DYNAM, gets for each entry name a copy of the program with WORKING-STORAGE of its own, as if a separate compile unit were called (p. 560), which a CANCEL of that name resets, where p. 549 says only that a second entry point must not be called dynamically without a CANCEL between. CALL finds an entry name among the programs of the source and those already loaded, then in a program library as a member of that name, an alias, as NAME(ALIAS) or binder ALIAS statements make one (pp. 548, 560)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALTERED_GO_TO_RESET,
        claim: "Altered GO TOs are put back as written whenever the program's WORKING-STORAGE is initialized: its first CALL, the first after a CANCEL of it, and every CALL of an INITIAL program (Language Reference SC27-8713-03, pp. 103, 318), and not on another CALL (Programming Guide SC27-8714-03, p. 548). Those of an independent segment, priority 50 to 99, are put back when control reaches the segment from a paragraph of another priority by falling into it, GO TO, PERFORM or a SORT or MERGE procedure, but not when a PERFORM made from it returns, nor when a CALL of the program begins in it: p. 265 says 'from a segment with a different priority-number', p. 318 'from another independent segment', and neither names a PERFORM's return",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DISPLAY_STREAM,
        claim: "DISPLAY writes to standard output as a program under z/OS UNIX does with its OUTDD ddname unallocated and _IGZ_SYSOUT unset: one stream of characters, a newline after each DISPLAY except one WITH NO ADVANCING (Programming Guide SC27-8714-03, pp. 36-37; Language Reference SC27-8713-03, pp. 333-335), and UPON any device writes to the same stream. To a ddname IBM writes a record for each DISPLAY, whose first byte is ' ', or '+' after one WITH NO ADVANCING (Programming Guide p. 37); that route is not modelled",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: RANDOM_GENERATOR,
        claim: "FUNCTION RANDOM is Park and Miller's minimal standard generator. An argument n starts a sequence at state n mod 2147483646 + 1, a first reference without one starts it as 0 does, and each reference sets the state s to 16807 s mod 2147483647 and returns the new state divided by 2147483647 in long HFP, which is exclusively between zero and one; so the arguments 0 to 2,147,483,645 give distinct sequences and larger ones repeat them. IBM documents the interface (Language Reference SC27-8713-03, p. 629), a long floating-point result under either ARITH (Programming Guide SC27-8714-03, p. 58), and that its generator is not CEERAN0's, 950706376 s mod 2147483647 (Programming Reference SA38-0683-60, pp. 343-344), but not the generator. A fractional argument is truncated to an integer, and a negative one ends the run with abend IRONWORK",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ZERO_DIVISOR_CHECK,
        claim: "A zero divisor that no ON SIZE ERROR phrase takes, in an arithmetic statement without one or in an expression outside an arithmetic statement (a condition, a subscript, a reference modifier), is the program check of the instruction the compiler divides with: HFP divide, S0CF, when the expression is evaluated in floating point; fixed-point divide, S0C9, when the dividend and the divisor are made only of integer binary items and integer literals, at least one an item; decimal divide, S0CB, otherwise. The size error condition belongs to the arithmetic statements alone, and with ON SIZE ERROR any zero divisor, floating-point too, is one (Language Reference SC27-8713-03, p. 296; Programming Guide SC27-8714-03, p. 242); the manuals do not say which instructions the compiler divides with",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ERROR_DECLARATIVE_MODE,
        claim: "An EXCEPTION/ERROR procedure for an open mode serves a file open in that mode, and for OPEN one being opened in it, an OPEN of a file already open included (Language Reference SC27-8713-03, pp. 417, 714). A file that is not open, as for a READ, WRITE or CLOSE before its OPEN, is in no mode, so only a procedure that names it serves it. A procedure that names the file comes first (p. 714), and two procedures for one file, or for one open mode, are refused, as p. 714 forbids simultaneous requests for two",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ERROR_DECLARATIVE_STATUSES,
        claim: "Every I/O status whose first digit is not 0 runs the file's EXCEPTION/ERROR procedure once FILE STATUS holds it, unless the statement's AT END or INVALID KEY phrase takes a 1x or 2x status, and then no procedure runs (Language Reference SC27-8713-03, pp. 299, 303-304, 432, 714); a 0x status runs none. The procedure returns control to the end of the statement, and NOT AT END and NOT INVALID KEY are not run: none of ironwork's statuses is a critical error, after which p. 714 says control does not return. The implicit CLOSE at the end of the run or at CANCEL runs no procedure (Programming Guide SC27-8714-03, pp. 179, 204, 219)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_FILE_DECLARATIVE,
        claim: "An EXCEPTION/ERROR procedure serves the OPEN, READ, WRITE and CLOSE a SORT or MERGE does of its USING and GIVING files (Language Reference SC27-8713-03, pp. 457-458); the end of a USING file runs none. After it, that file's processing ends: a USING file gives the records read before the failure and is closed, a GIVING file is closed, and the operation goes on with SORT-RETURN 0, unless the procedure moved 16 to SORT-RETURN, which stops it at once with SORT-RETURN 16, as the Programming Guide has the procedure do to report the failure (SC27-8714-03, pp. 232, 234-235). A file with no procedure fails as SORT_FILE_FAILURE says. Under FASTSRT a USING or GIVING file that an INPUT, OUTPUT or file-specific procedure serves is COBOL's (p. 233), which FASTSRT_FILES does not list",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUG_RUNTIME_OPTION,
        claim: "USE FOR DEBUGGING procedures run only in a program compiled WITH DEBUGGING MODE and run under the Language Environment runtime option DEBUG, which -debug stands for; NODEBUG, the default, keeps them from running, and debugging lines, once compiled, run under either (Language Reference SC27-8713-03, pp. 771-772; Programming Guide SC27-8714-03, pp. 431, 446). Without WITH DEBUGGING MODE both are comments, and a contained program has the mode of the program containing it (LR pp. 121, 772)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUG_LINE_NUMBER,
        claim: "DEBUG-LINE holds, in six digits with leading zeros, the number of the line the statement starts on in its own source file. Under NONUMBER, the default, IBM puts the compiler-generated number there (Language Reference SC27-8713-03, p. 19), the listing's line number, which counts the lines of COPY members too: a statement after a COPY, or in a member, is numbered otherwise by IBM",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUG_LINE_STATEMENT,
        claim: "The statement DEBUG-LINE names is the one that sent control to the procedure (Language Reference SC27-8713-03, p. 20): the PERFORM on each repetition, the GO TO, the SORT or MERGE, the input-output statement whose condition ran a USE procedure; for fall through, the statement last started in the procedure before, or the section header control passed through. CONTINUE, EXIT, NEXT SENTENCE and a separator period carry no position in ironwork, so after one of them DEBUG-LINE names the statement before it",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUG_CONTENTS_LENGTH,
        claim: "DEBUG-CONTENTS is 30 characters, so DEBUG-ITEM is 86 bytes. The Language Reference gives it as PICTURE X(n) (SC27-8713-03, p. 19), and the procedures Enterprise COBOL debugs put at most 13 characters in it (p. 20)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUG_NAME_FORM,
        claim: "DEBUG-NAME is the procedure-name as the USE FOR DEBUGGING sentence writes it, a section that qualifies it joined by OF (Language Reference SC27-8713-03, p. 19); under ALL PROCEDURES it is the procedure's own name, unqualified. Control entering a section runs the section's debugging procedure, then, by fall through, that of its first paragraph",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DEBUGGING_SECTION_REFERENCES,
        claim: "A debugging section may PERFORM or GO TO a procedure of another debugging section, and of an EXCEPTION/ERROR section: p. 771 of the Language Reference (SC27-8713-03) forbids referring to a procedure in a debugging section from a statement outside of the debugging section, read here as outside every debugging section, as the CCVS85 DB tests assume, and p. 716 forbids references to nondeclarative procedures only. Without WITH DEBUGGING MODE a debugging section is a comment in full, header and USE sentence included, so its names are not defined and its text is not checked (p. 772)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: GLOBAL_DECLARATIVES,
        claim: "A program's declaratives run for its own statements only. USE GLOBAL AFTER EXCEPTION/ERROR for an open mode, and USE GLOBAL BEFORE REPORTING for a report group of a contained program without its own procedure for it, would serve another program's statements (Language Reference SC27-8713-03, p. 715; Report Writer Precompiler SC26-4301-04, 4.7.2 rule 5 and 4.7.3 rule 4), and are refused in a program that contains others; elsewhere GLOBAL changes nothing. A GLOBAL procedure for a named file is kept, since a contained program cannot name another program's file in ironwork, which has no GLOBAL files",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SYNC_SUBORDINATE_GROUP,
        claim: "SYNCHRONIZED on a group at level 02 to 49 synchronizes each elementary item within it, as the clause does on a level-01 group; the Language Reference allows it on elementary items and level-01 groups (SC27-8713-03, p. 231) and says nothing of other groups",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SYNC_ONLY_WHEN_WRITTEN,
        claim: "A binary, COMP-1, COMP-2, POINTER or INDEX item is aligned only when SYNCHRONIZED applies to it. Table 15 of the Language Reference says a subordinate binary item is aligned on 2 or 4 bytes 'when the synchronized clause is not specified', and in the same row that 'when SYNCHRONIZED is not specified for binary items, no space is reserved for slack bytes' (SC27-8713-03, p. 232); ironwork takes the second, and the first as a slip for 'specified'. A synchronized binary item of 10 to 18 digits is aligned on 4 bytes, as the slack-byte algorithm gives (p. 233), not on 8",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SYNC_SLACK_OWNER,
        claim: "Slack bytes before a synchronized item belong to the group of the elementary item before it (Language Reference SC27-8713-03, p. 234), so a group that ended just before them grows by them; but when that group is a table or a redefinition, or ends earlier than the slack begins, the slack bytes stay in the group that holds it and the table's occurrences keep their length",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SYNC_REDEFINES_REFUSED,
        claim: "A synchronized item that starts a redefinition where its boundary would need slack bytes stops the compile. The Language Reference says such an item must not need them and that the redefined item must be aligned for it (SC27-8713-03, pp. 232-233), but not what the compiler does when a program breaks the rule",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DECIMAL_COMMA_SEPARATOR,
        claim: "Under DECIMAL-POINT IS COMMA a comma between digits, or after a space, parenthesis or sign and before a digit, is a numeric literal's decimal point, and a comma after any other word is a separator even with a digit after it, so T(1,2) has the one subscript 1,2 and T(I,2) has two. The Language Reference says only that a separator comma is a comma followed by a space (SC27-8713-03, p. 49) and that the clause exchanges the comma's and the period's functions in numeric literals (p. 131); a period before a digit is then an error",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DECIMAL_COMMA_DISPLAY_LITERAL,
        claim: "DISPLAY of a numeric literal writes it as the program wrote it, so under DECIMAL-POINT IS COMMA its decimal point is a comma; the Language Reference says nothing of how DISPLAY shows a numeric literal (SC27-8713-03, pp. 333-334)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ARITH_DIGIT_LIMITS,
        claim: "A numeric or numeric-edited PICTURE whose digit positions, scaling positions P included, exceed 18 under ARITH(COMPAT) or 31 under ARITH(EXTEND), and a numeric literal with more digits than that, stop the compile (Language Reference SC27-8713-03, pp. 45, 209, 217-218; Programming Guide SC27-8714-03, p. 349). Neither manual gives the diagnostic's severity, and P is counted for every numeric item, where the Language Reference counts it for numeric-edited items and arithmetic operands (p. 209)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MULTIPLE_RESULTS,
        claim: "An arithmetic statement with several receivers computes what they share once, before any is stored, and each receiver in turn then takes it or combines it with its own current value, its subscripts evaluated then (Language Reference SC27-8713-03, p. 298). For ADD, SUBTRACT, MULTIPLY and DIVIDE without GIVING the shared part is the operands other than the receiver; for COMPUTE and the GIVING forms it is the whole expression, so a receiver that COMPUTE names twice gets the same result twice",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALTER_DEBUGGING,
        claim: "Once an ALTER has run, the debugging section that serves each paragraph it alters runs, once for each TO [PROCEED TO] phrase in the order written, with DEBUG-LINE the ALTER, DEBUG-NAME the altered paragraph as DEBUG_NAME_FORM gives it, and DEBUG-CONTENTS the procedure-name after TO PROCEED TO, a qualifier after OF (Language Reference SC27-8713-03, pp. 19-20, 716). A procedure named only after TO PROCEED TO gets no debugging section from the ALTER: p. 716 says an ALTER 'referring to the named procedure', and Table 2 on p. 20 has an ALTER row for procedure-name-1 alone, as CCVS85 DB105A expects under ALL PROCEDURES. Under ALL PROCEDURES an ALTER in the declaratives runs none, as p. 716 says; there an ALTER of a paragraph that a USE FOR DEBUGGING names still runs its section, as p. 716 makes no exception for it",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PERFORM_RETURN_POINTS,
        claim: "An out-of-line PERFORM arms a return point at the end of its range's last paragraph (Language Reference SC27-8713-03, p. 419), one per activation, since a CALL resets return points (Programming Guide SC27-8714-03, p. 547). Control that passes that end by any path, falling through or by GO TO, returns to the PERFORM, so PERFORM B THRU A with A before B returns when control reaches the end of A, and a range that passes the end of another active PERFORM's range returns there to that PERFORM. Neither manual says what a PERFORM that control leaves by GO TO leaves behind: its return point stays armed, as the Programming Guide's warning against ranges that keep control from the end implies (p. 772), until control passes it and returns after that PERFORM, which then puts back the point it displaced; ironwork refuses at run time to return so into a PERFORM that repeats or is inside another statement. EXIT SECTION goes to the end of the section, past the return point of a performed paragraph in it (LR p. 345)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_FUNCTION_ARGUMENTS,
        claim: "A floating-point argument, item or expression, is allowed wherever a function takes a numeric argument and refused where it takes an integer (Language Reference SC27-8713-03, p. 507). INTEGER and INTEGER-PART of one return an integer of 30 digits, 31 under ARITH(EXTEND), and ABS, MAX, MIN and REM with one are evaluated in floating point and return it (Programming Guide SC27-8714-03, pp. 799 and 801). The guide names REM a mixed function where the Language Reference types it numeric (p. 633), and gives the precision only of floating-point functions: a mixed function is evaluated here in long floating point, extended under ARITH(EXTEND), as they are",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_FUNCTION_ROUNDING,
        claim: "The floating-point intrinsic functions are computed in binary to 128 bits and rounded to the nearest long HFP value under ARITH(COMPAT), extended under ARITH(EXTEND), ties away from zero; a fixed-point argument is first converted to HFP of that precision (C5). IBM computes SQRT, EXP, EXP10, LOG, LOG10 and the trigonometric functions with Language Environment's math services, CEESDSQT and the rest (Programming Guide SC27-8714-03, p. 58), whose results can differ from the nearest value in the last hexadecimal digit. ANNUITY, PRESENT-VALUE and the statistics functions, which have no such service, are computed the same way, where IBM's generated code may truncate at each HFP step",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOATING_POINT_FUNCTIONS,
        claim: "ACOS, ANNUITY, ASIN, ATAN, COS, LOG, LOG10, MEAN, MEDIAN, MIDRANGE, PRESENT-VALUE, RANDOM, SIN, SQRT, STANDARD-DEVIATION, TAN and VARIANCE are floating-point functions, as earlier Programming Guides listed them; E, PI, EXP, EXP10 and NUMVAL-F are, as the Language Reference says (SC27-8713-03, pp. 553-557, 609); SECONDS-FROM-FORMATTED-TIME is, as its example's inexact result shows (p. 507), and SECONDS-PAST-MIDNIGHT with it. ABS, MAX, MIN, RANGE, REM and SUM are floating point when any argument is (Programming Guide, p. 799; C100). An expression holding a floating-point function is evaluated in floating point (pp. 62-63). NUMVAL and NUMVAL-C, which the Language Reference also calls floating point (pp. 605, 608), stay fixed point here for now",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_DOMAIN,
        claim: "An argument outside a function's domain ends the run with abend IRONWORK: SQRT of a negative number, LOG or LOG10 of zero or less, ASIN or ACOS beyond -1 to +1, ANNUITY with a negative rate or periods that are not a positive integer, PRESENT-VALUE at a rate of -1 or less, FACTORIAL beyond 28 (29 under ARITH(EXTEND)), a century window whose end year is outside 1700 to 9999, HEX-TO-CHAR or BIT-TO-CHAR of other characters or of a length that is not a multiple of 2 or 8. IBM leaves such values undefined (Language Reference SC27-8713-03, p. 500) and Language Environment's math services signal a condition. SIN, COS and TAN of an argument beyond 2^63 times pi/2, which ironwork does not reduce, end the run the same way. A result beyond HFP's range is an exponent overflow, S0CC, and one below it zero (C8)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMVAL_TEST_RULES,
        claim: "TEST-NUMVAL, TEST-NUMVAL-C and TEST-NUMVAL-F follow the formats of NUMVAL, NUMVAL-C and NUMVAL-F (Language Reference SC27-8713-03, pp. 605-609, 651-655), choosing where they are silent: CR and DB in either case; NUMVAL-C's grouping separator only between digits and before the decimal point; NUMVAL-F's E in either case, spaces allowed around it, its exponent sign optional, and the 16-digit mantissa limit with an exponent not checked; a string that stops short, or holds only spaces, gives its length + 1. NUMVAL-C's currency string defaults to $. NUMVAL-F of a string that breaks them returns zero, as NUMVAL does here",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_CLOCK,
        claim: "SECONDS-PAST-MIDNIGHT, FORMATTED-CURRENT-DATE, whose offset is therefore +0000, and the year of execution that YEAR-TO-YYYY, DATE-TO-YYYYMMDD and DAY-TO-YYYYDDD window by, read the run unit's clock as UTC to the hundredth of a second, as CURRENT-DATE does here (+0000); z/OS gives local time, and finer seconds (Language Reference SC27-8713-03, p. 631)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: UUID4_SOURCE,
        claim: "UUID4 takes its 122 random bits from the process's randomly keyed hasher over the clock, not from a cryptographic generator; IBM uses the Message-Security-Assist random number facility where the machine has it (Language Reference SC27-8713-03, p. 669). The version and variant bits are set and the string is lowercase, as IBM's example shows",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FORMATTED_DATETIME_RULES,
        claim: "The FORMATTED functions, INTEGER-OF-FORMATTED-DATE, SECONDS-FROM-FORMATTED-TIME and TEST-FORMATTED-DATETIME take the formats of the Language Reference (SC27-8713-03, pp. 504-506), choosing where it is silent or inconsistent: the decimal separator of a fractional-seconds format appears in the data, as its rules say, though several of its examples omit it (pp. 561, 566, 568, 629); fractional seconds are truncated; a comma may stand for the period; a UTC format moves the date as well as the time by the offset; a week may be 53 where the ISO year has 53 weeks, though p. 506 says 01 to 52; an offset sign of 0 takes only 00 hours and minutes; INTEGER-OF-FORMATTED-DATE reads the date part alone, as p. 579 says the time part does not change its result. TEST-FORMATTED-DATETIME names the first position at which a field can no longer be in range, or a value longer than its format errs at the first extra character. A format that is not one of IBM's, which Enterprise COBOL refuses at compile time, ends the run here",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ROUNDED_EXTRA_PLACE,
        claim: "A receiver named with ROUNDED counts in dmax with one decimal place more than it holds, so a quotient, or an intermediate cut back to dmax places, keeps the digit that rounding reads: DIVIDE 44.1 INTO a PIC 9(4)V9 of 1661.7 ROUNDED gives 37.7, as CCVS85 NC117A and NC171A expect. The Programming Guide says only that under ROUNDED one more decimal place, and one more integer place, might be carried for accuracy if necessary (SC27-8714-03, p. 794); the Language Reference's ROUNDED phrase compares the result's fraction with the receiver's (SC27-8713-03, p. 296)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CURRENCY_SIGNS,
        claim: "Once a program has a CURRENCY SIGN clause, $ is a currency symbol in its PICTUREs only if a clause names it: the Language Reference says the currency symbol is $ or the character a clause or the CURRENCY option specifies, and that the clause overrides the option (SC27-8713-03, pp. 130, 212), not that $ stays. A floating currency string of a value longer than one character ends in the position left of the first digit shown, the first currency position holding the whole value (p. 210). NUMVAL-C and TEST-NUMVAL-C without argument-2 take as cs the value of the program's only CURRENCY SIGN clause, where p. 616 names the currency symbol, and $ otherwise. A hexadecimal currency sign literal is refused, since its character depends on the code page",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: JSON_GENERATE_RULES,
        claim: "JSON GENERATE (Language Reference SC27-8713-03, pp. 369-382) writes zoned, packed, binary, index and internal floating-point items as JSON numbers and every other elementary item as a string, choosing where the manual is silent: a table element that a SUPPRESS ... WHEN phrase leaves out is left out of its array, and a table all of whose elements are left out is left out; the members of an unnamed group join its parent's object; a COMP-1 or COMP-2 value takes the digits of its exact HFP value rounded to 9 or 18 significant digits; a character an EBCDIC ENCODING cannot hold becomes X'3F'; when the receiver is too small it holds the leading bytes of the document, whole characters for a national receiver, and COUNT names that many character positions; JSON-CODE and JSON-STATUS are declared in each program that has the statement rather than as GLOBAL in the outermost one",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: XML_PARSE_RULES,
        claim: "XML PARSE runs as under XMLPARSE(XMLSS); XMLPARSE(COMPAT) and VALIDATING are not supported. Where the manuals are silent: a document that is not well formed gives XML-CODE with z/OS XML System Services' return code 12 and the non-validating parser's reason (SA38-0681-50, Appendix B): 2004 when it ends before the root's end tag, 2019 with no root, 3000 a duplicate attribute, 3008 -- in a comment, 3022 < in an attribute value, 3028 a bad character reference, 3035 a mismatched end tag, 3060 a malformed XML declaration or a later processing instruction named xml, 3061 an undeclared entity, 3062 any other character out of place, text after the root included, 3065 a second root; an undeclared prefix is Enterprise COBOL's warning 00040800 or 00040801 and ends the parse, even when the procedure resets XML-CODE. At END-OF-INPUT, XML-CODE 1 takes identifier-1's content, evaluated again, as the next segment, and any other value ends the input, so an unfinished document is then an exception. Markup a segment ends inside is held until it is complete, while content, comments and processing-instruction data are reported in parts, the target again before each later part (Programming Guide SC27-8714-03, pp. 652-653); START-OF-CDATA-SECTION waits for a character after <![CDATA[; namespace declarations are reported after START-OF-ELEMENT and before the attributes; a character reference to a character the document's code page lacks is a NATIONAL-CHARACTER event; XML-TEXT for EXCEPTION holds the document up to the error; the registers' fragments live in run-unit storage released after each event",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: XML_GENERATE_RULES,
        claim: "Where the Language Reference (SC27-8713-03, pp. 484-494) and the Programming Guide (SC27-8714-03, pp. 663-669, 817) are silent, XML GENERATE: trims alphanumeric-edited and numeric-edited values of trailing spaces only, as items of class alphanumeric; puts an unnamed group's members in its parent's element; places TYPE CONTENT items in the parent's content in the order of the data description, among its child elements; writes an element with no content as a start and an end tag, never an empty-element tag, as the Programming Guide's examples show, and keeps a group with no attributes and no content unless a SUPPRESS phrase is given; lets an item's own SUPPRESS ... WHEN decide for it in place of every EVERY phrase; writes a value holding a character XML 1.0 cannot hold as the item's storage in upper-case hexadecimal under its name prefixed hex., sets 417 and goes to ON EXCEPTION once the whole document is written, with 400 before 417 and 417 before 418; drops a namespace's trailing spaces, escapes it as an attribute value, gives 416 for a character XML cannot hold, and ignores NAMESPACE-PREFIX when the namespace is empty; names an EBCDIC CCSID in the XML declaration as IBM- and at least three digits, as the Programming Guide's IBM-037 shows; makes a national item in a document in an EBCDIC code page exception 420 at run time, where the manual makes it a compile-time rule, and a character the code page lacks its ? with 418; and leaves the receiver and COUNT unchanged for 411, 414, 415, 416, 419 and 420",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: JSON_PARSE_RULES,
        claim: "Where the Language Reference (SC27-8713-03, pp. 382-396) and the Programming Guide (SC27-8714-03, pp. 609-617, 819-822) are silent, JSON PARSE: takes only an object or an array as the outermost value (100 otherwise); gives 104 for an object or array where an elementary item stands, and for any other value where a group or table stands; lets a pair that names a suppressed item pass without status 2; reads an unnamed group's members as its parent's and leaves an unnamed table alone; compares duplicate pairs as parsed values, 4 when equal and 103 when not, the first staying; ends the walk at an exception, leaving what it set, with JSON-STATUS as far as it got, and gives 106 when no value reached an elementary item or a null; sets an INDICATING indicator whenever its item's pair is met, the first value for null and the second otherwise; reads a string for a numeric receiver as spaces, a sign, digits with at most one decimal point, and spaces (the form of APAR PH65883); moves a number into an alphanumeric or national receiver only as an integer, as MOVE moves an integer literal, the sign dropped; truncates fraction digits beyond the receiver's; sets 128 when a numeric receiver loses high-order digits, 256 when a string loses characters other than spaces or an integer loses digits, and 512 with X'3F' for each character the code page lacks; rounds a number once into COMP-1 or COMP-2; accepts WITH DETAIL without issuing the IGZ messages; and, as JSON GENERATE does, takes a table named without its last subscript as the whole table",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CORRESPONDING_PAIRS,
        claim: "MOVE, ADD and SUBTRACT CORRESPONDING name one sending and one receiving group, neither reference-modified nor a level-66, 77 or 88 item. Two items under them correspond when they have the same name and the same qualifiers up to the groups, neither is FILLER, and neither is described with RENAMES, REDEFINES, OCCURS or a USAGE of INDEX, POINTER, FUNCTION-POINTER, PROCEDURE-POINTER or OBJECT REFERENCE, which also leaves out everything under such an item; for MOVE at least one is elementary and the move is valid in IBM's table of elementary moves, and for ADD and SUBTRACT both are elementary numeric. Each MOVE pair gives the result of its own MOVE, and ROUNDED and the SIZE ERROR phrases apply to every ADD or SUBTRACT pair, ON SIZE ERROR running once after all of them (Language Reference for Enterprise COBOL 6.4, 'CORRESPONDING phrase', 'MOVE statement', 'ADD statement', 'SUBTRACT statement', 'SIZE ERROR phrases' and 'Valid and invalid elementary moves')",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CORRESPONDING_CHOICES,
        claim: "Where the Language Reference leaves CORRESPONDING open: pairs are processed in the order of the sending group's entries; the items of a FILLER group are not considered; an item is alphabetic when its PICTURE holds only A, as ironwork has no alphabetic category of its own; a numeric-edited item is not numeric for ADD and SUBTRACT, following the rule over the manual's example, which adds two; ADD and SUBTRACT evaluate every sending item before storing any receiver (C97), which differs from pair-by-pair only when a receiving item overlaps a later sending one; and no message is given when no items correspond",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMPROC_MIG_WARNS,
        claim: "NUMPROC(MIG) on a CBL or PROCESS card is a warning (W, return code 4), and the compile takes the default NUMPROC, NOPFD, as ironwork has no installation defaults, whatever NUMPROC an earlier option set (Migration Guide GC27-8715-03, Table 23, p. 112, and Table 32, p. 167). The guide gives neither the message's number nor its text: the message is ironwork's own",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INVALID_OPTION_DISCARDED,
        claim: "A suboption that an option ironwork reads does not have, such as TRUNC(FAST), ARITH(X), NOCOMPILE(U) or a CODEPAGE that is not a number, is an error (E, return code 8) and the option is discarded, the setting before it staying in force, as the Migration Guide records for removed TEST suboptions: 'Error (Invalid option diagnostic, option discarded)' (GC27-8715-03, Table 34, p. 168); the Programming Guide shows the compiler diagnosing a CBL statement's options and carrying on (SC27-8714-03, pp. 279-280). The message's number and text are not in the manuals ironwork has: the text is ironwork's, unchanged from when the option stopped the compile. A CODEPAGE that is a number but no single-byte EBCDIC page ironwork carries still stops the compile (S), since IBM would compile the program in that page and ironwork cannot read it so; an option name that is in no table of IBM's still passes without a message",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OPTIONS_WITHOUT_EFFECT,
        claim: "Options Enterprise COBOL 6.4 no longer has are accepted without effect. LIB, which the compiler now always behaves as having, and SIZE (Migration Guide GC27-8715-03, Table 32, p. 167) are informational (return code 0), as Enterprise COBOL 6.3 gives them for invocation parameters LIB and SIZE(2097152) in job output in the corpus: IGYOS4090-I 'The \"LIB\" option specification is no longer required. COBOL library processing is always in effect.' and IGYOS4013-I 'The \"SIZE\" option is no longer supported.' (SamMoussa961_COBOL, CLHELLO JOB03701); ironwork gives them for a CBL or PROCESS card too, and takes SZ as SIZE's abbreviation. FLAGSAA and NOFDUMP are warnings (W, return code 4), as the guide says IBM warns for each (Table 23, p. 112), with ironwork's text since the guide gives none. FDUMP, which IBM maps to TEST, and NOLIB pass without a message, as TEST does here; no source ironwork has shows IBM's message for NOLIB. The messages are ironwork's own words",
        basis: Basis::Observed,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NON_COBOL_CHARACTERS,
        claim: "A single-byte character outside the basic COBOL character set (Language Reference SC27-8713-03, Table 1, pp. 3-6), outside a literal, comment or PICTURE string, is accepted with IGYLI0163-E, 'Non-COBOL character \"%\" was found in column 8. The character was accepted.' (Migration Guide GC27-8715-03, p. 127), one message for each such character: an error (E, return code 8), so the program still runs under NOCOMPILE(S). Accepted means it is read as a character of the word it is in, or as a word of its own, which the parse then takes or refuses as it would any word. The guide calls non-COBOL the EBCDIC characters outside the set (p. 125), so control characters count; a character beyond U+00FF, which z/OS would hold in DBCS, is still refused, as are $ and &, whose own messages ironwork keeps. COPY REPLACING works on the text before it is read, so a character that REPLACING removes, which IBM diagnoses in the member, gives no message here",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NO_PROGRAM_END,
        claim: "A program with no STOP RUN, GOBACK or EXIT PROGRAM statement anywhere in its PROCEDURE DIVISION, DECLARATIVES included, gets IGYPS2091-W, 'No \"STOP RUN\", \"GOBACK\" or \"EXIT PROGRAM\" was found in the program. Check program logic to verify that the program will exit.' (Migration Guide GC27-8715-03, p. 131): a warning (W, return code 4) with no line, in ironwork's words. A class definition and its methods, which end with EXIT METHOD, get no such warning. One that has an EXEC CICS RETURN or EXEC CICS XCTL is exempt by default, by the operator's choice pending an Enterprise COBOL listing of such a program, and --cics-return-warning says what it gets: once, the default, an informational note (return code 0) in place of the warning; always, the warning; never, nothing. The evidence points the other way: the CICS translator turns EXEC CICS RETURN into Call 'DFHEI1' using by content x'0e0800000600001000' end-call, with no GOBACK after it (CICS TS Application Programming Guide SC34-6433-06, pp. 87-88; IBM's CICS TS COBOL translation-output page shows the same), and the guide names only the three statements, so Enterprise COBOL itself would likely warn. The note is once per ironwork invocation because check, run and cics print the messages of one program, the first in the source",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: USE_WITHOUT_PARAGRAPH,
        claim: "A DECLARATIVES section whose USE statement is followed at once by the next section or END DECLARATIVES gets IGYPS2036-I, 'A paragraph-name was missing after the \"USE\" statement.', informational (return code 0), on the line that follows, as Enterprise COBOL 6.3 gives it for CCVS85 IC401M, DB301M, DB302M and DB305M in the compile listings of eclipse-che4z's COBOL language server tests. No manual ironwork has lists the message. A USE statement followed by statements with no paragraph-name, and a debugging section read as a comment without WITH DEBUGGING MODE, get none, as no listing shows what IBM gives for them",
        basis: Basis::Observed,
        oracle: Oracle::EnterpriseCobol,
    },
];

pub fn get(id: &str) -> &'static Assumption {
    ASSUMPTIONS.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("no assumption {id}"))
}

/// The register as one C series: each entry's number is its 1-based position, so the numbers hold
/// only while the register is appended to and never reordered or trimmed.
pub fn c_series() -> impl Iterator<Item = (usize, &'static Assumption)> {
    ASSUMPTIONS.iter().enumerate().map(|(i, a)| (i + 1, a))
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

    #[test]
    fn c_series_numbers_run_from_one_without_gaps() {
        let numbers: Vec<usize> = c_series().map(|(n, _)| n).collect();
        assert_eq!(numbers, (1..=ASSUMPTIONS.len()).collect::<Vec<_>>());
    }

    #[test]
    fn c_series_keeps_every_id_once_and_leaves_the_register_alone() {
        let before: Vec<&str> = ASSUMPTIONS.iter().map(|a| a.id).collect();
        let seen: Vec<&str> = c_series().map(|(_, a)| a.id).collect();
        assert_eq!(seen, before);
        assert_eq!(ASSUMPTIONS.iter().map(|a| a.id).collect::<Vec<_>>(), before);
    }
}
