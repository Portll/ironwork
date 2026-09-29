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
pub const BMS_RECEIVE_NULLS: &str = "C28";
pub const BMS_INPUT_JUSTIFY: &str = "C29";
pub const BMS_EXTENDED_ORDER: &str = "C30";
pub const BMS_CONSTANTS_UNVERIFIED: &str = "C31";
pub const BMS_SEND_DATA_CHOICE: &str = "C32";
pub const CICS_INITIAL_AID: &str = "C33";
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
pub const REPORT_WRITER_PRECOMPILER: &str = "RW1";
pub const REPORT_TOTALS_BEFORE_PAGE_FIT: &str = "RW2";
pub const REPORT_SOURCE_SUM_CORRELATION: &str = "RW3";
pub const REPORT_PAGE_REGION_DEFAULTS: &str = "RW4";
pub const REPORT_LINE_WRITES: &str = "RW5";
pub const REPORT_NO_CARRIAGE_CONTROL: &str = "RW6";
pub const REPORT_RECORD_LENGTH: &str = "RW7";
pub const REPORT_SUM_OVERFLOW: &str = "RW8";
pub const REPORT_SOURCE_OVERFLOW: &str = "RW9";
pub const REPORT_SUPPRESS_PRINTING: &str = "RW10";
pub const REPORT_NEW_PAGES: &str = "RW11";
pub const REPORT_OUT_OF_ORDER: &str = "RW12";
pub const REPORT_CONTROL_AREA: &str = "RW13";
pub const OBJECT_REFERENCE_VALUE: &str = "J1";
pub const LOCAL_REFERENCES_KEPT: &str = "J2";
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
pub const OO_OPTIONS_NOT_REQUIRED: &str = "J13";
pub const CHAR_FROM_DISPLAY: &str = "J14";
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
        id: REPORT_NO_CARRIAGE_CONTROL,
        claim: "Report lines go through ironwork's WRITE ... ADVANCING, which puts no printer control character in the record: a fixed or variable record holds the CODE and the line, and a text DD takes line feeds and form feeds. Under Enterprise COBOL's default ADV each record would carry one byte more, the control character, first",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: REPORT_RECORD_LENGTH,
        claim: "A report file whose FD has no RECORD CONTAINS has records as long as the longest line of its reports, rounded up to a multiple of 4, plus the CODE (SC26-4301-04, 2.2.3 rule 7); under RECORDING MODE V each record ends after its last printed field (rule 9)",
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
        claim: "An object reference is four bytes, as under LP(32): zero for NULL, otherwise a number that names one object for the rest of the run unit, so two references to one object are equal byte for byte",
        basis: Basis::Chosen,
        oracle: Oracle::EnterpriseCobol,
    },
    Assumption {
        id: LOCAL_REFERENCES_KEPT,
        claim: "A local object reference stays valid after the method that obtained it returns, and the JNI services NewGlobalRef, NewLocalRef, DeleteGlobalRef and DeleteLocalRef change nothing; IBM frees a method's local references when it returns (Programming Guide, Managing local and global references), so a method that keeps one in OBJECT, FACTORY or method WORKING-STORAGE without NewGlobalRef fails on z/OS and runs here",
        basis: Basis::Chosen,
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
        id: OO_OPTIONS_NOT_REQUIRED,
        claim: "A program or class with object-oriented syntax is checked whatever THREAD, DLL and RECURSIVE say: IBM requires THREAD, DLL and RENT for it, and under THREAD a RECURSIVE program, which an installation's defaults may supply",
        basis: Basis::Chosen,
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
        claim: "Under FASTSRT DFSORT does the I/O of a SORT's only USING file and its only GIVING file, except a MERGE's, a line-sequential or variable-length relative file, one whose records differ from the SD's in format (fixed or variable) or largest length, and a GIVING file that is also the USING file; COBOL does the rest as under NOFASTSRT (Programming Guide SC27-8714-03, pp. 232-233, 369)",
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
