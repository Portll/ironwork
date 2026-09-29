use super::*;
use crate::{Abend, Ending, compile, files, unit};

/// 2026-09-27 13:05:09.25 UTC, a Sunday.
const CLOCK: unit::Clock = unit::Clock::Fixed(1_790_514_309, 25);

fn page() -> &'static CodePage {
    CodePage::by_ccsid(1140).unwrap()
}

fn line(s: &str) -> String {
    format!("           {s}\n")
}

const DATA: &str = "       01  IN-STR.\n           05 IN-LEN PIC S9(4) BINARY.\n           05 IN-TEXT PIC X(60).\n       01  PIC-STR.\n           05 PIC-LEN PIC S9(4) BINARY.\n           05 PIC-TEXT PIC X(60).\n       01  LILIAN PIC 9(9) BINARY.\n       01  DAY-NO PIC 9(9) BINARY.\n       01  SECS COMP-2.\n       01  SECS-X REDEFINES SECS PIC X(8).\n       01  SECS-N PIC 9(11)V999.\n       01  GREG PIC X(17).\n       01  OUT-80 PIC X(80) VALUE SPACES.\n       01  FC.\n           05 FC-SEV PIC 9(4) BINARY.\n           05 FC-MSG PIC 9(4) BINARY.\n           05 FC-CTL PIC X.\n           05 FC-FAC PIC X(3).\n           05 FC-ISI PIC 9(9) BINARY.\n       01  FC-X REDEFINES FC PIC X(12).\n";

fn program(data: &str, body: &[String]) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{DATA}{data}       PROCEDURE DIVISION.\n{}{}",
        body.concat(),
        line("GOBACK.")
    )
}

/// MOVEs text into a halfword-prefixed string: IN or PIC.
fn set(item: &str, text: &str) -> Vec<String> {
    let value = if text.trim().is_empty() { "SPACES".to_owned() } else { format!("'{text}'") };
    vec![line(&format!("MOVE {} TO {item}-LEN", text.len())), line("MOVE"), line(&format!("    {value}")), line(&format!("    TO {item}-TEXT"))]
}

fn run(source: &str, dds: &[String]) -> (String, String, Result<(Ending, i16), Abend>) {
    let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
    let first = programs.remove(0);
    let compiled = compile(first, &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let library = unit::Library { programs, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ending = compiled.execute(library, files::Dds::new(dds, false).unwrap(), None, CLOCK, &mut out, &mut err);
    (String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap(), ending)
}

/// The run's DISPLAY lines, trailing blanks removed.
fn lines(source: &str) -> Vec<String> {
    let (out, err, ending) = run(source, &[]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    out.lines().map(|l| l.trim_end().to_owned()).collect()
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ironwork-le-{}-{name}", std::process::id()))
}

#[test]
fn lilian_days_start_on_15_october_1582() {
    assert_eq!(lilian(1582, 10, 15), 1);
    assert_eq!(lilian(1988, 5, 16), 148_138);
    assert_eq!(lilian(1990, 6, 4), 148_887);
    assert_eq!(lilian(2000, 1, 1), 152_385);
    assert_eq!(lilian(2024, 2, 29), 161_210);
    assert_eq!(lilian(1970, 1, 1), 141_428);
    assert_eq!(LAST_LILIAN, 3_074_324);
    assert_eq!(Stamp::from_unix(0, 0), Stamp { lilian: 141_428, millis: 0 });
    assert_eq!((weekday(1), weekday(148_138), weekday(152_385)), (6, 2, 7));
    let f = Stamp { lilian: 152_444, millis: 0 }.fields();
    assert_eq!((f.year, f.month, f.day, f.yday), (2000, 2, 29, 60));
}

#[test]
fn condition_tokens_carry_severity_number_and_facility() {
    assert_eq!(INSUFFICIENT.symbol(), "CEE2EB");
    assert_eq!(INSUFFICIENT.token(), [0x00, 0x03, 0x09, 0xCB, 0x59, 0xC3, 0xC5, 0xC5, 0, 0, 0, 0]);
    assert_eq!(DATE_VALUE.token()[..8], [0x00, 0x03, 0x09, 0xCC, 0x59, 0xC3, 0xC5, 0xC5]);
    assert_eq!((DATE_TRUNCATED.symbol(), DATE_TRUNCATED.token()[4]), ("CEE2EU".to_owned(), 0x51));
    assert_eq!((DUMP_OPTIONS.symbol(), DESTINATION.symbol(), FREE_ADDRESS.symbol()), ("CEE30U".to_owned(), "CEE0E3".to_owned(), "CEE0PA".to_owned()));
}

#[test]
fn lilian_seconds_are_long_hfp_to_the_millisecond() {
    for ms in [86_400_000, 12_799_191_601_078, 12_799_191_661_986, 265_621_679_999_999] {
        let bytes = seconds_hfp(ms);
        assert_eq!(bytes[0] & 0x80, 0);
        assert_eq!(hfp_millis(bytes), Some(ms));
    }
    assert_eq!(seconds_hfp(86_400_000), [0x45, 0x15, 0x18, 0, 0, 0, 0, 0]);
    assert_eq!(seconds_hfp(0), [0; 8]);
}

#[test]
fn picture_terms_and_delimiters() {
    let t = |s: &str| terms(&page().encode(s).unwrap(), page());
    assert_eq!(t("YYYY-MM-DD"), [Term::Year(4), Term::Literal(0x60), Term::Month(false), Term::Literal(0x60), Term::Day(false)]);
    assert_eq!(t("HH:MM"), [Term::Hour(false), Term::Literal(0x7A), Term::Minute]);
    assert_eq!(t("Mmmmmmmmmz ZD")[0], Term::MonthName { upper: [true, false, false, false, false, false, false, false, false].to_vec(), trim: true });
    assert_eq!(t("MMMMMMMMMZD")[1], Term::Day(true));
    assert_eq!(t("<JJJJ> YY.MM.DD")[0], Term::Era);
    assert_eq!(t("S P"), [Term::Literal(0xE2), Term::Literal(0x40), Term::Literal(0xD7)]);
}

#[test]
fn dump_options_and_the_s806_message() {
    assert_eq!(dump_options("TRACE FILE VAR STOR"), ("CEEDUMP".to_owned(), false));
    assert_eq!(dump_options("NOTRACEBACK,FNAME(MYDUMP) BLOCKS"), ("MYDUMP".to_owned(), false));
    assert!(dump_options("TRACE BOGUS").1);
    assert!(missing("CEEHDLR").contains("Language Environment callable service"));
    assert!(missing("CEESDLOG").contains("Language Environment callable service"));
    assert!(missing("CEEXYZ").contains("no such program"));
}

fn days_case(text: &str, picture: &str) -> Vec<String> {
    let mut v = set("IN", text);
    v.extend(set("PIC", picture));
    v.push(line("CALL 'CEEDAYS' USING IN-STR PIC-STR LILIAN FC"));
    v.push(line("DISPLAY LILIAN ' ' FC-SEV ' ' FC-MSG"));
    v
}

#[test]
fn ceedays_reads_a_date_by_its_picture() {
    let cases = [
        ("1582-10-15", "YYYY-MM-DD", "000000001 0000 0000"),
        ("5/16/88", "MM/DD/YY", "000148138 0000 0000"),
        ("6/2/88", "MM/DD/YY", "000148155 0000 0000"),
        ("060288", "MMDDYY", "000148155 0000 0000"),
        ("88154", "YYDDD", "000148155 0000 0000"),
        ("  1988154", "YYYYDDD", "000148155 0000 0000"),
        ("August 14, 1966", "Mmmmmmmmmmmz DD, YYYY", "000140192 0000 0000"),
        ("09 JUN 88", "DD MMM YY", "000148162 0000 0000"),
        ("20000101", "YYYYMMDD", "000152385 0000 0000"),
        ("2024-02-29", "YYYY-MM-DD", "000161210 0000 0000"),
        ("2023-02-29", "YYYY-MM-DD", "000000000 0003 2508"),
        ("2024-13-01", "YYYY-MM-DD", "000000000 0003 2517"),
        ("2024-01-X1", "YYYY-MM-DD", "000000000 0003 2520"),
        ("          ", "YYYY-MM-DD", "000000000 0003 2507"),
        ("1582-10-14", "YYYY-MM-DD", "000000000 0003 2513"),
        ("63.05.16", "<JJJJ> YY.MM.DD", "000000000 0003 2518"),
        ("88/05/16 19:00", "YY/MM/DD HH:MI", "000148138 0000 0000"),
    ];
    let body: Vec<String> = cases.iter().flat_map(|(t, p, _)| days_case(t, p)).collect();
    let expected: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(lines(&program("", &body)), expected);
}

#[test]
fn the_feedback_code_is_all_zero_on_success_and_a_cee_token_on_failure() {
    let data = "       01  EXPECTED PIC X(12).\n           88 BAD-DATE VALUE X'000309CC59C3C5C5'.\n           88 ALL-ZERO VALUE LOW-VALUES.\n";
    let mut body = vec![line("MOVE ALL 'X' TO FC-X")];
    body.extend(days_case("2024-02-29", "YYYY-MM-DD"));
    body.push(line("MOVE FC-X TO EXPECTED"));
    body.push(line("IF ALL-ZERO DISPLAY 'CEE000' END-IF"));
    body.extend(days_case("2024-02-30", "YYYY-MM-DD"));
    body.push(line("MOVE FC-X(1:8) TO EXPECTED"));
    body.push(line("IF BAD-DATE AND FC-FAC = 'CEE' AND FC-ISI = 0"));
    body.push(line("    DISPLAY 'CEE2EC' END-IF"));
    assert_eq!(lines(&program(data, &body)), ["000161210 0000 0000", "CEE000", "000000000 0003 2508", "CEE2EC"]);
}

#[test]
fn an_omitted_feedback_code_signals_the_condition_and_ends_the_run() {
    let mut body = set("IN", "2024-02-30");
    body.extend(set("PIC", "YYYY-MM-DD"));
    body.push(line("MOVE 148138 TO LILIAN"));
    body.push(line("CALL 'CEEDATE' USING LILIAN PIC-STR OUT-80 OMITTED"));
    body.push(line("DISPLAY 'NO CONDITION ON SUCCESS'"));
    body.push(line("CALL 'CEEDAYS' USING IN-STR PIC-STR LILIAN OMITTED"));
    body.push(line("DISPLAY 'NOT REACHED'"));
    let (out, _, ending) = run(&program("", &body), &[]);
    assert_eq!(out, "NO CONDITION ON SUCCESS\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "U4038");
    assert!(abend.message.contains("CEE2EC") && abend.message.contains("omitted"), "{}", abend.message);
}

fn date_case(lilian: i64, picture: &str) -> Vec<String> {
    let mut v = vec![line(&format!("MOVE {lilian} TO LILIAN"))];
    v.extend(set("PIC", picture));
    v.push(line("CALL 'CEEDATE' USING LILIAN PIC-STR OUT-80 FC"));
    v.push(line("DISPLAY FC-MSG '|' OUT-80"));
    v
}

#[test]
fn ceedate_writes_a_lilian_date_in_the_pictures_form() {
    let cases = [
        (148_138, "YYYY-MM-DD", "0000|1988-05-16"),
        (148_138, "YYYY-ZM-ZD", "0000|1988-5-16"),
        (148_138, "Wwwwwwwwwz, Mmmmmmmmmz ZD, YYYY", "0000|Monday, May 16, 1988"),
        (148_142, "WWW., MMM DD, YYYY", "0000|FRI., MAY 20, 1988"),
        (148_143, "Wwwwwwwwww Mmmmmmmmmm DD, YYYY", "0000|Saturday   May        21, 1988"),
        (148_141, "YY.DDD", "0000|88.140"),
        (148_138, "RRRR/YY", "0000|V   /88"),
        (148_142, "YY/MM/DD HH:MI:SS.99", "0000|88/05/20 00:00:00.00"),
        (148_142, "YYYY/ZM/ZD ZH:MI AP", "0000|1988/5/20 0:00 AM"),
        (139_370, "The date is Wwwwwwwwwz, Mmmmmmmmmz ZD, YYYY", "0000|The date is Thursday, May 14, 1964"),
        (3_074_324, "YYYYMMDD", "0000|99991231"),
        (0, "YYYYMMDD", "2512|"),
        (3_074_325, "YYYYMMDD", "2512|"),
        (148_138, "", "0000|05/16/88"),
    ];
    let body: Vec<String> = cases.iter().flat_map(|(l, p, _)| date_case(*l, p)).collect();
    let expected: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(lines(&program("", &body)), expected);
}

#[test]
fn a_date_longer_than_80_characters_is_truncated_with_a_severity_2_condition() {
    let data = "       01  BIG-PIC.\n           05 BIG-LEN PIC S9(4) BINARY VALUE 81.\n           05 BIG-TEXT PIC X(81).\n";
    let body = [
        line("MOVE ALL 'Wwwwwwwwwwwwwwwwwwww' TO BIG-TEXT"),
        line("MOVE '!' TO BIG-TEXT(81:1)"),
        line("MOVE 148138 TO LILIAN"),
        line("CALL 'CEEDATE' USING LILIAN BIG-PIC OUT-80 FC"),
        line("DISPLAY FC-SEV ' ' FC-MSG ' ' OUT-80(61:6)"),
        line("MOVE 80 TO BIG-LEN"),
        line("CALL 'CEEDATE' USING LILIAN BIG-PIC OUT-80 FC"),
        line("DISPLAY FC-SEV ' ' FC-MSG ' ' OUT-80(61:6)"),
    ];
    assert_eq!(lines(&program(data, &body)), ["0002 2526 Monday", "0000 0000 Monday"]);
}

#[test]
fn ceegmt_ceeloct_and_ceegmto_read_the_clock_as_utc() {
    let data = "       01  HOURS PIC S9(9) BINARY VALUE -1.\n       01  MINS PIC S9(9) BINARY VALUE -1.\n       01  OFFSET COMP-2 VALUE 1.\n       01  SECS2 COMP-2.\n       01  SECS2-X REDEFINES SECS2 PIC X(8).\n";
    let body = [
        line("CALL 'CEEGMT' USING LILIAN SECS FC"),
        line("MOVE SECS TO SECS-N"),
        line("DISPLAY LILIAN ' ' SECS-N ' ' FC-SEV"),
        line("MOVE 0 TO LILIAN"),
        line("CALL 'CEELOCT' USING LILIAN SECS2 GREG FC"),
        line("MOVE SECS2 TO SECS-N"),
        line("DISPLAY LILIAN ' ' SECS-N ' ' GREG"),
        line("IF SECS-X = SECS2-X DISPLAY 'SAME INSTANT' END-IF"),
        line("MOVE 0 TO SECS2"),
        line("CALL 'CEEUTC' USING LILIAN SECS2 FC"),
        line("IF SECS-X = SECS2-X DISPLAY 'CEEUTC IS CEEGMT' END-IF"),
        line("CALL 'CEEGMTO' USING HOURS MINS OFFSET FC"),
        line("IF HOURS = 0 AND MINS = 0 AND OFFSET = 0 DISPLAY 'UTC' END-IF"),
    ];
    assert_eq!(
        lines(&program(data, &body)),
        ["000162151 14009893509250 0000", "000162151 14009893509250 20260927130509250", "SAME INSTANT", "CEEUTC IS CEEGMT", "UTC"]
    );
}

fn datm_case(seconds: &str, picture: &str) -> Vec<String> {
    let mut v = vec![line(&format!("COMPUTE SECS = {seconds}"))];
    v.extend(set("PIC", picture));
    v.push(line("CALL 'CEEDATM' USING SECS PIC-STR OUT-80 FC"));
    v.push(line("DISPLAY FC-MSG '|' OUT-80"));
    v
}

#[test]
fn ceedatm_writes_lilian_seconds_as_a_timestamp() {
    let cases = [
        ("12799191601.000", "YYMMDD HH:MI:SS", "0000|880516 19:00:01"),
        ("12799191601.000", "YYYY-MM-DD HH:MI:SS AP", "0000|1988-05-16 07:00:01 PM"),
        ("12799191661.986", "DD MMM YY HH:MM", "0000|16 MAY 88 19:01"),
        ("12799191661.986", "WWW, MMM DD, YYYY ZH:MI AP", "0000|MON, MAY 16, 1988 7:01 PM"),
        ("12799191661.986", "Wwwwwwwwwz, ZM/ZD/YY HH:MI:SS.99", "0000|Monday, 5/16/88 19:01:01.98"),
        ("12799191662.009", "SS.999 a.p. W", "0000|02.009 p.m. M"),
        ("12799123200", "ZH:MI AP", "0000|12:00 AM"),
        ("14009893509.250", "YYYYMMDDHHMISS", "0000|20260927130509"),
        ("86399", "YYYYMMDD", "2505|"),
        ("12799191601", "", "0000|05/16/88 7:00:01 PM"),
    ];
    let body: Vec<String> = cases.iter().flat_map(|(s, p, _)| datm_case(s, p)).collect();
    let expected: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(lines(&program("", &body)), expected);
}

fn secs_case(text: &str, picture: &str) -> Vec<String> {
    let mut v = set("IN", text);
    v.extend(set("PIC", picture));
    v.push(line("CALL 'CEESECS' USING IN-STR PIC-STR SECS FC"));
    v.push(line("COMPUTE SECS-N ROUNDED = SECS"));
    v.push(line("DISPLAY SECS-N ' ' FC-MSG"));
    v
}

#[test]
fn ceesecs_reads_a_timestamp_and_fills_omitted_time_with_zeros() {
    let cases = [
        ("1988-05-16 19:00:01.078", "YYYY-MM-DD HH:MI:SS.999", "12799191601078 0000"),
        ("92/6/3 3.35.03 PM", "YY/MM/DD HH.MI.SS AP", "12926964903000 0000"),
        ("92.155 3.35.03 pm", "YY.DDD HH.MI.SS AP", "12926964903000 0000"),
        ("1992-05-17", "YYYY-MM-DD-HH:MI", "12925440000000 0000"),
        ("1988-05-16 25:00", "YYYY-MM-DD HH:MI", "00000000000000 2510"),
        ("1988-05-16 19:61", "YYYY-MM-DD HH:MI", "00000000000000 2516"),
        ("1988-05-16 19:0X", "YYYY-MM-DD HH:MI", "00000000000000 2525"),
    ];
    let body: Vec<String> = cases.iter().flat_map(|(t, p, _)| secs_case(t, p)).collect();
    let expected: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(lines(&program("", &body)), expected);
}

#[test]
fn ceedywk_numbers_the_days_from_sunday() {
    let body: Vec<String> = [148_138, 1, 162_151, 0]
        .iter()
        .flat_map(|l| [line(&format!("MOVE {l} TO LILIAN")), line("CALL 'CEEDYWK' USING LILIAN DAY-NO FC"), line("DISPLAY DAY-NO ' ' FC-MSG")])
        .collect();
    assert_eq!(lines(&program("", &body)), ["000000002 0000", "000000006 0000", "000000001 0000", "000000000 2512"]);
}

#[test]
fn cee3abd_ends_the_run_with_a_user_abend() {
    let data = "       01  ABCODE PIC S9(9) BINARY.\n       01  TIMING PIC S9(9) BINARY.\n";
    let body = [line("MOVE 0 TO TIMING"), line("MOVE 999 TO ABCODE"), line("DISPLAY 'ABENDING'"), line("CALL 'CEE3ABD' USING ABCODE, TIMING"), line("DISPLAY 'NOT REACHED'")];
    let (out, _, ending) = run(&program(data, &body), &[]);
    assert_eq!(out, "ABENDING\n");
    let abend = ending.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.message.as_str()), ("U0999", "CALL CEE3ABD: user abend 999 without clean-up"));
    let body = [line("MOVE 1 TO TIMING"), line("MOVE 4101 TO ABCODE"), line("CALL 'CEE3ABD' USING ABCODE TIMING")];
    let abend = run(&program(data, &body), &[]).2.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.message.as_str()), ("U0005", "CALL CEE3ABD: user abend 5 with normal enclave termination"));
}

fn run_cics(source: &str, dds: &[String]) -> (String, Result<(Ending, crate::cics::Task), Abend>) {
    let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
    let first = programs.remove(0);
    let compiled = compile(first, &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let library = unit::Library { programs, ..Default::default() };
    let task = crate::cics::Task { transid: "LE01".into(), termid: "T001".into(), ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ending = compiled.execute_cics(library, files::Dds::new(dds, false).unwrap(), task, CLOCK, &mut out, &mut err);
    assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
    (String::from_utf8(out).unwrap(), ending)
}

#[test]
fn cee3abd_in_a_cics_task_is_a_transaction_abend_named_by_the_code() {
    let data = "       01  ABCODE PIC S9(9) BINARY.\n       01  TIMING PIC S9(9) BINARY.\n";
    let body = [line("MOVE 0 TO TIMING"), line("MOVE 999 TO ABCODE"), line("CALL 'CEE3ABD' USING ABCODE, TIMING"), line("DISPLAY 'NOT REACHED'")];
    let (out, ending) = run_cics(&program(data, &body), &[]);
    assert_eq!(out, "");
    let abend = ending.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.message.as_str()), ("0999", "CALL CEE3ABD: transaction abend 0999"));
    let body = [line("MOVE 1 TO TIMING"), line("MOVE 4101 TO ABCODE"), line("CALL 'CEE3ABD' USING ABCODE TIMING")];
    assert_eq!(run_cics(&program(data, &body), &[]).1.unwrap_err().code, "0005");
}

#[test]
fn ceemout_and_cee3dmp_in_a_cics_task_write_to_cese_and_no_dd() {
    let data = "       01  DEST PIC S9(9) BINARY VALUE 2.\n       01  TITLE PIC X(80) VALUE 'CICS DUMP'.\n       01  OPTS PIC X(255) VALUE 'FNAME(MYDUMP)'.\n";
    let mut body = set("IN", "Hello from CEEMOUT");
    body.push(line("CALL 'CEEMOUT' USING IN-STR DEST FC"));
    body.push(line("CALL 'CEE3DMP' USING TITLE OPTS FC"));
    body.push(line("DISPLAY FC-MSG"));
    let (sysout, dump) = (temp("cics-sysout.txt"), temp("cics-mydump.txt"));
    let dds = [format!("SYSOUT={}", sysout.display()), format!("MYDUMP={}", dump.display())];
    let (out, ending) = run_cics(&program(data, &body), &dds);
    let (_, task) = ending.unwrap_or_else(|a| panic!("{a:?}"));
    assert_eq!(out, "0000\n");
    assert!(!sysout.exists() && !dump.exists());
    let cese: Vec<String> = task.td.get("CESE").expect("CESE written").iter().map(|item| page().decode(item)).collect();
    assert_eq!(cese[0], "Hello from CEEMOUT");
    assert!(cese[1].starts_with("CEE3DMP: CICS DUMP") && cese[1].contains("2026-09-27 13:05:09"), "{cese:?}");
    assert_eq!(cese[2], "Options: FNAME(MYDUMP)");
    assert_eq!(cese[4], "  T");
}

/// CardDemo's batch programs end with CALL 'CEE3ABD' and no USING.
#[test]
fn a_service_given_fewer_arguments_than_it_takes_is_ironworks_own_abend() {
    let data = "       01  ABCODE PIC S9(9) BINARY VALUE 999.\n";
    let cases = [
        ("CALL 'CEE3ABD'", "CALL CEE3ABD passes 0 arguments; CEE3ABD takes abcode, clean-up"),
        ("CALL 'CEE3ABD' USING ABCODE", "CALL CEE3ABD passes 1 argument; CEE3ABD takes abcode, clean-up"),
        ("CALL 'CEEDATE' USING LILIAN PIC-STR OUT-80", "CALL CEEDATE passes 3 arguments; CEEDATE takes input_Lilian_date, picture_string, output_char_date, fc"),
    ];
    for (call, message) in cases {
        let body = [line("DISPLAY 'ABENDING'"), line(call), line("DISPLAY 'NOT REACHED'")];
        let (out, _, ending) = run(&program(data, &body), &[]);
        assert_eq!(out, "ABENDING\n");
        let abend = ending.unwrap_err();
        assert_eq!((abend.code.as_str(), abend.message), ("IRONWORK", format!("{message}, and with fewer z/OS is unpredictable")));
    }
}

#[test]
fn a_service_is_called_through_an_identifier_and_a_program_of_its_name_comes_first() {
    let data = "       01  SERVICE PIC X(8) VALUE 'CEEDAYS'.\n";
    let mut body = set("IN", "20000101");
    body.extend(set("PIC", "YYYYMMDD"));
    body.push(line("CALL SERVICE USING IN-STR PIC-STR LILIAN FC"));
    body.push(line("DISPLAY LILIAN"));
    body.push(line("CALL 'CEEDATE' USING LILIAN PIC-STR OUT-80 FC"));
    body.push(line("DISPLAY OUT-80"));
    let main = program(data, &body);
    let user = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CEEDATE.\n       PROCEDURE DIVISION.\n           DISPLAY 'THE USER PROGRAM CEEDATE'\n           GOBACK.\n";
    let (out, err, ending) = run(&format!("{main}       END PROGRAM T.\n{user}       END PROGRAM CEEDATE.\n"), &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out.lines().map(str::trim_end).collect::<Vec<_>>(), ["000152385", "THE USER PROGRAM CEEDATE", ""]);
}

#[test]
fn a_service_ironwork_does_not_provide_is_s806_naming_it() {
    let body = [line("CALL 'CEEHDLR' ON EXCEPTION DISPLAY 'EXCEPTION' END-CALL"), line("CALL 'CEEHDLR' USING FC")];
    let (out, _, ending) = run(&program("", &body), &[]);
    assert_eq!(out, "EXCEPTION\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "S806");
    assert_eq!(abend.message, "CALL CEEHDLR: CEEHDLR is a Language Environment callable service that ironwork for COBOL does not provide yet");
}

#[test]
fn ceemout_writes_to_the_message_file_or_standard_error() {
    let data = "       01  DEST PIC S9(9) BINARY VALUE 2.\n";
    let mut body = set("IN", "Hello from CEEMOUT");
    body.push(line("CALL 'CEEMOUT' USING IN-STR DEST FC"));
    body.push(line("CALL 'CEEMOUT' USING IN-STR DEST FC"));
    body.push(line("MOVE 3 TO DEST"));
    body.push(line("CALL 'CEEMOUT' USING IN-STR DEST FC"));
    body.push(line("DISPLAY FC-SEV ' ' FC-MSG"));
    let source = program(data, &body);
    let (out, err, ending) = run(&source, &[]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!((out.as_str(), err.as_str()), ("0003 0451\n", "Hello from CEEMOUT\nHello from CEEMOUT\n"));
    let path = temp("sysout.txt");
    std::fs::write(&path, "from an earlier run\n").unwrap();
    let (_, err, ending) = run(&source, &[format!("SYSOUT={}", path.display())]);
    assert!(ending.is_ok() && err.is_empty(), "{ending:?} {err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello from CEEMOUT\nHello from CEEMOUT\n");
}

#[test]
fn cee3dmp_writes_a_titled_dump_to_its_dd() {
    let data = "       01  TITLE PIC X(80) VALUE 'DUMP FROM THE TEST'.\n       01  OPTS PIC X(255) VALUE 'TRACE FILE VAR STOR'.\n";
    let body = [line("CALL 'CEE3DMP' USING TITLE OPTS FC"), line("DISPLAY FC-MSG"), line("MOVE 'FNAME(MYDUMP) NOSUCH' TO OPTS"), line("CALL 'CEE3DMP' USING TITLE OPTS FC"), line("DISPLAY FC-MSG")];
    let path = temp("mydump.txt");
    let (out, err, ending) = run(&program(data, &body), &[format!("MYDUMP={}", path.display())]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "0000\n3102\n");
    assert!(err.starts_with("CEE3DMP: DUMP FROM THE TEST") && err.contains("2026-09-27 13:05:09") && err.contains("\n  T\n"), "{err}");
    let dump = std::fs::read_to_string(&path).unwrap();
    assert!(dump.starts_with("CEE3DMP: DUMP FROM THE TEST") && dump.contains("Options: FNAME(MYDUMP) NOSUCH"), "{dump}");
}

#[test]
fn ceegtst_storage_outlives_the_call_that_got_it_and_ceefrst_frees_it() {
    let main_data = "       01  HEAP-ID PIC S9(9) BINARY VALUE 0.\n       01  SIZE-N PIC S9(9) BINARY VALUE 64.\n       01  PTR POINTER.\n       LINKAGE SECTION.\n       01  AREA-L PIC X(5).\n";
    let body = [
        line("CALL 'SUB' USING PTR"),
        line("SET ADDRESS OF AREA-L TO PTR"),
        line("DISPLAY AREA-L"),
        line("CALL 'CEEFRST' USING PTR FC"),
        line("DISPLAY FC-MSG"),
        line("CALL 'CEEFRST' USING PTR FC"),
        line("DISPLAY FC-MSG"),
        line("MOVE 0 TO SIZE-N"),
        line("CALL 'CEEGTST' USING HEAP-ID SIZE-N PTR FC"),
        line("DISPLAY FC-MSG"),
        line("MOVE 7 TO HEAP-ID"),
        line("MOVE 8 TO SIZE-N"),
        line("CALL 'CEEGTST' USING HEAP-ID SIZE-N PTR FC"),
        line("DISPLAY FC-MSG"),
    ];
    let main = program(main_data, &body);
    let sub = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  HEAP-ID PIC S9(9) BINARY VALUE 0.\n       01  SIZE-N PIC S9(9) BINARY VALUE 64.\n       01  FC PIC X(12).\n       LINKAGE SECTION.\n       01  P POINTER.\n       01  AREA-L PIC X(5).\n       PROCEDURE DIVISION USING P.\n           CALL 'CEEGTST' USING HEAP-ID SIZE-N P FC\n           SET ADDRESS OF AREA-L TO P\n           MOVE 'HELLO' TO AREA-L\n           GOBACK.\n";
    let (out, err, ending) = run(&format!("{main}       END PROGRAM T.\n{sub}       END PROGRAM SUB.\n"), &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "HELLO\n0000\n0810\n0808\n0803\n");
}

/// CardDemo's CSUTLDTC, cut down: variable-length strings and the CEEIGZCT tokens as 88-levels.
#[test]
fn the_carddemo_date_check_runs() {
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CSUTLDTC.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n         01 WS-DATE-TO-TEST.\n              02  Vstring-length      PIC S9(4) BINARY.\n              02  Vstring-text.\n                  03  Vstring-char    PIC X\n                              OCCURS 0 TO 256 TIMES\n                              DEPENDING ON Vstring-length\n                                 of WS-DATE-TO-TEST.\n         01 WS-DATE-FORMAT.\n              02  Vstring-length      PIC S9(4) BINARY.\n              02  Vstring-text.\n                  03  Vstring-char    PIC X\n                              OCCURS 0 TO 256 TIMES\n                              DEPENDING ON Vstring-length\n                                 of WS-DATE-FORMAT.\n         01 OUTPUT-LILLIAN    PIC S9(9) USAGE IS BINARY.\n          01 FEEDBACK-CODE.\n           02  FEEDBACK-TOKEN-VALUE.\n             88  FC-INVALID-DATE       VALUE X'0000000000000000'.\n             88  FC-BAD-DATE-VALUE     VALUE X'000309CC59C3C5C5'.\n             88  FC-INVALID-MONTH      VALUE X'000309D559C3C5C5'.\n               03  SEVERITY        PIC S9(4) BINARY.\n               03  MSG-NO          PIC S9(4) BINARY.\n               03  CASE-SEV-CTL    PIC X.\n               03  FACILITY-ID     PIC XXX.\n           02  I-S-INFO        PIC S9(9) BINARY.\n       01  DATES PIC X(30) VALUE '2024-02-292024-02-302024-13-01'.\n       01  I PIC 99.\n       PROCEDURE DIVISION.\n           PERFORM VARYING I FROM 1 BY 10 UNTIL I > 30\n             MOVE 10 TO VSTRING-LENGTH OF WS-DATE-TO-TEST\n             MOVE DATES(I:10) TO VSTRING-TEXT OF WS-DATE-TO-TEST\n             MOVE 10 TO VSTRING-LENGTH OF WS-DATE-FORMAT\n             MOVE 'YYYY-MM-DD' TO VSTRING-TEXT OF WS-DATE-FORMAT\n             CALL \"CEEDAYS\" USING WS-DATE-TO-TEST, WS-DATE-FORMAT,\n                  OUTPUT-LILLIAN, FEEDBACK-CODE\n             EVALUATE TRUE\n               WHEN FC-INVALID-DATE DISPLAY 'Date is valid'\n               WHEN FC-BAD-DATE-VALUE DISPLAY 'Datevalue error'\n               WHEN FC-INVALID-MONTH DISPLAY 'Invalid month'\n               WHEN OTHER DISPLAY 'Date is invalid'\n             END-EVALUATE\n           END-PERFORM\n           GOBACK.\n";
    assert_eq!(lines(source), ["Date is valid", "Datevalue error", "Invalid month"]);
}

/// CBSA's CRECUST and EBUD03 copy CEEIGZCT into the token's first 8 bytes and test CEE000 OF FC.
#[test]
fn copy_ceeigzct_names_each_symbolic_feedback_code() {
    let token = "           02 CONDITION-TOKEN-VALUE.\n           COPY CEEIGZCT.\n              03 SEVERITY PIC S9(4) BINARY.\n              03 MSG-NO PIC S9(4) BINARY.\n              03 CASE-SEV-CTL PIC X.\n              03 FACILITY-ID PIC XXX.\n           02 I-S-INFO PIC S9(9) BINARY.\n";
    let whole = "       01  FW.\n           COPY CEEIGZCT.\n           02 FW-TOKEN PIC X(8).\n           02 FW-ISI PIC S9(9) BINARY.\n";
    let data = format!("       01  FB.\n{token}{whole}");
    let mut body = Vec::new();
    for date in ["2024-02-29", "2024-02-30", "2024-13-01", "          "] {
        body.extend(set("IN", date));
        body.extend(set("PIC", "YYYY-MM-DD"));
        body.push(line("CALL 'CEEDAYS' USING IN-STR PIC-STR LILIAN FB"));
        body.push(line("EVALUATE TRUE"));
        for name in ["CEE000", "CEE2EB", "CEE2EC", "CEE2EL"] {
            body.push(line(&format!("  WHEN {name} OF FB DISPLAY '{name}'")));
        }
        body.push(line("  WHEN OTHER DISPLAY MSG-NO OF FB"));
        body.push(line("END-EVALUATE"));
    }
    body.push(line("MOVE LOW-VALUES TO FW"));
    body.push(line("IF NOT CEE000 OF FW DISPLAY 'TWELVE BYTES' END-IF"));
    assert_eq!(lines(&program(&data, &body)), ["CEE000", "CEE2EC", "CEE2EL", "CEE2EB", "TWELVE BYTES"]);
}

#[test]
fn ceeigzct_holds_every_condition_the_services_return() {
    let named: Vec<(u16, u8)> = syntax::feedback::conditions().collect();
    let returned = [
        DESTINATION, HEAP_ID, HEAP_SIZE, FREE_ADDRESS, HEAP_SHORT, SECONDS_RANGE, INSUFFICIENT, DATE_VALUE, HOURS, LILIAN_RANGE, DATE_RANGE,
        MINUTES, MONTH, PICTURE, SECONDS_VALUE, DAYS_NONNUMERIC, SECS_NONNUMERIC, DATE_TRUNCATED, TIMESTAMP_TRUNCATED, DUMP_OPTIONS,
    ];
    for c in returned {
        assert!(named.contains(&(c.number, c.severity)), "{c:?}");
        assert!(!c.text().is_empty(), "{c:?}");
    }
}

/// IBM's IGYTSALE sample builds its report heading from one 80-character picture.
#[test]
fn a_picture_carries_literal_text_between_its_terms() {
    let data = "       01  PICT-HDR-V.\n           02 PICT-HDR-LEN USAGE BINARY PICTURE 9999 VALUE 80.\n           02 PICT-HDR.\n              05 PIC X(9) VALUE 'Wwwwwwwww'.\n              05 PIC X(11) VALUE SPACES.\n              05 PIC X(23) VALUE 'C O B O L   S P O R T S'.\n              05 PIC X(8) VALUE SPACES.\n              05 PIC X(10) VALUE 'MM/DD/YYYY'.\n              05 PIC X(4) VALUE SPACES.\n              05 PIC X(5) VALUE 'HH:MI'.\n              05 PIC X(10) VALUE SPACES.\n";
    let body = [line("CALL 'CEELOCT' USING LILIAN SECS GREG FC"), line("CALL 'CEEDATM' USING SECS PICT-HDR-V OUT-80 FC"), line("DISPLAY OUT-80")];
    let expected = format!("{:<20}C O B O L   S P O R T S{:8}09/27/2026{:4}13:05", "Sunday", "", "");
    assert_eq!(lines(&program(data, &body)), [expected]);
}
