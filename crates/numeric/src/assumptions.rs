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

impl Basis {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Documented => "documented",
            Self::Recalled => "recalled",
            Self::Chosen => "chosen",
            Self::Observed => "observed",
        }
    }
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
pub const FLOAT_NARROWING_ROUNDS: &str = "C7";
pub const LE_MASKS_UNDERFLOW: &str = "C8";
pub const PREFERRED_RESULT_SIGNS: &str = "C9";
pub const ZONED_BY_PACK: &str = "C10";
pub const CCSID_TABLES: &str = "C11";
pub const WORKING_STORAGE_LAYOUT: &str = "C12";
pub const NOPFD_REPAIRS_UNSIGNED_INPUT: &str = "C13";
pub const DISPLAY_OF_NONDISPLAY_NUMERIC: &str = "C14";
pub const ACCEPT_AT_END: &str = "C15";
pub const SYSIN_CARD_IMAGES: &str = "C261";
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
pub const LE_SSRANGE_U4038: &str = "L19";
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
pub const JSON_PARSE_RULES: &str = "C200";
pub const DLI_TRANSLATION: &str = "C201";
pub const PASSWORD_IGNORED: &str = "C202";
pub const CORRESPONDING_PAIRS: &str = "C130";
pub const CORRESPONDING_CHOICES: &str = "C131";
pub const STOP_LITERAL: &str = "C132";
pub const NUMPROC_MIG_WARNS: &str = "C120";
pub const INVALID_OPTION_DISCARDED: &str = "C121";
pub const OPTIONS_WITHOUT_EFFECT: &str = "C122";
pub const NON_COBOL_CHARACTERS: &str = "C123";
pub const NO_PROGRAM_END: &str = "C124";
pub const USE_WITHOUT_PARAGRAPH: &str = "C125";
pub const PICTURE_ENDS_AT_ITS_SEPARATOR: &str = "C195";
pub const ZONED_COMPARED_AS_BYTES: &str = "C221";
pub const APOST_EVERYWHERE: &str = "C210";
pub const CURRENCY_OPTION: &str = "C211";
pub const NSYMBOL_DBCS: &str = "C212";
pub const INITIAL_UNDER_THREAD: &str = "C217";
pub const VLR_WITHOUT_VARYING: &str = "C218";
pub const VLR_RECORDS_CHECKED: &str = "C219";
pub const VSAM_DATA_SET_LEFT_OPEN: &str = "C220";
pub const DISPSIGN_SEPARATE: &str = "C213";
pub const LILIAN_INTEGER_DATES: &str = "C214";
pub const CEECBLDY_UNDER_LILIAN: &str = "C215";
pub const COMPLETE_SET_OF_QUALIFIERS: &str = "C216";
pub const INSPECT_FUNCTION_SUBJECT: &str = "C190";
pub const INSPECT_NATIONAL_FUNCTION_RESULT: &str = "C191";
pub const NATIONAL_CASE_AND_REVERSE: &str = "C192";
pub const INVDATA_CLEANSIGN: &str = "C222";
pub const INVDATA_ZONES_COMPARED: &str = "C223";
pub const OPTIMIZED_ZONES_COMPARED: &str = "C262";
pub const ALPHANUMERIC_MOVED_UNCHECKED: &str = "C240";
pub const NUMERIC_MOVED_UNCHECKED: &str = "C260";
pub const STATEMENT_LIMIT_IS_TIME: &str = "C241";
pub const INITCHECK_ANALYSIS: &str = "C224";
pub const INITCHECK_MESSAGE: &str = "C225";
pub const NUMCHECK_SENDERS: &str = "C228";
pub const NUMCHECK_MESSAGE: &str = "C229";
pub const NUMCHECK_LAX_REDEFINES: &str = "C280";
pub const NUMCHECK_ALWAYS_FAILS: &str = "C281";
pub const DBCS_UNDER_SINGLE_BYTE_PAGE: &str = "C282";
pub const DBCS_LITERAL_SOURCE: &str = "C283";
pub const MIXED_PAGE_DATA: &str = "C284";
pub const DBCS_DISPLAY: &str = "C285";
pub const DBCS_HOST_VARIABLES: &str = "C286";
pub const PARMCHECK_BUFFER: &str = "C226";
pub const PARMCHECK_MESSAGE: &str = "C227";
pub const PARM_ARGUMENTS_BEFORE_LAST_SLASH: &str = "C250";
pub const PARM_AREA_PADDED: &str = "C251";
pub const ABBREVIATED_RELATIONS: &str = "C150";
pub const PARAGRAPH_IN_OWN_SECTION: &str = "C151";
pub const REPLACE_STATEMENT: &str = "C160";
pub const VARIABLY_LOCATED_ITEMS: &str = "C161";
pub const EXTERNAL_STORAGE: &str = "C180";
pub const GLOBAL_NAMES: &str = "C181";
pub const SET_TO_ENTRY: &str = "C140";
pub const HEX_CURRENCY_SIGN: &str = "C141";
pub const INITIALIZE_FLOAT_NUMERIC: &str = "C171";
pub const FLOAT_VALUE_LITERAL: &str = "C172";
pub const CICS_ABEND_EXITS: &str = "C142";
pub const FUNCTION_SOURCE_ORDER: &str = "C270";
pub const FUNCTION_NAMED_AS_INTRINSIC: &str = "C271";
pub const FUNCTION_ARGUMENT_TEMPORARIES: &str = "C272";
pub const FUNCTION_SQL_CICS: &str = "C273";
pub const FUNCTION_INVOCATION_ORDER: &str = "C274";
pub const INSPECT_NATIONAL_ITEM: &str = "C230";
pub const INSPECT_OPERAND_USAGE: &str = "C231";
pub const INSPECT_OPERAND_MADE_NATIONAL: &str = "C232";
pub const CICS_ABEND_LABEL_GO_TO: &str = "C236";
pub const CICS_ABEND_EXIT_ACROSS_CALL: &str = "C237";
pub const CICS_ABEND_LABEL_OWNER: &str = "C238";
pub const CICS_ABEND_EXIT_ACROSS_XCTL: &str = "C239";
pub const CICS_RETURN_ENDS_THE_LEVEL: &str = "C233";
pub const CICS_HANDLERS_ACROSS_CALL: &str = "C234";
pub const CICS_CONDITION_LABEL_OWNER: &str = "C235";
pub const CICS_RETURN_BELOW_THE_FIRST_LEVEL: &str = "C143";
pub const CICS_STOP_RUN_ENDS_THE_LEVEL: &str = "C144";
pub const CICS_RUN_UNIT_PER_LINK: &str = "C145";
pub const CICS_HANDLERS_ACROSS_XCTL: &str = "C146";
pub const CICS_NO_OBJECT_ORIENTED_COBOL: &str = "C147";
pub const CICS_TRANSFER_TO_A_RUNNING_PROGRAM: &str = "C148";
pub const CICS_ENCLAVE_EXTERNALS_AND_HEAP: &str = "C126";
pub const RECURSIVE_CALL_OF_AN_ACTIVE_PROGRAM: &str = "C127";
pub const CICS_RETURN_COMMAREA_LENGTH: &str = "C128";
pub const CICS_RUN_UNIT_STORAGE_RELEASED: &str = "C129";
pub const TRAP_OFF_LEAVES_FILES_OPEN: &str = "C152";
pub const CICS_TRANSFER_COMMAREA_LENGTH: &str = "C103";
pub const CICS_RANDOM_PER_RUN_UNIT: &str = "C104";
pub const CICS_RETURN_CODE_PER_RUN_UNIT: &str = "C105";
pub const CICS_ENTRY_POINTERS_ACROSS_RUN_UNITS: &str = "C106";
pub const INITIALIZE_REFERENCE_MODIFIED: &str = "C300";
pub const SORT_INVALID_DIGIT_ABENDS: &str = "C340";
pub const SORT_IFTHEN_FIXED_LENGTH: &str = "C341";
pub const SORT_MASK_GROUPS_OF_THREE: &str = "C342";
pub const SORT_PATTERN_DECIMAL_POINT: &str = "C343";
pub const ALTERNATE_KEYS_FROM_THE_BASE: &str = "C350";
pub const PATH_PRESENTS_THE_BASE: &str = "C351";
pub const GENERATED_COMPONENT_NAMES: &str = "C352";
pub const BLDINDEX_NON_ENDING_ERRORS: &str = "C353";
pub const LISTCAT_WHAT_THE_CATALOG_KEEPS: &str = "C354";
pub const UPGRADE_SET_AFTER_THE_STEP: &str = "C355";
pub const BLDINDEX_REFUSALS: &str = "C356";
pub const PRINT_LISTING_LAYOUT: &str = "C358";
pub const PRINT_RANGE_ENDS: &str = "C359";
pub const INSPECT_SIGNED_ZONED: &str = "C330";
pub const RELATIVE_NUMBER_BELOW_ONE: &str = "C331";
pub const DISPLAY_NUMERIC_FUNCTION: &str = "C332";
pub const BY_VALUE_TO_REFERENCE: &str = "C333";
pub const FLOAT_EXPONENTIATION: &str = "C334";
pub const MIXED_FUNCTION_PLACES: &str = "C390";
pub const MAX_MIN_INTEGER_PLACES: &str = "C391";
pub const INTEGER_FUNCTION_DIGITS: &str = "C392";
pub const ABS_PLACES: &str = "C393";
pub const NUMERIC_FUNCTION_MOVED: &str = "C394";
pub const ASSIGN_ITEM_NAMES_A_DD: &str = "C360";
pub const ASSIGN_ITEM_FORMS: &str = "C361";
pub const PREPARED_STATEMENT_LIFETIME: &str = "C400";
pub const EXECUTE_IMMEDIATE_OF_A_QUERY: &str = "C401";
pub const STATEMENT_STRING_KINDS: &str = "C402";
pub const UPSI_SWITCHES: &str = "C410";
pub const UPSI_FROM_THE_PARM: &str = "C411";
pub const SET_SWITCH_CONDITION_TRUE: &str = "C412";
pub const ACCEPT_FROM_CONSOLE: &str = "C440";
pub const CALL_BY_PROGRAM_ID: &str = "C441";
pub const COMMAND_LINE_FROM_PARM: &str = "C442";
pub const DESCRIBED_COLUMNS: &str = "C403";
pub const SQLDA_CHECKS: &str = "C404";
pub const CLASS_ORDINALS: &str = "C430";
pub const ROWSET_ENDS_SHORT: &str = "C420";
pub const ROWSET_ROW_POSITION: &str = "C421";
pub const POSITIONED_ON_A_ROWSET: &str = "C422";
pub const CALL_FROM_A_RECORDING: &str = "C423";
pub const NOT_ATOMIC_SUMMARY: &str = "C424";

pub const ASSUMPTIONS: &[Assumption] = &[
    Assumption {
        id: HFP_EXTENDED_LOW_HALF,
        claim: "An extended HFP result's low half carries the high characteristic minus 14, modulo 128, and is all zero when the value is all zero bits (SA22-7832-14, HFP extended format, p. 18-4)",
        basis: Basis::Documented,
        oracle: Oracle::Hercules,
    },
    Assumption {
        id: HFP_FROM_FIXED_TRUNCATES,
        claim: "CONVERT FROM FIXED to HFP (CEFR, CDFR, CXFR, CEGR, CDGR, CXGR) normalizes the result and rounds it toward zero, truncating the hexadecimal digits the precision cannot hold (z/Architecture Principles of Operation SA22-7832-13, p. 18-11; Figure 9-15, Comparison of Rounding Action, p. 9-17), and Hercules follows the Principles of Operation",
        basis: Basis::Documented,
        oracle: Oracle::Hercules,
    },
    Assumption {
        id: INTERMEDIATE_TABLE,
        claim: "An operation's intermediate result has i integer and d decimal places: for + and -, one integer place more than the operand with more, and the more decimal places; for *, the sum of each; for /, the dividend's integer places and the divisor's decimal places together, and the dividend's decimal places less the divisor's or dmax, whichever is more. Up to 30 digits (31 under ARITH(EXTEND)) i and d are carried; beyond that, N-d and d when d <= dmax, else i and N-i when i+dmax <= N, else N-dmax and dmax; digits beyond are truncated (Programming Guide SC27-8714-03, pp. 794-795)",
        basis: Basis::Documented,
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
        claim: "An HFP value moved or stored into a fixed-point receiver is rounded in the receiver's low-order position, with or without ROUNDED, and keeps at most 9 significant digits from short precision and 18 from long, the rest zero (Programming Guide SC27-8714-03, p. 52); the Language Reference's COMBINED-DATETIME example, 143951.1886781248 from a long value of 143951.18867812478..., shows a COMPUTE rounding (SC27-8713-03, p. 542). Rounding is half away from zero; an extended value keeps every digit the receiver holds",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_NARROWING_ROUNDS,
        claim: "A floating-point value moved or stored into a narrower COMP-1 or COMP-2 is rounded in the low-order position, as LOAD ROUNDED rounds (Programming Guide SC27-8714-03, p. 52)",
        basis: Basis::Documented,
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
        claim: "Data produced by COBOL arithmetic statements conforms to the IBM system standards: the sign is X'C' when the result is positive or zero and X'D' when it is negative for a signed zoned or packed item, and X'F' for an unsigned one (Programming Guide SC27-8714-03, p. 392, NUMPROC). The sentence carries no NUMPROC condition, and under NUMPROC(NOPFD) the preferred sign is always generated in the receiver (p. 53)",
        basis: Basis::Documented,
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
        claim: "DISPLAY shows a packed or binary item as zoned digits of its PICTURE, a negative value's sign overpunched on the last digit and a positive value's digits unsigned (Programming Guide SC27-8714-03, p. 363, Table 48; Language Reference SC27-8713-03, p. 334); a COMP-5 item, or any binary item under TRUNC(BIN), shows its whole binary value in 5, 10, or 19 (signed) or 20 digits for a halfword, fullword or doubleword. Under --dialect gnucobol DISPLAY shows them as cobc -std=ibm-strict does, whatever DISPSIGN says: a signed item's sign, + or -, before its digits, a packed item's digits those of its PICTURE, and any binary item's whole value in 5, 10 or 20 digits for a halfword, fullword or doubleword",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ACCEPT_AT_END,
        claim: "ACCEPT from SYSIN at its end leaves the receiving item unchanged and the run continues. Under --dialect gnucobol the item takes a space, as cobc moves one at the end of its input, so a numeric or numeric-edited item becomes zero and any other is filled with spaces",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SYSIN_CARD_IMAGES,
        claim: "ACCEPT from SYSIN reads each line of the SYSIN file as one record, 80 bytes as a card of in-stream data is: a shorter line is padded with spaces to 80, and a longer line is a record of its own length, as a variable-length record would be. The receiving item is filled from consecutive records with no conversion, editing or check, the last record cut where the item ends; at the end of SYSIN after some data the rest of the item is spaces, and before any data the item is unchanged (C15) (Language Reference SC27-8713-03, pp. 307-308: 'There is no editing or error checking of the incoming data', each record concatenated with the previous, a fixed-length record used whole). A numeric receiver takes the characters as they are, so an empty line gives spaces and a non-digit is a data exception only where the item is next read as a number. The record length of a SYSIN data set other than in-stream cards is not known to ironwork, and 80 is chosen. A national receiver still takes one line, converted from the code page, where IBM takes UTF-16 data unconverted",
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
        claim: "Indexed and relative files report the file status values of Table 34 (Language Reference SC27-8713-03, pp. 300-302): 02 only for an indexed file with an alternate key that allows duplicates, when a READ finds the next record by the key of reference has the same key or a WRITE or REWRITE makes a duplicate alternate key value, so never for a relative file; 14 for a sequential READ of a relative file whose record number has more digits than the RELATIVE KEY; 21 for a sequentially accessed indexed file's WRITE whose prime key is not above the last, or a REWRITE that changed the prime key; 22 for a duplicate prime key, relative record number, or alternate key without DUPLICATES; 23 for no such record; 24 for a WRITE beyond the file's boundaries, or a sequential WRITE whose relative record number has more digits than the RELATIVE KEY; 43 for a sequential-access REWRITE or DELETE whose last input-output statement was not a successful READ; 46 for a sequential READ with no valid next record; and 47, 48 and 49 for the wrong open mode",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALTERNATE_KEY_ORDER,
        claim: "Records sharing an alternate key come back in the order in which they were placed in the set of records with that key (Language Reference SC27-8713-03, pp. 150, 152, 428; z/OS 3.1 DFSMS Using Data Sets, idad400/d4349). A REWRITE may change an alternate key (Language Reference, p. 434), and VSAM updates the alternate indexes of the upgrade set on every update (idad400/gu123). IBM does not state where a record REWRITTEN with a new alternate key goes among the others with it; ironwork places it after them, the REWRITE being when it enters that key's set",
        basis: Basis::Chosen,
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
        claim: "START compares the key with data-name-1 as alphanumeric items, whatever their category, over the shorter length, as if the longer were truncated on the right, and PROGRAM COLLATING SEQUENCE has no effect (Language Reference SC27-8713-03, p. 456). The comparison runs left to right on single-byte character values ordered by their hexadecimal value (pp. 276-277)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_FRESH_STORAGE,
        claim: "A COBOL program reached by EXEC CICS LINK gets a new initialized copy of its WORKING-STORAGE on each entry, and its run unit is reinitialized; one reached by CALL gets initialized WORKING-STORAGE on its first entry within a CICS logical level and its last-used state on later entries at that level (CICS TS 6.x, Rules for calling subprograms, dfhp3_cobol_subprog_rules). A program reached by XCTL starts a run unit and is not a subprogram (CICS TS 6.x, Flow of control between programs and subprograms, dfhp3_cobol_subprog_flow), CICS obtains a separate copy of working storage every time an application program runs (Quasi-reentrant application programs, dfhp3_concepts_quasirent), and a main program is initialized each time it is called (Programming Guide SC27-8714-03, p. 547). IBM does not say in one sentence that an XCTL target gets fresh WORKING-STORAGE; ironwork reads these together and gives it fresh storage",
        basis: Basis::Chosen,
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
        claim: "A HANDLE ABEND exit receives control when a condition nothing handles abends the task (AEIx), and when a condition whose HANDLE CONDITION label another program set abends it APC2 (C235), as for any abend it can intercept, and is deactivated by being taken (C142)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_LENGTH_DEFAULTS_TO_INTO,
        claim: "Under the translator's default LENGTH option, a COBOL EXEC CICS command that omits LENGTH gets a generated length, the referenced variable's (CICS TS 6.x, Translator options provided by the CICS-supplied command-level language translator, dfhp3_transl_options_intro; EXEC CICS command argument values, dfhp4_argumentvalues). For READ, READNEXT and READPREV with INTO it is the largest record the program accepts: a longer record is truncated to it, LENGERR is raised, and the LENGTH area gets the record's untruncated length (CICS TS 6.x, READ, dfhp4_read; READNEXT, dfhp4_readnext; READPREV, dfhp4_readprev). For READQ TS and READQ TD with INTO it is the most data the program accepts, and longer data is truncated with LENGERR (READQ TS, dfhp4_readqts; READQ TD, dfhp4_readqtd). CICS also raises LENGERR when it reads a fixed-length record into an area longer than the record (dfhp4_read); ironwork does not, and delivers the shorter record",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_PROGRAM_CHECK_IS_ASRA,
        claim: "A program check in a user task abends the task with ASRA (CICS TS 6.x, Processing operating system abends and program checks, dfht21n; ASRA). Protection exceptions (interrupt code 4, S0C4) and data exceptions (code 7, S0C7) are program checks (What type of program check occurred, dfhs10q). CICS reports ASRD instead when the check comes from invoking CICS macros or accessing the CSA or TCA (Transaction abend codes: AEYD, AICA, ASRA, ASRB, and ASRD, dfhs1l7), which a COBOL program on ironwork cannot do, and an active HANDLE ABEND's action takes place (Language Environment abend and condition handling, dfhp3_langenv_abend)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_BROWSE_SKIP,
        claim: "Changing RIDFLD before the next READNEXT repositions the browse to the new identifier, from which it continues (CICS TS 6.x, READNEXT, dfhp4_readnext), what IBM calls skip sequential processing (Efficient data set operations, dfhp3c00110). In a browse started with GTEQ, STARTBR's default for a KSDS or RRDS (STARTBR, dfhp4_startbr), the next record is the first whose key is greater than or equal to the new RIDFLD (dfhp4_readnext); in a generic browse the new RIDFLD must be generic (dfhp4_readnext), and X'FF' keys cannot reposition a browse (Sequential reading (browsing), dfhp3u7). IBM does not say where a browse started with EQUAL goes when repositioned to a key no record has; ironwork applies the at-or-after rule there too",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_RECEIVE_NULLS,
        claim: "RECEIVE MAP sets the whole input structure to nulls before mapping (CICS TS 6.x, Formatted screen input, dfhp31g), except on MAPFAIL, when the input map is not set to nulls (RECEIVE MAP, dfhp4_receivemap). Only fields whose modified data tag is on are transmitted and mapped, the operator setting it by entering, changing or erasing data and the program by sending the field with MDT in its ATTRB (dfhp31g). A field the operator erased has L = 0 and the X'80' bit on in F (dfhp31g; Finding the cursor, dfhp31j). ironwork does not set the cursor flag X'02' that CURSLOC=YES asks for",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_INPUT_JUSTIFY,
        claim: "Input data lands left-justified and blank-filled unless JUSTIFY says otherwise, and a field with ATTRB=NUM and no JUSTIFY right-justified and zero-filled (CICS TS 6.x, Formatted screen input, dfhp31g; BMS macro DFHMDF, dfhp473). A JUSTIFY naming one value of a pair implies the other: LEFT implies BLANK, RIGHT implies ZERO, BLANK implies LEFT and ZERO implies RIGHT (dfhp473)",
        basis: Basis::Documented,
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
        claim: "DFHBMSCA names DFHBMPEM, DFHBMPNL, DFHBMPFF and DFHBMPCR as the printer end-of-message, new-line, form-feed and carriage-return characters and lists no values for them (CICS TS 6.x, BMS constants, dfhp4_bmsconstants). IBM gives EM as X'19' (CICS 3270 printers, dfhp3ee) and NL, FF and CR as X'15', X'0C' and X'0D' (BMS support for non-3270 terminals, dfhp31r; CICS 3270 printer options, dfhp3e7), and ironwork gives the four constants those values. ironwork's DFHNULL is X'00'; CICS TS 6.x lists DFHNULL in neither DFHBMSCA nor DFHAID, and TXSeries lists it in DFHAID as the null value. The binding of each constant to its value is recalled",
        basis: Basis::Recalled,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BMS_SEND_DATA_CHOICE,
        claim: "Without MAPONLY or DATAONLY, SEND MAP sends every field of the map (CICS TS 6.x, Building the output screen, dfhp3c3). A named field's display data comes from the symbolic map's O subfield when its first character is not null, else from the map's INITIAL value, else nulls; its field attribute comes from the A subfield unless that byte is null or one of the values that remain from an input operation, X'80', X'02' and X'82', else from the field's ATTRB (dfhp3c3). With DATAONLY BMS sends only what the program gave (dfhp3c3)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_INITIAL_AID,
        claim: "IBM's text conflicts. The application guide says the EIB fields that describe the input, EIBAID among them, are not set at the start of a task initiated by unsolicited terminal input, and that a RECEIVE posts them (CICS TS 6.x, EIB feedback on terminal control operations, dfhp34x); IBM's CEDF example shows EIBAID = X'7D' at program initiation (CEDF, dfha7or), and the 3270 bridge sets EIBAID at task start to the key that started the transaction (MQCIH fields for 3270 transaction request messages, fg15730_; Inbound BRIH message header, dfhtmeu). ironwork sets EIBAID from the initiating input before any RECEIVE, as CEDF shows; a program written to IBM's guidance issues a RECEIVE first and sees the same value either way",
        basis: Basis::Chosen,
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
        claim: "With fc OMITTED, a service that fails signals its condition (z/OS 3.1 Language Environment Programming Reference, Parameter list for invoking callable services, ceea300/icspl; Programming Guide SC27-8714-03, p. 790), and RETURN-CODE is not altered (p. 790). An unhandled condition of severity 2 or more is promoted to T_I_U and terminates the thread (z/OS 3.1 Language Environment Programming Guide, default responses to unhandled conditions, Table 1, ceea200/ceea200138); under the default ABTERMENC(ABEND) the enclave ends with user abend U4038, reason code 1, for a software-raised condition (Programming Reference, ABTERMENC, ceea300/abterm; Programming Guide, Abend codes generated by ABTERMENC(ABEND), ceea200/encflgf). A severity-1 condition lets the run continue; IBM issues its message when the frame is a COBOL program's (ceea200/inmsg, ceea200/ceea200138), and ironwork issues none",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LE_CEE3ABD,
        claim: "CEE3ABD ends the run with user abend abcode modulo 4096, the ABEND macro's user completion code, to which SA38-0683-60 says abcode passes unchecked; every clean-up value ends it alike: open files are closed, as at any U code (C152), and neither a CEEDUMP nor a system dump is written",
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
        claim: "ironwork fails a MERGE whose input file holds records out of the merge order with SORT-RETURN 16 before any record is output, as DFSORT's ICE068A ends a merge (z/OS 3.1 DFSORT Messages, Codes and Diagnosis, icem100/kc00066). IBM's documents point the other way for a COBOL MERGE: FASTSRT applies only to the format 1 SORT (Programming Guide SC27-8714-03, p. 369), so the compiler does a MERGE's GIVING through an output procedure (p. 228) that DFSORT reaches as a COBOL-generated E35 exit (z/OS 3.1 Language Environment Programming Guide, ceea200/clcsrt3), and DFSORT does not sequence-check a merge whose E35 exit has no output data set (z/OS 3.1 DFSORT Application Programming Guide, icea100/ase35). Read together they say a COBOL MERGE is not checked, returns 0, and outputs each record as the merge selection reaches it; no single IBM sentence says so, and even under DFSORT's own check the records ahead of the out-of-sequence one have been output when ICE068A ends the merge (icea100, input, user exit and output logic examples)",
        basis: Basis::Chosen,
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
        claim: "A zoned (trailing sign) or packed sort or merge key of -0 collates before +0 in ascending order and after it in descending order. The compiler hands such keys to DFSORT as ZD and PD fields (z/OS 3.1 DFSORT Application Programming Guide, DFSORT formats for COBOL data types, icea100), and DFSORT orders them so under SZERO=YES (DFSORT Installation and Customization SC23-6881-70, p. 99), the IBM-supplied default (p. 100). The Language Reference says numeric keys compare by the rules of a relation condition, under which all zero values compare equal (Language Reference SC27-8713-03, pp. 279, 397, 449), and no IBM page reconciles the two; ironwork follows DFSORT's default, and a site running SZERO=NO would see the Language Reference's equality",
        basis: Basis::Chosen,
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
        id: LE_SSRANGE_U4038,
        claim: "Under SSRANGE with its default ABD suboption, an out-of-range reference signals a severity-3 condition (Programming Guide SC27-8714-03, pp. 411-412; z/OS 3.1 Language Environment Programming Guide, Interpreting runtime messages, ceea200/inmsg); nothing handles it, so the run ends with user abend U4038 under the default ABTERMENC(ABEND) (Programming Reference, ABTERMENC, ceea300/abterm; Programming Guide, ceea200/encflgf). For reference modification IBM issues IGZ0072S for a start below 1 or past the item's current length, IGZ0073S for a length of 0 or less under NOZLEN, and IGZ0074S for a start and length that reach past the item's end (z/OS 3.1 Language Environment Runtime Messages, ceea900/cs00507, cs00508, cs00509). IBM checks a subscripted reference's effective address against the table taken at its maximum size, not each subscript (Programming Guide, p. 411; IGZ0006S, ceea900/cs00446), and a variable-length group's composite length for IGZ0007S (ceea900/cs00447). ironwork checks each subscript against its own dimension's occurrences and each OCCURS DEPENDING ON object's count, issuing IGZ0006S and IGZ0007S, so a reference whose inner subscript is out of its dimension but whose address stays inside the table abends on ironwork and not on z/OS. SSRANGE(MSG) and SSRANGE(ZLEN) are not modelled",
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
        claim: "Enterprise COBOL compiles a class's methods as Java native methods (Programming Guide SC27-8714-03, p. 521), and the JNI passes a nonstatic native method a reference to its object as a local reference, valid for the call and freed after the method returns (JNI Specification, Java SE 21, ch. 2, Native Method Arguments; Global and Local References). SELF refers to the object instance used to invoke the currently executing method (Language Reference SC27-8713-03, p. 15), and the Programming Guide's list of local references names parameters, RETURNING values, JNI results and INVOKE ... NEW without mentioning SELF (p. 721). ironwork reads SELF as the reference the JNI passes, a local reference in the method's own frame that expires when the method returns",
        basis: Basis::Chosen,
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
        claim: "The JNI reference services behave as the JNI specification defines them (JNI Specification, Java SE 21, ch. 4, Global and Local References; Object Operations): NewGlobalRef and NewLocalRef make a new reference to the object and NULL for NULL; DeleteGlobalRef and DeleteLocalRef do nothing for NULL; IsSameObject is true for two references to one object or two NULLs; GetObjectRefType answers 0 for NULL, 1 for a local reference and 2 for a global one; PopLocalFrame frees the current frame's local references and gives a local reference in the previous frame to its argument's object, or NULL for NULL; and local references are freed when the native method returns, with any frames it left open. The Programming Guide documents NewGlobalRef, DeleteGlobalRef and DeleteLocalRef (SC27-8714-03, pp. 722-723) and IsSameObject (p. 697), and its JNI.cpy declares PushLocalFrame, PopLocalFrame, NewLocalRef, EnsureLocalCapacity (p. 848) and GetObjectRefType (p. 850). Where the specification is silent or allows failure, ironwork chooses: EnsureLocalCapacity always succeeds, a delete given the other kind of reference ends the run, and PopLocalFrame with no frame open ends the run",
        basis: Basis::Chosen,
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
        claim: "ironwork reads a name in an INTO list written without its colon as a host variable, as real programs write it (FETCH C INTO CSR-ENTITY, CSR-PROJ-ID). Db2 13 for z/OS says all references to host variables must be preceded by a colon, and that the precompiler issues an error for a missing colon or reads the name as an unqualified column name where a column name can be referenced (SQL Reference, References to host variables, db2z_refs2hostvars); an INTO list is not such a place, and a name there without a colon is how Db2 13 writes a global variable, SQL variable or SQL parameter target (SELECT INTO, db2z_sql_selectinto; Global variables, db2z_globalvars). No IBM page found says an older precompiler read it as a host variable",
        basis: Basis::Chosen,
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
        claim: "A CALL of an ENTRY name begins at the statement after the ENTRY and binds the entry's USING list alone, so a LINKAGE item only the PROCEDURE DIVISION USING names has no address (Language Reference SC27-8713-03, pp. 264, 320, 339-340). A static CALL, of a literal under NODYNAM, enters the one copy of the program that its PROGRAM-ID enters, in its last-used state (Programming Guide SC27-8714-03, pp. 548, 553-554); a dynamic CALL, of an identifier or of a literal under DYNAM, gets for each entry name a copy of the program with WORKING-STORAGE of its own, as if a separate compile unit were called (p. 560), which a CANCEL of that name resets, where p. 549 says only that a second entry point must not be called dynamically without a CANCEL between. CALL finds an entry name among the programs of the source and those already loaded, then in a program library as a member of that name, an alias, as NAME(ALIAS) or binder ALIAS statements make one (pp. 548, 560). Under --dialect gnucobol every entry name enters the program's one copy, as cobc's do: a dynamic CALL of an entry name shares the program's WORKING-STORAGE, and a CANCEL of an entry name does nothing",
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
        claim: "A file statement of a contained program that has no EXCEPTION/ERROR procedure of its own for the file, by name or by open mode, runs the first USE GLOBAL procedure of the programs containing it, innermost out, for the file and then for the mode (Language Reference SC27-8713-03, pp. 714-715), as a procedure of the program that declares it: over that program's storage as it stood when control left it, its LINKAGE addresses included, with PERFORMs of its own. Control comes back after the statement; STOP RUN in the procedure ends the run, and GO TO out of it, GOBACK and EXIT PROGRAM (which p. 714 forbids while a declarative of a nested program is active) are refused when reached. USE GLOBAL BEFORE REPORTING for a report group of a contained program without its own procedure for it would serve another program's report (Report Writer Precompiler SC26-4301-04, 4.7.2 rule 5 and 4.7.3 rule 4) and is refused in a program that contains others",
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
        claim: "DISPLAY of a numeric literal writes it as the program wrote it, so under DECIMAL-POINT IS COMMA its decimal point is a comma; the Language Reference says nothing of how DISPLAY shows a numeric literal (SC27-8713-03, pp. 333-334). Under --dialect gnucobol it writes the literal without its decimal point, as cobc does: 1.5 shows as 15 and -0.25 as -025",
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
        claim: "A floating-point argument, item or expression, is allowed wherever a function takes a numeric argument and refused where it takes an integer (Language Reference SC27-8713-03, p. 507). INTEGER and INTEGER-PART of one return an integer of 30 digits, 31 under ARITH(EXTEND), and ABS, MAX, MIN, RANGE, REM and SUM with one are evaluated in floating point and return it (Programming Guide SC27-8714-03, pp. 799 and 801). The guide names REM a mixed function where the Language Reference types it numeric (p. 633), and gives the precision only of floating-point functions: a mixed function is evaluated here in long floating point, extended under ARITH(EXTEND), as they are",
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
        claim: "ACOS, ASIN, ATAN, COS, EXP, EXP10, LOG, LOG10, SIN, SQRT and TAN give the results of Language Environment's long-precision floating-point services under ARITH(COMPAT) and its extended-precision services under ARITH(EXTEND), and RANDOM a long result under either (Programming Guide SC27-8714-03, p. 58). E and PI are long floating-point approximations under ARITH(COMPAT) (Language Reference SC27-8713-03, pp. 553, 617). NUMVAL, NUMVAL-C and NUMVAL-F return floating-point approximations (pp. 605, 608, 609), long under ARITH(COMPAT) and extended under ARITH(EXTEND) (Programming Guide, p. 115). COMBINED-DATETIME returns a long-precision approximation whatever ARITH says, as IBM Docs' current topic says (Enterprise COBOL 6.4 Language Reference, COMBINED-DATETIME, SS6SG3_6.4.0/lr/ref/rlinfcdt), where the June 2024 PDF shows a 23-digit ARITH(EXTEND) result (Language Reference, p. 535). MAX, MIN, RANGE, REM and SUM are evaluated and returned in floating point when any argument is floating point (Programming Guide, p. 799); ABS's type follows its argument (Language Reference, p. 517), and ironwork treats it as they are (C100). An expression that references a floating-point function, or a mixed function with a floating-point argument, is evaluated in floating point (Programming Guide, pp. 62, 800-801). ironwork also treats ANNUITY, MEAN, MEDIAN, MIDRANGE, PRESENT-VALUE, STANDARD-DEVIATION, VARIANCE, SECONDS-FROM-FORMATTED-TIME and SECONDS-PAST-MIDNIGHT as floating-point functions, which the 6.4 manuals type only as numeric",
        basis: Basis::Chosen,
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
        claim: "A receiver named with ROUNDED counts in dmax with one decimal place more than it holds, so a quotient, or an intermediate cut back to dmax places, keeps the digit that rounding reads: DIVIDE 44.1 INTO a PIC 9(4)V9 of 1661.7 ROUNDED gives 37.7, as CCVS85 NC117A and NC171A expect. The Programming Guide says only that under ROUNDED one more decimal place, and one more integer place, might be carried for accuracy if necessary (SC27-8714-03, p. 794); the Language Reference's ROUNDED phrase compares the result's fraction with the receiver's (SC27-8713-03, p. 296). Under --dialect gnucobol the extra place counts in the statement's last operation alone, whose result the receivers take, and every operation below it carries dmax with each receiver's own places, as cobc -std=ibm-strict truncates intermediate results to dmax and computes the last one exactly: COMPUTE D ROUNDED = D + E / 3 keeps E / 3 to D's two places. Under --assume C101=off the extra place counts in no operation, as the Programming Guide's 'might be carried' allows: COMPUTE S ROUNDED = 1661.7 / DIV2, DIV2 44.1 and S a PIC 99V9, gives 37.6",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CURRENCY_SIGNS,
        claim: "Once a program has a CURRENCY SIGN clause, $ is a currency symbol in its PICTUREs only if a clause names it: the Language Reference says the currency symbol is $ or the character a clause or the CURRENCY option specifies, and that the clause overrides the option (SC27-8713-03, pp. 130, 212), not that $ stays. A floating currency string of a value longer than one character ends in the position left of the first digit shown, the first currency position holding the whole value (p. 210). NUMVAL-C and TEST-NUMVAL-C without argument-2 take as cs the value of the program's only CURRENCY SIGN clause, where p. 616 names the currency symbol, the CURRENCY option's character when there is no clause (p. 212; C211), and $ otherwise. A hexadecimal currency sign literal is read as C141 says",
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
        claim: "XML PARSE runs as under XMLPARSE(XMLSS); XMLPARSE(COMPAT) and VALIDATING are not supported. Where the manuals are silent: a document that is not well formed gives XML-CODE with z/OS XML System Services' return code 12 and the non-validating parser's reason (SA38-0681-50, Appendix B): 2004 when it ends before the root's end tag, 2019 with no root, 3000 a duplicate attribute, 3008 -- in a comment, 3022 < in an attribute value, 3028 a bad character reference, 3035 a mismatched end tag, 3060 a malformed XML declaration or a later processing instruction named xml, 3061 an undeclared entity, 3062 any other character out of place, text after the root included, 3065 a second root; an undeclared prefix is Enterprise COBOL's warning 00040800 or 00040801, reported before the name's own event, and the parse goes on only when the processing procedure sets XML-CODE to zero, as the Programming Guide's Table 83 shows (pp. 656-658); after any other exception XML-CODE keeps the parser's code whatever the procedure sets. At END-OF-INPUT, XML-CODE 1 takes identifier-1's content, evaluated again, as the next segment, and any other value ends the input, so an unfinished document is then an exception. Markup a segment ends inside is held until it is complete, while content, comments and processing-instruction data are reported in parts, the target again before each later part (Programming Guide SC27-8714-03, pp. 652-653); START-OF-CDATA-SECTION waits for a character after <![CDATA[; namespace declarations are reported after START-OF-ELEMENT and before the attributes; a character reference to a character the document's code page lacks is a NATIONAL-CHARACTER event; XML-TEXT for EXCEPTION holds the document up to the error; the registers' fragments live in run-unit storage released after each event",
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
        id: DLI_TRANSLATION,
        claim: "EXEC DLI is read as the IMS translator reads it (IMS Application Programming: EXEC DLI Commands for CICS and IMS, SC18-7811-04, chapters 4-6): a command by its name or longer spelling (DELETE, from the book's C sample, for DLET), its options checked against the book's Format diagrams (pp. 35-81), GHU, GHN and GHNP taking their Get command's (p. 102), AIB allowed wherever PCB is (p. 5), and GMSG, ICMD and RCMD, which the book leaves to the Operations Guide, not checked; the data an option names and the right-hand side of each WHERE comparison declared; SEGMENT or PSB in double parentheses naming an area. The book names no relational operators or connectors beyond its examples' =, >=, >, < and AND and OR, so EQ, NE, GT, GE, LT, LE, <=, ¬= and the symbols &, |, * and + are accepted too. The DL/I interface block is declared at the head of WORKING-STORAGE with the book's COBOL labels over the 40 bytes its C declaration gives (p. 6), the unnamed bytes FILLER, unless the program declares DIBSTAT itself; the book does not name the 01 level, so it is DLZDIB, DL/I's name for the block, and the translator's IS GLOBAL is left out. A command is checked, not run: reaching one ends the run",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PASSWORD_IGNORED,
        claim: "A SELECT's PASSWORD clause is read and has no effect: the files ironwork for COBOL opens carry no VSAM passwords, so the password items are neither checked against the file nor required to hold one before OPEN (Language Reference SC27-8713-03, p. 152, where IBM requires a valid password for a VSAM file)",
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
        id: STOP_LITERAL,
        claim: "STOP literal communicates the literal to the operator and suspends the program until the operator intervenes, then continues with the next statement (Language Reference for Enterprise COBOL 6.4, 'STOP statement'). ironwork has no operator to wait for: it writes the literal as DISPLAY does and continues at once",
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
        claim: "A suboption that an option ironwork reads does not have, such as TRUNC(FAST), ARITH(X), NOCOMPILE(U) or a CODEPAGE that is not a number, is an error (E, return code 8) and the option is discarded, the setting before it staying in force, as the Migration Guide records for removed TEST suboptions: 'Error (Invalid option diagnostic, option discarded)' (GC27-8715-03, Table 34, p. 168); the Programming Guide shows the compiler diagnosing a CBL statement's options and carrying on (SC27-8714-03, pp. 279-280). The message's number and text are not in the manuals ironwork has: the text is ironwork's, unchanged from when the option stopped the compile. A CODEPAGE that is a number but no EBCDIC page ironwork carries, single-byte or one of the mixed pages of the Programming Guide's Table 47, still stops the compile (S), since IBM would compile the program in that page and ironwork cannot read it so; an option name that is in no table of IBM's still passes without a message",
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
    Assumption {
        id: PICTURE_ENDS_AT_ITS_SEPARATOR,
        claim: "A PICTURE character-string is delimited only by a separator space, comma, semicolon or period (Language Reference SC27-8713-03, p. 48), and a separator comma, semicolon or period is that character followed by a space (p. 50), so only the last such character before the space is a separator and any before it belong to the string: PIC 9,9,9,. is 9,9,9, with an insertion comma at its end, and PIC 999.. is 999. with its decimal point at its end, as CCVS85 NC125A writes them. As an IBM extension the string so ended may also be followed by a separator comma or semicolon and further clauses, where the 85 standard requires the separator period (p. 744), so PIC 999., VALUE ZERO is 999. too. DECIMAL-POINT IS COMMA exchanges the period's and comma's functions only within PICTURE strings and numeric literals (pp. 131, 208), not as separators, so the same holds under it",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ZONED_COMPARED_AS_BYTES,
        claim: "A zoned integer compared with a nonnumeric operand, an alphanumeric, alphanumeric-edited, numeric-edited or group item, an alphanumeric or hexadecimal literal, or a figurative constant other than ZERO, is compared as the bytes it holds, without being read as a number: a numeric integer in such a comparison is treated as moved to an alphanumeric item of its size (Language Reference, comparison of numeric and alphanumeric operands), which for zoned data copies the digits. Under ZWB, IBM's default, a sign it overpunches is removed first (its zone made F); under NOZWB it is kept (Programming Guide SC27-8714-03, p. 431: 'Use NOZWB if you want to test input numeric fields for SPACES'); a separate sign is left out either way. So an unsigned item holding spaces equals SPACES, and a signed one does under NOZWB, where reading it as a number would end in a data exception. A scaled item is compared as its digits, as before",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: APOST_EVERYWHERE,
        claim: "Under APOST the figurative constant QUOTE is the apostrophe wherever the program uses it: X'7D' in an alphanumeric item, X'0027' in a national one, and as an entry of an ALPHABET clause. The Programming Guide says only that [ALL] QUOTE and QUOTES represent apostrophes under APOST and quotation marks under QUOTE, and that either may delimit a literal whichever is in effect (SC27-8714-03, p. 347)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CURRENCY_OPTION,
        claim: "CURRENCY(literal), in a program with no CURRENCY SIGN clause of its own or of a containing program, acts as a CURRENCY SIGN clause whose value is the literal's one character, standing for itself: that character is the PICTURE currency symbol in place of $, and an edited item shows it. The Language Reference says the currency symbol is $ or the one character the CURRENCY option or a CURRENCY SIGN clause gives, and that a CURRENCY SIGN clause makes the option ignored (SC27-8713-03, pp. 130, 211); the Programming Guide lists the characters the literal may not be (SC27-8714-03, p. 358). That the currency sign value is the character itself, and that nothing is said when the option is ignored, are chosen. A hexadecimal literal is read in the program's code page once every card is applied, so CODEPAGE may follow it; one whose character the option may not name is an error and the option is discarded, as an invalid suboption is (C121). NUMVAL-C's default currency string is not changed here",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NSYMBOL_DBCS,
        claim: "Under NSYMBOL(DBCS) an N literal is a DBCS literal and a PICTURE of N, alone or with B, and no USAGE, its own or a group's, is USAGE DISPLAY-1 (Programming Guide SC27-8714-03, pp. 387-388); the option is read from the CBL and PROCESS cards before the source is read, as a card is the only place a program can set it. NX literals are not N literals. NSYMBOL(NATIONAL) with NODBCS on the cards, which are one level of precedence, leaves DBCS in effect, as Table 46 forces it (p. 344), with a warning (W, return code 4) in ironwork's words, since the message Enterprise COBOL gives for an option dropped in conflict resolution is IGYOS4020-W (J19), as for INITIAL with THREAD (C217); NODBCS alone, with NSYMBOL(NATIONAL) only as the default, is taken as written",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INITIAL_UNDER_THREAD,
        claim: "The INITIAL option with THREAD, on the same or different CBL and PROCESS cards and in either order, is ignored and NOINITIAL forced, with an error message (Programming Guide SC27-8714-03, p. 344, Table 46); under THREAD IBM diagnoses the INITIAL option as an error (p. 418). ironwork gives a warning (W, return code 4) in its own words, as for NORENT with THREAD, since the message Enterprise COBOL gives for an option dropped in conflict resolution is IGYOS4020-W (J19), and the program, its nested programs and the options in its load module are NOINITIAL. An IS INITIAL clause under THREAD keeps J13's error. A class definition gets the warning once and its methods none; the option makes no method initial, as INITIAL is an attribute of a program and its nested programs (p. 374; Language Reference SC27-8713-03, p. 103)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: VLR_WITHOUT_VARYING,
        claim: "VLR(STANDARD) checks the length of a variable-length record a READ returns against the least and greatest of the file's level-01 records, and VLR(COMPAT) against its RECORD IS VARYING IN SIZE FROM min TO max (Programming Guide SC27-8714-03, pp. 422-424, Table 52); a FROM or TO the clause leaves out is the least or greatest level-01 record (Language Reference SC27-8713-03, p. 187), and a level-01 record's least length counts an OCCURS DEPENDING ON table at its fewest occurrences, 1 when the entry has no integer-1 TO (pp. 188, 204). A file without RECORD IS VARYING is checked against its level-01 records under COMPAT as under STANDARD, whether it has RECORD CONTAINS integer-4 TO integer-5, whose integers the record descriptions decide and must match (pp. 187, 191), RECORD CONTAINS integer-3, or no RECORD clause: the guide names only the VARYING declaration as what COMPAT checks, and gives no case without one. A file with no level-01 record is checked against its record area alone",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: VLR_RECORDS_CHECKED,
        claim: "VLR changes the file status alone: under either setting the READ succeeds and delivers the same bytes (Programming Guide SC27-8714-03, p. 422). A record shorter than the check's minimum fills the record area only as far as its length, leaving the rest as it was, which IBM calls undefined, and one longer than the record area is truncated to it (Language Reference SC27-8713-03, pp. 431, 434); the record area, the larger of the RECORD clause's maximum and the longest level-01 record, is taken as the 'maximum record definition size' (p. 431), so a 70-byte record of Table 52's file is delivered whole and READ INTO moves its 70 bytes (p. 188). The check covers records of variable length: a sequential file whose DD or FD is variable, and an indexed, relative or I-O sequential file held in that format. A fixed-length record keeps its status 04 for a data set that ends in a short record or a record longer than the area, whatever VLR says, and a line-sequential file or text DD, whose short lines IBM fills with spaces (Programming Guide p. 218), is not checked",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: VSAM_DATA_SET_LEFT_OPEN,
        claim: "A VSAM data set, which an indexed or relative file's DD names, keeps the open-for-output indicator of its catalog entry as a file beside it, its path with .open-for-output added: OPEN OUTPUT, I-O or EXTEND sets it, and only the successful CLOSE after an OPEN for output takes it away, so an OPEN INPUT leaves it as it was (z/OS 3.1 DFSMS Using Data Sets, idad400/d4011). At OPEN, VSAM implicitly issues a VERIFY when it finds the indicator on, and the OPEN goes on (d4011, idad400/verif; DFSMS Macro Instructions for Data Sets, idad500/x1cb, reason code 118, X'76', the high-used RBA verified). ironwork's verify always succeeds, and the OPEN's status is 97 under VSAMOPENFS(COMPAT), the default, or 00 under VSAMOPENFS(SUCC) (Programming Guide SC27-8714-03, pp. 199, 424; Language Reference SC27-8713-03, p. 303, Table 34), for an OPTIONAL file too. Status 97 sets file status key 1 to 9, so it runs the file's EXCEPTION/ERROR procedure (LR p. 706), and it reports an OPEN that succeeded, so with no FILE STATUS and no procedure the run goes on (LR p. 411; PG p. 204). Files a run unit leaves open are closed when it ends, and an abend leaves the indicator on only where Language Environment closes no file (C152). Sequential files, which ironwork does not hold as VSAM data sets, are never marked; nor are the files of CICS file control, which opens and closes them with each task. ironwork job deletes the mark with its data set, DEFINE CLUSTER starts a data set without one, and REPRO into a data set takes it away, as the CLOSE after REPRO's OPEN for output does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DISPSIGN_SEPARATE,
        claim: "Under DISPSIGN(SEP) DISPLAY shows a signed binary, packed or overpunched zoned item as a leading + or - followed by the digits DISPSIGN(COMPAT) shows, unsigned (Programming Guide SC27-8714-03, pp. 362-363, Table 48), so a COMP-5 or TRUNC(BIN) binary item keeps its 5, 10, 19 or 20 digits (C14). The Language Reference shows the sign 'as if SIGN IS SEPARATE was specified' (SC27-8713-03, p. 334), which without LEADING would put it last; the guide's leading sign and its table are followed. IBM's table lists no other kind, and the rest is chosen: a zoned item with SIGN SEPARATE, whose sign is separate already, shows as stored, a trailing sign staying last; an unsigned item, and a value that is not an item (a literal, a function's result), is unchanged; a national decimal item, which ironwork does not lay out yet, would be unchanged too, IBM naming only binary, packed and zoned. An overpunched zoned item is read as its bytes, as COMPAT shows them, not as arithmetic reads it: the sign is - when the sign position's zone is X'B' or X'D' and + for any other zone, one that is no sign included, and the digits are the bytes with that zone made X'F', so invalid data is shown rather than ending the run",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LILIAN_INTEGER_DATES,
        claim: "Under INTDATE(LILIAN) every integer date a date function takes or returns is a Lilian day, 15 October 1582 being day 1 (Programming Guide SC27-8714-03, pp. 59, 375): INTEGER-OF-DATE, DATE-OF-INTEGER, DAY-OF-INTEGER, INTEGER-OF-DAY, FORMATTED-DATE, FORMATTED-DATETIME and INTEGER-OF-FORMATTED-DATE, which the Language Reference marks as INTDATE's (SC27-8713-03, pp. 551, 555, 571, 573, 583, 585, 587). The Language Reference gives the ranges under ANSI only: integer dates 1 to 3,067,671 and years 1601 to 9999 (p. 509). Under LILIAN the dates run, as Language Environment's date services take them, from 15 October 1582 to 31 December 9999, so integer dates from 1 to 3,074,324, standard dates from 15821015, Julian dates from 1582288, and a formatted date's year from 1582. TEST-DATE-YYYYMMDD, TEST-DAY-YYYYDDD, TEST-FORMATTED-DATETIME and SECONDS-FROM-FORMATTED-TIME carry no INTDATE note and keep years from 1601 (pp. 637, 653, 655, 657). FORMATTED-CURRENT-DATE takes and gives no integer date, so INTDATE does not touch it. COMBINED-DATETIME, the other function the note is on (p. 541), takes an integer date in the same range",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CEECBLDY_UNDER_LILIAN,
        claim: "Under INTDATE(LILIAN) a CALL whose target is the literal 'CEECBLDY' is diagnosed and calls CEEDAYS, which takes the same arguments and gives a Lilian day (Programming Guide SC27-8714-03, p. 375). The guide gives neither the message nor its severity: it is a warning (W, return code 4) in ironwork's words, since the program no longer calls what it names. A CALL of an identifier that holds 'CEECBLDY' is not converted, as the guide names the literal only, and ends the run as ironwork does not provide CEECBLDY; nor is a literal under INTDATE(ANSI)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMPLETE_SET_OF_QUALIFIERS,
        claim: "Under QUALIFY(EXTEND) a reference the standard's rules find ambiguous names the one candidate it gives a complete set of qualifiers, 'every level in the containing hierarchy of names' (Programming Guide SC27-8714-03, p. 400; 'every qualifier is specified', Language Reference SC27-8713-03, p. 68); with no such candidate, or two, it stays ambiguous. A data item's complete set is the name of every group that holds it, nearest first, up to its level-01 entry: a FILLER or unnamed group has no name to give and is no level of it, and a level-66 item's hierarchy is its record. A record's file-name may follow but is not needed, since the Language Reference lets a level-01 name that is the only one of its level be referenced under EXTEND (p. 67), which a record of an FD or SD could not be if its file-name were needed (p. 69); a LINAGE-COUNTER's complete set is its file-name or nothing. A condition-name's hierarchy starts with its conditional variable, which qualifies it (p. 70). The rule applies to RENAMES operands too, among the items of the record. A SUM operand naming a REPORT SECTION entry is found by its report-name and does not follow it",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSPECT_FUNCTION_SUBJECT,
        claim: "INSPECT's identifier-1 can be an alphanumeric or national function-identifier in a TALLYING-only INSPECT (format 1): a function-identifier can be used wherever a sending data item of its category can, except as a receiving operand (Language Reference SC27-8713-03, p. 77), the INSPECT data flow evaluates a function-identifier once, as the first operation (p. 360), and TALLYING leaves identifier-1 unchanged. REPLACING (formats 2 and 3) and CONVERTING (format 4) copy their result back to identifier-1 (Table 40, p. 359), so a function there is a receiving operand and is refused when compiled (S); so is an integer or numeric function as identifier-1, since one can be used only where an arithmetic expression can (pp. 77, 505) and identifier-1 must be a DISPLAY, DISPLAY-1 or NATIONAL item or group (p. 355). MIN, MAX and CONTENT-OF, whose type follows their arguments, are not refused. The manuals give neither the message numbers nor their text: the messages are ironwork's. A function-identifier as identifier-3 to identifier-7 is read as any sending operand is",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSPECT_NATIONAL_FUNCTION_RESULT,
        claim: "INSPECT TALLYING of a national function result counts national characters (two-byte encoding units) and matches comparands and BEFORE or AFTER INITIAL delimiters only at character boundaries, and a figurative constant there is one national character (Language Reference SC27-8713-03, p. 355). An alphanumeric literal or value there is made national as C232 says; an inspected data item of usage NATIONAL is C230. No intrinsic function returns DBCS, so a DBCS function result does not arise",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NATIONAL_CASE_AND_REVERSE,
        claim: "UPPER-CASE, LOWER-CASE and REVERSE of a national argument are national (Language Reference SC27-8713-03, pp. 597, 635, 671), as TRIM's already was (p. 665), and lowering types all four so. REVERSE keeps a surrogate pair as one character (p. 635). The case functions map each character by Unicode's case mapping where that gives one character and leave it alone otherwise, so the result keeps the argument's length as the manual requires; IBM names UnicodeData.txt, whose simple mappings differ from this in a few characters, such as U+0130, which is left as it is here",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INVDATA_CLEANSIGN,
        claim: "Under INVDATA(CLEANSIGN), the default once INVDATA is given and part of ZONEDATA(MIG) and ZONEDATA(NOPFD), a packed-decimal or zoned item whose sign half-byte is a digit, 0 to 9, is read with that half-byte made F, positive, rather than ending in a data exception: the compiler 'generates code to clean the sign nibble of USAGE DISPLAY and USAGE PACKED-DECIMAL data items on input to compare, add, subtract, multiply, and divide operations', and not for SIGN IS SEPARATE (Programming Guide SC27-8714-03, p. 378). Which valid sign the cleaning produces is not stated; F is chosen. It applies wherever the item is read as a number, a MOVE's sending item included, where IBM names only comparisons and arithmetic. Under NOCLEANSIGN and NOINVDATA such a sign is a data exception, as before",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INVDATA_ZONES_COMPARED,
        claim: "Under INVDATA(NOFORCENUMCMP), the default once INVDATA is given and part of ZONEDATA(NOPFD), an unsigned zoned integer compared with zero (ZERO, or a numeric literal of value zero), or with an unsigned zoned integer of its own length, is compared as the bytes it holds, zones included, so an item holding X'F0F040F0' is not equal to ZERO: the compiler compares zoned data 'in the same manner as COBOL 4 or earlier versions', by an alphanumeric comparison where those considered the zone bits, and IBM's VALUE1 example gives false under INVDATA(NOFORCENUMCMP) at any OPT setting (Programming Guide SC27-8714-03, pp. 377-378). Which comparisons COBOL 4 made by their bytes is not listed; these two, where the bytes of equal values are always equal, are chosen, and a condition-name's value is compared as the relation of the two would be. Other comparisons, and every comparison under FORCENUMCMP and ZONEDATA(MIG), read the digits and ignore the zones; NOINVDATA is C262",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: OPTIMIZED_ZONES_COMPARED,
        claim: "Under NOINVDATA, IBM's default, the comparisons of C223 (an unsigned zoned integer against zero or against an unsigned zoned integer of its own length, a condition-name's values included) are made by their bytes, zones included, at OPTIMIZE(1) and OPTIMIZE(2), and as numbers at OPTIMIZE(0), IBM's default: NOINVDATA lets 'the compiler ... generate a string comparison to avoid numeric conversion', and IBM's VALUE1 example, X'F0F040F0' compared with ZERO, is true at OPT(0) and false at OPT(1) and OPT(2) (Programming Guide SC27-8714-03, pp. 377-378). Every other comparison reads its operands' digits at any level, so a digit half above 9, or a sign the NUMPROC setting does not accept, is a data exception at the comparison, as PACK and CP give it; that this is IBM's code for them at OPT(1) and OPT(2) is recalled, not documented. IBM states only that invalid data makes a reference 'undefined' and its results 'unpredictable' (p. 53), that 'digits and sign codes must be valid no matter what options are used' and data that is not may 'behave differently at different levels of optimization' (p. 395), and that NOCLEANSIGN increases 'the probability of a S0C7 abend' when an operand of a comparison has an invalid sign (p. 378). A numeric literal of value zero is compared as ZERO is, which the optimizer cannot tell apart; that too is chosen. Under NOINVDATA and --dialect gnucobol the comparison with zero is made as numbers at every OPTIMIZE level, and the comparison of two unsigned zoned integers of one length by their bytes at every level, as cobc compares them",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALPHANUMERIC_MOVED_UNCHECKED,
        claim: "An alphanumeric sender MOVEd to a numeric receiver is moved as if it were an unsigned integer, aligned on the assumed decimal point and padded with zeros, and a signed receiver gets a positive sign (Language Reference SC27-8713-03, pp. 175, 404). IBM leaves the result undefined when the sender holds anything but digits (Programming Guide SC27-8714-03, p. 53): NOINVDATA assumes the data is valid (p. 377), NUMCHECK(ZON) adds a class test for each such sender (p. 388), and PACK and UNPACK check no sign or digit codes where CVB, ZAP, SRP and ED do (z/Architecture Principles of Operation SA22-7832-13, pp. 7-318, 7-428, 7-232, 8-14, 8-13, 8-8). ironwork's model of the generated code under NONUMCHECK: for a zoned or packed integer receiver without P scaling the MOVE is a byte copy, PACK or UNPK, the receiver takes the low half of each of the sender's rightmost bytes as its digits with zeros to the left, a low half that is not a digit stays in the receiver, and the data exception comes where the item is next read as a number; a receiver with decimal places or P scaling, a binary receiver and a numeric-edited receiver read the sender as a number at the MOVE",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMERIC_MOVED_UNCHECKED,
        claim: "Under NONUMCHECK the generated code checks a zoned or packed sender at none of these MOVEs, the MOVE of a WRITE, REWRITE or RELEASE FROM phrase included: to a zoned item, to a packed item from a zoned one, to a packed item of its own kind and scaling under NUMPROC(PFD), and to an alphanumeric, alphanumeric-edited or group item. NUMCHECK(ZON) and NUMCHECK(PAC) add a class test for each zoned or packed sender (Programming Guide SC27-8714-03, p. 388), and NUMCHECK(ZON(LAX)) leaves the sender of a zoned-to-zoned or zoned-to-alphanumeric MOVE to be checked if the sender is subsequently used in a numeric context (p. 391). IBM leaves invalid data to the generated code: NOINVDATA assumes valid data (p. 377), and results differ with OPT and ARCH (Migration Guide GC27-8715-03, pp. 201, 205-206). For valid data an alphanumeric receiver gets a signed sender's unsigned value (Language Reference SC27-8713-03, p. 403) and an unsigned zoned sender's sign unchanged (Programming Guide, p. 53). PACK and UNPACK check no codes where ZAP, SRP, CVB and ED do (z/Architecture Principles of Operation SA22-7832-13, pp. 7-318, 7-428, 8-14, 8-13, 7-232, 8-8). ironwork's model of the generated code: these MOVEs are a byte copy, PACK, UNPK and an OI that makes a sign F; where the sender's digits or sign are not decimal, each receiver digit takes the low half of the sender's digit of the same power of ten, or zero where there is none, and the data exception comes where the item is next read as a number; a sign half that is a digit stays in a signed receiver's sign place, an unsigned receiver's sign is F, and SIGN SEPARATE reads any character but '-' as positive; an alphanumeric receiver gets a zoned sender's bytes with an overpunched sign's zone made F, or a packed sender's digits unpacked with F zones; a packed sender to another packed shape (ZAP or SRP), and any zoned or packed sender to a binary (CVB), numeric-edited (ED) or floating-point receiver, is read as a number at the MOVE and ends in S0C7 there",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: STATEMENT_LIMIT_IS_TIME,
        claim: "A run given a statement limit (run or job --statement-limit) ends with S322 once that many statements have started, at the next start of the first statement of the loop it is in (the lowest-placed of the statements that recur among the last 4,096 starts, in their outermost frame and the program's own source), or where the count ran out when no statement recurs or that one does not start again within 4,096 more; S322 stands for the system completion code z/OS gives a job step that runs past the CPU time its JOB or EXEC TIME= parameter allows (MVS System Codes, 322). A count of statement starts stands in for CPU time so the end falls at the same statement on every run and on both executors, and placing it at the loop rather than where the count ran out keeps it there whatever ran before the loop; it says nothing of how long the program would run on z/OS, and a loop whose iterations start no statement is not stopped by it",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INITCHECK_ANALYSIS,
        claim: "INITCHECK follows the procedure's control flow to a fixed point: IF, EVALUATE, SEARCH, the ON and NOT ON phrases, inline and out-of-line PERFORM, GO TO, GO TO DEPENDING ON, ALTER, NEXT SENTENCE, EXIT PARAGRAPH, SECTION, PERFORM and PERFORM CYCLE, STOP RUN and GOBACK, and fall-through between paragraphs; under LAX an item counts as set where some path to the statement sets it, under STRICT where every path does (Programming Guide SC27-8714-03, pp. 373-374). As IBM states: LINKAGE, FILE SECTION, EXTERNAL and GLOBAL items are not analysed; a table is one item, set when any element is; an item referenced anywhere with reference modification counts as set; a BY REFERENCE argument is not a use and a BY CONTENT or BY VALUE one is; an item whose address is taken (ADDRESS OF, a BY REFERENCE argument, an EXEC SQL host variable, an EXEC CICS argument) makes its whole level-01 record, with any record that REDEFINES it, address-taken, and every CALL, INVOKE and EXEC statement sets all address-taken records; setting through a pointer is not tracked. What else sets an item is chosen: a VALUE clause; a receiver of MOVE, COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE (and REMAINDER), INITIALIZE, ACCEPT, SET (a condition-name sets its conditional variable), STRING, UNSTRING (with DELIMITER IN, COUNT IN, POINTER and TALLYING), INSPECT REPLACING or CONVERTING and an INSPECT TALLYING counter, SEARCH VARYING, PERFORM VARYING, READ INTO, RETURN INTO, JSON PARSE INTO, JSON and XML GENERATE's receiver and COUNT IN, and a CALL's or INVOKE's RETURNING item; a file's FILE STATUS data-names and RELATIVE KEY by any statement on the file; SQLCA, SQLDA and the DL/I interface block, which the coprocessor and translators pass on every EXEC statement, and a symbolic map RECEIVE MAP or SEND MAP names, are address-taken. The records of the CICS, SQL and DL/I members ironwork supplies when no library holds them (DFHAID, DFHBMSCA, SQLCA and the like) are not analysed: IBM's copies give constants VALUE clauses that ironwork's leave out, and the translators set the rest. Setting a group sets every elementary item in it and a group is set when they all are; setting an item sets every item whose storage it shares through REDEFINES or RENAMES. A use reads each elementary item of a group, and a group is reported by its first that is not set. FILLER items are not analysed. An out-of-line PERFORM, a SORT's input or output procedure and XML PARSE's processing procedure are calls of their range, which returns at the end of its last paragraph: a range's entry state joins those of every statement that performs it, and each continues with its own state plus what every path through the range sets. PERFORM ... UNTIL or VARYING with TEST BEFORE, PERFORM ... TIMES with other than a literal above zero, and XML PARSE's procedure may run no times; WHEN OTHER aside, EVALUATE may select no WHEN. AT END, INVALID KEY and ON EXCEPTION paths leave unset what the statement would otherwise set, and so does ON SIZE ERROR; without such a phrase only the successful path continues. Every operand of a condition is a use, short-circuiting aside, and FUNCTION LENGTH's argument is not. A USE FOR DEBUGGING section may run as control enters each procedure it names, or any procedure for ALL PROCEDURES, and a USE AFTER EXCEPTION/ERROR section after any statement on a file it serves, or on any file for an open mode; procedures only EXEC CICS HANDLE, EXEC SQL WHENEVER or USE BEFORE REPORTING would reach, and code no path reaches, are not analysed. An ENTRY point starts from the VALUE clauses. IBM's analysis is more accurate under OPT(1) and OPT(2) (p. 374), where its optimizer may prove paths impossible that this one does not",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INITCHECK_MESSAGE,
        claim: "INITCHECK's message is IBM's 7311, severity W, return code 4: the Programming Guide's sample MSGEXIT names message 7311(W) as 'the case of INITCHECK messages about uninitialized data items' (SC27-8714-03, Appendix E, p. 841). Its prefix and text are not in the manuals ironwork holds, so ironwork gives the warning in its own words: once per statement and item used, at the statement's position, naming the item and, for a group, the first of its elementary items not set, as IBM reports only the first uninitialized item in a group (p. 374). Under LAX it says no path to the statement sets the item; under STRICT that a path does not",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PARMCHECK_BUFFER,
        claim: "Under PARMCHECK the buffer of n bytes, 100 by default, starts at the byte after the end of the WORKING-STORAGE the program declares, the furthest any of its 01 and 77 items reaches, with no slack bytes between, so a CALL that writes one byte past the last item changes it: IBM puts it 'following the last data item in the WORKING-STORAGE section' (Programming Guide SC27-8714-03, p. 397) and does not say whether it is aligned; were it on a doubleword, a write into the slack bytes before it would go unseen on z/OS. The special registers and Report Writer data ironwork adds to WORKING-STORAGE after the program's own items (the SORT, XML and JSON registers, DEBUG-ITEM, LINAGE-COUNTER, a report's counters), which are no items the program declares, follow the buffer, as do the file record areas and index-names; every offset after the buffer moves by n, and a program compiled without PARMCHECK keeps its layout byte for byte. The buffer is no data item and nothing names it. It holds zeros until a CALL sets it to X'AA'; each CALL of a program, of a Language Environment callable service and through a function-pointer sets it after the arguments are evaluated and checks it once the called program returns, before RETURNING is stored and before NOT ON EXCEPTION runs. A CALL after which the run ends, by STOP RUN or an abend in the called program, is not checked",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PARMCHECK_MESSAGE,
        claim: "IBM documents what PARMCHECK's message holds but not its number or text: under MSG a runtime warning with the name of the parameter, the line number of the CALL statement and the program name, issued after the CALL, and under ABD a similar message at a terminating level that causes an abend (Programming Guide SC27-8714-03, p. 397); the corpus of public job output searched has PARMCHECK only in option listings. The message is ironwork's, with no IGZ number: 'PARMCHECK: SUB, called at line 14 of program MAIN, wrote past the end of WORKING-STORAGE, beyond parameter LAST-ITEM', the program being the one with the CALL. Under MSG it goes to standard error after 'ironwork: line:column: ' and the run goes on; under ABD the run ends with U4038, as a Language Environment condition of severity 3 that nothing handles ends it under the default ABTERMENC(ABEND), as an SSRANGE failure does. The parameter named is chosen: of the CALL's arguments whose storage starts in the calling program's own WORKING-STORAGE, so BY REFERENCE ones (a BY CONTENT or BY VALUE argument is a copy elsewhere), a LINKAGE item counting when its address is there, the one starting nearest the buffer, since a called program that declares it longer reaches the buffer soonest; of two starting at the same byte, the later in the USING list. With no such argument the message names none. Which bytes changed is not reported",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMCHECK_SENDERS,
        claim: "NUMCHECK (and ZONECHECK, which is NUMCHECK(ZON)) tests an item where a statement reads it as a sender: an operand of an arithmetic expression, a comparison or a condition-name's variable, a MOVE's sender, a receiver that is also a sender (ADD A TO B), a subscript, and a BY CONTENT or BY VALUE argument, before the statement uses it; not a receiver alone, a BY REFERENCE argument, a class test, or DISPLAY, which shows the bytes and uses no value (Programming Guide SC27-8714-03, pp. 388-391, 427). A zoned item is tested as IF NUMERIC tests it, after INVDATA(CLEANSIGN) has cleaned its sign (C222); a packed one too, with the spare half-byte of an even digit count zero; a binary one for a value of more digits than its PICTURE, except COMP-5, and under TRUNC(BIN) only with BIN(TRUNCBIN). An alphanumeric item moved to a numeric receiver is tested as an unsigned integer. ZON(NOALPHNUM) leaves a zoned item untested in a comparison with an alphanumeric item, literal or figurative constant. ZON(LAX) leaves a zoned item moved to a zoned or alphanumeric receiver untested, and tolerates the two redefinitions of C280. A test the compiler finds always fails is reported when compiled and removed (C281). Each test is made each time the statement runs, where IBM removes redundant ones, so a loop may report more often than Enterprise COBOL does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMCHECK_MESSAGE,
        claim: "NUMCHECK's run-time message names the item, its bytes in hexadecimal, the program and, by its position, the line, as IBM's does (Programming Guide SC27-8714-03, p. 391), in ironwork's words with no IGZ message number, since no Enterprise COBOL output ironwork has shows one. Under MSG it is written to the error stream and the statement runs; under ABD the run ends with U4038, the abend a Language Environment condition of severity 3 gives, as SSRANGE's does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PARM_ARGUMENTS_BEFORE_LAST_SLASH,
        claim: "A COBOL main program that EXEC PGM=,PARM= starts receives as its program arguments what precedes the PARM's last slash, the rest being runtime options; when there are only invalid runtime options the whole string is the argument, so 11/16/1967 reaches the program whole. This is CBLOPTS(ON), the non-CICS default CBLOPTS=((ON),OVR) (Language Environment Programming Guide, COBOL compatibility considerations; Programming Reference, CBLOPTS)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PARM_AREA_PADDED,
        claim: "A runtime option after the last slash is known by its full name, or its NO form, from the CEEXOPT sample's list; an abbreviation counts as invalid, so a PARM whose only runtime options are abbreviated reaches the program whole. The first PROCEDURE DIVISION USING item addresses a halfword length and the arguments in the program's code page, followed by X'00' up to 100 bytes, JCL's longest PARM, so a program that reads its whole PIC X(100) parameter field reads zeros past the arguments rather than leaving storage. A step with no PARM passes a length of zero. Items after the first are not addressed",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ABBREVIATED_RELATIONS,
        claim: "An abbreviated combined relation condition (Language Reference SC27-8713-03, pp. 287-289) implies the last stated subject, or the last stated subject and relational operator, a NOT just before the operator being part of it, so A NOT > B OR C is (A NOT > B) OR (A NOT > C), and IS may come before an implied operator, as CCVS85 NC250A writes AND IS NOT LESS THAN. Parentheses after AND, OR or NOT take the implied subject and operator in. After the right parenthesis the subject and operator stated before the parentheses are current again, whatever was stated inside, so A = B AND (C OR < D) OR 2 ends with A = 2; the manual's rule 10 does not say which operator follows a right parenthesis. A class or sign condition leaves the implied subject and operator as they were. A bare name after AND or OR, with NOT or left parentheses between or not, is a condition-name when it names one and otherwise an object, decided when the name is resolved. A relational operator followed by parentheses that hold objects joined by AND, OR and NOT is distributed over them (rule 5), and NOT just after that parenthesis is refused. The manual's examples (Table 31, p. 289) agree with their unabbreviated forms in ironwork's tests; no Enterprise COBOL listing ironwork has shows what follows a right parenthesis or a class condition in an abbreviation",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PARAGRAPH_IN_OWN_SECTION,
        claim: "A paragraph-name need not be qualified when referred to within the section in which it appears (Language Reference SC27-8713-03, p. 68), so an unqualified name that more than one procedure has names the paragraph of that name in the section the reference is written in, when that section has one; from any other section it is refused as naming more than one paragraph, as before. The manual states the rule for references in general; ironwork applies it to each. Before any check, the compiler qualifies the names of GO TO, GO TO DEPENDING ON, PERFORM and its THRU, ALTER, SORT and MERGE procedures and XML PARSE's processing procedure with the section they are written in, so the compiler, the interpreter and the LIR lowering find the same paragraph. USE FOR DEBUGGING ON resolves from its declarative section, EXEC SQL WHENEVER GO TO from the paragraph of the SQL statement it follows, as the precompiler writes its GO TO there, and an EXEC CICS HANDLE label from the paragraph of the HANDLE command, with the same rule. CCVS85 NC208A's GO TO PAR-3C, unqualified in the section that has a PAR-3C, is the corpus case",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPLACE_STATEMENT,
        claim: "REPLACE applies after COPY, to the whole expanded text, EXEC SQL and EXEC CICS statements included, as the Db2 coprocessor has it (Language Reference SC27-8713-03, pp. 708-712; Programming Guide SC27-8714-03, p. 513). A REPLACE is in effect from its period to the next REPLACE statement or the end of the source file: the manual says the end of the separately compiled program, so a batch of several programs in one file carries a REPLACE past the END PROGRAM of the one it is in, where IBM would stop it. The word REPLACE starts a statement only when pseudo-text, LEADING, TRAILING or OFF follows it, and wherever it is: the manual asks for a separator period before it, but its own example (pp. 709-710) has one after a DISPLAY with none, and gives that program's output. Matching is COPY REPLACING's: text words compared a word at a time, case aside outside literals, with a separator comma or semicolon never a word, so pseudo-text-1 that is only a comma or semicolon matches nothing. The replacing text takes the position of the first word it replaces, for messages and for Area A, where IBM puts each of its words in the area it is written in within pseudo-text-2; and REPLACE ALSO and REPLACE LAST OFF, which the 2014 standard has and the manual does not, are refused",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: VARIABLY_LOCATED_ITEMS,
        claim: "An item after an OCCURS DEPENDING ON table in its record, and not within it, is variably located (Language Reference SC27-8713-03, pp. 205-206; Programming Guide SC27-8714-03, pp. 81-82): each reference places it back from where the table's maximum puts it by the bytes of the occurrences past the object's current value, and a group holding such tables is as long as their current counts make it; its VALUE is placed as if each table held its maximum (LR p. 246). A receiving group that holds the objects of its tables is at its maximum length only when nothing after it in its record moves with them, since the guide has the actual length used when a variably located item follows (PG pp. 78-79). The location is worked out from the objects' values at each reference, so a count changed between two references moves what follows, as the guide says (p. 82), with no data moved; JSON GENERATE and XML GENERATE place each member as they write it, and JSON PARSE as it reaches the member's pair, after any earlier pair has set a count. Refused by name: an OCCURS DEPENDING ON object that is itself variably located, and a sort key or INITIALIZE target that is (LR pp. 205, 351, 402, 454), which IBM forbids; and, keeping the message ironwork gave before, a table with items after it that is within another table or holds one, a table with variable-length elements, whose element length the interpreter keeps at its maximum",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: EXTERNAL_STORAGE,
        claim: "An EXTERNAL data record, and an EXTERNAL file's connector and record area, are the run unit's: the first program activated that describes one allocates it, zeroed as ironwork zeroes WORKING-STORAGE before VALUE clauses, since the record takes no VALUE (Language Reference SC27-8713-03, pp. 197, 246; Programming Guide SC27-8714-03, p. 573), and every later description shares it until the run unit ends (pp. 65, 184), in a CICS task the enclave a LINK or XCTL starts (C126). A record and a file are looked up by name in separate name spaces, and a WORKING-STORAGE record that redefines an EXTERNAL one shares its storage (p. 226). A description of another size than the one in the run unit ends the run with an ironwork abend when the program describing it is activated: the manuals say the records must define the same number of bytes, and the file descriptions the same maximum record size (pp. 186, 197), but not what the runtime does when they do not. CANCEL and an INITIAL program's return leave an EXTERNAL file open (Programming Guide pp. 178, 203); the end of the run unit closes it. INDEXED BY indexes of an EXTERNAL record stay the program's own (p. 197). Under --dialect gnucobol a later description of an EXTERNAL record shorter than the run unit's shares its storage, with a warning on standard error, as cobc's does; a longer one still ends the run",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: GLOBAL_NAMES,
        claim: "A contained program sees the GLOBAL records and files of each program containing it, at that program's storage, record area and file connector (Language Reference SC27-8713-03, pp. 63-66, 185, 197). A name resolves among the program's own names and every GLOBAL name of the programs containing it, qualified as usual; where more than one item qualifies, the one declared nearest wins, the program's own first (p. 66). A file-name the program or a nearer program declares hides a farther GLOBAL one. A contained program called while a program containing it is not running, which only ironwork's flat program library allows, ends the run when it uses that program's GLOBAL names. Refused as not supported yet: a GLOBAL file whose FILE STATUS, RECORD KEY, ALTERNATE RECORD KEY or RELATIVE KEY is not a GLOBAL name of the program declaring it, as IBM resolves them there; LINAGE or REPORT on a GLOBAL file in a program that contains others, and on an EXTERNAL file; INDEXED BY in a GLOBAL record of a program that contains others, whose index is global too (p. 65); a GLOBAL file a contained program and its declaring program use differently as a print file, since ironwork decides that from each program's own WRITE statements; and SET ADDRESS OF a GLOBAL LINKAGE record from a contained program",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SET_TO_ENTRY,
        claim: "SET procedure-pointer or function-pointer TO ENTRY literal or identifier (Language Reference SC27-8713-03, 'Format 6: SET for procedure-pointer and function-pointer data items', pp. 449-450) resolves the entry when the SET runs: the program holding the name is found and loaded as a CALL of the name would find it, static for a literal under NODYNAM and dynamic for an identifier or under DYNAM (Programming Guide SC27-8714-03, pp. 557-558), and a name no program or Language Environment service has abends S806 at the SET. The manuals do not say when a dynamic entry is loaded; ironwork loads it at the SET because the pointer then holds an entry address. The pointer holds a value of ironwork's own, the same for each SET of the same name and resolution, so two pointers set to one entry compare equal; a CALL through it enters the entry as a CALL of the name does, with the arguments passed as the BY phrases say (pp. 320, 559), and ON EXCEPTION never runs, the entry having been found. After a CANCEL of the program the pointer is undefined (p. 558); ironwork's CALL through it loads the program again. identifier-9, a user-defined function returning a pointer, and SET TO a pointer a non-COBOL program set are not run, ironwork having neither",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: HEX_CURRENCY_SIGN,
        claim: "CURRENCY SIGN IS X'...' gives the currency sign value the literal's bytes are in the program's code page, the CODEPAGE option's, as the Programming Guide's table of the euro sign's code points by code page has it, X'9F' in 1140 and X'5A' in 1142 (SC27-8714-03, pp. 64-65). Without PICTURE SYMBOL the literal is one byte, and the character it is must be one a PICTURE currency symbol can be; with it, the characters must include no digit, +, -, . or , (Language Reference SC27-8713-03, pp. 129-130). The symbol a PICTURE writes is the character the byte is, so the source spells it as the code page shows it; a lowercase letter, the same byte in every code page ironwork carries, keeps its case in a PICTURE as an alphanumeric symbol's does. The guide's own example, X'9F' WITH PICTURE SYMBOL 'U' (p. 64), is refused, since the Language Reference excludes U from literal-7 (p. 130)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INITIALIZE_FLOAT_NUMERIC,
        claim: "INITIALIZE's VALUE and REPLACING phrases take a COMP-1 or COMP-2 receiver as of category NUMERIC, reading the Language Reference's 'a data item of category internal floating-point ... is treated as if it were in the NUMERIC category' (SC27-8713-03, p. 352) as applying to receivers as well as to identifier-2, since rule 2 (p. 353) names no implied sending item for a floating-point category otherwise. A POINTER item, which no category names, is set to NULL with no phrase or with DEFAULT, as ironwork's INITIALIZE did before the phrases, and is left alone otherwise. The VALUE phrase finds no VALUE clause on a FILE SECTION or LINKAGE SECTION item, whose VALUE ironwork does not apply at all, and gives an OCCURS item's one VALUE to every occurrence. DBCS, EGCS, NATIONAL-EDITED and UTF-8 name no item, as ironwork has no items of those categories",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_VALUE_LITERAL,
        claim: "A floating-point literal in the VALUE clause of a COMP-1 or COMP-2 item (Language Reference SC27-8713-03, pp. 45, 246) gives the item the value its mantissa times ten to its exponent has, written in fixed point and converted to hexadecimal floating point as MOVE converts a fixed-point literal; Enterprise COBOL converts the literal when it compiles, and its conversion may round the last hexadecimal digit differently. One whose value needs more than 31 digits in fixed point, such as 1.0E+40 or 1.0E-35, is refused, as is a floating-point literal in the VALUE of a fixed-point item, as IBM refuses it. A floating-point literal is read as one only in an item's VALUE clause, not in a level-88 VALUE or the PROCEDURE DIVISION, and a fixed-point VALUE on a COMP-1 or COMP-2 item, which IBM refuses unless it is zero, is still accepted",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ABEND_EXITS,
        claim: "HANDLE ABEND gives each logical level one exit: PROGRAM or LABEL replaces the level's exit, active, CANCEL (the default) deactivates it and RESET reactivates it, also after CICS deactivated it on entry; PUSH HANDLE suspends it until POP HANDLE (CICS TS 5.3 Application Programming Reference SC34-7402-00, pp. 314-315; Application Programming Guide SC34-7401-00, pp. 364-365, 371-373). When the task abends, the exit of the level the abend happened at, else of each level above in turn, takes the first active one, deactivated as it is entered. A LABEL is a GO TO in the program that set it, the program levels below gone (C236). A PROGRAM must be one a LINK could find when HANDLE ABEND runs, else PGMIDERR, and is entered as by LINK with the COMMAREA and EIBCALEN of the program that set the exit; when it returns, the level that set it ends and control passes to the level above, or the task ends normally, and an abend in it goes on to the levels above; one that cannot be loaded then abends APCT, which goes on likewise. Intercepted are the transaction abends: a condition's AEIx, ABEND ABCODE, ASRA for a program check and APCT; not ABEND CANCEL, ASPx or APSJ, and, by ironwork's choice, not its own IRONWORK refusals or the batch codes a CICS task does not end with here (S806 from CALL, file status abends). ASSIGN ABCODE gives the code of the abend an exit was given, spaces before any. A CALLed subprogram is at its caller's logical level (C237, C238), where RETURN and XCTL end the whole level (C233), and the program XCTL starts is at the level of the one that issued it, with the level's exit (C239)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMCHECK_LAX_REDEFINES,
        claim: "Under NUMCHECK(ZON(LAX)) the two redefinitions the Programming Guide lists are recognised from the data description alone (SC27-8714-03, pp. 390-391). An unsigned zoned item, not in a table, whose level-01 or level-77 record REDEFINES a level-01 or level-77 signed zoned item whose sign is a trailing overpunch, and whose last byte is that item's last byte, is tested as a signed item is. A zoned item, unsigned or signed with a trailing overpunch and not in a table, that starts at the first byte of a level-01 or level-77 numeric-edited item its record REDEFINES may hold a space in each leading byte over a position from the edited PICTURE's start through its last leading Z, the insertion characters among those Z symbols included: ZZ,ZZ9.99 gives five, ZZ99.99 two and $ZZ9 none. Spaces and digits may mix there, as the Guide says. Insertion characters after the last leading Z, as in ZZ,999, are not tolerated, though zero suppression may leave them as spaces. The Guide shows explicit REDEFINES only, so the records of an FD, which share storage without one, are not taken as redefinitions",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMCHECK_ALWAYS_FAILS,
        claim: "When the compiler can determine that a NUMCHECK test will always find invalid data, it gives an error-level message and removes the test, under MSG and ABD alike (Programming Guide SC27-8714-03, pp. 388, 391); the Guide does not say when it can. ironwork determines it for an item, not in a table, that holds for the whole run what one alphanumeric VALUE clause gives it, the item's own or a group's above it, with no other VALUE clause reaching its bytes: a WORKING-STORAGE or LOCAL-STORAGE item that the INITCHECK analysis (C224) finds no statement sets and no CALL, INVOKE or EXEC statement reaches by its address, and that no table SORT or READ of a RECORD VARYING DEPENDING ON file sets. The references are those the interpreter tests that the compiler can name: an arithmetic expression's operands, PERFORM TIMES counts and expressions in a relation among them; a MOVE's sender, alphanumeric ones as integers for a numeric receiver; a numeric item compared with a number, ZERO, an expression or another numeric item, except under INVDATA(NOFORCENUMCMP); a condition-name's conditional variable; and a BY CONTENT or BY VALUE argument; each without subscripts or reference modification. The message, at the reference, names the item, its bytes and the fault, severity E (return code 8), in ironwork's words since the Guide gives no number, and the test there is removed. Any other reference keeps its test at run time, where Enterprise COBOL may have removed it. A program with object-oriented syntax is not analysed",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_SOURCE_ORDER,
        claim: "A run enters the first program of its source, ahead of the user-defined functions and function prototypes before it. IBM makes the first program or user-defined function of a batch compilation the default entry point, and asks for a binder ENTRY statement naming the main program when a function comes first, as IBM requires of a function with no prototype (Programming Guide, 'Structuring user-defined functions' and 'Link-editing user-defined functions'); ironwork runs as if that ENTRY statement were given. `run` and `check` compile the source's functions and prototypes with the program and report their messages first, as IBM compiles the whole compilation group; a function that does not compile stops the run, and a source of functions alone has no program to run. A function is loaded by its external name, AS literal-1 or else its function-name, matched without regard to case and without PGMNAME's truncation, as CALL matches program names",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_NAMED_AS_INTRINSIC,
        claim: "No user-defined function takes an intrinsic function's name, neither in FUNCTION-ID nor in the REPOSITORY paragraph without INTRINSIC. The Language Reference forbids LENGTH, RANDOM, SIGN, SUM and WHEN-COMPILED as a REPOSITORY paragraph's user-defined function names ('REPOSITORY paragraph') and says nothing of the other intrinsic names, nor whether FUNCTION name then invokes the intrinsic function or the user's; ironwork refuses the name rather than guess which, so Enterprise COBOL may compile a program ironwork refuses",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_ARGUMENT_TEMPORARIES,
        claim: "Conformance is between a formal parameter and a data item (Language Reference, 'USING phrase', conformance of parameters for user-defined functions): an argument that is a literal, an arithmetic expression or a function's value has no description to conform. It is evaluated before the function is entered and stored, as MOVE stores a value, in a temporary with the formal parameter's description, which the function addresses BY REFERENCE. IBM's own example passes literals so, docalc('add' 10 0.23) to PIC X(3), 999 and V999, giving result=010230 (Programming Guide, 'Invoking user-defined functions'). A BY VALUE argument, a data item's too, is stored the same way, which is COMPUTE's truncation for a numeric parameter and SET's copy for a pointer. A numeric parameter takes only what COMPUTE could send it, as IBM says of BY VALUE and ISO 1989 of every parameter: an alphanumeric, hexadecimal or national literal, or an index or numeric-edited item BY VALUE, is refused. A figurative constant is no function argument (Language Reference, 'Function-identifier'). A reference-modified data item is passed BY REFERENCE at its first byte without the conformance check, since its length is known only when the statement runs",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_SQL_CICS,
        claim: "The Programming Guide says SQL, CICS and JAVAIOP cannot be used with user-defined functions, as the function definition must come ahead of the program using it ('Structuring user-defined functions'), and does not say which compilations that reaches. ironwork refuses EXEC SQL and EXEC CICS in a function definition or prototype and in a program its source defines or prototypes a function before; a program that invokes a function defined in another source, with no prototype in its own source, is not refused",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FUNCTION_INVOCATION_ORDER,
        claim: "A user-defined function runs when the operand holding it is evaluated, a statement's operands in the order the interpreter evaluates them, left to right for DISPLAY and an arithmetic expression, so an operand after the invocation sees what the function stored through a BY REFERENCE argument and one before it does not; IBM documents neither when nor in what order a statement's function-identifiers are evaluated. Functions are always recursive (Programming Guide, 'Using user-defined functions'): every activation shares the function's WORKING-STORAGE and has LOCAL-STORAGE of its own, as a RECURSIVE program's do. STOP RUN in a function ends the run unit from the statement that invoked it, as in a called program",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSPECT_NATIONAL_ITEM,
        claim: "INSPECT of a data item of usage NATIONAL, reference-modified or not, works in national characters (two-byte encoding units): TALLYING counts them, CHARACTERS takes one at a time, comparands and BEFORE or AFTER INITIAL delimiters match only at character boundaries, CHARACTERS BY takes a one-character national substitution field, and CONVERTING pairs the national characters of its operands by ordinal position (Language Reference SC27-8713-03, pp. 355-359). A figurative constant there is a one-character national literal (p. 355) of the value the Programming Guide gives (SC27-8714-03, p. 132), and as a substitution field it fills each occurrence of the subject field (p. 357). Both executors take the unit from the inspected item's kind; lowering keeps the literals of an INSPECT of a national item as values, which rt reads as national characters, and builds no CONVERTING table of bytes for one. ironwork holds no DBCS data (C212), national group, or national-edited or national numeric item, so an INSPECT of one does not arise",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSPECT_OPERAND_USAGE,
        claim: "Every identifier of an INSPECT but the count field has the inspected item's usage, and every literal is national when the inspected item is of usage NATIONAL and alphanumeric when it is of usage DISPLAY; a figurative constant not beginning with ALL takes either usage (Language Reference SC27-8713-03, p. 355; Programming Guide SC27-8714-03, pp. 112, 128). ironwork refuses each operand that breaks this as a severe error (S): with a national inspected item, a data item not of usage NATIONAL or an alphanumeric, hexadecimal or numeric literal; with any other, a national item or literal. The manuals give neither the message numbers nor their text: the messages are ironwork's. A numeric literal or ALL literal with an inspected item that is not national is still taken as its characters, as before this entry. A function-identifier operand is not checked when compiled, since ironwork knows an intrinsic function's category only when it runs (C232)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INSPECT_OPERAND_MADE_NATIONAL,
        claim: "Where INSPECT works in national characters, of a national item (C230) or a national function value (C191), an operand whose value is not national, an alphanumeric function value or a literal of a TALLYING of a function value, is converted to national characters through the program's code page, as MOVE converts an alphanumeric sender to a national receiver. IBM requires such an operand to be national (Language Reference SC27-8713-03, p. 355) and says nothing of a run that has one, so the conversion is ironwork's choice. A national value as an operand of an INSPECT of an item that is not national is compared byte by byte",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ABEND_LABEL_GO_TO,
        claim: "When a HANDLE ABEND LABEL exit is taken in a COBOL program, control returns to the HANDLE ABEND command with the registers restored and a GO TO is executed there (CICS TS 6.x, HANDLE ABEND, topic dfhp4_handleabend; 'Creating a program-level abend program or routine', dfhp3mh). The return points of out-of-line PERFORMs are not registers but the program's own state, which a CALL resets (Programming Guide SC27-8714-03, p. 547), so the PERFORMs the abend left keep theirs armed, as a GO TO out of them would (C99): control that later passes the end of such a range returns after its PERFORM, which puts back the return point it displaced, and a PERFORM that repeats or is inside another statement is refused there as C99 says. The GO TO is the HANDLE ABEND command's, so a debugging section on the label gets that command's line as DEBUG-LINE; the manuals say nothing of PERFORMs here",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ABEND_EXIT_ACROSS_CALL,
        claim: "A CALLed subprogram runs at its caller's logical level (CICS TS 6.x, 'Flow of control between programs and subprograms', dfhp3_cobol_subprog_flow). A static CALL, and a CALL of a contained program, leaves the level's HANDLE ABEND exit in effect in the subprogram, and an exit the subprogram sets is the level's after it returns ('Rules for calling subprograms', dfhp3_cobol_subprog_rules: for a statically called program, abend handling remains in effect irrespective of CBLPSHPOP). A dynamic CALL runs as under CBLPSHPOP(ON), the default (Enterprise COBOL 6.4 Performance Tuning Guide, CBLPSHPOP): the caller's exit is suspended, as by PUSH HANDLE, until the subprogram returns, when the exit the subprogram set is dropped and the caller's put back, as by POP HANDLE; an abend in the subprogram meets the exit as the subprogram left it. The Programming Guide describes that PUSH for a CALL of any program that is not contained and says nothing of static calls (SC27-8714-03, p. 503); ironwork follows the CICS rule. HANDLE CONDITION, IGNORE CONDITION and PUSH HANDLE's stack go with a CALL in the same way (C234)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ABEND_LABEL_OWNER,
        claim: "A HANDLE ABEND LABEL exit is taken only in the activation of the program that set it, when an abend reaches the logical level there, from that program or from a level below it. An abend that reaches the level in another program, one the setter CALLed, one that CALLed it or one that took its place by XCTL, or after the setter has returned, abends the task APC2, the code CICS gives for an illegal branch following an abend with an active handle label abend, an out-of-block GO TO to an inactive block (CICS TS 6.x, abend code APC2); the Programming Guide says a HANDLE LABEL cannot handle an abend caused by another program invoked with CALL, and that such cross-program branching ends the transaction (SC27-8714-03, p. 503). The exit is deactivated as when taken, ASSIGN ABCODE gives the code it was reached with, and APC2 goes on to the levels above. A program CALLed again is a new activation, which does not take a label its earlier one set. A PROGRAM exit is entered where the logical level runs, whichever of its programs set the exit or abended",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ABEND_EXIT_ACROSS_XCTL,
        claim: "XCTL transfers control to another program at the same logical level and releases the program that issued it (CICS TS 6.x, XCTL, dfhp4_xctl). The HANDLE ABEND exit belongs to the logical level, not to a program: a HANDLE ABEND overrides any preceding one in any application program at the same logical level, and only one exit at each level can be active ('Abnormal termination recovery', dfhp378), where HANDLE CONDITION applies only to the program that issued it and is deactivated by LINK and XCTL (dfhp4_handlecondition, dfhp3_exc_handlecondition). The program XCTL starts therefore has the level's exit; the manuals do not say XCTL cancels it. A PROGRAM exit is entered with the COMMAREA of the program that issued the HANDLE ABEND, not of the one that abended (dfhp4_handleabend): ironwork keeps that program's DFHCOMMAREA and EIBCALEN when the command runs, and when it had none the program running the level gives its own. A LABEL exit abends APC2 when an abend reaches it (C238), the program that set it having been released",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RETURN_ENDS_THE_LEVEL,
        claim: "EXEC CICS RETURN and XCTL end the logical level they are issued at, whichever of its programs issues them. A program a COBOL CALL reaches, static or dynamic, is at its caller's logical level, and a run unit is what the task, a LINK or an XCTL starts, with the programs it CALLs (CICS TS 6.x, 'Flow of control between programs and subprograms', dfhp3_cobol_subprog_flow). RETURN in a CALLed program terminates the calling program ('Rules for calling subprograms', dfhp3_cobol_subprog_rules) and goes back to the program that LINKed to the level, or at the task's first level to CICS (dfhp3_cobol_subprog_flow; RETURN, dfhp4_return); TRANSID and COMMAREA are allowed there, where RETURN goes back to CICS, as for the program running the level; below it COMMAREA raises INVREQ and TRANSID is allowed (C143). XCTL in a CALLed program starts its program as the one running the level, the CALL chain released with the run unit (XCTL, dfhp4_xctl), with a copy of the COMMAREA, as every XCTL passes, and the exit C239 gives; the level ends when that program does. Each program of the CALL chain then ends as its CALL comes back, as by GOBACK there: nothing after the CALL runs, NOT ON EXCEPTION and the RETURNING item included, and the PERFORMs in progress end with the activations whose return points they are (C99). The manuals do not say what becomes of the CALL statements; ironwork ends their programs so",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_HANDLERS_ACROSS_CALL,
        claim: "HANDLE CONDITION, IGNORE CONDITION and the PUSH HANDLE stack belong to the logical level, as the HANDLE ABEND exit does (C237): a program a CALL reaches is at its caller's level and inherits the current HANDLE commands, and POP HANDLE undoes the last PUSH HANDLE at the current link level (CICS TS 6.x, PUSH HANDLE, dfhp4_pushhandle; POP HANDLE, dfhp4_pophandle). For a static CALL, and a CALL of a contained program, condition, AID and abend handling remain in effect whatever CBLPSHPOP says ('Rules for calling subprograms', dfhp3_cobol_subprog_rules): the subprogram starts with the caller's handlers and stack, and the caller has them as the subprogram left them, its HANDLE CONDITION, IGNORE CONDITION, PUSH and POP HANDLE included, as with a CALL that pushes nothing, where the caller inherits any settings made in the subprogram (Enterprise COBOL 6.4 Performance Tuning Guide, CBLPSHPOP). A dynamic CALL runs under CBLPSHPOP(ON), the default: COBOL issues a PUSH HANDLE on entry, after which no condition or abend handling is active in the subprogram until it issues its own, and a POP HANDLE when control returns, which drops what the subprogram set and puts the caller's back (dfhp3_cobol_subprog_rules; Programming Guide SC27-8714-03, p. 503). ironwork takes those as a PUSH and a POP on the level's stack: a POP HANDLE in the subprogram with no PUSH of its own undoes the CALL's, the return's POP then undoes the PUSH before it, and with none left, which the manuals do not cover, changes nothing. A subprogram that abends does not return: the abend meets the handlers as it left them",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_CONDITION_LABEL_OWNER,
        claim: "A HANDLE CONDITION label, like a HANDLE ABEND one (C238), belongs to the program activation that issued the HANDLE CONDITION. The label must be in the same PROCEDURE DIVISION as the command that causes the branch, a HANDLE label cannot handle a condition caused by another program invoked with CALL, and the attempt at cross-program branching ends the transaction (Programming Guide SC27-8714-03, pp. 503-504). A condition raised where the label it goes to was set by another activation, a program the CALL passed the handlers from or to, or one that has returned, therefore abends the task APC2, the code CICS ends a dynamically called program with when it abends under CBLPSHPOP(OFF) with its caller's condition handling active (dfhp3_cobol_subprog_rules; CICS TS 6.x, abend code APC2), and a HANDLE ABEND exit can intercept it (C24). The Programming Guide says a condition in a nested program goes to its container's label with unpredictable results (p. 504); ironwork abends APC2 there too. IGNORE CONDITION, which names no label, applies in whichever program raises the condition, and a program CALLed again is a new activation, which does not take a label its earlier one set",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RETURN_BELOW_THE_FIRST_LEVEL,
        claim: "The COMMAREA, IMMEDIATE and CHANNEL options of RETURN can be used only when RETURN returns control to CICS; otherwise INVREQ occurs with RESP2 2, 'a RETURN command with the CHANNEL, COMMAREA, or IMMEDIATE option is issued by a program that is not at the highest logical level' (CICS TS 6.x, RETURN, dfhp4_return). TRANSID is not among them: specified on a program that is not at the highest level, it is the transaction identifier for the terminal's next input unless an error on COMMAREA, INPUTMSG or CHANNEL on the final RETURN clears it (dfhp4_return). A RETURN TRANSID below the first level therefore names the next transaction, which a later RETURN TRANSID replaces, and one that raises INVREQ names none. ironwork raises none of the errors that clear it, models no terminal-less task (RESP2 1) and keeps CHANNEL only for the INVREQ",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_STOP_RUN_ENDS_THE_LEVEL,
        claim: "STOP RUN terminates the run unit, the Language Environment enclave, whose main routine it ends (z/OS 3.1 Language Environment Programming Guide, 'The enclave defines the scope of language statements', ceea200118). Under CICS a run unit is what the task, a LINK or an XCTL starts, with the programs it CALLs ('CICS run unit', ceea200254; CICS TS 6.x, 'Flow of control between programs and subprograms', dfhp3_cobol_subprog_flow), and a program at level 2, LINKed, CALLed there or started there by XCTL, can use GOBACK, STOP RUN or EXEC CICS RETURN to return to the level 1 program that LINKed to it (dfhp3_cobol_subprog_flow). STOP RUN therefore ends the logical level it runs at, as RETURN does (C233): a LINK, or a HANDLE ABEND PROGRAM exit entered as by LINK, comes back from it, and at the task's first level it ends the task. The Programming Guide's table of termination statements says instead that in a CICS environment STOP RUN terminates the entire transaction, including all programs running within it (SC27-8714-03, p. 546, Table 70); ironwork follows CICS and Language Environment, whose rules for the level are the more specific",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RUN_UNIT_PER_LINK,
        claim: "Under CICS a run unit, the Language Environment enclave, is entered at the start of the task or by a LINK or XCTL, and each enclave has its own heap storage and other Language Environment resources (CICS TS 6.x, 'Flow of control between programs and subprograms', dfhp3_cobol_subprog_flow; z/OS 3.1 Language Environment Programming Guide, 'CICS run unit', ceea200254). On each entry to a LINKed program a new initialized copy of its WORKING-STORAGE is provided and the run unit is reinitialized; a program a static or dynamic CALL reaches gets a new initialized copy on its first entry within a logical level and its last-used state on later entries at the same level ('Rules for calling subprograms', dfhp3_cobol_subprog_rules). ironwork therefore keeps each loaded program's state, its storage, open files, CLOSE WITH LOCK, ALTERs, activity and whether a dynamic CALL entered it, per CICS run unit: a LINK, an XCTL and a HANDLE ABEND PROGRAM exit, entered as by LINK, each start one where every program starts in its initial state with storage of its own, and when it ends the files its programs left open are closed, as an enclave's are, and the run unit that started it has its programs as it left them. A program active in a higher run unit can be CALLed in a lower one. CANCEL and INITIAL act within the run unit. XCTL passes control to a new enclave at the same logical level (ceea200254), so a program CALLed both before and after an XCTL starts afresh after it, which the logical-level wording of the rules leaves open. The files are closed as when STOP RUN ends an enclave (ceea200118). EXTERNAL data and Language Environment heap storage belong to the enclave too (C126); the objects of object-oriented programs stay with the task",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_HANDLERS_ACROSS_XCTL,
        claim: "HANDLE CONDITION and IGNORE CONDITION apply only to the program in which they are specified and remain active while it is being executed (CICS TS 6.x, HANDLE CONDITION, dfhp4_handlecondition; IGNORE CONDITION, dfhp4_ignorecondition), and when control passes to another program by LINK or XCTL the HANDLE CONDITION commands active in the calling program are deactivated ('Using the HANDLE CONDITION command', dfhp3_exc_handlecondition). XCTL releases the program that issues it (XCTL, dfhp4_xctl), so the program it starts has none of them. The HANDLE ABEND exit belongs to the logical level and stays (C239). POP HANDLE restores the state before a PUSH HANDLE executed at the current link level, and raises INVREQ only when no such PUSH has been executed (POP HANDLE, dfhp4_pophandle); XCTL keeps the link level, and the manuals do not say it discards the stack. ironwork keeps the level's PUSH HANDLE stack across XCTL, with the HANDLE ABEND exit each entry suspended, and empties each entry's HANDLE CONDITION and IGNORE CONDITION, which belonged to the released program: a POP HANDLE in the new program undoes a PUSH the released one made and restores that exit, with no condition handling. HANDLE AID, which ironwork accepts and does not act on, has no state to carry",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_NO_OBJECT_ORIENTED_COBOL,
        claim: "COBOL programs that have object-oriented syntax for Java interoperability cannot run in CICS (Programming Guide SC27-8714-03, p. 495), and COBOL class definitions and methods cannot contain EXEC CICS statements, cannot be run in CICS and cannot be compiled with the CICS option (p. 679); the CICS reserved-word table flags INVOKE, METHOD, OBJECT and FACTORY (p. 502). An INVOKEd method therefore shares no CICS handlers with the program that invokes it, and starts with none either: it does not run. ironwork compiles such a program as before and refuses an INVOKE that a CICS task reaches, before any operand is evaluated, with an IRONWORK abend",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_TRANSFER_TO_A_RUNNING_PROGRAM,
        claim: "XCTL releases the program that issues it, and the program it transfers control to is loaded only if it is not already in main storage (CICS TS 6.x, XCTL, dfhp4_xctl); none of XCTL's conditions is for a target that is running or in the CALL chain. XCTL and LINK each pass control to a new Language Environment enclave (z/OS 3.1 Language Environment Programming Guide, 'CICS run unit', ceea200254), where the target starts afresh (C145). So XCTL to the program that issues it, to one in the CALL chain it releases, or to the task's first program, runs that program from its start with new storage, and LINK to a program running at a higher level, the task's first program included, runs a copy of its own; a HANDLE ABEND PROGRAM exit naming one is entered the same way. ironwork's run unit holds no handle for the task's first program, so each activation carries a reference to it for these transfers and for a CALL of it (C127)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ENCLAVE_EXTERNALS_AND_HEAP,
        claim: "External data persists over the lifetime of an enclave, and its scope is the enclosing enclave, COBOL EXTERNAL data items included (z/OS 3.1 Language Environment Programming Guide, 'The enclave defines the scope and visibility of the following types of data', ceea200117; Language Environment, 'Name scope of external data', cee142377: under COBOL the enclave). Each enclave has its own heap storage, and a LINK or XCTL passes control to a new enclave ('CICS run unit', ceea200254); the storage Language Environment uses for its heap at each CICS link level is freed when the program terminates (CICS TS 6.x, 'Language Environment storage', dfhp3_langenv_storage), and Language Environment generally does not allow language file sharing across enclaves (dfhp3_langenv_oview). A LINK, an XCTL and a HANDLE ABEND PROGRAM exit therefore start a run unit (C145) with no EXTERNAL record or file and an empty heap: the first program there that describes an EXTERNAL record or file gets it afresh, zeroed (C180), and CEEGTST takes storage from the new heap. When the run unit ends, the EXTERNAL files it left open are closed with its programs' files, its EXTERNAL data and heap blocks are gone, and the run unit above has its own back. The manuals do not say what CEEFRST does with a block another enclave got; ironwork gives CEE0810, the condition for an address that is not heap storage, both for a block of the run unit above and for one of a run unit that has ended. A batch run is one enclave and is unchanged",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: RECURSIVE_CALL_OF_AN_ACTIVE_PROGRAM,
        claim: "With the RECURSIVE clause a program can be reentered while a previous invocation is still active, and without it an active program cannot be (Language Reference SC27-8713-03, p. 102); only a RECURSIVE program can execute a CALL that directly or indirectly calls itself (p. 318). A recursive CALL of a program without RECURSIVE signals a condition, and if it is unhandled the run unit ends (Programming Guide SC27-8714-03, p. 557): IGZ0064S, 'A recursive call to active program program-name in compilation unit compilation-unit was attempted', and the application is terminated (z/OS 3.1 Language Environment Runtime Messages, IGZ0064S, cs00499). ironwork ends the run with U4038 and that message, as it ends one for an SSRANGE condition nothing handles (L19), naming the outermost program of the CALLed program's source as the compilation unit. The run unit's first program is no exception: a RECURSIVE main program can CALL itself, as the Programming Guide's factorial program does (p. 15), finding its WORKING-STORAGE in its last-used state, and a program stays active when a CALL of it returns while an earlier activation of it is still running. Under CICS a program is active only in its own run unit (C145): a CALL of the task's first program in a run unit a LINK or XCTL started runs a fresh copy, and one in the first program's own run unit is a recursive call. ironwork's run unit holds no handle for the first program, so each activation carries a reference to it (C148); a function or a method, whose activation does not, cannot CALL it, an ironwork refusal",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RETURN_COMMAREA_LENGTH,
        claim: "The valid range for the length of the COMMAREA RETURN passes is 0 through 32763 bytes, and outside it the LENGERR condition occurs, RESP2 11, whose default action terminates the task abnormally (CICS TS 6.x, RETURN, dfhp4_return), with AEIV. The length is LENGTH when it is given, and otherwise the COMMAREA item's, as the translator supplies it. A TRANSID specified below the highest level is cleared when there is an error on COMMAREA on the final RETURN (dfhp4_return); ironwork clears the next TRANSID on that LENGERR whichever program named it, the failing RETURN included, and a RETURN that RESP or a handler goes on from has set neither TRANSID nor COMMAREA. COMMAREA below the highest level raises INVREQ with RESP2 2 whatever its length (C143); the manual does not order the two conditions, and ironwork tests the level first, so LENGERR comes only from the RETURN to CICS. A LENGTH greater than the COMMAREA item, which the manual says gives unpredictable results and may give LENGERR, passes the item's bytes. A LINKAGE item with no address raises RESP2 26, a COMMAREA address of zero with a length that is not, as it does for LINK and XCTL (C103), and with LENGTH(0) passes a COMMAREA of no bytes",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RUN_UNIT_STORAGE_RELEASED,
        claim: "Running in CICS, a reentrant COBOL program's WORKING-STORAGE is allocated from heap storage (Programming Guide SC27-8714-03, p. 40) and persists until the end of the run unit or until the program is cancelled (p. 479); each enclave has its own heap (z/OS 3.1 Language Environment Programming Guide, 'CICS run unit', ceea200254; C126), freed when the program at the CICS link level terminates (CICS TS 6.x, 'Language Environment storage', dfhp3_langenv_storage). When the run unit a LINK, an XCTL or a HANDLE ABEND PROGRAM exit started ends (C145), ironwork therefore releases the storage of every program activated in it with the rest of the memory the run unit took. A program first loaded there stays loaded, as CICS keeps a program in main storage once loaded (XCTL, dfhp4_xctl), with no storage of its own: its next activation, in a later LINK's run unit or by a CALL at a higher level, gets new storage in its initial state. A pointer the level above kept to the released storage no longer reaches the program's, which on z/OS has been freed. The program a LINK or XCTL names is loaded before its run unit starts, in the run unit that issued the command, and keeps the storage loading gave it there",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: TRAP_OFF_LEAVES_FILES_OPEN,
        claim: "When the run unit ends normally, all open files are closed, and when it ends abnormally they are closed if TRAP(ON) is in effect (Programming Guide SC27-8714-03, p. 203), Language Environment's default (z/OS 3.1 Language Environment Programming Reference, TRAP, ceea300/cltrap). Under TRAP(OFF), Language Environment is not told of a program check or an abend and does not close the files high-level languages opened, so records might be lost (cltrap), and the VSAM CLOSE the abend invokes does not update the data set's catalog information (z/OS 3.1 DFSMS Using Data Sets, idad400/clds9), which leaves a VSAM data set opened for output marked open (C220). ironwork reads TRAP from the runtime options of a job step's PARM or run's --parm, after the last slash (C250), the last TRAP there deciding, as cltrap has the last of several STAE or SPIE decide; a CICS task takes the default. ironwork takes S0C4 to S0CF and S322 as the program checks and abends TRAP(OFF) hides; a failing I/O status or a U code ends the run through a condition Language Environment handles under either setting (CEESGL is unaffected by TRAP, cltrap), closing the files as ironwork's own stops do. A file left open is written as CLOSE writes it, every record reaching the data set, where on z/OS buffers are not flushed",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_TRANSFER_COMMAREA_LENGTH,
        claim: "LINK and XCTL raise LENGERR, whose default action terminates the task abnormally, with AEIV, with RESP2 11 for a COMMAREA length less than 0 or greater than the permitted length, and RESP2 26 for a COMMAREA address of zero with a length that is not (CICS TS 6.x, LINK, dfhp4_link; XCTL, dfhp4_xctl). XCTL's limit is 32763; LINK says only 'the permitted length', and ironwork takes 32763 for it too, the limit XCTL and RETURN state (dfhp4_return, C128) and the longest COMMAREA the EXCI LINK allows (dfhtm4w). The length is LENGTH, else the COMMAREA item's, which the translator moves to the length argument and passes with the item BY REFERENCE ('Translated code for EXEC CICS commands', dfhp4_translatedcode). A LINKAGE item with no address, such as the DFHCOMMAREA of a program given no COMMAREA, whose parameter list entry is X'00000000' (dfhp4_translatedcode), therefore reaches CICS at address zero: it raises RESP2 26, not the ASRA a reference to it gives, and with LENGTH(0) passes no COMMAREA. With no COMMAREA option, EIBCALEN is zero ('Passing data to other programs by using COMMAREA', dfhp37u) and LENGTH is not read. The manuals do not order LENGERR and PGMIDERR; ironwork tests the COMMAREA first. A program the library does not hold raises PGMIDERR with RESP2 1, a program with no installed resource definition and no autoinstall, as HANDLE ABEND PROGRAM does (dfhp4_handleabend): ironwork has no program definitions. Not raised: RESP2 12 and 13, for DATALENGTH, which CICS checks only for a remote or dynamic LINK (dfhp4_link); 27 and 28, for INPUTMSGLEN and for a destructive overlap while copying; INVREQ's RESP2 values, for INPUTMSG, SYSID, SYNCONRETURN, TRANSID, channels, Java, applications on platforms, GLUEs and TRUEs, PLT programs and DPL; NOTAUTH; and PGMIDERR 2, 3, 9 and 21 to 27, for disabled, unloadable and remote program definitions, autoinstall, dynamic routing and Liberty, none of which ironwork has",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RANDOM_PER_RUN_UNIT,
        claim: "If the first reference to FUNCTION RANDOM in the run unit does not specify argument-1, the seed value used is zero, an argument starts a new sequence, and later references without one return the next number in the current sequence (Language Reference SC27-8713-03, 'RANDOM', p. 621). Under CICS a LINK, an XCTL and a HANDLE ABEND PROGRAM exit each start a run unit, a Language Environment enclave of its own (C145), so the sequence is the run unit's: the first reference without an argument there starts from seed zero whatever the run unit that issued the command has drawn, a seed given there starts that run unit's sequence only, and when it ends the issuing run unit goes on with its own sequence where it left it. A batch run is one run unit and is unchanged",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_RETURN_CODE_PER_RUN_UNIT,
        claim: "When a COBOL program ends, its RETURN-CODE goes to the operating system, or to the calling program, whose own RETURN-CODE is set to it; RETURN-CODE is implicitly a binary halfword with VALUE ZERO (Language Reference SC27-8713-03, p. 24). A LINK is not a CALL: when the enclave a LINK or XCTL started ends without the CICS thread ending, its return code is placed in the RESP2 field of the LINK or XCTL and its reason code is discarded (z/OS 3.1 Language Environment Programming Guide, 'Finding the return and reason code from the enclave', ceea200407), the return code being the user return code, a COBOL program's RETURN-CODE, plus the reason code ('How the Language Environment enclave return code is calculated', ceea200105), which is zero for a normal end. Each run unit a LINK, an XCTL or a HANDLE ABEND PROGRAM exit starts (C145) therefore begins with RETURN-CODE zero, and a LINK that comes back sets EIBRESP2 and RESP2 to the RETURN-CODE its run unit ended with, by RETURN or by STOP RUN (C144), with EIBRESP and RESP zero again whatever its programs' commands left, and leaves the issuing program's RETURN-CODE as it was. XCTL releases the program that issues it (dfhp4_xctl), so its RESP2 is never read; ironwork gives the LINK above the level, or the end of the task, the RETURN-CODE the program XCTL started ends with. The manuals do not say what the CALL the translator makes of a command does to the issuing program's RETURN-CODE, and ironwork leaves it as it was for every command. A batch run is one run unit and is unchanged, its programs sharing one RETURN-CODE",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CICS_ENTRY_POINTERS_ACROSS_RUN_UNITS,
        claim: "A function-pointer or procedure-pointer SET TO ENTRY holds a value of ironwork's own naming the entry (C140), and the task keeps one list of those entries rather than one per CICS run unit: a pointer set in one run unit and passed to another, in a COMMAREA or by a CALL, enters the entry as a CALL of its name in the run unit that CALLs through it does, where the program starts afresh if it has not run there (C145), and so does one set in a run unit that has since ended. On z/OS the pointer holds the entry's address, and CICS keeps a program in main storage once it is loaded (XCTL, dfhp4_xctl). The manuals say only that a pointer to an entry of a program later cancelled is undefined (Programming Guide SC27-8714-03, p. 558), not what a pointer means in another enclave",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INITIALIZE_REFERENCE_MODIFIED,
        claim: "INITIALIZE of a reference-modified identifier has one receiver, the unique data item reference modification creates, which is an elementary item of category alphanumeric, national for a national item and alphabetic for an alphabetic one (Language Reference SC27-8713-03, pp. 75-76). Only its character positions change: a group is not walked for its elementary items, and a numeric DISPLAY item takes SPACE there, the implied sending item for those categories, not ZERO (pp. 351-353). The unique data item has no data description entry, so the VALUE phrase finds no VALUE clause for it (p. 352): the VALUE phrase alone leaves it unchanged, REPLACING moves to it when it names its category, and DEFAULT or no phrase gives it SPACE. The compiler's IGYPS2047-W check takes the same category",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_INVALID_DIGIT_ABENDS,
        claim: "A SORT step that edits or converts a ZD or PD field holding a digit nibble above 9 ends with a data exception, S0C7. DFSORT's manual gives two outcomes for an invalid digit, 'a data exception (0C7 ABEND) or incorrect numeric output' (z/OS 3.1 DFSORT Application Programming Guide, OUTFIL OUTREC, p,m,f,edit, Table 7 notes), without saying which happens when; ironwork takes the abend",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_IFTHEN_FIXED_LENGTH,
        claim: "Fixed-length records an INREC, OUTREC or OUTFIL IFTHEN edit makes all take one length: IFOUTLEN when it is given, else the longest of the input record and of the record each clause's BUILD, OVERLAY or PUSH items make, the shorter records padded with blanks. DFSORT says it 'sets an appropriate LRECL ... based on the build, overlay, find/replace and group operation items specified by the IFTHEN clauses' and 'does not analyze the possible results of WHEN=(logexp) conditions' (INREC control statement, IFTHEN and IFOUTLEN), without giving the rule; this one is chosen to match it",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_MASK_GROUPS_OF_THREE,
        claim: "The edit masks M0-M5 and M10-M26 are built as 31-digit patterns whose integer digits are grouped in threes from the decimal point or the right, as Table 8 of OUTFIL OUTREC shows them and as Table 11's output lengths imply. Table 8 prints M22 as SI III III III III IIII III III III IIT,TT, with one group of four; ironwork takes that group as three, the M22 length d + 1 + d/3 holding only then",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SORT_PATTERN_DECIMAL_POINT,
        claim: "In an EDIT or EDxy pattern, a period immediately followed by a digit position is the significant decimal point DFSORT names: digits before the first nonzero insignificant digit, significant digit or significant decimal point become blanks, and an insignificant digit after one is shown (OUTFIL OUTREC, edit patterns). The manual does not define which period is significant; so EDIT=(III.II) edits 5 as .05 and EDIT=(IIT.TT) as 0.05",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ALTERNATE_KEYS_FROM_THE_BASE,
        claim: "A program's ALTERNATE RECORD KEY reads and writes go to the base cluster's records as they stand, as an alternate index in the upgrade set holds them. Each alternate index path has its own DD, named from the base cluster's ddname with 1, 2 and on (Programming Guide SC27-8714-03, 'Allocating VSAM files'). ironwork does not open those DDs. An alternate index defined NOUPGRADE or never built gives the program the same records as a current one, and a key that does not match the catalog, which gives OPEN status 39 on z/OS ('Opening a file'), is not checked",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PATH_PRESENTS_THE_BASE,
        claim: "A DD that names a path gives the program the base cluster's records in the order of the path's alternate index: by alternate key, and under one key in the order its prime keys are stored. A path over the cluster itself gives them in prime key order. A prime key the base cluster no longer holds reaches no record. What the program writes through the path goes back to the base cluster by prime key when the step ends. The path's alternate index is then rebuilt, and so is the rest of the upgrade set when the path is defined with UPDATE",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: GENERATED_COMPONENT_NAMES,
        claim: "A data or index component that DEFINE does not name gets the name VSAM generates (z/OS 3.1 DFSMS Using Data Sets, 'Naming a cluster'): a last qualifier CLUSTER becomes DATA or INDEX, a name of up to 38 characters gains .DATA or .INDEX, and one of 39 to 42 gains .D or .I. A longer name keeps its first four qualifiers at most and gains qualifiers VSAM makes up. ironwork derives those from the name, so they differ from a z/OS catalog's",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BLDINDEX_NON_ENDING_ERRORS,
        claim: "BLDINDEX's non-ending errors end the command with condition code 4: a base record too short for the alternate key (IDC1644I), a repeated key in a UNIQUEKEY alternate index (IDC1645I, once for each prime key it leaves out), and prime keys past the record size (IDC1646I), each followed by IDC1653I. IBM's message pages give no code, and in the condition-code table 4 is a function that continued after a warning. An alternate index record starts with a five-byte header: X'01' for prime-key pointers, a halfword count, the pointer length and the key length. IBM gives the header's length and contents but not their order",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LISTCAT_WHAT_THE_CATALOG_KEEPS,
        claim: "LISTCAT names the catalog IRONWORK.CATALOG and lists every data set in the data set directory that is not a VSAM object or a generation data group as NONVSAM. ALL lists what ironwork's catalog keeps: associations, KEYLEN, RKP, AXRKP, AVGLRECL, MAXLRECL, the organization, key uniqueness, REC-TOTAL, UPGRADE or UPDATE, and a generation data group's LIMIT, SCRATCH and EMPTY. It leaves out HISTORY, SMSDATA, RLSDATA, PROTECTION, space, volumes and control interval sizes, which ironwork does not keep",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: UPGRADE_SET_AFTER_THE_STEP,
        claim: "VSAM brings an alternate index in the upgrade set up to date as each record is written. ironwork rebuilds it from the base cluster when a step that changed the base cluster's data set ends, abended or not, and after a REPRO into the base cluster. A program sees no difference between the two, since its own ALTERNATE RECORD KEY reads use the base cluster (C350)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BLDINDEX_REFUSALS,
        claim: "BLDINDEX needs a base cluster with at least one record, and an empty alternate index or one defined with REUSE (z/OS 3.1 DFSMS Access Method Services, BLDINDEX). Otherwise ironwork refuses it with its own message and condition code 12. IBM documents both conditions but not the message or code. A duplicate name on DEFINE is condition code 8, as the condition-code table says",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_LISTING_LAYOUT,
        claim: "IDCAMS PRINT lists as the Access Method Services samples show (z/OS 3.1, idai200/dgt3i239-dgt3i241, da6i2249): LISTING OF DATA SET -name, each record under KEY OF RECORD - (hexadecimal for DUMP and HEX, characters for CHARACTER, da6i2245), RBA OF RECORD - or RECORD SEQUENCE NUMBER -. DUMP lines are 116 columns: a four-digit offset, 32 bytes in groups of four with a wider gap after 16, characters between asterisks; HEX lines are 120 digits, CHARACTER lines 120 characters after a blank line. Characters are the PN chain's (idai200/parm), others print as periods. IDC0005I counts the records listed (ieam600/idc0005i). FROMKEY starts at its key or the next higher, TOKEY stops at its key or the next lower, a key ending X'5C' is generic; a key longer than the data set's ends with IDC3310I (m009223), a key on a data set without keys with IDC3311I (m009224); an empty cluster fails OPEN with 160 (idad500/x1cb): IDC3300I, IDC3351I, condition code 12 (idai200/ccodes)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PRINT_RANGE_ENDS,
        claim: "ironwork chooses: a blank line between records; CHARACTER's blank line and RBA's hyphen from dgt3i239 over da6i2249; RELATIVE RECORD NUMBER - n, empty slots unlisted; an RBA as the sum of earlier record lengths, without control interval boundaries; PN as PL/I's 60-character set, recalled, so lower case prints as periods; a short key generic without the asterisk too; FROMKEY with COUNT and SKIP with TOKEY allowed; SKIP and COUNT through a path in alternate key order. FROMKEY above every key or SKIP past the end ends with IDC3006I, code 12 (m009121); key errors follow IDC3302I; errors replace the listing; a listing of no records (an empty sequential data set, TOKEY below the start, COUNT(0)) ends with IDC0005I 0 and code 4",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DBCS_UNDER_SINGLE_BYTE_PAGE,
        claim: "The Programming Guide has a program with DBCS data items or DBCS literals compiled with one of the mixed CCSIDs of its Table 47 (SC27-8714-03, p. 354), and does not say what a single-byte CODEPAGE does with them. Under a single-byte page ironwork compiles DBCS items and moves and compares their bytes as under a mixed one, since those operations convert nothing; where DBCS data becomes characters, in DISPLAY, JSON GENERATE, a move to a national item, a comparison with one, and NATIONAL-OF, the DBCS space X'4040' is U+3000 and every other character U+FFFD. A DBCS literal, whose bytes only the DBCS component can give, ends the run with abend code IRONWORK where it is used, in ironwork's words, as an alphanumeric literal the page cannot encode does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DBCS_LITERAL_SOURCE,
        claim: "A DBCS literal's opening delimiter is followed by a shift-out and its closing one preceded by a shift-in (Language Reference SC27-8713-03, p. 41), which a source held in Unicode, as ironwork reads it, no longer carries: ironwork takes the characters between G' (or G\", or N' and N\" under NSYMBOL(DBCS)) and the closing delimiter as the literal, removing a shift-out (U+000E) after the opening delimiter and a shift-in (U+000F) before the closing one where they are present, and encodes each character with the DBCS component of the CODEPAGE, two code points that one DBCS code stands for taking that code. A literal of no character, or of more than 28 (p. 42), is refused when read",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MIXED_PAGE_DATA,
        claim: "Under a mixed CODEPAGE an alphanumeric literal may hold DBCS characters, delimited by shift-out and shift-in (Language Reference SC27-8713-03, pp. 39-40), and alphanumeric data converted to characters (DISPLAY, NATIONAL-OF, a move to a national item) is read the same way: a single byte is the single-byte component's character, and the bytes between X'0E' and the next X'0F' are DBCS characters, two bytes each, the shifts themselves no characters. Encoding prefers the single-byte character and opens a shift-out run only for a character the single bytes lack. A single byte the component leaves unassigned is U+001A, as IBM's conversions substitute, an unassigned DBCS code U+FFFD, and an odd byte left before a shift-in U+FFFD; Enterprise COBOL's conversion of such bytes is not documented",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DBCS_DISPLAY,
        claim: "DISPLAY transfers a DBCS item to the output device with shift-out and shift-in around it (Language Reference SC27-8713-03, p. 333); ironwork writes DISPLAY's line as Unicode text, where a DBCS item or literal is its characters through the CODEPAGE's DBCS component and the shift codes, which are not characters, are not written",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DBCS_HOST_VARIABLES,
        claim: "A DBCS item of PICTURE G or N without B is a GRAPHIC host variable of its characters, and a group of a 49-level binary halfword and a 49-level such item VARGRAPHIC, its length counting DBCS characters, as Db2 declares them; an item whose PICTURE has B has no SQL type. A GRAPHIC value reaches PostgreSQL, or a recording, as the text the CODEPAGE's DBCS component gives its characters, and text comes back as DBCS characters padded with DBCS spaces or cut at a character, SQLWARN1 and the indicator taking the length in characters; under a single-byte CODEPAGE, or for a character the DBCS component lacks, the value is SQLCODE -330, as one the code page cannot convert. Db2's own GRAPHIC conversion between CCSIDs is not modelled",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: INSPECT_SIGNED_ZONED,
        claim: "INSPECT examines a signed zoned item as if it had been moved to an unsigned zoned item of its length: an overpunched sign is read as its digit, and a separate sign is not examined and not replaced (Language Reference SC27-8713-03, p. 359, Table 40). The table says REPLACING and CONVERTING copy their result back, and not what becomes of an overpunched sign. ironwork keeps it: a byte at the sign's place that a phrase changes to a digit takes the old sign half, and one no phrase changes is left as it was. The unsigned image changes only the sign byte's zone, as an alphanumeric image of the item does",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: RELATIVE_NUMBER_BELOW_ONE,
        claim: "A relative record number below 1 names no record area, the first being number 1 (Language Reference SC27-8713-03, p. 147). A random WRITE with one reports 24, as a write beyond the file's boundaries, and a random READ, REWRITE or DELETE 23, as a record that does not exist. Table 34 (pp. 300-301) gives the meanings of 23 and 24 and does not name a record number below 1",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DISPLAY_NUMERIC_FUNCTION,
        claim: "DISPLAY of an integer or numeric intrinsic function is refused when compiled (S): such a function can be used only where an arithmetic expression can be specified (Language Reference SC27-8713-03, p. 499; Programming Guide SC27-8714-03, p. 56), and DISPLAY's operands are identifiers and literals (p. 333). The manuals give neither the message number nor its text: the message is ironwork's, and the severity is the one C190 gives INSPECT of such a function. MAX and MIN, whose type follows their arguments, are refused when the first is numeric (pp. 591, 599), and CONTENT-OF is not. Nor is a user-defined function: the Language Reference lets a numeric one be used wherever an arithmetic expression can be (p. 77) without saying only there, and DISPLAY shows its value as its RETURNING item",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: BY_VALUE_TO_REFERENCE,
        claim: "A COBOL program CALLed with an argument BY VALUE whose formal parameter is received BY REFERENCE: the Language Reference requires BY VALUE for both the argument and the parameter (SC27-8713-03, p. 322) and does not say what happens when they differ. On z/OS the parameter list then holds the value where the called program reads an address, and the result turns on what that value addresses. ironwork binds the parameter to storage of its own holding the value, as it does for a parameter received BY VALUE, and the called program runs",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: FLOAT_EXPONENTIATION,
        claim: "An exponent with decimal places, or one holding a division or an exponentiation when dmax is above zero, makes its expression floating point, as a floating-point operand or function does (Programming Guide SC27-8714-03, pp. 796, 800). ironwork takes an operand's decimal places from its description, a function's only from a user-defined function's RETURNING item, and dmax as the statement's or the evaluated expression's. A floating-point exponentiation is evaluated in long precision, extended under ARITH(EXTEND) (p. 800); the manuals do not give Language Environment's algorithm, and ironwork gives the value nearest the exact power, computed as the floating-point functions are (C110): an integer exponent by repeated squaring, any other as e^(y ln |x|). Zero to a positive power is zero. Table 32 of the Language Reference (SC27-8713-03, pp. 296-297) gives the rest: zero to a negative power is a size error, and without ON SIZE ERROR the program ends abnormally, which ironwork does with the HFP divide exception, S0CF, a division by zero raises; zero to the power zero is 1, and a negative number to a fractional power is computed with the base's absolute value, each with a message, when no SIZE ERROR phrase is written. ironwork gives those two values whether or not ON SIZE ERROR is written, issues no message, and does not run the phrase for them",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MIXED_FUNCTION_PLACES,
        claim: "MAX, MIN, RANGE, REM and SUM of fixed-point arguments carry as many decimal places as their arguments have at most, their inner-dmax, whichever argument MAX or MIN returns, and contribute that many to the dmax of an expression holding them, their outer-dmax (Programming Guide SC27-8714-03, pp. 794, 799): COMPUTE R = FUNCTION MAX(A B) / 3 * 3 with A PIC 9V99 VALUE 1, B PIC 9 VALUE 5 and R PIC 99V9 gives 4.9. Each step of their algorithm takes the fixed-point table's places (p. 795): RANGE has one integer place more than MAX's value, REM the integer places of argument-1 and argument-2 and argument-2's decimal places plus one, SUM one more for each argument, each cut to 30 digits, 31 under ARITH(EXTEND) (p. 797). An argument's dmax is read from its description: an item's or literal's decimal places, an expression's dmax, an embedded function's outer-dmax (p. 794)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: MAX_MIN_INTEGER_PLACES,
        claim: "MAX and MIN of fixed-point arguments carry as many integer places as the argument with the most. The Language Reference says the value is the content of the winning argument (SC27-8713-03, pp. 591, 599), and the Programming Guide gives its decimal places (C390) and has every argument assigned to one function result (SC27-8714-03, p. 799), which holds them all only so. The integer places show only where the value's digits do, as in a MOVE to an alphanumeric item: MAX(N M) with N PIC 999 VALUE 5 and M PIC 9(5) gives 00005. cobc gives the winning argument's own field, 005 (libcob/intrinsic.c, cob_intr_max); --dialect gnucobol does not switch this, since that field also keeps the winning argument's decimal places, which IBM documents (docs/dialect.md 5.5)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: INTEGER_FUNCTION_DIGITS,
        claim: "INTEGER of a fixed-point argument has one digit more than the argument, INTEGER-PART as many, and MOD as many as the shorter of its arguments, high-order digits beyond them dropped, so MOD(N H) with N PIC S9 VALUE -3 and H PIC 999 VALUE 100 is 7 (Language Reference SC27-8713-03, p. 601: 'The function result is an integer with as many digits as the shorter of argument-1 and argument-2'; Programming Guide SC27-8714-03, pp. 798-799). Each is an integer function, with no decimal places and an outer-dmax of zero (p. 798). cobc keeps every digit, 97",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ABS_PLACES,
        claim: "ABS of a fixed-point argument has the argument's places, and its decimal places count in the dmax of an expression holding it as a mixed function's do (C390). IBM types ABS integer or numeric as its argument is (Language Reference SC27-8713-03, p. 517) and gives it no precision, and says a numeric function's result has decimal places when an argument has (Programming Guide SC27-8714-03, p. 62). cobc's ABS has its argument's field (cob_intr_abs)",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: NUMERIC_FUNCTION_MOVED,
        claim: "MOVE of an integer or numeric intrinsic function is refused when compiled (S) under --compliance strict, under either dialect: 'numeric functions are not valid as senders in MOVE statements' (Programming Guide SC27-8714-03, p. 119), such a function can be used only where an arithmetic expression can (Language Reference SC27-8713-03, p. 499), MOVE's sender is an identifier or a literal (p. 400), and no numeric function is among the valid operands of an elementary move (p. 402), whatever the receiver. MAX and MIN are refused when the first argument is numeric (pp. 591, 599), as C332 decides for DISPLAY; CONTENT-OF and a user-defined function are not. The manuals give neither the message number nor its text: the message is ironwork's, and the severity C332's. Under --compliance extended the MOVE is accepted with IWX0008-W, as cobc accepts it, and moves the function's value at its precision (C390 to C392) by IBM's rules for a numeric sender: an integer's digits to an alphanumeric item, and a value with decimal places refused there at run time (p. 404)",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ASSIGN_ITEM_NAMES_A_DD,
        claim: "A file whose ASSIGN names a data item (--compliance extended) takes, at each OPEN, the item's value without its blanks as a DD name, folded to upper case, as GnuCOBOL maps a name with no directory to a file through DD_name and Micro Focus through dd_name. A value that cannot be a DD name (a path, a name with a period, more than eight characters) names no DD, nor does a name the run was not given, and OPEN fails as it does for a missing DD: status 35 for a file that must exist. ironwork never opens a host file a program names, where GnuCOBOL and Micro Focus would open the path",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ASSIGN_ITEM_FORMS,
        claim: "Under --compliance extended, ASSIGN TO a name that is an alphanumeric or group item's names that item, as GnuCOBOL's default assign clause and Micro Focus's ASSIGN(DYNAMIC) take it, with IWX0007-W; DYNAMIC and USING always name an item and EXTERNAL never does, and a name no item has stays a DD name. Under strict the name is a DD name, Enterprise COBOL's assignment-name never being a data item (Language Reference SC27-8713-03, ASSIGN clause), and DYNAMIC and USING are refused. The input trace records the item's value as a dynamic-file-path sink at the SELECT, where cobolwork places the finding, once for each file an OPEN names, with that file's input alone",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: PREPARED_STATEMENT_LIFETIME,
        claim: "A prepared statement lasts until its name is prepared again or its unit of work ends: COMMIT, ROLLBACK, SYNCPOINT, a -911, or the end of the run unit or CICS task destroys every statement the program prepared, but COMMIT keeps the SELECT of a cursor declared WITH HOLD that is open when it runs, as Db2 13 for z/OS describes prepared-statement persistence (SQL Reference, PREPARE). That is a plan bound with KEEPDYNAMIC(NO), BIND's default: ironwork binds no plan, so no statement is kept further. An EXECUTE of a destroyed statement is -518 and an OPEN of a cursor for one -514",
        basis: Basis::Documented,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: EXECUTE_IMMEDIATE_OF_A_QUERY,
        claim: "EXECUTE IMMEDIATE of a select-statement is SQLCODE -518 (SQLSTATE 07003), as Db2 13 for z/OS's -518 explanation lists it, and does not reach the database. Db2 for Linux, UNIX and Windows documents SQL0084N for the same statement",
        basis: Basis::Documented,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: STATEMENT_STRING_KINDS,
        claim: "The runtime drops a dynamic statement string's SQL comments, a simple comment ending at the end of its line and a bracketed one nesting (SQL Reference, SQL comments), and reads what the statement is from its first words, outside quoted strings: a select-statement starts with SELECT, WITH, VALUES or a parenthesis; COMMIT and ROLLBACK, alone or with WORK, end the unit of work as the static statements do; SAVEPOINT, RELEASE SAVEPOINT and ROLLBACK TO SAVEPOINT are refused by name, as the static ones are; an SQL statement Db2 13 for z/OS does not prepare (SQL Reference, PREPARE: CALL, CONNECT, DECLARE CURSOR, DESCRIBE, EXECUTE, FETCH, OPEN and the like), or an empty string, is SQLCODE -084 (SQLSTATE 42612) without reaching the database; the other statements PREPARE lists (ALTER, CREATE, DROP, GRANT, INSERT, LOCK TABLE, MERGE, SET, TRUNCATE, UPDATE and the like) go to the database, which answers them; and words that begin none of these are SQLCODE -104 (SQLSTATE 42601), Db2's answer to a symbol it cannot read, without reaching the database, so a statement only the backend has never runs. A parameter marker is a question mark outside a quoted string. Db2 parses the whole statement; ironwork leaves everything past the first words to the database",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: UPSI_SWITCHES,
        claim: "The UPSI switches UPSI-0 to UPSI-7 are one copy that every program of the run unit shares (Programming Guide SC27-8714-03, p. 595). The runtime option UPSI(nnnnnnnn) sets them, its leftmost digit UPSI-0's, 1 on and 0 off, and they are all off without it (Language Environment Programming Reference, UPSI; Programming Guide, p. 431). A SPECIAL-NAMES entry UPSI-n [IS mnemonic-name] with ON STATUS and OFF STATUS condition-names gives the conditions that test the switch, the mnemonic-name being their conditional variable, which can qualify them and which SET ... TO ON or OFF names to set the switch, and nothing else names (Language Reference SC27-8713-03, pp. 125-127, 283, 442-443); a contained program has its container's entries (p. 124). ironwork keeps each switch as a one-byte EXTERNAL record of the run unit that only these names reach",
        basis: Basis::Documented,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: UPSI_FROM_THE_PARM,
        claim: "ironwork reads the UPSI runtime option from a job step's PARM, after its last slash as CBLOPTS(ON) has it (C250), the last UPSI there deciding; it reads no CEEOPTS DD or _CEE_RUNOPTS, and a CICS task or a run with no PARM has every switch off. An UPSI that is not eight digits, each 0 or 1, is named on standard error and leaves the switches off, as Language Environment ignores a runtime option it cannot read. The switches a PARM sets are marked as input, as the PARM is",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: SET_SWITCH_CONDITION_TRUE,
        claim: "SET condition-name TO TRUE of an UPSI switch's condition-name sets the switch to that status when the switch's entry has a mnemonic-name, which the Language Reference makes the condition-names' conditional variable (SC27-8713-03, p. 127), and is refused when it has none, SET TO TRUE needing a conditional variable (p. 443); SET TO FALSE is refused, a switch-status condition having no WHEN SET TO FALSE value. cobc 3.2 refuses SET TO TRUE of a switch-status condition, and a condition-name qualified by a mnemonic-name, in both cases",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ACCEPT_FROM_CONSOLE,
        claim: "ACCEPT ... FROM CONSOLE, or from a mnemonic-name SPECIAL-NAMES gives CONSOLE, reads standard input as ACCEPT from the system input device does, record after record until the receiver is full. On z/OS the operator replies at the console, a system message code and AWAITING REPLY shown first, each reply at most 114 characters, left-justified and padded with spaces, and an empty reply leaving the receiver unchanged (Language Reference SC27-8713-03, pp. 307-308); a run on ironwork has no operator, and a test or job gives the replies as standard input's lines",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: CALL_BY_PROGRAM_ID,
        claim: "A CALL of a name that no member of the program libraries has, as a file named for it, finds the .cbl or .cob file there whose PROGRAM-ID is the name: the directories in order, each one's files in name order, the first that holds the program. z/OS finds a called program as a member of STEPLIB, JOBLIB or the link list by its member name, and ends the run S806 when none has it; a build that link-edits each program under its PROGRAM-ID gives every program a member of that name, and ironwork's libraries are source directories, whose file names need not be. A member of the name comes first, as before",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: COMMAND_LINE_FROM_PARM,
        claim: "Under --compliance extended, ACCEPT ... FROM COMMAND-LINE gives the job step's PARM program arguments as written, what precedes the last slash when runtime options follow it (C250); ARGUMENT-NUMBER how many words they hold, split at blanks; and ARGUMENT-VALUE the next word, the exception taken and the receiver left unchanged once none is left. DISPLAY n UPON ARGUMENT-NUMBER makes word n the next, the last word when n is past them, as cobc 3.2 gives it, and none when n is below 1. Micro Focus and GnuCOBOL read the operating system's command line, whose words the shell splits, quotes kept together, and whose word 0 is the command; a z/OS PARM has neither. A run with no PARM has an empty command line. The text moves as an alphanumeric sender and the count as a numeric one, each marked as input when a PARM gave it, as the PARM is. cobc 3.2 also runs NOT ON EXCEPTION when the exception is taken; ironwork runs the one phrase",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: DESCRIBED_COLUMNS,
        claim: "DESCRIBE describes a result column from what the database gave PREPARE. Against PostgreSQL a column's name is upper-cased, as Db2 folds an undelimited name; it allows NULL unless it is a table's column declared NOT NULL; char(n), varchar(n), smallint, integer, bigint, numeric(p,s), real, double precision, date, time, timestamp(p) and bytea are Db2's CHAR, VARCHAR, SMALLINT, INTEGER, BIGINT, DECIMAL, REAL, DOUBLE, DATE, TIME, TIMESTAMP and VARBINARY, and a string declared without a length is VARCHAR(32704), Db2's longest. A NUMERIC with no precision and any other type have no Db2 type, and DESCRIBE abends SQL naming it. No backend keeps column labels: USING LABELS gives each SQLNAME length 0 and USING ANY the name. A string column's SQLDATA holds the CODEPAGE's CCSID, its DBCS component's for GRAPHIC, and SQLIND is zero. With too few SQLVARs only SQLDAID, SQLDABC and SQLD are set and SQLCODE stays 0, as Db2 13 for z/OS does without the SQL standard option (SQL Reference, DESCRIBE OUTPUT)",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: SQLDA_CHECKS,
        claim: "An SQLDA that USING DESCRIPTOR names is checked before the statement runs, and one that cannot be used is SQLCODE -804 (SQLSTATE 07002) with Db2's reason code as SQLERRMC: 07 for a negative SQLN or SQLD or an SQLDA past the run unit's storage, 14 for an SQLDABC below SQLN x 44 + 16, 11 for an SQLD above SQLN, 08 (input) or 16 (output) for an SQLTYPE ironwork does not read (LOBs, binary strings, NUL-terminated strings, or a length that does not fit the type), and 12 (input) or 13 (output) for an SQLDATA, or the SQLIND of an odd SQLTYPE, that is zero or does not address storage the variable fits in. Db2 13 for z/OS lists the reasons (SQLCODE -804) without saying which it checks first",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: CLASS_ORDINALS,
        claim: "A numeric literal of a CLASS clause is an ordinal number from 1 to the number of characters in the alphabet, each corresponding to the ordinal position of a character in the single-byte EBCDIC or ASCII collating sequence, and an alphanumeric literal is an actual single-byte EBCDIC character (Language Reference SC27-8713-03, CLASS clause, p. 129). ironwork takes ordinal n as the character of code point n - 1 in the program's code page, X'C1' for 194 under an EBCDIC page, whatever PROGRAM COLLATING SEQUENCE says, and encodes an alphanumeric literal in that code page, a hexadecimal literal's bytes being taken as written. A THROUGH range holds the code points between its ends, in either order. NIST CCVS85's NC174A bounds its class ORDINAL-A-THROUGH-D by the ordinal numbers of A and D in the native character set, which its User Guide's X-cards 90 and 91 hold",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: ROWSET_ENDS_SHORT,
        claim: "A rowset FETCH that gets every row it asks for is SQLCODE 0, even when the last of them is the result table's last row, and the next FETCH is +100; one that gets fewer is +100 with the rows it got, SQLERRD(3) their count and the arrays' later elements left as they were (Db2 13 SQL, FETCH: 'SQLERRD3 is set to 5 for the 5 returned rows, SQLSTATE is set to 02000, and SQLCODE is set to +100'). IBM's pages differ on the full rowset: GET DIAGNOSTICS says 'An end of data warning might not occur' when the rows returned equal the rows requested, the Application Programming guide that +100 is set 'if the last row in the table has been returned with the set of rows'; PostgreSQL's FETCH FORWARD n does not say whether rows follow. An SQLCODE a recording gives stands",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: ROWSET_ROW_POSITION,
        claim: "After a rowset FETCH, a row FETCH moves from the rowset's first row, so after rows 1 to 5 it gives row 2, and a NEXT ROWSET then starts at the row after it; a NEXT ROWSET without FOR n ROWS asks for the rows the last rowset FETCH asked for, or one after a row FETCH (Db2 13 SQL, FETCH, Table 6: 'Cursor is positioned on row 2'). The runtime keeps the rows a rowset read and answers from them, so the database is asked only for rows past them. COMMIT leaves a held cursor before the row after its current position",
        basis: Basis::Documented,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: POSITIONED_ON_A_ROWSET,
        claim: "A positioned UPDATE or DELETE of a cursor on a rowset of more than one row, or on a row a row FETCH took from rows a rowset read, abends EXEC by name: Db2 changes every row of the rowset ('If the cursor is positioned on a rowset, all of the rows in the rowset are updated'), and the backend's own cursor stands on the last row it gave. FOR ROW n OF ROWSET is refused by name",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: CALL_FROM_A_RECORDING,
        claim: "CALL of a stored procedure runs from a recording, whose = line gives each host-variable argument as the procedure returns it, or - for one it does not return: Db2 takes each parameter's mode from its catalogue ('The attributes of the parameters are determined by the current server'), which no backend ironwork has holds, and ironwork runs no procedure. The PostgreSQL backend refuses CALL by name. An answer below zero assigns nothing, where Db2 leaves INOUT arguments unchanged and OUT ones undefined; +466 sets SQLWARN9 to Z, and its result sets wait on ASSOCIATE LOCATORS and ALLOCATE CURSOR, which are refused by name. CALL naming its procedure in a host variable, and CALL ... USING DESCRIPTOR, are refused by name",
        basis: Basis::Chosen,
        oracle: Oracle::Db2,
    },
    Assumption {
        id: NOT_ATOMIC_SUMMARY,
        claim: "A NOT ATOMIC CONTINUE ON SQLEXCEPTION INSERT with a row that fails is -253 (22529) when another row went in and -254 (22530) when none did, SQLERRD(3) the rows inserted, as the INSERT statement and SQLCODE -253 pages give it ('SQLSTATE 22529, SQLCODE -253. At least one row was successfully inserted, but one or more errors occurred'); the Application Programming guide's example of the same case gives SQLCODE 0. Each row's own condition is for GET DIAGNOSTICS, which ironwork does not run. The PostgreSQL backend gives each row its own savepoint, and an ATOMIC insert one savepoint for all, so a failure undoes every row and SQLERRD(3) is 0",
        basis: Basis::Documented,
        oracle: Oracle::Db2,
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
