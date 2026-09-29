//! The copy members IBM's translators and products supply, as ironwork's own declarations of the
//! layouts and names IBM documents: Db2 13 for z/OS SQL Reference ("The included SQLCA", "The
//! included SQLDA"), CICS TS Data Areas ("EIB - EXEC interface block") and CICS TS Application
//! Programming Reference, "BMS-related constants" (DFHAID, DFHBMSCA); and CEEIGZCT, Language
//! Environment's symbolic feedback codes (`crate::feedback`). A library member of the same name is
//! found first, as it would be ahead of the system library on z/OS.

/// The EXEC interface block the CICS translator adds to a program's LINKAGE SECTION, in IBM's
/// order: EIBCALEN at X'18', EIBFN at X'1B', EIBRCODE at X'1D', EIBRESP at X'4C'. IBM names neither
/// the halfword at X'14' nor the byte at X'3E'.
pub const EIB: &[(&str, &str)] = &[
    ("EIBTIME", "S9(7) COMP-3"),
    ("EIBDATE", "S9(7) COMP-3"),
    ("EIBTRNID", "X(4)"),
    ("EIBTASKN", "S9(7) COMP-3"),
    ("EIBTRMID", "X(4)"),
    ("FILLER", "S9(4) COMP"),
    ("EIBCPOSN", "S9(4) COMP"),
    ("EIBCALEN", "S9(4) COMP"),
    ("EIBAID", "X(1)"),
    ("EIBFN", "X(2)"),
    ("EIBRCODE", "X(6)"),
    ("EIBDS", "X(8)"),
    ("EIBREQID", "X(8)"),
    ("EIBRSRCE", "X(8)"),
    ("EIBSYNC", "X(1)"),
    ("EIBFREE", "X(1)"),
    ("EIBRECV", "X(1)"),
    ("FILLER", "X(1)"),
    ("EIBATT", "X(1)"),
    ("EIBEOC", "X(1)"),
    ("EIBFMH", "X(1)"),
    ("EIBCOMPL", "X(1)"),
    ("EIBSIG", "X(1)"),
    ("EIBCONF", "X(1)"),
    ("EIBERR", "X(1)"),
    ("EIBERRCD", "X(4)"),
    ("EIBSYNRB", "X(1)"),
    ("EIBNODAT", "X(1)"),
    ("EIBRESP", "S9(8) COMP"),
    ("EIBRESP2", "S9(8) COMP"),
    ("EIBRLDBK", "X(1)"),
];

const DFHAID: &[(&str, Option<&str>)] = &[
    ("DFHNULL", Some("00")), ("DFHENTER", Some("7D")), ("DFHCLEAR", Some("6D")), ("DFHCLRP", Some("6A")), ("DFHPEN", Some("7E")),
    ("DFHOPID", Some("E6")), ("DFHMSRE", Some("E7")), ("DFHSTRF", Some("88")), ("DFHTRIG", Some("7F")), ("DFHPA1", Some("6C")),
    ("DFHPA2", Some("6E")), ("DFHPA3", Some("6B")), ("DFHPF1", Some("F1")), ("DFHPF2", Some("F2")), ("DFHPF3", Some("F3")),
    ("DFHPF4", Some("F4")), ("DFHPF5", Some("F5")), ("DFHPF6", Some("F6")), ("DFHPF7", Some("F7")), ("DFHPF8", Some("F8")),
    ("DFHPF9", Some("F9")), ("DFHPF10", Some("7A")), ("DFHPF11", Some("7B")), ("DFHPF12", Some("7C")), ("DFHPF13", Some("C1")),
    ("DFHPF14", Some("C2")), ("DFHPF15", Some("C3")), ("DFHPF16", Some("C4")), ("DFHPF17", Some("C5")), ("DFHPF18", Some("C6")),
    ("DFHPF19", Some("C7")), ("DFHPF20", Some("C8")), ("DFHPF21", Some("C9")), ("DFHPF22", Some("4A")), ("DFHPF23", Some("4B")),
    ("DFHPF24", Some("4C")),
];

/// IBM's "BMS-related constants", each with its byte where one is known here. DFHBMFLG carries the
/// conditions DFHERASE and DFHCURSR instead of a value.
const DFHBMSCA: &[(&str, Option<&str>)] = &[
    ("DFHBMPEM", Some("19")), ("DFHBMPNL", Some("15")), ("DFHBMPFF", Some("0C")), ("DFHBMPCR", Some("0D")), ("DFHBMASK", Some("F0")),
    ("DFHBMUNP", Some("40")), ("DFHBMUNN", Some("50")), ("DFHBMPRO", Some("60")), ("DFHBMBRY", Some("C8")), ("DFHBMDAR", Some("4C")),
    ("DFHBMFSE", Some("C1")), ("DFHBMPRF", Some("61")), ("DFHBMASF", Some("F1")), ("DFHBMASB", Some("F8")), ("DFHBMPSO", Some("0E")),
    ("DFHBMPSI", Some("0F")), ("DFHBMEOF", Some("80")), ("DFHBMCUR", Some("02")), ("DFHBMEC", Some("82")), ("DFHBMFLG", None),
    ("DFHBMDET", None), ("DFHSA", Some("28")), ("DFHERROR", None), ("DFHCOLOR", Some("42")), ("DFHPS", Some("43")), ("DFHHLT", Some("41")),
    ("DFH3270", Some("C0")), ("DFHVAL", Some("C1")), ("DFHOUTLN", Some("C2")), ("DFHBKTRN", Some("46")), ("DFHALL", None),
    ("DFHDFT", Some("FF")), ("DFHDFCOL", None), ("DFHBLUE", Some("F1")), ("DFHRED", Some("F2")), ("DFHPINK", Some("F3")),
    ("DFHGREEN", Some("F4")), ("DFHTURQ", Some("F5")), ("DFHYELLO", Some("F6")), ("DFHNEUTR", Some("F7")), ("DFHBASE", None),
    ("DFHDFHI", None), ("DFHBLINK", Some("F1")), ("DFHREVRS", Some("F2")), ("DFHUNDLN", Some("F4")), ("DFHMFIL", None),
    ("DFHMENT", None), ("DFHMFE", None), ("DFHMT", None), ("DFHMFT", None), ("DFHMET", None), ("DFHMFET", None), ("DFHUNNOD", None),
    ("DFHUNIMD", None), ("DFHUNNUM", None), ("DFHUNNUB", None), ("DFHUNINT", None), ("DFHUNNON", None), ("DFHPROTI", None),
    ("DFHPROTN", None), ("DFHDFFR", None), ("DFHUNDER", None), ("DFHRIGHT", None), ("DFHOVER", None), ("DFHLEFT", None), ("DFHBOX", None),
    ("DFHSOSI", None), ("DFHTRANS", None), ("DFHOPAQ", None),
];

fn constants(group: &str, names: &[(&str, Option<&str>)]) -> String {
    let mut out = format!("       01  {group}.\n");
    for (n, value) in names {
        match value {
            Some(v) => out.push_str(&format!("           02 {n} PIC X VALUE X'{v}'.\n")),
            None => out.push_str(&format!("           02 {n} PIC X.\n")),
        }
        if *n == "DFHBMFLG" {
            out.push_str("              88 DFHERASE VALUES X'80' X'82'.\n              88 DFHCURSR VALUES X'02' X'82'.\n");
        }
    }
    out
}

/// The text of a system member, for a COPY or EXEC SQL INCLUDE no library answers.
pub fn member(name: &str) -> Option<String> {
    Some(match name.to_ascii_uppercase().as_str() {
        "SQLCA" => [
            "       01  SQLCA.",
            "           05 SQLCAID PIC X(8).",
            "           05 SQLCABC PIC S9(9) COMP-5.",
            "           05 SQLCODE PIC S9(9) COMP-5.",
            "           05 SQLCADE REDEFINES SQLCODE PIC S9(9) COMP-5.",
            "           05 SQLERRM.",
            "              49 SQLERRML PIC S9(4) COMP-5.",
            "              49 SQLERRMC PIC X(70).",
            "           05 SQLERRP PIC X(8).",
            "           05 SQLERRD PIC S9(9) COMP-5 OCCURS 6.",
            "           05 SQLWARN.",
            "              10 SQLWARN0 PIC X.",
            "              10 SQLWARN1 PIC X.",
            "              10 SQLWARN2 PIC X.",
            "              10 SQLWARN3 PIC X.",
            "              10 SQLWARN4 PIC X.",
            "              10 SQLWARN5 PIC X.",
            "              10 SQLWARN6 PIC X.",
            "              10 SQLWARN7 PIC X.",
            "           05 SQLEXT.",
            "              10 SQLWARN8 PIC X.",
            "              10 SQLWARN9 PIC X.",
            "              10 SQLWARNA PIC X.",
            "              10 SQLSTATE PIC X(5).",
            "              10 SQLSTAT REDEFINES SQLSTATE PIC X(5).",
            "",
        ]
        .join("\n"),
        "SQLDA" => [
            "       01  SQLDA.",
            "           05 SQLDAID PIC X(8).",
            "           05 SQLDABC PIC S9(9) BINARY.",
            "           05 SQLN PIC S9(4) BINARY.",
            "           05 SQLD PIC S9(4) BINARY.",
            "           05 SQLVAR OCCURS 0 TO 750 TIMES DEPENDING ON SQLN.",
            "              10 SQLVAR1.",
            "                 15 SQLTYPE PIC S9(4) BINARY.",
            "                 15 SQLLEN PIC S9(4) BINARY.",
            "                 15 FILLER REDEFINES SQLLEN.",
            "                    20 SQLPRECISION PIC X.",
            "                    20 SQLSCALE PIC X.",
            "                 15 SQLDATA POINTER.",
            "                 15 SQLIND POINTER.",
            "                 15 SQLNAME.",
            "                    49 SQLNAMEL PIC S9(4) BINARY.",
            "                    49 SQLNAMEC PIC X(30).",
            "",
        ]
        .join("\n"),
        "DFHEIBLK" => {
            let mut out = "       01  DFHEIBLK.\n".to_owned();
            for (n, p) in EIB {
                out.push_str(&format!("           02 {n} PIC {p}.\n"));
            }
            out
        }
        "DFHAID" => constants("DFHAID", DFHAID),
        "DFHBMSCA" => constants("DFHBMSCA", DFHBMSCA),
        "JNI" => crate::jni::member(),
        "CEEIGZCT" => crate::feedback::member(),
        _ => return None,
    })
}

/// EIBRESP values by condition name, as the CICS Application Programming Reference lists them, for
/// DFHRESP(condition), which the translator replaces with the number.
const RESP: &[(&str, i32)] = &[
    ("NORMAL", 0), ("ERROR", 1), ("RDATT", 2), ("WRBRK", 3), ("EOF", 4), ("EODS", 5), ("EOC", 6), ("INBFMH", 7), ("ENDINPT", 8),
    ("NONVAL", 9), ("NOSTART", 10), ("TERMIDERR", 11), ("FILENOTFOUND", 12), ("DSIDERR", 12), ("NOTFND", 13), ("DUPREC", 14),
    ("DUPKEY", 15), ("INVREQ", 16), ("IOERR", 17), ("NOSPACE", 18), ("NOTOPEN", 19), ("ENDFILE", 20), ("ILLOGIC", 21), ("LENGERR", 22),
    ("QZERO", 23), ("SIGNAL", 24), ("QBUSY", 25), ("ITEMERR", 26), ("PGMIDERR", 27), ("TRANSIDERR", 28), ("ENDDATA", 29),
    ("INVTSREQ", 30), ("EXPIRED", 31), ("RETPAGE", 32), ("RTEFAIL", 33), ("RTESOME", 34), ("TSIOERR", 35), ("MAPFAIL", 36),
    ("INVERRTERM", 37), ("INVMPSZ", 38), ("IGREQID", 39), ("OVERFLOW", 40), ("INVLDC", 41), ("NOSTG", 42), ("JIDERR", 43),
    ("QIDERR", 44), ("NOJBUFSP", 45), ("DSSTAT", 46), ("SELNERR", 47), ("FUNCERR", 48), ("UNEXPIN", 49), ("NOPASSBKRD", 50),
    ("NOPASSBKWR", 51), ("SEGIDERR", 52), ("SYSIDERR", 53), ("ISCINVREQ", 54), ("ENQBUSY", 55), ("ENVDEFERR", 56), ("IGREQCD", 57),
    ("SESSIONERR", 58), ("SYSBUSY", 59), ("SESSBUSY", 60), ("NOTALLOC", 61), ("CBIDERR", 62), ("INVEXITREQ", 63), ("INVPARTNSET", 64),
    ("INVPARTN", 65), ("PARTNFAIL", 66), ("USERIDERR", 69), ("NOTAUTH", 70), ("VOLIDERR", 71), ("SUPPRESSED", 72), ("END", 83),
    ("DISABLED", 84), ("TASKIDERR", 91), ("TCIDERR", 92), ("DSNNOTFOUND", 93), ("LOADING", 94), ("MODELIDERR", 95), ("PARTNERIDERR", 97),
    ("PROFILEIDERR", 98), ("LOCKED", 100), ("RECORDBUSY", 101), ("UOWNOTFOUND", 102), ("CONTAINERERR", 110), ("TOKENERR", 112),
    ("CSDERR", 119), ("DUPRES", 120), ("CHANNELERR", 122), ("CCSIDERR", 123), ("TIMEDOUT", 124), ("CODEPAGEERR", 125), ("INCOMPLETE", 126),
    ("BUSY", 128),
];

pub fn resp_code(condition: &str) -> Option<i32> {
    RESP.iter().find(|(n, _)| n.eq_ignore_ascii_case(condition)).map(|&(_, v)| v)
}

/// CICS commands whose second word is part of the command, not an option.
pub fn cics_two_word(first: &str, second: &str) -> bool {
    matches!(
        (first, second),
        ("SEND" | "RECEIVE", "MAP" | "TEXT" | "CONTROL" | "PAGE" | "PARTNSET")
            | ("HANDLE", "CONDITION" | "AID" | "ABEND")
            | ("IGNORE", "CONDITION")
            | ("PUSH" | "POP", "HANDLE")
            | ("READQ" | "WRITEQ" | "DELETEQ", "TS" | "TD")
            | ("GET" | "PUT" | "DELETE" | "MOVE", "CONTAINER")
            | ("WEB", _)
            | ("INQUIRE" | "SET" | "CREATE" | "DISCARD", _)
            | ("START", "BROWSE" | "TRANSID")
            | ("GETNEXT", _)
            | ("ENDBROWSE", _)
    )
}

