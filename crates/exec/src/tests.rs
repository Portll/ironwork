use super::*;
use crate::testing::{Executor, Harness, compile_errors, ebcdic, line};
use compile::refused;
use numeric::Options;
use std::collections::BTreeMap;
use syntax::ast::Stmt;
use syntax::{Error, Severity};

mod arguments;
mod assign;
mod classes;
mod collating;
mod compliance;
mod corresponding;
mod data;
mod data_division;
mod dbcs;
mod declaratives;
mod diagnostics;
mod dialect;
mod endings;
mod differential;
mod documented;
mod function;
mod initcheck;
mod intrinsic;
mod json;
mod json_parse;
mod linage;
mod lowering;
mod numcheck;
mod oo;
mod operands;
mod parmcheck;
mod printer;
mod procedure;
mod quote_currency_nsymbol;
mod report;
mod scope;
mod sort;
mod vm;
mod statements;
mod switches;
mod taint;
mod xml;
mod xml_generate;

fn program(options: &str, data: &str, procedure: &str) -> String {
    let card = if options.is_empty() { String::new() } else { format!("       CBL {options}\n") };
    format!(
        "{card}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{procedure}"
    )
}

fn run_with(source: &str, flags: &[&str]) -> (String, String, Result<Ending, Abend>) {
    let o = Harness::source(source).flags(flags).run(Executor::Interpreter);
    (o.out, o.err, o.ending)
}

fn run(source: &str) -> String {
    let (out, err, ending) = run_with(source, &[]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    out
}

/// The severe messages compiling the program gives, none when it compiles.
fn severe(source: &str) -> Vec<String> {
    match compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &[]) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().filter(|e| e.severity == Severity::Severe).map(|e| e.message).collect(),
    }
}

#[test]
fn layout_sizes_offsets_redefines_and_occurs() {
    let parsed = syntax::parse(&program(
        "",
        "       01  G.\n           05 A PIC S9(5) COMP-3.\n           05 B REDEFINES A PIC X(3).\n           05 T OCCURS 4 TIMES.\n              10 T1 PIC X.\n              10 T2 PIC 9(4) COMP.\n           05 F COMP-2.\n       01  H PIC X.\n",
        &line("GOBACK."),
    ))
    .unwrap();
    let c = compile(parsed, &[]).unwrap();
    let find = |n: &str| c.layout.items.iter().find(|i| i.name.as_deref() == Some(n)).unwrap();
    assert_eq!((find("A").offset, find("A").size), (0, 3));
    assert_eq!(find("B").offset, 0);
    assert_eq!((find("T").offset, find("T").size, find("T").occurs), (3, 3, 4));
    assert_eq!(find("T2").dims, [(3, 4)]);
    assert_eq!((find("F").offset, find("F").size), (15, 8));
    assert_eq!(find("G").size, 23);
    assert_eq!(find("H").offset, 24);
}

#[test]
fn display_move_and_arithmetic() {
    let out = run(&program(
        "",
        "       01  A PIC S9(3)V99 COMP-3 VALUE 12.5.\n       01  B PIC 9(5).\n       01  C PIC X(5) VALUE 'AB'.\n",
        &[line("COMPUTE B ROUNDED = A * 3 + 0.5"), line("DISPLAY B '|' C '|'"), line("MOVE B TO C"), line("DISPLAY C"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "00038|AB   |\n00038\n");
}

#[test]
fn perform_varying_and_paragraphs() {
    let out = run(&program(
        "",
        "       01  I PIC 9(2).\n       01  S PIC 9(4) VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 10"),
            &line("    PERFORM ADD-IT"),
            &line("END-PERFORM"),
            &line("DISPLAY S"),
            &line("GOBACK."),
            "       ADD-IT.\n",
            &line("ADD I TO S."),
        ]
        .concat(),
    ));
    assert_eq!(out, "0055\n");
}

#[test]
fn conditions_signs_initialize_size_error_and_control_flow() {
    let out = run(&program(
        "",
        "       01  G.\n           05 CODE-X PIC X VALUE 'B'.\n              88 IS-AB VALUE 'A' 'B'.\n           05 SL PIC S9(3) SIGN LEADING SEPARATE VALUE -42.\n           05 N PIC 9(2) VALUE 99.\n           05 T PIC X(2) OCCURS 2 VALUE 'ZZ'.\n       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("IF IS-AB DISPLAY 'AB' END-IF"),
            &line("DISPLAY SL"),
            &line("ADD 1 TO N ON SIZE ERROR DISPLAY 'SIZE'"),
            &line("    NOT ON SIZE ERROR DISPLAY 'FITS' END-ADD"),
            &line("DISPLAY N"),
            &line("INITIALIZE G"),
            &line("DISPLAY '[' CODE-X T(2) ']' N"),
            &line("PERFORM P1 THRU P2 3 TIMES"),
            &line("DISPLAY K"),
            &line("GO TO P3."),
            "       P1.\n",
            &line("CONTINUE."),
            "       P2.\n",
            &line("ADD 1 TO K."),
            "       P3.\n",
            &line("PERFORM UNTIL K = 0 SUBTRACT 1 FROM K END-PERFORM"),
            &line("DISPLAY 'END ' K"),
            &line("STOP RUN."),
        ]
        .concat(),
    ));
    assert_eq!(out, "AB\n-042\nSIZE\n99\n[   ]00\n3\nEND 0\n");
}

#[test]
fn sections_perform_fall_through_and_exit_section() {
    let out = run(&program(
        "",
        "       01  K PIC 9 VALUE 0.\n",
        &[
            "       MAIN SECTION.\n",
            &line("PERFORM WORK"),
            &line("PERFORM STEP OF OTHER-S"),
            &line("DISPLAY 'K=' K"),
            &line("GO TO FINISH."),
            "       WORK SECTION.\n",
            "       STEP.\n",
            &line("ADD 1 TO K."),
            "       SKIPPED.\n",
            &line("EXIT SECTION."),
            "       NEVER.\n",
            &line("ADD 5 TO K."),
            "       OTHER-S SECTION.\n",
            "       STEP.\n",
            &line("ADD 2 TO K."),
            "       FINISH SECTION.\n",
            &line("DISPLAY 'DONE'"),
            &line("STOP RUN."),
        ]
        .concat(),
    ));
    assert_eq!(out, "K=3\nDONE\n");
}

fn exit_program(procedure: &[&str]) -> String {
    program("", "       01  I PIC 9(2).\n       01  K PIC 9 VALUE 0.\n", &procedure.concat())
}

#[test]
fn exit_paragraph_leaves_the_paragraph_and_falls_to_the_next() {
    let out = run(&exit_program(&[
        "       MAIN-LINE.\n",
        &line("DISPLAY 'M'."),
        "       P1.\n",
        &line("DISPLAY 'A1'"),
        &line("EXIT PARAGRAPH"),
        &line("DISPLAY 'NOT REACHED'."),
        "       P2.\n",
        &line("DISPLAY 'B1'."),
        &line("EXIT PARAGRAPH."),
        &line("DISPLAY 'NOT REACHED 2'."),
        "       P3.\n",
        &line("EXIT PARAGRAPH."),
        &line("DISPLAY 'NOT REACHED 3'."),
        "       P4.\n",
        &line("IF K = 0"),
        &line("    EXIT PARAGRAPH"),
        &line("END-IF"),
        &line("DISPLAY 'NOT REACHED 4'."),
        "       P5.\n",
        &line("DISPLAY 'END'"),
        &line("STOP RUN."),
    ]));
    assert_eq!(out, "M\nA1\nB1\nEND\n");
}

#[test]
fn exit_paragraph_in_a_performed_paragraph_returns_to_the_performer() {
    let out = run(&exit_program(&[
        "       MAIN-LINE.\n",
        &line("PERFORM P1"),
        &line("DISPLAY 'BACK'"),
        &line("PERFORM P2"),
        &line("DISPLAY 'END'"),
        &line("STOP RUN."),
        "       P1.\n",
        &line("DISPLAY 'A1'"),
        &line("EXIT PARAGRAPH"),
        &line("DISPLAY 'NOT REACHED'."),
        "       P2.\n",
        &line("EXIT PARAGRAPH."),
        &line("DISPLAY 'NOT REACHED 2'."),
    ]));
    assert_eq!(out, "A1\nBACK\nEND\n");
}

#[test]
fn exit_section_leaves_the_whole_section() {
    let out = run(&exit_program(&[
        "       MAIN SECTION.\n",
        &line("DISPLAY 'M'."),
        "       S1 SECTION.\n",
        "       S1-A.\n",
        &line("DISPLAY 'S1A'"),
        &line("EXIT SECTION"),
        &line("DISPLAY 'NOT REACHED'."),
        "       S1-B.\n",
        &line("DISPLAY 'NOT REACHED S1B'."),
        "       S2 SECTION.\n",
        "       S2-A.\n",
        &line("DISPLAY 'S2A'."),
        &line("EXIT SECTION."),
        &line("DISPLAY 'NOT REACHED S2A'."),
        "       S2-B.\n",
        &line("DISPLAY 'NOT REACHED S2B'."),
        "       S3 SECTION.\n",
        "       S3-A.\n",
        &line("DISPLAY 'S3'"),
        &line("STOP RUN."),
    ]));
    assert_eq!(out, "M\nS1A\nS2A\nS3\n");
}

#[test]
fn exit_section_opening_a_paragraph_and_in_a_performed_section() {
    let out = run(&exit_program(&[
        "       MAIN SECTION.\n",
        &line("PERFORM S1"),
        &line("DISPLAY 'BACK'"),
        &line("GO TO S3."),
        "       S1 SECTION.\n",
        "       S1-A.\n",
        &line("EXIT SECTION."),
        &line("DISPLAY 'NOT REACHED'."),
        "       S1-B.\n",
        &line("DISPLAY 'NOT REACHED S1B'."),
        "       S3 SECTION.\n",
        &line("DISPLAY 'S3'"),
        &line("STOP RUN."),
    ]));
    assert_eq!(out, "BACK\nS3\n");
}

#[test]
fn exit_perform_ends_an_inline_loop() {
    let out = run(&exit_program(&[
        &line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
        &line("    DISPLAY 'TOP ' I"),
        &line("    IF I = 3"),
        &line("        EXIT PERFORM"),
        &line("    END-IF"),
        &line("    DISPLAY 'BOTTOM ' I"),
        &line("END-PERFORM"),
        &line("DISPLAY 'AFTER ' I"),
        &line("GOBACK."),
    ]));
    assert_eq!(out, "TOP 01\nBOTTOM 01\nTOP 02\nBOTTOM 02\nTOP 03\nAFTER 03\n");
}

#[test]
fn exit_perform_opening_the_loop_body_and_leaving_only_the_inner_loop() {
    let out = run(&exit_program(&[
        &line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 2"),
        &line("    PERFORM UNTIL K > 5"),
        &line("        ADD 1 TO K"),
        &line("        IF K = 2"),
        &line("            EXIT PERFORM"),
        &line("        END-IF"),
        &line("    END-PERFORM"),
        &line("    DISPLAY 'I=' I ' K=' K"),
        &line("    MOVE 0 TO K"),
        &line("END-PERFORM"),
        &line("PERFORM"),
        &line("EXIT PERFORM"),
        &line("DISPLAY 'NOT REACHED'"),
        &line("END-PERFORM"),
        &line("DISPLAY 'END'"),
        &line("GOBACK."),
    ]));
    assert_eq!(out, "I=01 K=2\nI=02 K=2\nEND\n");
}

#[test]
fn exit_perform_cycle_skips_the_rest_of_one_iteration() {
    let out = run(&exit_program(&[
        &line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
        &line("    DISPLAY 'TOP ' I"),
        &line("    IF I = 3"),
        &line("        EXIT PERFORM CYCLE"),
        &line("    END-IF"),
        &line("    DISPLAY 'BOTTOM ' I"),
        &line("END-PERFORM"),
        &line("DISPLAY 'AFTER ' I"),
        &line("GOBACK."),
    ]));
    assert_eq!(out, "TOP 01\nBOTTOM 01\nTOP 02\nBOTTOM 02\nTOP 03\nTOP 04\nBOTTOM 04\nTOP 05\nBOTTOM 05\nAFTER 06\n");
}

#[test]
fn exit_perform_cycle_as_the_first_statement_of_the_body() {
    let out = run(&exit_program(&[
        &line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 3"),
        &line("EXIT PERFORM CYCLE"),
        &line("DISPLAY 'NOT REACHED'"),
        &line("END-PERFORM"),
        &line("DISPLAY 'AFTER ' I"),
        &line("GOBACK."),
    ]));
    assert_eq!(out, "AFTER 04\n");
}

#[test]
fn exit_perform_outside_an_inline_perform_is_a_severe_error() {
    let source = exit_program(&[
        "       MAIN-LINE.\n",
        &line("PERFORM P1"),
        &line("PERFORM 2 TIMES"),
        &line("    IF K = 0 EXIT PERFORM CYCLE END-IF"),
        &line("END-PERFORM"),
        &line("EXIT PERFORM"),
        &line("GOBACK."),
        "       P1.\n",
        &line("IF K = 0"),
        &line("    EXIT PERFORM CYCLE"),
        &line("END-IF."),
    ]);
    let errors = compile(syntax::parse(&source).unwrap(), &[]).err().expect("refused");
    let found: Vec<(u32, &str, Severity)> = errors.iter().map(|e| (e.pos.line, e.message.as_str(), e.severity)).collect();
    assert_eq!(found, [(13, "EXIT PERFORM must be inside an inline PERFORM", Severity::Severe), (17, "EXIT PERFORM CYCLE must be inside an inline PERFORM", Severity::Severe)]);
}

#[test]
fn evaluate_true_also_thru_and_other() {
    let out = run(&program(
        "",
        "       01  N PIC 99 VALUE 15.\n          88 TEENS VALUE 13 THRU 19.\n       01  C PIC X VALUE 'B'.\n          88 IS-B VALUE 'B'.\n",
        &[
            line("EVALUATE N ALSO C"),
            line("  WHEN 1 THRU 9 ALSO ANY DISPLAY 'LOW'"),
            line("  WHEN 10 THRU 20 ALSO 'A'"),
            line("  WHEN 10 THRU 20 ALSO 'B' DISPLAY 'MID-AB'"),
            line("  WHEN OTHER DISPLAY 'OTHER'"),
            line("END-EVALUATE"),
            line("EVALUATE TRUE"),
            line("  WHEN N > 20 DISPLAY 'BIG'"),
            line("  WHEN IS-B AND TEENS DISPLAY 'B'"),
            line("END-EVALUATE"),
            line("EVALUATE N WHEN NOT 15 DISPLAY 'NOT' WHEN OTHER DISPLAY 'IS'"),
            line("END-EVALUATE"),
            line("PERFORM VARYING N FROM 1 BY 1 UNTIL N > 9"),
            line("  IF N = 2 EXIT PERFORM CYCLE END-IF"),
            line("  IF N = 4 EXIT PERFORM END-IF"),
            line("  DISPLAY N"),
            line("END-PERFORM"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "MID-AB\nB\nIS\n01\n03\n");
}

#[test]
fn edited_receivers_and_de_editing() {
    let out = run(&program(
        "",
        "       01  AMT PIC S9(5)V99 COMP-3 VALUE -1234.5.\n       01  E PIC $$,$$9.99CR.\n       01  Z PIC ZZ9 BLANK WHEN ZERO.\n       01  BACK PIC S9(5)V99.\n       01  D PIC XXBXX.\n",
        &[
            line("MOVE AMT TO E"),
            line("MOVE 0 TO Z"),
            line("DISPLAY '[' E '][' Z ']'"),
            line("MOVE E TO BACK"),
            line("COMPUTE E ROUNDED = BACK / 3"),
            line("MOVE 'ABCD' TO D"),
            line("DISPLAY BACK ' ' E ' ' D"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "[$1,234.50CR][   ]\n012345}   $411.50CR AB CD\n");
}

#[test]
fn a_rounded_quotient_keeps_the_digit_rounding_reads() {
    let out = run(&program(
        "",
        "       01  DIV2 PIC 99V9 VALUE 44.1.\n       01  DIV3 PIC 9(4)V9 VALUE 1661.7.\n       01  I PIC 99 VALUE 2.\n       01  C PIC 99V9.\n       01  T PIC 99V9.\n       01  Q PIC 9V9.\n       01  R PIC 9V99.\n",
        &[
            line("DIVIDE DIV2 INTO DIV3 ROUNDED"),
            line("DIVIDE 4 INTO I ROUNDED"),
            line("COMPUTE C ROUNDED = 1661.7 / DIV2"),
            line("COMPUTE T = 1661.7 / DIV2"),
            line("DIVIDE 3 INTO 2 GIVING Q ROUNDED REMAINDER R"),
            line("DISPLAY DIV3 ' ' I ' ' C ' ' T ' ' Q ' ' R"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "00377 01 377 376 07 020\n");
}

fn file_program(select: &str, fd: &str, data: &str, procedure: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. F.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n{select}       DATA DIVISION.\n       FILE SECTION.\n{fd}       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{procedure}"
    )
}

fn run_files(source: &str, dds: &[String]) -> (String, String, Result<Ending, Abend>) {
    let o = Harness::source(source).dds(dds).run(Executor::Interpreter);
    (o.out, o.err, o.ending)
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ironwork-{}-{name}", std::process::id()))
}

#[test]
fn a_text_file_copied_record_by_record() {
    let (input, output) = (temp("in.txt"), temp("out.txt"));
    std::fs::write(&input, "alpha\nbeta gamma\n").unwrap();
    let source = file_program(
        "           SELECT IN-F ASSIGN TO UT-S-INDD\n               ORGANIZATION IS LINE SEQUENTIAL\n               FILE STATUS IS IN-STATUS.\n           SELECT OUT-F ASSIGN TO OUTDD\n               ORGANIZATION LINE SEQUENTIAL.\n",
        "       FD  IN-F.\n       01  IN-REC PIC X(20).\n       FD  OUT-F.\n       01  OUT-REC PIC X(24).\n",
        "       01  IN-STATUS PIC XX.\n       01  EOF PIC X VALUE 'N'.\n       01  N PIC 9 VALUE 0.\n",
        &[
            line("OPEN INPUT IN-F OUTPUT OUT-F"),
            line("PERFORM UNTIL EOF = 'Y'"),
            line("    READ IN-F AT END MOVE 'Y' TO EOF"),
            line("    NOT AT END ADD 1 TO N"),
            line("        MOVE IN-REC TO OUT-REC"),
            line("        WRITE OUT-REC"),
            line("    END-READ"),
            line("END-PERFORM"),
            line("DISPLAY N ' ' IN-STATUS"),
            line("CLOSE IN-F OUT-F"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let dds = [format!("INDD={}", input.display()), format!("OUTDD={}", output.display())];
    let (out, err, ending) = run_files(&source, &dds);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "2 10\n");
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "alpha\nbeta gamma\n");
}

#[test]
fn close_reel_and_no_rewind_find_no_reel_and_close_with_lock_refuses_a_later_open() {
    let data = temp("close.dat");
    let source = file_program(
        "           SELECT F ASSIGN TO FDD FILE STATUS IS FS.\n",
        "       FD  F.\n       01  F-REC PIC X(4).\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT F"),
            line("WRITE F-REC FROM 'ONE'"),
            line("CLOSE F REEL"),
            line("DISPLAY FS"),
            line("WRITE F-REC FROM 'TWO'"),
            line("CLOSE F WITH NO REWIND"),
            line("DISPLAY FS"),
            line("OPEN INPUT F"),
            line("CLOSE F LOCK"),
            line("DISPLAY FS"),
            line("OPEN INPUT F"),
            line("DISPLAY FS"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("FDD={}", data.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "07\n07\n00\n38\n");
    assert_eq!(std::fs::read(&data).unwrap().len(), 8);
    let keyed = file_program(
        "           SELECT X ASSIGN TO XDD ORGANIZATION INDEXED\n               RECORD KEY IS X-KEY.\n",
        "       FD  X.\n       01  X-REC.\n           05 X-KEY PIC X.\n",
        "",
        &line("CLOSE X UNIT."),
    );
    assert!(compile_errors(&keyed).contains("CLOSE X: REEL, UNIT and NO REWIND are not valid for an indexed or relative file"));
}

#[test]
fn open_takes_no_rewind_with_or_without_with_and_reversed() {
    let data = temp("norewind.dat");
    let source = file_program(
        "           SELECT F ASSIGN TO FDD FILE STATUS IS FS.\n",
        "       FD  F.\n       01  F-REC PIC X(4).\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT F NO REWIND"),
            line("WRITE F-REC FROM 'ONE'"),
            line("CLOSE F"),
            line("OPEN INPUT F WITH NO REWIND"),
            line("READ F"),
            line("DISPLAY F-REC FS"),
            line("CLOSE F"),
            line("OPEN INPUT F REVERSED"),
            line("DISPLAY FS"),
            line("CLOSE F"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("FDD={}", data.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "ONE 00\n00\n");
}

#[test]
fn fixed_and_variable_records_are_ebcdic_bytes() {
    let (fixed, variable) = (temp("f.dat"), temp("v.dat"));
    let source = file_program(
        "           SELECT F-F ASSIGN TO FDD.\n           SELECT V-F ASSIGN TO VDD.\n",
        "       FD  F-F RECORDING MODE IS F.\n       01  F-REC PIC X(4).\n       FD  V-F RECORDING MODE IS V RECORD VARYING FROM 1 TO 6.\n       01  V-SHORT PIC X(2).\n       01  V-LONG PIC X(6).\n",
        "",
        &[
            line("OPEN OUTPUT F-F V-F"),
            line("WRITE F-REC FROM 'AB'"),
            line("WRITE V-SHORT FROM 'XY'"),
            line("WRITE V-LONG FROM 'LONGER'"),
            line("CLOSE F-F V-F"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let dds = [format!("FDD={}", fixed.display()), format!("VDD={}", variable.display())];
    let (_, err, ending) = run_files(&source, &dds);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(std::fs::read(&fixed).unwrap(), [0xC1, 0xC2, 0x40, 0x40]);
    assert_eq!(std::fs::read(&variable).unwrap(), [0, 6, 0, 0, 0xE7, 0xE8, 0, 10, 0, 0, 0xD3, 0xD6, 0xD5, 0xC7, 0xC5, 0xD9]);
}

/// Variable-length records behind their RDWs: `n` bytes of `fill` each.
fn rdw_records(records: &[(usize, u8)]) -> Vec<u8> {
    records.iter().flat_map(|&(n, fill)| [((n + 4) as u16).to_be_bytes().as_slice(), &[0, 0], &vec![fill; n]].concat()).collect()
}

/// Table 52's file (Programming Guide SC27-8714-03, pp. 423-424), RECORD VARYING FROM 10 TO 80
/// with level-01 records of 20 and 50 bytes, read `reads` times INTO a 90-byte item under the CBL
/// card given; each READ shows its status and how much of the item the record filled.
fn table_52(card: &str, select: &str, reads: usize) -> String {
    let record = |name: &str, size: usize| format!("       01  {name}.\n           02 {name}-KEY PIC X(4).\n           02 PIC X({}).\n", size - 4);
    let source = file_program(
        &format!("           SELECT V-FILE ASSIGN TO MYDD{select}\n               FILE STATUS IS FS.\n"),
        &[
            "       FD  V-FILE\n           BLOCK CONTAINS 0 RECORDS\n           RECORD VARYING IN SIZE FROM 10 TO 80\n           RECORDING MODE V.\n",
            &record("REC-20", 20),
            &record("REC-50", 50),
        ]
        .concat(),
        "       01  FS PIC XX.\n       01  W PIC X(90).\n       01  L PIC 99.\n",
        &[
            line("OPEN INPUT V-FILE"),
            line(&format!("PERFORM {reads} TIMES")),
            line("    MOVE SPACES TO W"),
            line("    READ V-FILE NEXT INTO W"),
            line("    COMPUTE L = FUNCTION LENGTH(FUNCTION TRIM(W TRAILING))"),
            line("    DISPLAY FS ' ' L"),
            line("END-PERFORM"),
            line("CLOSE V-FILE"),
            line("GOBACK."),
        ]
        .concat(),
    );
    format!("{card}{source}")
}

#[test]
fn a_variable_length_read_s_status_follows_table_52_under_each_vlr_setting() {
    let path = temp("table-52.dat");
    let lengths = [5, 15, 40, 70, 90];
    std::fs::write(&path, rdw_records(&lengths.map(|n| (n, 0xC1)))).unwrap();
    let dds = [format!("MYDD={}", path.display())];
    let read = |card: &str| {
        let (out, err, ending) = run_files(&table_52(card, "", lengths.len()), &dds);
        assert!(ending.is_ok(), "{ending:?} {err}");
        out
    };
    let standard = "04 05\n04 15\n00 40\n04 70\n04 80\n";
    assert_eq!(read(""), standard);
    assert_eq!(read("       CBL VLR(STANDARD)\n"), standard);
    assert_eq!(read("       CBL VLR(C)\n"), "04 05\n00 15\n00 40\n00 70\n04 80\n");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn an_indexed_file_of_variable_length_records_reads_by_table_52_too() {
    let path = temp("table-52.ksds");
    let records: Vec<u8> = [(5, b'1'), (15, b'2'), (40, b'3'), (70, b'4')]
        .iter()
        .flat_map(|&(n, key)| {
            let record = [vec![0xF0, 0xF0, 0xF0, 0xF0 | (key - b'0')], vec![0xC1; n - 4]].concat();
            [((n + 4) as u16).to_be_bytes().as_slice(), &[0, 0], &record].concat()
        })
        .collect();
    std::fs::write(&path, records).unwrap();
    let dds = [format!("MYDD={}", path.display())];
    let select = "\n               ORGANIZATION INDEXED ACCESS DYNAMIC\n               RECORD KEY REC-20-KEY";
    let read = |card: &str| {
        let (out, err, ending) = run_files(&table_52(card, select, 4), &dds);
        assert!(ending.is_ok(), "{ending:?} {err}");
        out
    };
    assert_eq!(read("       CBL VLR(S)\n"), "04 05\n04 15\n00 40\n04 70\n");
    assert_eq!(read("       CBL VLR(COMPAT)\n"), "04 05\n00 15\n00 40\n00 70\n");
    std::fs::remove_file(&path).unwrap();
}

/// An indexed file on KDD and a relative file on RDD, both with FILE STATUS FS, a sequential file on
/// SDD, and `body` as the procedure, under the CBL `card`.
fn left_open_program(card: &str, declaratives: &str, body: &[&str]) -> String {
    let program = file_program(
        "           SELECT K-FILE ASSIGN TO KDD ORGANIZATION INDEXED\n               RECORD KEY K-KEY FILE STATUS IS FS.\n           SELECT R-FILE ASSIGN TO RDD ORGANIZATION RELATIVE\n               RELATIVE KEY R-NUM FILE STATUS IS FS.\n           SELECT S-FILE ASSIGN TO SDD.\n",
        "       FD  K-FILE.\n       01  K-REC.\n           05 K-KEY PIC X(4).\n       FD  R-FILE.\n       01  R-REC PIC X(4).\n       FD  S-FILE.\n       01  S-REC PIC X(4).\n",
        "       01  FS PIC XX.\n       01  R-NUM PIC 9(4).\n       01  N PIC S9(3) COMP-3 VALUE 1.\n       01  Z PIC S9(3) COMP-3 VALUE 0.\n       01  ABCODE PIC S9(9) BINARY VALUE 999.\n       01  CLEAN-0 PIC S9(9) BINARY VALUE 0.\n       01  CLEAN-1 PIC S9(9) BINARY VALUE 1.\n",
        &[declaratives, &body.iter().map(|l| line(l)).collect::<String>()].concat(),
    );
    format!("{card}{program}")
}

/// The data sets of [`left_open_program`], which `reset` empties of records and marks.
struct LeftOpen {
    paths: [std::path::PathBuf; 3],
}

impl LeftOpen {
    fn new(name: &str) -> Self {
        let paths = ["ksds", "rrds", "seq"].map(|kind| temp(&format!("{name}.{kind}")));
        let it = Self { paths };
        it.reset();
        it
    }

    fn dds(&self) -> Vec<String> {
        ["KDD", "RDD", "SDD"].iter().zip(&self.paths).map(|(dd, p)| format!("{dd}={}", p.display())).collect()
    }

    fn reset(&self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(files::open_mark(p));
        }
    }

    /// Which of the indexed, relative and sequential data sets are marked open for output.
    fn marked(&self) -> [bool; 3] {
        self.paths.each_ref().map(|p| files::open_mark(p).exists())
    }

    /// Writes a record to each file, then ends as `end` says, with `parm` as the job step's PARM;
    /// the interpreter and the VM must agree.
    fn write(&self, end: &str, parm: &str) -> Result<Ending, Abend> {
        let body = ["OPEN OUTPUT K-FILE R-FILE S-FILE", "MOVE 'K001' TO K-KEY", "WRITE K-REC", "MOVE 1 TO R-NUM", "WRITE R-REC FROM 'R001'", "WRITE S-REC FROM 'S001'", end, "GOBACK."];
        let source = left_open_program("", "", &body);
        self.reset();
        let walker = Harness::source(&source).dds(&self.dds()).parm(parm).run(Executor::Interpreter);
        let marked = self.marked();
        self.reset();
        let vm = Harness::source(&source).dds(&self.dds()).parm(parm).run(Executor::Vm);
        assert_eq!((&vm.out, &vm.ending, self.marked()), (&walker.out, &walker.ending, marked), "{}", walker.err);
        walker.ending
    }
}

/// An ending without Language Environment's termination activities leaves a VSAM data set marked
/// open: a program check under TRAP(OFF), and CEE3ABD without clean-up or under TRAP(OFF)
/// (assumptions L6 and C152).
#[test]
fn only_an_ending_without_termination_activities_leaves_a_vsam_data_set_open() {
    let data = LeftOpen::new("left-open");
    let cases: [(&str, &str, Option<&str>, [bool; 3]); 9] = [
        ("CLOSE K-FILE R-FILE S-FILE", "", None, [false; 3]),
        ("CONTINUE", "", None, [false; 3]),
        ("DIVIDE Z INTO N", "", Some("S0CB"), [false; 3]),
        ("CLOSE K-FILE R-FILE S-FILE DIVIDE Z INTO N", "/TRAP(OFF)", Some("S0CB"), [false; 3]),
        ("READ S-FILE", "/TRAP(OFF)", Some("U4038"), [false; 3]),
        ("DIVIDE Z INTO N", "/TRAP(OFF)", Some("S0CB"), [true, true, false]),
        ("CALL 'CEE3ABD' USING ABCODE CLEAN-0", "", Some("U0999"), [true, true, false]),
        ("CALL 'CEE3ABD' USING ABCODE CLEAN-1", "", Some("U0999"), [false; 3]),
        ("CALL 'CEE3ABD' USING ABCODE CLEAN-1", "/TRAP(OFF)", Some("U0999"), [true, true, false]),
    ];
    for (end, parm, abend, marked) in cases {
        let ending = data.write(end, parm);
        assert_eq!(ending.as_ref().err().map(|a| a.code.to_string()), abend.map(str::to_string), "{end} {parm}: {ending:?}");
        assert_eq!(data.marked(), marked, "{end} {parm}");
    }
    let records = std::fs::read(&data.paths[0]).unwrap();
    assert_eq!(records, ebcdic("K001"), "the records are written as CLOSE writes them");
    data.reset();
}

#[test]
fn the_next_open_of_a_data_set_left_open_is_97_under_vsamopenfs_compat_and_00_under_succ() {
    let data = LeftOpen::new("verified");
    let reopen = [
        "OPEN INPUT K-FILE",
        "DISPLAY FS",
        "CLOSE K-FILE",
        "OPEN INPUT R-FILE",
        "DISPLAY FS",
        "CLOSE R-FILE",
        "OPEN I-O K-FILE",
        "DISPLAY FS",
        "CLOSE K-FILE",
        "OPEN INPUT K-FILE",
        "DISPLAY FS",
        "READ K-FILE",
        "DISPLAY FS ' ' K-KEY",
        "CLOSE K-FILE",
        "OPEN INPUT S-FILE",
        "CLOSE S-FILE",
        "GOBACK.",
    ];
    for (card, verified) in [("", "97"), ("       CBL VSAMOPENFS(COMPAT)\n", "97"), ("       CBL VSAMOPENFS(SUCC)\n", "00"), ("       CBL VS(S)\n", "00")] {
        assert_eq!(data.write("DIVIDE Z INTO N", "/TRAP(OFF)").unwrap_err().code, "S0CB");
        let o = Harness::source(&left_open_program(card, "", &reopen)).dds(&data.dds()).run(Executor::Interpreter);
        assert!(o.ending.is_ok(), "{card}: {:?} {}", o.ending, o.err);
        assert_eq!(o.out, format!("{verified}\n{verified}\n{verified}\n00\n00 K001\n"), "{card}");
        assert_eq!(data.marked(), [false, true, false], "{card}: only CLOSE after an OPEN for output takes the mark away");
    }
    data.reset();
}

#[test]
fn status_97_runs_the_error_procedure_and_with_no_file_status_does_not_end_the_run() {
    let data = LeftOpen::new("verified-declarative");
    let declaratives = "       DECLARATIVES.\n       K-ERR SECTION.\n           USE AFTER ERROR PROCEDURE ON K-FILE.\n       K-ERR-1.\n           DISPLAY 'ERROR PROCEDURE ' FS.\n       END DECLARATIVES.\n       MAIN SECTION.\n       M.\n";
    let reopen = ["OPEN I-O K-FILE", "DISPLAY 'OPEN ' FS", "CLOSE K-FILE", "GOBACK."];
    for (card, declaratives, out) in [
        ("", declaratives, "ERROR PROCEDURE 97\nOPEN 97\n"),
        ("       CBL VSAMOPENFS(SUCC)\n", declaratives, "OPEN 00\n"),
    ] {
        assert_eq!(data.write("DIVIDE Z INTO N", "/TRAP(OFF)").unwrap_err().code, "S0CB");
        let o = Harness::source(&left_open_program(card, declaratives, &reopen)).dds(&data.dds()).run(Executor::Interpreter);
        assert!(o.ending.is_ok(), "{card}: {:?} {}", o.ending, o.err);
        assert_eq!(o.out, out, "{card}");
    }
    let unhandled = file_program(
        "           SELECT K-FILE ASSIGN TO KDD ORGANIZATION INDEXED\n               RECORD KEY K-KEY.\n",
        "       FD  K-FILE.\n       01  K-REC.\n           05 K-KEY PIC X(4).\n",
        "",
        &[line("OPEN INPUT K-FILE"), line("READ K-FILE NEXT"), line("DISPLAY K-KEY"), line("GOBACK.")].concat(),
    );
    assert_eq!(data.write("DIVIDE Z INTO N", "/TRAP(OFF)").unwrap_err().code, "S0CB");
    let o = Harness::source(&unhandled).dds(&data.dds()).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?} {}", o.ending, o.err);
    assert_eq!(o.out, "K001\n");
    data.reset();
}

#[test]
fn file_status_codes_and_optional_files() {
    let source = file_program(
        "           SELECT OPTIONAL MAYBE ASSIGN TO NODD FILE STATUS S1.\n           SELECT MUST ASSIGN TO NODD2 FILE STATUS S2.\n",
        "       FD  MAYBE.\n       01  M-REC PIC X.\n       FD  MUST.\n       01  R-REC PIC X.\n",
        "       01  S1 PIC XX.\n       01  S2 PIC XX.\n",
        &[
            line("OPEN INPUT MAYBE"),
            line("DISPLAY S1"),
            line("READ MAYBE AT END DISPLAY 'END ' S1 END-READ"),
            line("OPEN INPUT MUST"),
            line("DISPLAY S2"),
            line("READ MUST END-READ"),
            line("DISPLAY S2"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "05\nEND 10\n35\n47\n");
    let missing = [format!("NODD={}", temp("no-such-data-set").display()), format!("NODD2={}", temp("no-such-data-set").display())];
    let (out, err, ending) = run_files(&source, &missing);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "05\nEND 10\n35\n47\n");
}

/// An OPEN or CLOSE of a VSAM file returns control with no FILE STATUS or declarative to take its
/// failure (assumption C451); the READ of the file it left closed is a logic error, IGZ0020S.
#[test]
fn a_vsam_open_returns_control_and_the_read_after_it_ends_with_igz0020s() {
    let select = "           SELECT K-F ASSIGN TO NODD\n               ORGANIZATION IS INDEXED\n               RECORD KEY IS K-KEY.\n";
    let fd = "       FD  K-F.\n       01  K-REC.\n           05 K-KEY PIC X(4).\n";
    let source = file_program(select, fd, "", &[line("OPEN INPUT K-F"), line("DISPLAY 'AFTER OPEN'"), line("READ K-F"), line("GOBACK.")].concat());
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let o = Harness::source(&source).run(executor);
        assert_eq!(o.out, "AFTER OPEN\n", "{name} {}", o.err);
        let abend = o.ending.unwrap_err();
        assert!(abend.code == "U4038" && abend.message.starts_with("IGZ0020S A logic error occurred.") && abend.message.contains("The status code was 47."), "{name} {abend:?}");
    }
}

#[test]
fn an_unhandled_io_failure_ends_the_run() {
    let source = file_program("           SELECT X-F ASSIGN TO NODD.\n", "       FD  X-F.\n       01  X-REC PIC X.\n", "", &[line("OPEN INPUT X-F"), line("GOBACK.")].concat());
    let (_, _, ending) = run_files(&source, &[]);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "U4038");
    assert!(abend.message.starts_with("IGZ0035S There was an unsuccessful OPEN or CLOSE of file X-F in program "), "{}", abend.message);
    assert!(abend.message.contains("The status code was 35.") && abend.message.contains("--dd NODD=path"), "{}", abend.message);
}

fn ebcdic_text(bytes: &[u8]) -> String {
    zarch::ebcdic::CodePage::by_ccsid(1140).unwrap().decode(bytes)
}

#[test]
fn indexed_files_by_prime_and_alternate_keys() {
    let path = temp("ksds.dat");
    let _ = std::fs::remove_file(&path);
    let source = file_program(
        "           SELECT CUST ASSIGN TO CUSTDD ORGANIZATION IS INDEXED\n               ACCESS MODE IS DYNAMIC RECORD KEY IS C-ID\n               ALTERNATE RECORD KEY IS C-CITY WITH DUPLICATES\n               FILE STATUS IS FS.\n",
        "       FD  CUST.\n       01  C-REC.\n           05 C-ID PIC X(3).\n           05 C-CITY PIC X(4).\n           05 C-NAME PIC X(5).\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT CUST"),
            line("MOVE '003PERTCAROL' TO C-REC"),
            line("WRITE C-REC"),
            line("DISPLAY FS"),
            line("MOVE '001SYDNALICE' TO C-REC"),
            line("WRITE C-REC"),
            line("DISPLAY FS"),
            line("MOVE '002PERTBOBBY' TO C-REC"),
            line("WRITE C-REC"),
            line("DISPLAY FS"),
            line("MOVE '001XXXXXXXXX' TO C-REC"),
            line("WRITE C-REC INVALID KEY DISPLAY 'DUP ' FS END-WRITE"),
            line("CLOSE CUST"),
            line("OPEN I-O CUST"),
            line("MOVE '002' TO C-ID"),
            line("READ CUST INVALID KEY DISPLAY 'NF' END-READ"),
            line("DISPLAY C-NAME"),
            line("READ CUST NEXT"),
            line("DISPLAY C-ID"),
            line("READ CUST NEXT AT END DISPLAY 'EOF ' FS END-READ"),
            line("READ CUST NEXT"),
            line("DISPLAY FS"),
            line("MOVE '000PERTZED' TO C-REC"),
            line("WRITE C-REC"),
            line("DISPLAY FS"),
            line("MOVE 'PERT' TO C-CITY"),
            line("START CUST KEY IS = C-CITY"),
            line("PERFORM 3 TIMES"),
            line("    READ CUST NEXT"),
            line("    DISPLAY C-ID ' ' FS"),
            line("END-PERFORM"),
            line("MOVE '002' TO C-ID"),
            line("READ CUST"),
            line("MOVE 'BRIS' TO C-CITY"),
            line("REWRITE C-REC"),
            line("DISPLAY FS"),
            line("MOVE '001' TO C-ID"),
            line("DELETE CUST"),
            line("MOVE '009' TO C-ID"),
            line("DELETE CUST INVALID KEY DISPLAY 'NO ' FS END-DELETE"),
            line("CLOSE CUST"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("CUSTDD={}", path.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "00\n00\n02\nDUP 22\nBOBBY\n003\nEOF 10\n46\n02\n002 02\n003 02\n000 00\n00\nNO 23\n");
    assert_eq!(ebcdic_text(&std::fs::read(&path).unwrap()), "000PERTZED  002BRISBOBBY003PERTCAROL");
}

#[test]
fn indexed_sequential_access_keeps_keys_in_order() {
    let path = temp("kseq.dat");
    let _ = std::fs::remove_file(&path);
    let source = file_program(
        "           SELECT K ASSIGN TO KDD ORGANIZATION INDEXED\n               RECORD KEY K-ID FILE STATUS FS.\n",
        "       FD  K.\n       01  K-REC.\n           05 K-ID PIC XX.\n           05 K-V PIC XX.\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT K"),
            line("WRITE K-REC FROM 'B1XX'"),
            line("WRITE K-REC FROM 'A1YY' INVALID KEY DISPLAY 'SEQ ' FS"),
            line("END-WRITE"),
            line("WRITE K-REC FROM 'C1ZZ'"),
            line("CLOSE K"),
            line("OPEN I-O K"),
            line("DELETE K"),
            line("DISPLAY FS"),
            line("READ K"),
            line("MOVE 'Q9' TO K-ID"),
            line("REWRITE K-REC INVALID KEY DISPLAY 'KEY ' FS END-REWRITE"),
            line("READ K"),
            line("MOVE 'QQ' TO K-V"),
            line("REWRITE K-REC"),
            line("DISPLAY FS"),
            line("READ K AT END DISPLAY 'END' END-READ"),
            line("CLOSE K"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("KDD={}", path.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "SEQ 21\n43\nKEY 21\n00\nEND\n");
    assert_eq!(ebcdic_text(&std::fs::read(&path).unwrap()), "B1XXC1QQ");
}

#[test]
fn relative_files_keep_empty_slots() {
    let path = temp("rrds.dat");
    let _ = std::fs::remove_file(&path);
    let source = file_program(
        "           SELECT REL ASSIGN TO RELDD ORGANIZATION IS RELATIVE\n               ACCESS MODE IS RANDOM RELATIVE KEY IS RK\n               FILE STATUS IS FS.\n           SELECT RSEQ ASSIGN TO RELDD ORGANIZATION RELATIVE\n               ACCESS SEQUENTIAL RELATIVE KEY RK2.\n",
        "       FD  REL.\n       01  R-REC PIC X(4).\n       FD  RSEQ.\n       01  S-REC PIC X(4).\n",
        "       01  RK PIC 9(4).\n       01  RK2 PIC 9(4).\n       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT REL"),
            line("MOVE 3 TO RK"),
            line("WRITE R-REC FROM 'CCCC'"),
            line("MOVE 1 TO RK"),
            line("WRITE R-REC FROM 'AAAA'"),
            line("MOVE 0 TO RK"),
            line("WRITE R-REC FROM 'ZZZZ' INVALID KEY DISPLAY 'BAD ' FS"),
            line("END-WRITE"),
            line("MOVE 9999 TO RK"),
            line("WRITE R-REC FROM 'ZZZZ'"),
            line("DISPLAY FS"),
            line("DELETE REL"),
            line("DISPLAY FS"),
            line("CLOSE REL"),
            line("OPEN I-O REL"),
            line("MOVE 2 TO RK"),
            line("READ REL INVALID KEY DISPLAY 'EMPTY ' FS END-READ"),
            line("WRITE R-REC FROM 'BBBB'"),
            line("MOVE 1 TO RK"),
            line("DELETE REL"),
            line("MOVE 9999 TO RK"),
            line("DELETE REL"),
            line("CLOSE REL"),
            line("OPEN INPUT RSEQ"),
            line("PERFORM 3 TIMES"),
            line("    READ RSEQ AT END DISPLAY 'END'"),
            line("    NOT AT END DISPLAY RK2 ' ' S-REC END-READ"),
            line("END-PERFORM"),
            line("CLOSE RSEQ"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("RELDD={}", path.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "BAD 24\n00\n49\nEMPTY 23\n0002 BBBB\n0003 CCCC\nEND\n");
    assert_eq!(std::fs::read(&path).unwrap(), [0, 0, 0, 0, 0xC2, 0xC2, 0xC2, 0xC2, 0xC3, 0xC3, 0xC3, 0xC3]);
}

#[test]
fn a_sequential_file_opened_i_o_is_rewritten_in_place() {
    let path = temp("seqio.txt");
    std::fs::write(&path, "abc\ndef\n").unwrap();
    let source = file_program(
        "           SELECT SEQ ASSIGN TO SEQDD FILE STATUS IS FS.\n",
        "       FD  SEQ.\n       01  S-REC PIC X(3).\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN I-O SEQ"),
            line("READ SEQ"),
            line("REWRITE S-REC FROM 'XYZ'"),
            line("DISPLAY FS"),
            line("REWRITE S-REC"),
            line("DISPLAY FS"),
            line("WRITE S-REC"),
            line("DISPLAY FS"),
            line("READ SEQ"),
            line("DISPLAY S-REC"),
            line("READ SEQ AT END DISPLAY 'END' END-READ"),
            line("CLOSE SEQ"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("SEQDD={}:text", path.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "00\n43\n48\ndef\nEND\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "XYZ\ndef\n");
}

#[test]
fn an_indexed_file_loads_from_a_text_dd_and_an_unhandled_invalid_key_abends() {
    let path = temp("ksds.txt");
    std::fs::write(&path, "01ONE\n02TWO\n").unwrap();
    let source = file_program(
        "           SELECT K ASSIGN TO KDD ORGANIZATION INDEXED\n               ACCESS RANDOM RECORD KEY K-ID.\n",
        "       FD  K.\n       01  K-REC.\n           05 K-ID PIC XX.\n           05 K-V PIC X(3).\n",
        "",
        &[line("OPEN INPUT K"), line("MOVE '02' TO K-ID"), line("READ K"), line("DISPLAY K-V"), line("MOVE '07' TO K-ID"), line("READ K"), line("GOBACK.")].concat(),
    );
    let (out, _, ending) = run_files(&source, &[format!("KDD={}:text", path.display())]);
    assert_eq!(out, "TWO\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "U4038");
    assert!(abend.message.starts_with("Neither an INVALID KEY phrase, FILE STATUS nor a declarative was specified for file K in program F. The status code was 23."), "{}", abend.message);
    assert!(abend.message.contains("no record with that key"), "{}", abend.message);
}

/// A READ at the end of a file with no AT END phrase, FILE STATUS or declarative ends the run U4038
/// on both executors: the Programming Guide's flow figures terminate the run unit with a message,
/// and IBM names none (assumption C451).
#[test]
fn an_at_end_no_phrase_takes_ends_the_run_u4038() {
    let path = temp("at-end.txt");
    std::fs::write(&path, "ONE\n").unwrap();
    let source = file_program(
        "           SELECT S ASSIGN TO SDD.\n",
        "       FD  S.\n       01  S-REC PIC X(3).\n",
        "",
        &[line("OPEN INPUT S"), line("READ S"), line("DISPLAY S-REC"), line("READ S"), line("DISPLAY 'NOT HERE'"), line("GOBACK.")].concat(),
    );
    let dds = [format!("SDD={}:text", path.display())];
    let walker = Harness::source(&source).dds(&dds).run(Executor::Interpreter);
    let vm = Harness::source(&source).dds(&dds).run(Executor::Vm);
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending));
    let abend = walker.ending.unwrap_err();
    assert_eq!((walker.out.as_str(), abend.code.to_string()), ("ONE\n", "U4038".to_owned()));
    assert!(abend.message.starts_with("Neither an AT END phrase, FILE STATUS nor a declarative was specified for file S in program F. The status code was 10."), "{}", abend.message);
}

#[test]
fn file_statements_are_checked_against_the_organization() {
    let errors = |select: &str, fd: &str, data: &str, body: &str| {
        let parsed = syntax::parse(&file_program(select, fd, data, &line(body))).unwrap_or_else(|e| panic!("{e}"));
        compile(parsed, &[]).err().map(|e| e[0].message.clone()).unwrap_or_default()
    };
    let seq = "           SELECT F ASSIGN TO FDD.\n";
    let fd = "       FD  F.\n       01  F-REC.\n           05 F-K PIC XX.\n           05 F-V PIC XX.\n";
    let ksds = "           SELECT F ASSIGN TO FDD ORGANIZATION INDEXED\n               RECORD KEY F-K.\n";
    assert!(errors(seq, fd, "", "START F.").contains("not an indexed or relative file"));
    assert!(errors(seq, fd, "", "DELETE F.").contains("not an indexed or relative file"));
    assert!(errors(ksds, fd, "", "START F KEY < F-K.").contains("START KEY takes"));
    assert!(errors(ksds, fd, "", "READ F KEY IS F-V.").contains("F-V: not a key of F"));
    assert!(errors(ksds, fd, "       01  W PIC XX.\n", "READ F KEY IS W.").contains("not a key of F"));
    assert_eq!(errors(ksds, fd, "", "START F KEY >= F-K."), "");
    let no_key = "           SELECT F ASSIGN TO FDD ORGANIZATION INDEXED.\n";
    assert!(errors(no_key, fd, "", "GOBACK.").contains("needs a RECORD KEY"));
    let outside = "           SELECT F ASSIGN TO FDD ORGANIZATION INDEXED\n               RECORD KEY W.\n";
    assert!(errors(outside, fd, "       01  W PIC XX.\n", "GOBACK.").contains("must be in its records"));
    let rrds = "           SELECT F ASSIGN TO FDD ORGANIZATION RELATIVE\n               ACCESS RANDOM.\n";
    assert!(errors(rrds, fd, "", "GOBACK.").contains("needs a RELATIVE KEY"));
    let random = "           SELECT F ASSIGN TO FDD ORGANIZATION INDEXED\n               ACCESS RANDOM RECORD KEY F-K.\n";
    assert!(errors(random, fd, "", "START F.").contains("ACCESS MODE is RANDOM"));
    assert!(errors(random, fd, "", "READ F NEXT.").contains("ACCESS MODE is RANDOM"));
}

#[test]
fn exec_sql_is_checked_and_stops_the_run_when_reached() {
    let data = [
        "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
        "           EXEC SQL BEGIN DECLARE SECTION END-EXEC.\n",
        "       01  CUST.\n           05 CUST-ID PIC 9(6).\n           05 CUST-NAME PIC X(30).\n",
        "       01  NAME-IND PIC S9(4) COMP-5.\n",
        "           EXEC SQL END DECLARE SECTION END-EXEC.\n",
    ]
    .concat();
    let body = [
        line("EXEC SQL WHENEVER SQLERROR CONTINUE END-EXEC"),
        line("DISPLAY 'BEFORE'"),
        line("EXEC SQL SELECT NAME INTO :CUST.CUST-NAME :NAME-IND"),
        line("    FROM CUSTOMER WHERE ID = :CUST-ID AND X = ':NOTAVAR'"),
        line("END-EXEC"),
        line("IF SQLCODE NOT = 0 DISPLAY SQLSTATE END-IF"),
        line("GOBACK."),
    ]
    .concat();
    let (out, _, ending) = run_with(&program("", &data, &body), &[]);
    assert_eq!(out, "BEFORE\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "EXEC");
    assert!(abend.message.contains("EXEC SQL SELECT"), "{}", abend.message);
    let bad = program("", &data, &[line("EXEC SQL SELECT A INTO :NOPE FROM T END-EXEC"), line("GOBACK.")].concat());
    let errors = compile(syntax::parse(&bad).unwrap(), &[]).err().unwrap();
    assert!(errors[0].message.contains("NOPE is not defined"));
}

#[test]
fn exec_cics_gets_the_translators_additions_and_checks_its_arguments() {
    let data = "       01  WS-AREA PIC X(20).\n       01  WS-RESP PIC S9(8) COMP.\n           COPY DFHAID.\n       LINKAGE SECTION.\n       01  DFHCOMMAREA PIC X(20).\n";
    let body = [
        line("IF EIBCALEN = 0 AND EIBAID = DFHENTER CONTINUE END-IF"),
        line("EXEC CICS HANDLE CONDITION NOTFND(NOT-FOUND) END-EXEC"),
        line("EXEC CICS LINK PROGRAM('SUBPGM') COMMAREA(WS-AREA)"),
        line("     LENGTH(LENGTH OF WS-AREA) RESP(WS-RESP) END-EXEC"),
        line("IF WS-RESP NOT = DFHRESP(NORMAL) DISPLAY 'FAILED' END-IF"),
        line("EXEC CICS RETURN END-EXEC."),
        "       NOT-FOUND.\n".to_owned(),
        line("GOBACK."),
    ]
    .concat();
    let parsed = syntax::parse(&program("", data, &body)).unwrap();
    assert_eq!(parsed.using.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["DFHEIBLK", "DFHCOMMAREA"]);
    let Stmt::Exec(link) = &parsed.paragraphs[0].statements[2] else { panic!("{:?}", parsed.paragraphs[0].statements[2]) };
    assert_eq!(link.command, "LINK");
    assert_eq!(link.options.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["PROGRAM", "COMMAREA", "LENGTH", "RESP"]);
    assert!(compile(parsed, &[]).is_ok());
    let bad = program("", data, &[line("EXEC CICS LINK PROGRAM('X') COMMAREA(MISSING) END-EXEC"), line("GOBACK.")].concat());
    let errors = compile(syntax::parse(&bad).unwrap(), &[]).err().unwrap();
    assert!(errors[0].message.contains("MISSING is not defined"));
}

#[test]
fn an_invalid_packed_sign_abends_with_s0c7() {
    let (_, _, ending) = run_with(
        &program("", "       01  G.\n           05 P PIC S9(3) COMP-3.\n           05 PX REDEFINES P PIC X(2).\n", &[line("MOVE X'1234' TO PX"), line("ADD 1 TO P"), line("GOBACK.")].concat()),
        &[],
    );
    assert_eq!(ending.unwrap_err().code, "S0C7");
}

#[test]
fn trunc_opt_reports_unless_silent() {
    let source = program("TRUNC(OPT)", "       01  B PIC 9(4) COMP.\n       01  V PIC 9(5) VALUE 12345.\n", &[line("COMPUTE B = V"), line("DISPLAY B"), line("GOBACK.")].concat());
    let (out, err, _) = run_with(&source, &[]);
    assert_eq!(out, "2345\n");
    assert!(err.contains("TRUNC(OPT)") && err.contains("12345"), "{err}");
    let (_, silent, _) = run_with(&source, &["-silent"]);
    assert!(silent.is_empty());
}

#[test]
fn display_shows_the_whole_binary_value_under_trunc_bin_and_for_comp_5() {
    let data = "       01  B PIC S9(4) BINARY.\n       01  C PIC S9(4) COMP-5.\n       01  U PIC 9(8) COMP-5.\n";
    let body = [line("MOVE 12345 TO B C"), line("MOVE 123456789 TO U"), line("DISPLAY B ' ' C ' ' U"), line("GOBACK.")].concat();
    assert_eq!(run(&program("TRUNC(BIN)", data, &body)), "12345 12345 0123456789\n");
    assert_eq!(run(&program("TRUNC(STD)", data, &body)), "2345 12345 0123456789\n");
}

/// The items of Table 48 (Programming Guide SC27-8714-03, p. 363).
const TABLE_48: &str = "       01  UB PIC 9(3) BINARY VALUE 111.\n       01  PB PIC S9(3) BINARY VALUE 111.\n       01  NB PIC S9(3) BINARY VALUE -111.\n       01  UPD PIC 9(3) COMP-3 VALUE 222.\n       01  PPD PIC S9(3) COMP-3 VALUE 222.\n       01  NPD PIC S9(3) COMP-3 VALUE -222.\n       01  UZ PIC 9(3) VALUE 333.\n       01  TP PIC S9(3) VALUE 333.\n       01  TN PIC S9(3) VALUE -333.\n       01  LP PIC S9(3) SIGN LEADING VALUE 333.\n       01  LN PIC S9(3) SIGN LEADING VALUE -333.\n";

#[test]
fn dispsign_sep_shows_a_signed_items_sign_before_its_digits_as_table_48_does() {
    let body = [line("DISPLAY UB ' ' PB ' ' NB"), line("DISPLAY UPD ' ' PPD ' ' NPD"), line("DISPLAY UZ ' ' TP ' ' TN ' ' LP ' ' LN"), line("GOBACK.")].concat();
    assert_eq!(run(&program("DISPSIGN(SEP)", TABLE_48, &body)), "111 +111 -111\n222 +222 -222\n333 +333 -333 +333 -333\n");
    assert_eq!(run(&program("DS(C)", TABLE_48, &body)), "111 111 11J\n222 222 22K\n333 33C 33L C33 L33\n");
    assert_eq!(run(&program("", TABLE_48, &body)), "111 111 11J\n222 222 22K\n333 33C 33L C33 L33\n");
}

#[test]
fn dispsign_sep_leaves_a_separate_sign_where_it_is_and_reads_a_zoned_sign_from_its_zone() {
    let data = "       01  SS PIC S9(3) SIGN TRAILING SEPARATE VALUE -333.\n       01  C5 PIC S9(4) COMP-5.\n       01  G.\n           05 BAD PIC S9(3).\n       01  F PIC S9(3)V99 COMP-3 VALUE -1.5.\n";
    let body = [line("MOVE 12345 TO C5"), line("MOVE '12 ' TO G"), line("DISPLAY SS ' ' C5 ' ' BAD ' ' F"), line("GOBACK.")].concat();
    assert_eq!(run(&program("DS(S)", data, &body)), "333- +12345 +120 -00150\n");
    assert_eq!(run(&program("DS(C)", data, &body)), "333- 12345 12  0015}\n");
}

#[test]
fn a_reference_cannot_leave_working_storage() {
    let (_, _, ending) = run_with(&program("", "       01  X PIC X(4).\n", &[line("MOVE 'A' TO X(1:4000)"), line("GOBACK.")].concat()), &[]);
    assert!(ending.unwrap_err().message.contains("outside the run unit's storage"));
}

#[test]
fn ssrange_catches_what_ibm_would_catch() {
    let data = "       01  G.\n           05 T PIC X OCCURS 3.\n       01  I PIC 9 VALUE 4.\n";
    let body = [line("MOVE 'A' TO T(I)"), line("GOBACK.")].concat();
    assert!(run_with(&program("", data, &body), &[]).2.is_ok());
    let abend = run_with(&program("SSRANGE", data, &body), &[]).2.unwrap_err();
    assert_eq!(abend.code, AbendCode::user(4038));
    assert!(abend.message.starts_with("IGZ0006S") && abend.message.contains("SSRANGE"), "{}", abend.message);
    assert!(run_with(&program("SSR(ZLEN)", data, &body), &[]).2.unwrap_err().message.contains("SSRANGE"));
    assert!(run_with(&program("SSR,NOSSR", data, &body), &[]).2.is_ok());
}

#[test]
fn ssrange_checks_the_address_the_subscripts_compose_against_the_whole_table() {
    let data = "       01  G.\n           05 R OCCURS 3.\n             10 E PIC X OCCURS 8.\n       01  I PIC 9.\n       01  J PIC 9.\n";
    for (i, j, inside) in [(2, 0, true), (1, 9, true), (3, 8, true), (3, 9, false), (1, 0, false)] {
        let body = [line(&format!("MOVE {i} TO I")), line(&format!("MOVE {j} TO J")), line("MOVE 'A' TO E(I, J)"), line("GOBACK.")].concat();
        for (executor, on) in [(Executor::Interpreter, "the interpreter"), (Executor::Vm, "the VM")] {
            let run = Harness::source(&program("SSRANGE", data, &body)).run(executor);
            match run.ending {
                Ok(_) => assert!(inside, "E({i}, {j}) on {on} ran"),
                Err(abend) => {
                    assert!(!inside, "E({i}, {j}) on {on}: {}", abend.message);
                    assert!(abend.message.starts_with("IGZ0006S the reference to E"), "{}", abend.message);
                }
            }
        }
    }
}

#[test]
fn a_load_module_carries_each_place_s_table_range_and_prints_it() {
    let data = "       01  G.\n           05 R OCCURS 3.\n             10 E PIC X OCCURS 8.\n       01  I PIC 9.\n       01  J PIC 9.\n";
    let body = [line("MOVE 'A' TO E(I, J)"), line("GOBACK.")].concat();
    let compiled = compile(syntax::parse(&program("SSRANGE", data, &body)).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let lowered = crate::lower::lower(&compiled).unwrap_or_else(|e| panic!("{e:?}"));
    let ranges: Vec<rt::lir::TableRange> = lowered.places.iter().filter_map(|p| p.table).collect();
    assert_eq!(ranges, [rt::lir::TableRange { displacement: 0, extent: 24 }]);
    let printed = rt::lir::Listing::of(&lowered).to_string();
    assert!(printed.contains(" table check 0+24"), "{printed}");
    let module = rt::module::read(&rt::module::write(std::slice::from_ref(&lowered))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(module.programs[0].places, lowered.places, "the LIR section's end carries them");
}

#[test]
fn ssrange_reference_modification_names_the_part_out_of_range() {
    let data = "       01  X PIC X(3).\n       01  S PIC S9.\n       01  L PIC S9.\n";
    for (start, length, id) in [(4, 1, "IGZ0072S"), (0, 1, "IGZ0072S"), (1, 0, "IGZ0073S"), (2, 3, "IGZ0074S")] {
        let body = [line(&format!("MOVE {start} TO S")), line(&format!("MOVE {length} TO L")), line("MOVE 'A' TO X(S:L)"), line("GOBACK.")].concat();
        let abend = run_with(&program("SSRANGE", data, &body), &[]).2.unwrap_err();
        assert_eq!(abend.code, AbendCode::user(4038));
        assert!(abend.message.starts_with(id), "({start}:{length}): {}", abend.message);
    }
}

#[test]
fn ssrange_finds_an_occurs_depending_on_object_past_its_maximum() {
    let data = "       01  N PIC 9 VALUE 4.\n       01  G.\n           05 T PIC X OCCURS 1 TO 3 DEPENDING ON N.\n";
    let body = [line("DISPLAY G"), line("GOBACK.")].concat();
    let abend = run_with(&program("SSRANGE", data, &body), &[]).2.unwrap_err();
    assert_eq!(abend.code, AbendCode::user(4038));
    assert!(abend.message.starts_with("IGZ0007S") && abend.message.contains("OCCURS DEPENDING ON"), "{}", abend.message);
}

#[test]
fn compile_errors_name_what_is_undefined() {
    let parsed = syntax::parse(&program("", "       01  X PIC X.\n", &line("MOVE Y TO X."))).unwrap();
    let errors = compile(parsed, &[]).err().unwrap();
    assert!(errors[0].message.contains("Y is not defined"));
    assert_eq!(errors[0].severity, Severity::Severe);
}

#[test]
fn exec_dli_is_checked_when_compiled_and_ends_the_run_when_reached() {
    let data = "       01  SSA PIC X(9).\n       01  AREA1 PIC X(80).\n       01  PCB-NUM PIC S9(4) COMP VALUE 1.\n       01  S PIC XX.\n";
    let compiled = |statements: &[&str]| {
        let body: String = statements.iter().map(|s| line(s)).chain([line("GOBACK.")]).collect();
        compile(syntax::parse(&program("", data, &body)).unwrap(), &[])
    };
    let errors = |statements: &[&str]| compiled(statements).err().map(|e| e.into_iter().map(|e| e.message).collect::<Vec<_>>()).unwrap_or_default();
    let good = ["MOVE DIBSTAT TO S", "EXEC DLI GET UNIQUE USING PCB(PCB-NUM)", "    SEGMENT(ROOT) INTO(AREA1) WHERE(KEY = SSA)", "END-EXEC", "EXEC DLI SCHD PSB((SSA)) NODHABEND END-EXEC"];
    assert!(errors(&good).is_empty(), "{:?}", errors(&good));
    let more = ["EXEC DLI STATISTICS USING PCB(1) INTO(AREA1)", "    VSAM FORMATTED LENGTH(360) END-EXEC", "EXEC DLI GMSG AIB(AREA1) WAITAOI END-EXEC", "EXEC DLI DELETE SEGMENT(ROOT) FROM(AREA1) END-EXEC"];
    assert!(errors(&more).is_empty(), "{:?}", errors(&more));
    assert_eq!(errors(&["EXEC DLI FETCH SEGMENT(ROOT) END-EXEC"]), ["EXEC DLI FETCH is not an EXEC DLI command"]);
    assert_eq!(errors(&["EXEC DLI TERM INTO(AREA1) END-EXEC"]), ["EXEC DLI TERM: INTO is not one of its options"]);
    assert_eq!(errors(&["EXEC DLI GU SEGMENT(ROOT) WHERE(KEY SSA) END-EXEC"]), ["EXEC DLI GU WHERE(KEY SSA): a relational operator after KEY"]);
    assert_eq!(errors(&["EXEC DLI GU SEGMENT(ROOT) INTO(NOWHERE) END-EXEC"]).len(), 1);
    assert_eq!(errors(&["EXEC DLI GU SEGMENT(ROOT) WHERE(KEY = NOWHERE) END-EXEC"]).len(), 1);
    let body: String = ["MOVE 'GB' TO DIBSTAT", "DISPLAY DIBSTAT ' ' LENGTH OF DLZDIB", "EXEC DLI GN SEGMENT(ROOT) INTO(AREA1) END-EXEC", "GOBACK."].into_iter().map(line).collect();
    let (out, _, ending) = run_with(&program("", data, &body), &[]);
    assert_eq!(out, "GB 000000040\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, AbendCode::Exec);
    assert!(abend.message.starts_with("IWR0060-S EXEC DLI GN was reached"), "{}", abend.message);
}

#[test]
fn the_compile_option_in_force_says_which_messages_refuse_a_program() {
    let message = |severity| Error::at(Pos::default(), "m").graded(severity);
    let options = |cards: &[&str], flags: &[&str]| {
        let mut o = Options::default();
        cards.iter().for_each(|c| assert_eq!(o.apply(c), Ok(true), "{c}"));
        flags.iter().for_each(|f| o.apply_flag(f).unwrap());
        o
    };
    let severities = [Severity::Informational, Severity::Warning, Severity::Error, Severity::Severe, Severity::Unrecoverable];
    // Whether no messages, then one of each severity from I to U, refuse the program.
    for (cards, flags, expected) in [
        (&[][..], &[][..], [false, false, false, false, true, true]),
        (&["NOCOMPILE(S)"], &[], [false, false, false, false, true, true]),
        (&["NOCOMPILE(E)"], &[], [false, false, false, true, true, true]),
        (&["NOC(W)"], &[], [false, false, true, true, true, true]),
        (&[], &["-warnings-block"], [false, false, true, true, true, true]),
        (&["COMPILE"], &[], [false, false, false, false, true, true]),
        (&["NOCOMPILE"], &[], [true, true, true, true, true, true]),
        (&["NOC(S)"], &["-warnings-block"], [false, false, false, false, true, true]),
        (&["NOC(E)"], &["-warnings-block"], [false, false, false, true, true, true]),
        (&["C"], &["-warnings-block"], [false, false, false, false, true, true]),
        (&["NOC(W)", "NOC(E)"], &[], [false, false, false, true, true, true]),
    ] {
        let o = options(cards, flags);
        let got: Vec<bool> = std::iter::once(refused(&[], &o)).chain(severities.map(|s| refused(&[message(Severity::Informational), message(s)], &o))).collect();
        assert_eq!(got, expected, "{cards:?} {flags:?}");
    }
}

#[test]
fn an_e_level_message_leaves_the_program_to_run_and_the_return_code_at_8() {
    let messages = [Error::warning(Pos::default(), "w"), Error::at(Pos::default(), "e").graded(Severity::Error)];
    assert!(!refused(&messages, &Options::default()));
    assert_eq!(syntax::return_code(&messages), 8);
    let blocking = {
        let mut o = Options::default();
        o.apply_flag("-warnings-block").unwrap();
        o
    };
    assert!(refused(&messages, &blocking));
}

#[test]
fn nocompile_alone_leaves_nothing_to_run_and_a_severity_it_does_not_name_is_discarded_with_an_error() {
    let compiled = |card: &str| compile(syntax::parse(&program(card, "", &line("GOBACK."))).unwrap(), &[]);
    assert_eq!(compiled("NOCOMPILE").err(), Some(Vec::new()));
    assert!(compiled("NOC(W)").is_ok());
    let discarded = compiled("NOC(U)").unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(discarded.diagnostics.iter().map(|e| (e.message.as_str(), e.severity)).collect::<Vec<_>>(), [("CBL NOC(U): NOC does not take (U)", Severity::Error)]);
    assert_eq!(discarded.options.object_code(), numeric::options::Compile::Until(numeric::options::Stop::S));
}

const FRAGMENTS: &[&str] = &[
    "'", "\"", "X'", "N'", "X'F", "(", ")", ":", ".", " . ", "-", " - ", "*", "**", "=", ">=", ",", ";", "PIC ", "PIC", "PICTURE IS ", "VALUE ",
    "VALUE ALL ", "COMP-3", "COMP-1", "BINARY", "NATIONAL", "OCCURS 99999999999", "OCCURS 0", "OCCURS 70000 ", "REDEFINES ", "SIGN LEADING SEPARATE",
    "IF ", "END-IF", "ELSE", "PERFORM ", "UNTIL ", "VARYING ", "FROM ", "BY ", "THRU ", "TIMES", "FUNCTION ", "LENGTH OF ", "OF ", "(1:", "ROUNDED",
    "ON SIZE ERROR ", "NOT ", " AND ", " OR ", "GIVING ", "REMAINDER ", "\n      -    '", "\n      -    ", "\n      *", "\n       CBL TRUNC(",
    "\n       01  ", "\n           05 ", " 88 ", " 66 ", " 77 ", "9(", "9(999)", "X(0)", "X(134217727)", "S9(31)V9(31)", "\t", "é", "€", "+.", "-.",
    "1.2.3", "ZERO", "SPACES", "SECTION.", "DIVISION.", "PROCEDURE", "DATA", "ZZ9", "N(5)", "\n", "EVALUATE ", "WHEN ", "OTHER ", "ALSO ",
    "THRU ", "FD ", "SELECT ", "ASSIGN TO ", "OPEN INPUT ", "READ ", "WRITE ", "AT END ", "EXEC SQL ", "END-EXEC", "COPY ", "$$,$$9.99CR",
    "**,**9", "BLANK WHEN ZERO", "EXIT PERFORM ", "EXIT SECTION", "CALL ", "USING ", "BY VALUE ", "BY CONTENT ", "RETURNING ",
    "LINKAGE SECTION.", "POINTER", "ADDRESS OF ", "SET ", "TO NULL", "UP BY ", "INDEXED BY ", "ACCEPT ", "FROM DATE", "CANCEL ",
    "OCCURS 1 TO 5 DEPENDING ON N ", "STRING ", "DELIMITED BY ", "INTO ", "WITH POINTER ", "UNSTRING ", "DELIMITER IN ", "COUNT IN ",
    "TALLYING IN ", "INSPECT ", "TALLYING ", "FOR ALL ", "FOR LEADING ", "CHARACTERS ", "REPLACING ", "FIRST ", "CONVERTING ",
    "BEFORE INITIAL ", "AFTER INITIAL ", "SEARCH ", "SEARCH ALL ", "ASCENDING KEY IS ", "NEXT SENTENCE", "LOCAL-STORAGE SECTION.",
    "FUNCTION NUMVAL(", "FUNCTION TRIM(", "LEADING)", "FUNCTION MOD(", "EXEC SQL SELECT A INTO :B FROM T END-EXEC", "EXEC CICS LINK PROGRAM('X') ",
    "COMMAREA(", "RESP(", "DFHRESP(NORMAL)", "ORGANIZATION INDEXED ", "ORGANIZATION RELATIVE ", "ACCESS DYNAMIC ", "RECORD KEY ",
    "ALTERNATE RECORD KEY ", "WITH DUPLICATES ", "RELATIVE KEY ", "START ", "KEY IS >= ", "REWRITE ", "DELETE ", "INVALID KEY ",
    "NOT INVALID KEY ", "READ NEXT ", "OPEN I-O ", "EXEC CICS READ FILE('F') INTO(X) RIDFLD(K) END-EXEC", "EXEC CICS STARTBR FILE('F') RIDFLD(K) GTEQ END-EXEC",
    "EXEC CICS READNEXT FILE('F') INTO(X) RIDFLD(K) END-EXEC", "EXEC CICS WRITEQ TS QUEUE('Q') FROM(X) END-EXEC", "EXEC CICS FORMATTIME ABSTIME(T) ",
    "EXEC CICS RETURN TRANSID('T') COMMAREA(X) END-EXEC", "EXEC CICS HANDLE ABEND LABEL(P) END-EXEC", "EXEC CICS PUSH HANDLE END-EXEC", "EXEC SQL INCLUDE SQLCA END-EXEC", ":B.C", "EXEC CICS HANDLE CONDITION ERROR(P) END-EXEC", "END PROGRAM X.", "\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. Y.\n", "IS RECURSIVE", "RETURN-CODE",
    "REPORT SECTION.", "\n       RD  R ", "REPORT IS R", "TYPE CF ", "TYPE PH ", "LINE PLUS ", "LINE 0 ", "NEXT PAGE ", "NEXT GROUP ", "COLUMN + ", "COLUMN RIGHT ", "SUM ",
    "UPON ", "RESET ON ", "GROUP INDICATE ", "PAGE LIMIT 3 ", "FIRST DETAIL 9 ", "FOOTING +", "CODE 'X' ", "GENERATE ", "INITIATE ", "TERMINATE ",
    "DECLARATIVES.", "USE BEFORE REPORTING ", "END DECLARATIVES.", "SUPPRESS PRINTING ", "LINE-COUNTER", "PAGE-COUNTER",
    "INVOKE ", "NEW ", "SELF ", "SUPER ", "OBJECT REFERENCE ", "USAGE OBJECT REFERENCE X ", "FUNCTION-POINTER", "USING BY VALUE ", "END-INVOKE",
    "ON EXCEPTION ", "EXIT METHOD", "END METHOD \"m\".", "METHOD-ID. \"m\".", "\n       IDENTIFICATION DIVISION.\n       METHOD-ID. \"m\".\n",
    "\n       IDENTIFICATION DIVISION.\n       OBJECT.\n", "END OBJECT.", "END FACTORY.", "END CLASS ", "CLASS-ID. ", "INHERITS ", "REPOSITORY. ",
    "CLASS X IS \"java.lang.Object\" ", "JNIENVPTR", "COPY JNI.",
    "SD ", "SORT ", "MERGE ", "RELEASE ", "RETURN ", "ON ASCENDING KEY ", "DESCENDING ", "WITH DUPLICATES IN ORDER ", "COLLATING SEQUENCE ",
    "INPUT PROCEDURE ", "OUTPUT PROCEDURE IS ", "GIVING ", "END-RETURN", "SORT-RETURN", "ALPHABET A IS STANDARD-1 ", "PROGRAM COLLATING SEQUENCE ",
    "ALPHABET B IS 'Z' THRU 'A' 'Q' ALSO 256 ALSO HIGH-VALUE ", "THROUGH ", "OCCURS 1 TO 20 DEPENDING ON ", "PIC 9(18) ", "PIC S9(31) ",
    "I-O-CONTROL. SAME RECORD AREA FOR ", "SAME AREA ", "\n       CBL FASTSRT\n", "\n       CBL THREAD,DLL\n", ",NORENT", ",NODBCS", "NOTHREAD",
    " IS INITIAL", "\n       END PROGRAM ", "LINAGE IS ", "LINAGE 0 ", "WITH FOOTING AT ", "LINES AT TOP ", "LINES AT BOTTOM ", "AT END-OF-PAGE ",
    "NOT AT EOP ", "LINAGE-COUNTER", " IN P ", "ADVANCING PAGE ", "BEFORE ADVANCING ",
    "ENTRY 'E' ", "ENTRY ", "ALTER ", "TO PROCEED TO ", "GO TO.", "GO TO ", " DEPENDING ON ",
    " AFTER ", "NO ADVANCING", "FUNCTION RANDOM", "FUNCTION RANDOM(", " <> ", " & ", "SECTION 50.", "SECTION 99", "\n       CBL DYNAM\n",
    " SYNC ", " SYNCHRONIZED RIGHT ", " COMP-2 SYNC ", "\n       66  RN RENAMES ", " THRU ", "PIC 99PP ", "PIC SVP(3)9 ", "PIC ZZZPP ", "PIC P",
    "DECIMAL-POINT IS COMMA. ", "1,5 ", ",25", "PIC Z.ZZ9,99 ", " FALSE 'N' ", " WHEN SET TO FALSE ", "SET X TO FALSE ", "CURRENCY SIGN '$' ",
    "CURRENCY SIGN 'W' ", "CURRENCY 'EUR ' WITH PICTURE SYMBOL 'y' ", "PIC WWW9 ", "PIC yy9,99 ",
];

fn mutate(base: &str, next: &mut impl FnMut() -> u64) -> String {
    let mut chars: Vec<char> = base.chars().collect();
    for _ in 0..1 + next() % 4 {
        let at = if chars.is_empty() { 0 } else { (next() as usize) % chars.len() };
        match next() % 6 {
            0 => {
                let f = FRAGMENTS[(next() as usize) % FRAGMENTS.len()];
                chars.splice(at..at, f.chars());
            }
            1 => {
                let end = (at + 1 + (next() as usize) % 24).min(chars.len());
                chars.drain(at..end);
            }
            2 => {
                let text: String = chars.iter().collect();
                let lines: Vec<&str> = text.lines().collect();
                let (a, b) = ((next() as usize) % lines.len().max(1), (next() as usize) % lines.len().max(1));
                let mut lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                if !lines.is_empty() {
                    lines.swap(a, b);
                    let dup = lines[a].clone();
                    lines.insert(b, dup);
                }
                chars = lines.join("\n").chars().collect();
            }
            3 if !chars.is_empty() => chars[at] = (b' ' + (next() % 95) as u8) as char,
            4 => chars.truncate(at),
            _ => {
                let end = (at + 1 + (next() as usize) % 40).min(chars.len());
                let copy: Vec<char> = chars[at..end].to_vec();
                chars.splice(at..at, copy);
            }
        }
    }
    chars.into_iter().collect()
}

/// The programs the fuzz tests mutate.
fn fuzz_corpus() -> Vec<String> {
    oracle::programs()
        .iter()
        .map(|p| p.source(false))
        .chain([
            program("TRUNC(OPT)", "       01  G.\n           05 A PIC S9(5)V99 COMP-3 VALUE -1.5.\n           05 T PIC X OCCURS 3.\n", &line("GOBACK.")),
            file_program(
                "           SELECT IN-F ASSIGN TO INDD\n               ORGANIZATION IS LINE SEQUENTIAL\n               FILE STATUS IS ST.\n",
                "       FD  IN-F RECORD CONTAINS 20 CHARACTERS.\n       01  IN-REC PIC X(20).\n",
                "       01  ST PIC XX.\n       01  N PIC 99 VALUE 5.\n       01  E PIC $$,$$9.99CR.\n",
                &[
                    "       MAIN SECTION.\n",
                    &line("OPEN INPUT IN-F"),
                    &line("EVALUATE N ALSO TRUE WHEN 1 THRU 9 ALSO ST = '00'"),
                    &line("    MOVE N TO E WHEN OTHER CONTINUE END-EVALUATE"),
                    &line("READ IN-F AT END EXIT SECTION END-READ"),
                    "       OTHER-S SECTION.\n",
                    &line("GOBACK."),
                ]
                .concat(),
            ),
            file_program(
                "           SELECT K ASSIGN TO KDD ORGANIZATION INDEXED ACCESS DYNAMIC\n               RECORD KEY K-ID ALTERNATE KEY K-ALT WITH DUPLICATES.\n           SELECT R ASSIGN TO RDD ORGANIZATION RELATIVE\n               ACCESS RANDOM RELATIVE KEY RK.\n",
                "       FD  K.\n       01  K-REC.\n           05 K-ID PIC XX.\n           05 K-ALT PIC XX.\n       FD  R.\n       01  R-REC PIC X(4).\n",
                "       01  RK PIC 9(4).\n",
                &[
                    line("OPEN I-O K R"),
                    line("START K KEY >= K-ALT INVALID KEY CONTINUE END-START"),
                    line("READ K NEXT AT END CONTINUE END-READ"),
                    line("READ K KEY IS K-ID INVALID KEY CONTINUE END-READ"),
                    line("REWRITE K-REC INVALID KEY CONTINUE END-REWRITE"),
                    line("DELETE R INVALID KEY CONTINUE NOT INVALID KEY CONTINUE"),
                    line("GOBACK."),
                ]
                .concat(),
            ),
            program(
                "",
                "       01  S PIC X(9) VALUE 'A,B C'.\n       01  N PIC 9.\n       01  T.\n           05 E PIC X OCCURS 3 ASCENDING KEY IS E INDEXED BY IX.\n",
                &[
                    line("STRING S DELIMITED BY ',' 'X' DELIMITED BY SIZE INTO S"),
                    line("UNSTRING S DELIMITED BY ',' OR ALL ' ' INTO S N"),
                    line("INSPECT S TALLYING N FOR ALL 'A' REPLACING FIRST 'B' BY 'C'"),
                    line("SEARCH ALL E WHEN E(IX) = 'B' NEXT SENTENCE END-SEARCH"),
                    line("GOBACK."),
                ]
                .concat(),
            ),
            file_program(
                "           SELECT P ASSIGN TO PDD.\n",
                "       FD  P REPORT IS R.\n",
                &[
                    "       01  K PIC X.\n       01  N PIC 9.\n       REPORT SECTION.\n       RD  R CONTROLS FINAL K PAGE 9 FIRST DETAIL 3 FOOTING 8.\n",
                    "       01  TYPE PH LINE 1 COLUMN 1 PIC 9 SOURCE PAGE-COUNTER.\n       01  D TYPE DE LINE PLUS 1.\n",
                    "           05 COLUMN 1 PIC X SOURCE K GROUP INDICATE.\n           05 R-N COLUMN + 2 PIC 9 SOURCE N.\n",
                    "       01  TYPE CF K NEXT GROUP NEXT PAGE LINE PLUS 2\n           COLUMN 1 PIC 99 SUM R-N RESET ON FINAL.\n",
                    "       01  TYPE RF LINE 4 NEXT PAGE COLUMN 1 VALUE 'END'.\n",
                ]
                .concat(),
                &[
                    "       DECLARATIVES.\n       U SECTION.\n           USE BEFORE REPORTING D.\n       U-1.\n           SUPPRESS PRINTING.\n       END DECLARATIVES.\n",
                    "       M SECTION.\n",
                    &line("OPEN OUTPUT P INITIATE R GENERATE D"),
                    &line("GENERATE R TERMINATE R CLOSE P"),
                    &line("GOBACK."),
                ]
                .concat(),
            ),
            file_program(
                "           SELECT S ASSIGN TO SORTWK1.\n           SELECT F ASSIGN TO FDD.\n           SELECT G ASSIGN TO GDD.\n       I-O-CONTROL.\n           SAME RECORD AREA FOR S F.\n",
                "       SD  S.\n       01  S-REC.\n           05 S-K PIC S9(3) COMP-3.\n           05 S-X PIC X.\n       FD  F.\n       01  F-REC PIC X(3).\n       FD  G.\n       01  G-REC PIC X(3).\n",
                "       01  T.\n           05 E PIC X OCCURS 3 ASCENDING KEY E.\n",
                &[
                    line("SORT S ON ASCENDING KEY S-K DESCENDING S-X"),
                    line("    WITH DUPLICATES IN ORDER"),
                    line("    INPUT PROCEDURE P-IN OUTPUT PROCEDURE P-OUT"),
                    line("MERGE S ON DESCENDING KEY S-X USING F G GIVING F"),
                    line("SORT E"),
                    line("IF SORT-RETURN NOT = 0 DISPLAY 'FAILED' END-IF."),
                    "       P-IN.\n".into(),
                    line("RELEASE S-REC FROM F-REC."),
                    "       P-OUT.\n".into(),
                    line("RETURN S INTO G-REC AT END CONTINUE"),
                    line("    NOT AT END CONTINUE END-RETURN."),
                ]
                .concat(),
            ),
            file_program(
                "           SELECT P ASSIGN TO PDD.\n",
                "       FD  P LINAGE IS N LINES WITH FOOTING AT 4\n           LINES AT TOP 1 LINES AT BOTTOM T.\n       01  P-REC PIC X(3).\n",
                "       01  N PIC 99 VALUE 6.\n       01  T PIC 9 COMP-3.\n",
                &[
                    line("OPEN OUTPUT P"),
                    line("WRITE P-REC IN P BEFORE ADVANCING 2 LINES AT END-OF-PAGE"),
                    line("    DISPLAY LINAGE-COUNTER OF P NOT AT EOP CONTINUE END-WRITE"),
                    line("WRITE P-REC AFTER ADVANCING PAGE CLOSE P"),
                    line("GOBACK."),
                ]
                .concat(),
            ),
            two_programs(
                "       01  A PIC X(3).\n       01  P POINTER.\n",
                &[line("SET P TO ADDRESS OF A"), line("CALL 'SUB' USING A BY VALUE P RETURNING A"), line("ACCEPT A FROM DATE"), line("GOBACK.")].concat(),
                "SUB",
                "       LINKAGE SECTION.\n       01  LA PIC X(3).\n       01  LP POINTER.\n",
                &["       PROCEDURE DIVISION USING LA BY VALUE LP RETURNING LA.\n", &line("SET ADDRESS OF LA TO LP"), &line("GOBACK.")].concat(),
            ),
        ])
        .chain(oo::fuzz_seeds())
        .chain(collating::fuzz_seeds())
        .chain(procedure::fuzz_seeds())
        .chain(declaratives::fuzz_seeds())
        .collect()
}

/// Mutated programs may be refused, but must never panic the reader, lexer, parser or compiler.
/// `IRONWORK_FUZZ_ITERATIONS` raises the count for a longer run.
#[test]
fn mutated_programs_never_panic_the_front_end() {
    let iterations: usize = std::env::var("IRONWORK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000);
    let corpus = fuzz_corpus();
    let mut seed = 0x853C_49E6_748F_EA9Bu64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 0..iterations {
        let mutated = mutate(&corpus[i % corpus.len()], &mut next);
        let outcome = std::panic::catch_unwind(|| {
            if let Ok(p) = syntax::parse(&mutated) {
                let _ = compile(p, &[]);
            }
        });
        if outcome.is_err() {
            let path = std::env::temp_dir().join(format!("ironwork-fuzz-{i}.cbl"));
            std::fs::write(&path, &mutated).unwrap();
            panic!("iteration {i} panicked; the input is in {}", path.display());
        }
    }
}

/// ironwork runs the oracle's own programs, and its output is what the model predicts.
#[test]
fn the_oracle_programs_run_and_match_the_model() {
    let programs = oracle::programs();
    let mut observed = BTreeMap::new();
    for p in &programs {
        let (out, err, ending) = run_with(&p.source(false), &["-silent"]);
        assert!(ending.is_ok(), "{}: {ending:?}\n{err}", p.name);
        observed.extend(oracle::parse_output(&out).cases);
    }
    let findings = oracle::check(&programs, &observed);
    let failures: Vec<String> = findings
        .iter()
        .filter(|f| f.verdict != oracle::Verdict::Match)
        .map(|f| match &f.verdict {
            oracle::Verdict::Mismatch { expected, observed } => format!("{}: expected {} observed {}", f.id, oracle::hex(expected), oracle::hex(observed)),
            _ => format!("{}: missing", f.id),
        })
        .collect();
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), findings.len(), failures.join("\n"));
}

fn run_unit(source: &str, dirs: Vec<std::path::PathBuf>, sysin: &str) -> (String, String, Result<(Ending, i16), Abend>) {
    let o = Harness::source(source).dirs(dirs).sysin(sysin).clock(unit::Clock::Fixed(1_790_510_400, 42)).run(Executor::Interpreter);
    (o.out, o.err, o.ending.map(|e| (e, o.return_code)))
}

fn two_programs(main_data: &str, main_body: &str, sub_head: &str, sub_data: &str, sub_body: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{main_data}       PROCEDURE DIVISION.\n{main_body}       END PROGRAM MAIN.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {sub_head}.\n       DATA DIVISION.\n{sub_data}{sub_body}       END PROGRAM SUB.\n"
    )
}

#[test]
fn call_by_reference_content_and_value_with_returning_and_return_code() {
    let source = two_programs(
        "       01  A PIC X(3) VALUE 'AAA'.\n       01  B PIC X(3) VALUE 'BBB'.\n       01  R PIC 9(4) VALUE 0.\n",
        &[
            line("CALL 'SUB' USING BY REFERENCE A BY CONTENT B BY VALUE 7"),
            line("    RETURNING R"),
            line("DISPLAY A ' ' B ' ' R ' ' RETURN-CODE"),
            line("CALL 'SUB' USING A B BY VALUE 1 RETURNING R"),
            line("DISPLAY R"),
            line("CANCEL 'SUB'"),
            line("CALL 'SUB' USING A B BY VALUE 1 RETURNING R"),
            line("DISPLAY R"),
            line("GOBACK."),
        ]
        .concat(),
        "SUB",
        "       WORKING-STORAGE SECTION.\n       01  COUNTER PIC 9(4) VALUE 0.\n       LINKAGE SECTION.\n       01  LA PIC X(3).\n       01  LB PIC X(3).\n       01  LV PIC S9(9) COMP-5.\n       01  LR PIC 9(4).\n",
        &[
            "       PROCEDURE DIVISION USING LA LB BY VALUE LV RETURNING LR.\n",
            &line("MOVE 'XYZ' TO LA LB"),
            &line("ADD 1 TO COUNTER"),
            &line("COMPUTE LR = COUNTER * 100 + LV"),
            &line("MOVE 4 TO RETURN-CODE"),
            &line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_unit(&format!("       CBL DYNAM\n{source}"), vec![], "");
    assert_eq!(ending.as_ref().map(|e| e.1), Ok(4), "{ending:?} {err}");
    assert_eq!(out, "XYZ BBB 0107 0004\n0201\n0101\n");
    assert_eq!(run_unit(&source, vec![], "").0, "XYZ BBB 0107 0004\n0201\n0301\n");
}

#[test]
fn a_missing_program_takes_on_exception_or_ends_as_ibm_documents() {
    let body = [line("MOVE 'NOPE' TO NAME"), line("CALL NAME ON EXCEPTION DISPLAY 'NOT FOUND' END-CALL"), line("CALL 'NOPE2'"), line("GOBACK.")].concat();
    let source = program("", "       01  NAME PIC X(8).\n", &body);
    let (out, _, ending) = run_unit(&source, vec![], "");
    assert_eq!(out, "NOT FOUND\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "IRONWORK");
    assert!(abend.message.starts_with("CALL NOPE2: IEW2456E SYMBOL NOPE2 UNRESOLVED"), "{}", abend.message);
    let dynamic = program("", "       01  NAME PIC X(8) VALUE 'NOPE'.\n", &[line("CALL NAME"), line("GOBACK.")].concat());
    let abend = run_unit(&dynamic, vec![], "").2.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.message.as_str()), ("U4038", "CEE3501S The module NOPE was not found."));
}

#[test]
fn a_linkage_item_with_no_address_is_a_protection_exception() {
    let source = two_programs(
        "",
        &[line("CALL 'SUB'"), line("GOBACK.")].concat(),
        "SUB",
        "       LINKAGE SECTION.\n       01  L PIC X.\n",
        &["       PROCEDURE DIVISION USING L.\n", &line("MOVE 'A' TO L"), &line("GOBACK.")].concat(),
    );
    assert_eq!(run_unit(&source, vec![], "").2.unwrap_err().code, "S0C4");
}

#[test]
fn stop_run_in_a_called_program_ends_the_run_unit_and_initial_programs_start_afresh() {
    let source = two_programs(
        "",
        &[line("CALL 'SUB'"), line("CALL 'SUB'"), line("EXIT PROGRAM"), line("DISPLAY 'AFTER'"), line("CALL 'SUB' USING 'STOP'"), line("DISPLAY 'NEVER'"), line("GOBACK.")].concat(),
        "SUB IS INITIAL",
        "       WORKING-STORAGE SECTION.\n       01  N PIC 9 VALUE 0.\n       LINKAGE SECTION.\n       01  L PIC X(4).\n",
        &[
            "       PROCEDURE DIVISION USING L.\n",
            &line("ADD 1 TO N"),
            &line("DISPLAY 'N=' N"),
            &line("IF ADDRESS OF L NOT = NULL AND L = 'STOP' STOP RUN END-IF"),
            &line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "N=1\nN=1\nAFTER\nN=1\n");
}

#[test]
fn pointers_and_address_of() {
    let source = two_programs(
        "       01  X PIC X(4) VALUE 'ABCD'.\n       01  P USAGE POINTER.\n       01  Q POINTER.\n",
        &[
            line("SET P TO ADDRESS OF X"),
            line("CALL 'SUB' USING BY VALUE P"),
            line("DISPLAY X"),
            line("SET Q TO P"),
            line("SET Q UP BY 2"),
            line("IF Q > P AND P NOT = NULL DISPLAY 'MOVED' END-IF"),
            line("SET P TO NULL"),
            line("IF P = NULL DISPLAY 'NULL' END-IF"),
            line("GOBACK."),
        ]
        .concat(),
        "SUB",
        "       LINKAGE SECTION.\n       01  PTR POINTER.\n       01  AREA-L PIC X(4).\n",
        &["       PROCEDURE DIVISION USING PTR.\n", &line("SET ADDRESS OF AREA-L TO PTR"), &line("MOVE 'WXYZ' TO AREA-L"), &line("GOBACK.")].concat(),
    );
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "WXYZ\nMOVED\nNULL\n");
}

#[test]
fn index_names_and_set_condition_to_true() {
    let out = run(&program(
        "",
        "       01  T.\n           05 E PIC X OCCURS 5 INDEXED BY IX.\n       01  FLAG PIC X VALUE 'N'.\n          88 DONE VALUE 'Y'.\n",
        &[
            line("MOVE 'ABCDE' TO T"),
            line("SET IX TO 2"),
            line("SET IX UP BY 2"),
            line("DISPLAY E(IX) E(IX - 1)"),
            line("PERFORM VARYING IX FROM 1 BY 1 UNTIL IX > 5 OR DONE"),
            line("    IF E(IX) = 'C' SET DONE TO TRUE END-IF"),
            line("END-PERFORM"),
            line("DISPLAY FLAG"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "DC\nY\n");
}

#[test]
fn condition_names_reach_a_filler_or_repeated_variable_by_its_item() {
    let data = [
        "       01  A.\n           05  FILLER PIC X VALUE 'Y'.\n               88  A-ON VALUE 'Y'.\n",
        "           05  S PIC 9 VALUE 1.\n               88  S-ONE VALUE 1.\n",
        "       01  B.\n           05  S PIC 9 VALUE 2.\n               88  S-TWO VALUE 2.\n               88  S-SIX VALUE 6.\n",
        "       01  T.\n           05  R OCCURS 2.\n               10  S PIC 9 VALUE 0.\n                   88  S-SET VALUE 5.\n",
    ]
    .concat();
    let out = run(&program(
        "",
        &data,
        &[
            line("IF A-ON AND S-ONE AND S-TWO DISPLAY 'ALL' END-IF"),
            line("SET S-SIX TO TRUE"),
            line("SET S-SET (2) TO TRUE"),
            line("IF S-SET (2) AND NOT S-SET (1) DISPLAY 'ROW 2' END-IF"),
            line("DISPLAY A B T."),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "ALL\nROW 2\nY1605\n");
}

#[test]
fn accept_dates_times_and_sysin() {
    let source = program(
        "",
        "       01  D8 PIC 9(8).\n       01  D6 PIC X(6).\n       01  T PIC 9(8).\n       01  W PIC 9.\n       01  J PIC 9(7).\n       01  L PIC X(10).\n       01  C PIC X(21).\n",
        &[
            line("ACCEPT D8 FROM DATE YYYYMMDD"),
            line("ACCEPT D6 FROM DATE"),
            line("ACCEPT T FROM TIME"),
            line("ACCEPT W FROM DAY-OF-WEEK"),
            line("ACCEPT J FROM DAY YYYYDDD"),
            line("ACCEPT L"),
            line("MOVE FUNCTION CURRENT-DATE TO C"),
            line("DISPLAY D8 ' ' D6 ' ' T ' ' W ' ' J ' [' L '] ' C"),
            line("ACCEPT L"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_unit(&source, vec![], "hello\n");
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "20260927 260927 12000042 7 2026270 [hello     ] 2026092712000042+0000\n");
    assert!(err.contains("SYSIN at its end"));
}

#[test]
fn nested_programs_and_program_libraries() {
    let dir = temp("proglib");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("LIBPGM.cbl"),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LIBPGM.\n       PROCEDURE DIVISION.\n           DISPLAY 'FROM LIBRARY'\n           GOBACK.\n",
    )
    .unwrap();
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       PROCEDURE DIVISION.\n           CALL 'INNER'\n           CALL 'LIBPGM'\n           GOBACK.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n           DISPLAY 'NESTED'\n           GOBACK.\n       END PROGRAM INNER.\n       END PROGRAM OUTER.\n";
    let (out, err, ending) = run_unit(source, vec![dir], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "NESTED\nFROM LIBRARY\n");
}

#[test]
fn runaway_recursion_abends_instead_of_exhausting_the_stack() {
    std::thread::Builder::new().stack_size(16 << 20).spawn(runaway).unwrap().join().unwrap();
}

fn runaway() {
    let (_, _, ending) = run_with(&program("", "", &["       P.\n", &line("PERFORM P.")].concat()), &[]);
    assert!(ending.unwrap_err().message.contains("nest deeper"));
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n           CALL 'R'\n           GOBACK.\n       END PROGRAM MAIN.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. R IS RECURSIVE.\n       PROCEDURE DIVISION.\n           CALL 'R'\n           GOBACK.\n       END PROGRAM R.\n";
    assert!(run_unit(source, vec![], "").2.unwrap_err().message.contains("nest deeper"));
}

/// The Programming Guide's factorial program (SC27-8714-03, pp. 14-15), a RECURSIVE main program
/// that CALLs itself.
#[test]
fn a_recursive_main_program_calls_itself() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. FACT RECURSIVE.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  NUMB PIC 9(4) VALUE 5.\n       01  FACT PIC 9(8) VALUE 0.\n       LOCAL-STORAGE SECTION.\n       01  NUM PIC 9(4).\n       PROCEDURE DIVISION.\n",
        &["MOVE NUMB TO NUM", "IF NUMB = 0", "    MOVE 1 TO FACT", "ELSE", "    SUBTRACT 1 FROM NUMB", "    CALL 'FACT'", "    MULTIPLY NUM BY FACT", "END-IF", "DISPLAY NUM '! = ' FACT", "GOBACK."].map(line).concat(),
    ]
    .concat();
    let (out, _, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "0000! = 00000001\n0001! = 00000001\n0002! = 00000002\n0003! = 00000006\n0004! = 00000024\n0005! = 00000120\n");
}

#[test]
fn a_call_of_an_active_program_that_is_not_recursive_ends_the_run_with_igz0015s_or_igz0064s() {
    let main = two_programs("", &[line("CALL 'SUB'"), line("GOBACK.")].concat(), "SUB", "", &["       PROCEDURE DIVISION.\n".into(), line("CALL 'MAIN'"), line("GOBACK.")].concat());
    let abend = run_unit(&main, vec![], "").2.unwrap_err();
    assert_eq!((abend.code.to_string(), abend.message.as_str()), ("U4038".into(), "IGZ0015S A recursive call was attempted to a program that was already active. The program name is MAIN."));
    let nested = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'A'"),
        "           GOBACK.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. A IS COMMON.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'B'"),
        "           GOBACK.\n       END PROGRAM A.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. B IS COMMON.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'A'"),
        "           GOBACK.\n       END PROGRAM B.\n       END PROGRAM OUTER.\n",
    ]
    .concat();
    let abend = run_unit(&nested, vec![], "").2.unwrap_err();
    assert_eq!(abend.message, "IGZ0064S A recursive call to active program A in compilation unit OUTER was attempted.");
}

#[test]
fn a_recursive_program_stays_active_after_a_call_of_itself_returns() {
    let sub = [
        "       WORKING-STORAGE SECTION.\n       01  N PIC 9 VALUE 0.\n       01  T PIC X(80) VALUE 'ACTIVE'.\n       01  O PIC X(255) VALUE SPACES.\n       01  FC PIC X(12).\n       PROCEDURE DIVISION.\n",
        &["ADD 1 TO N", "IF N = 1", "    CALL 'SUB'", "    CALL 'CEE3DMP' USING T O FC", "END-IF", "GOBACK."].map(line).concat(),
    ]
    .concat();
    let source = two_programs("", &[line("CALL 'SUB'"), line("GOBACK.")].concat(), "SUB RECURSIVE", &sub, "");
    let (_, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?}");
    assert!(err.contains("\n  MAIN\n  SUB\n"), "{err}");
}

#[test]
fn occurs_depending_on_sets_the_group_length() {
    let out = run(&program(
        "",
        "       01  N PIC 9 VALUE 2.\n       01  REC.\n           05 HDR PIC X(2) VALUE 'H:'.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON N.\n       01  COPY-OF PIC X(10).\n",
        &[
            line("MOVE 'A' TO ITEM(1)"),
            line("MOVE 'B' TO ITEM(2)"),
            line("MOVE ALL '*' TO COPY-OF"),
            line("MOVE REC TO COPY-OF"),
            line("DISPLAY '[' REC '] ' LENGTH OF REC ' [' COPY-OF ']'"),
            line("MOVE 4 TO N"),
            line("DISPLAY LENGTH OF REC"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "[H:AB] 000000004 [H:AB      ]\n000000006\n");
}


#[test]
fn string_and_unstring() {
    let out = run(&program(
        "",
        "       01  FIRST-N PIC X(10) VALUE 'JOHN'.\n       01  LAST-N PIC X(10) VALUE 'SMITH'.\n       01  FULL PIC X(12) VALUE ALL '.'.\n       01  P PIC 99 VALUE 1.\n       01  CSV PIC X(20) VALUE 'AB,,CDE  FG,H'.\n       01  F1 PIC X(4).\n       01  F2 PIC X(4).\n       01  F3 PIC X(4).\n       01  F4 PIC X(4).\n       01  D1 PIC X.\n       01  C1 PIC 9.\n       01  T PIC 9 VALUE 0.\n",
        &[
            line("STRING FIRST-N DELIMITED BY SPACE ' ' DELIMITED BY SIZE"),
            line("    LAST-N DELIMITED BY SPACE INTO FULL WITH POINTER P"),
            line("    ON OVERFLOW DISPLAY 'OVER'"),
            line("    NOT ON OVERFLOW DISPLAY '[' FULL '] ' P"),
            line("END-STRING"),
            line("STRING 'ABCDEFGHIJKLMN' DELIMITED BY SIZE INTO FULL"),
            line("    ON OVERFLOW DISPLAY 'OVER ' FULL END-STRING"),
            line("UNSTRING CSV DELIMITED BY ',' OR ALL SPACE"),
            line("    INTO F1 DELIMITER IN D1 COUNT IN C1 F2 F3 F4"),
            line("    TALLYING IN T"),
            line("END-UNSTRING"),
            line("DISPLAY F1 '|' F2 '|' F3 '|' F4 '|' D1 C1 T"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "[JOHN SMITH..] 11\nOVER ABCDEFGHIJKL\nAB  |    |CDE |FG  |,24\n");
}

#[test]
fn inspect_tallying_replacing_and_converting() {
    let out = run(&program(
        "",
        "       01  S PIC X(12) VALUE '  00A0B0,C0 '.\n       01  N1 PIC 99 VALUE 0.\n       01  N2 PIC 99 VALUE 0.\n",
        &[
            line("INSPECT S TALLYING N1 FOR LEADING SPACES"),
            line("    N2 FOR ALL '0' BEFORE INITIAL ','"),
            line("INSPECT S REPLACING ALL '0' BY 'X' AFTER INITIAL 'A'"),
            line("    FIRST 'B' BY 'b'"),
            line("DISPLAY N1 ' ' N2 ' [' S ']'"),
            line("INSPECT S CONVERTING 'ABC' TO 'abc'"),
            line("DISPLAY '[' S ']'"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "02 04 [  00AXbX,CX ]\n[  00aXbX,cX ]\n");
}

#[test]
fn inspect_counts_replaces_and_converts_a_national_item_in_its_characters() {
    let data = concat!(
        "       01  W PIC N(6) VALUE N'AB AB'.\n       01  P PIC N VALUE N'B'.\n       01  F PIC N(2) VALUE N'BX'.\n       01  T PIC N(2) VALUE N'yz'.\n",
        "       01  C1 PIC 99 VALUE 0.\n       01  C2 PIC 99 VALUE 0.\n       01  C3 PIC 99 VALUE 0.\n       01  C4 PIC 99 VALUE 0.\n       01  C5 PIC 99 VALUE 0.\n",
    );
    let out = run(&program(
        "",
        data,
        &[
            line("INSPECT W TALLYING C1 FOR CHARACTERS"),
            line("INSPECT W TALLYING C2 FOR ALL SPACES C3 FOR LEADING N'A'"),
            line("    C4 FOR ALL P AFTER INITIAL SPACE"),
            line("INSPECT W (2:3) TALLYING C5 FOR CHARACTERS"),
            line("DISPLAY C1 ' ' C2 ' ' C3 ' ' C4 ' ' C5"),
            line("INSPECT W REPLACING ALL SPACES BY ZERO"),
            line("INSPECT W REPLACING FIRST N'A' BY N'a' AFTER INITIAL ZERO"),
            line("INSPECT W REPLACING CHARACTERS BY N'*' BEFORE INITIAL P"),
            line("DISPLAY FUNCTION DISPLAY-OF(W)"),
            line("MOVE 0 TO C1"),
            line("INSPECT W TALLYING C1 FOR ALL P REPLACING ALL P BY N'b'"),
            line("    LEADING N'*' BY SPACE"),
            line("DISPLAY C1 ' ' FUNCTION DISPLAY-OF(W)"),
            line("INSPECT W CONVERTING N'ab0' TO N'XYZ' AFTER INITIAL SPACE"),
            line("DISPLAY FUNCTION DISPLAY-OF(W)"),
            line("MOVE N'XYXY' TO W"),
            line("INSPECT W CONVERTING F TO T BEFORE INITIAL N'Y'"),
            line("DISPLAY FUNCTION DISPLAY-OF(W)"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "06 02 01 01 03\n*B0aB0\n02  b0ab0\n YZXYZ\nzYXY  \n");
    let errors = |statement: &str| {
        let body: String = [statement, "GOBACK."].into_iter().map(line).collect();
        let parsed = syntax::parse(&program("", &format!("{data}       01  S PIC X(4).\n"), &body)).unwrap();
        compile(parsed, &[]).err().unwrap().into_iter().map(|e| e.message).collect::<Vec<_>>()
    };
    let national = |operand: &str| format!("INSPECT W: {operand} cannot be an operand here, since W is national and every operand but the count field must be national too");
    let display = |operand: &str| format!("INSPECT S: {operand} cannot be an operand here, since S is not national, and an operand can be national only when the inspected item is");
    assert_eq!(errors("INSPECT W TALLYING C1 FOR ALL 'A' BEFORE INITIAL S"), [national("an alphanumeric literal"), national("S")]);
    assert_eq!(errors("INSPECT W REPLACING ALL P BY X'C1'"), [national("an alphanumeric literal")]);
    assert_eq!(errors("INSPECT W CONVERTING S TO T"), [national("S")]);
    assert_eq!(errors("INSPECT S TALLYING C1 FOR ALL N'A'"), [display("a national literal")]);
    assert_eq!(errors("INSPECT S REPLACING CHARACTERS BY P AFTER INITIAL SPACE"), [display("P")]);
}

#[test]
fn inspect_tallying_counts_over_a_function_result_and_refuses_to_change_one() {
    let data = "       01  S PIC X(8) VALUE 'AbC'.\n       01  W PIC N(4) VALUE N'xy'.\n       01  N1 PIC 99 VALUE 0.\n       01  N2 PIC 99 VALUE 0.\n       01  N3 PIC 99 VALUE 0.\n";
    let out = run(&program(
        "",
        data,
        &[
            line("INSPECT FUNCTION REVERSE(S) TALLYING N1 FOR LEADING SPACES"),
            line("INSPECT FUNCTION UPPER-CASE(S (1:3)) TALLYING N2 FOR ALL 'B'"),
            line("    N3 FOR CHARACTERS BEFORE INITIAL FUNCTION UPPER-CASE('c')"),
            line("DISPLAY N1 ' ' N2 ' ' N3 ' [' S ']'"),
            line("MOVE 0 TO N1 N2 N3"),
            line("INSPECT FUNCTION REVERSE(W) TALLYING N1 FOR LEADING SPACES"),
            line("    N2 FOR ALL N'y' N3 FOR CHARACTERS"),
            line("DISPLAY N1 ' ' N2 ' ' N3 ' '"),
            line("    FUNCTION DISPLAY-OF(FUNCTION UPPER-CASE(W))"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "05 01 01 [AbC     ]\n02 01 01 XY  \n");
    let errors = |statement: &str| {
        let body: String = statement.split(" | ").chain(["GOBACK."]).map(line).collect();
        let parsed = syntax::parse(&program("", data, &body)).unwrap();
        compile(parsed, &[]).err().unwrap().into_iter().map(|e| e.message).collect::<Vec<_>>()
    };
    let receiving = |name: &str, phrase: &str| format!("INSPECT FUNCTION {name} {phrase}: {phrase} stores into the inspected item, and a function-identifier cannot be a receiving operand");
    assert_eq!(errors("INSPECT FUNCTION REVERSE(S) REPLACING ALL 'A' BY 'B'"), [receiving("REVERSE", "REPLACING")]);
    assert_eq!(errors("INSPECT FUNCTION TRIM(S) TALLYING N1 FOR ALL 'A' | REPLACING ALL 'A' BY 'B'"), [receiving("TRIM", "REPLACING")]);
    assert_eq!(errors("INSPECT FUNCTION UPPER-CASE(S) CONVERTING 'A' TO 'B'"), [receiving("UPPER-CASE", "CONVERTING")]);
    assert_eq!(
        errors("INSPECT FUNCTION LENGTH(S) TALLYING N1 FOR CHARACTERS"),
        ["INSPECT FUNCTION LENGTH: an integer or numeric function can be used only where an arithmetic expression can, not as the inspected item"]
    );
}

#[test]
fn search_serial_and_binary() {
    let out = run(&program(
        "",
        "       01  TBL VALUE 'A1B2C3D4E5'.\n           05 ENTRY-T OCCURS 5 ASCENDING KEY IS K INDEXED BY IX.\n              10 K PIC X.\n              10 V PIC 9.\n       01  UNSORTED VALUE 'C3A1E5B2D4'.\n           05 U OCCURS 5 ASCENDING KEY IS UK INDEXED BY UX.\n              10 UK PIC X.\n              10 UV PIC 9.\n",
        &[
            line("SET IX TO 1"),
            line("SEARCH ENTRY-T AT END DISPLAY 'NONE'"),
            line("    WHEN K(IX) = 'C' DISPLAY 'SERIAL ' V(IX)"),
            line("END-SEARCH"),
            line("SEARCH ALL ENTRY-T AT END DISPLAY 'NONE'"),
            line("    WHEN K(IX) = 'D' DISPLAY 'BINARY ' V(IX)"),
            line("END-SEARCH"),
            line("SEARCH ALL ENTRY-T AT END DISPLAY 'NO Z'"),
            line("    WHEN K(IX) = 'Z' DISPLAY 'Z?'"),
            line("END-SEARCH"),
            line("SET UX TO 1"),
            line("SEARCH U WHEN UK(UX) = 'A' DISPLAY 'SERIAL FINDS A'"),
            line("END-SEARCH"),
            line("SEARCH ALL U AT END DISPLAY 'BINARY MISSES A'"),
            line("    WHEN UK(UX) = 'A' DISPLAY 'BINARY FINDS A'"),
            line("END-SEARCH"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "SERIAL 3\nBINARY 4\nNO Z\nSERIAL FINDS A\nBINARY MISSES A\n");
}

#[test]
fn next_sentence_and_local_storage() {
    let source = two_programs(
        "",
        &[line("CALL 'SUB'"), line("CALL 'SUB'"), line("IF 1 = 1 NEXT SENTENCE ELSE DISPLAY 'NO' END-IF"), line("DISPLAY 'SKIPPED'."), line("DISPLAY 'NEXT'"), line("GOBACK.")].concat(),
        "SUB",
        "       WORKING-STORAGE SECTION.\n       01  W PIC 9 VALUE 0.\n       LOCAL-STORAGE SECTION.\n       01  L PIC 9 VALUE 0.\n",
        &["       PROCEDURE DIVISION.\n", &line("ADD 1 TO W L"), &line("DISPLAY 'W=' W ' L=' L"), &line("GOBACK.")].concat(),
    );
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "W=1 L=1\nW=2 L=1\nNEXT\n");
}

#[test]
fn intrinsic_functions() {
    let out = run(&program(
        "",
        "       01  TXT PIC X(12) VALUE '  -12.50 '.\n       01  CUR PIC X(12) VALUE '$1,234.5CR'.\n       01  N PIC S9(5)V99.\n       01  R PIC S9(5).\n       01  T PIC X(12).\n       01  D PIC 9(8).\n",
        &[
            line("COMPUTE N = FUNCTION NUMVAL(TXT)"),
            line("DISPLAY N"),
            line("COMPUTE N = FUNCTION NUMVAL-C(CUR)"),
            line("DISPLAY N"),
            line("MOVE FUNCTION TRIM(TXT) TO T"),
            line("DISPLAY '[' T ']'"),
            line("COMPUTE R = FUNCTION MOD(-7, 3) * 10 + FUNCTION REM(-7, 3)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION INTEGER(-2.5) * 10"),
            line("    + FUNCTION INTEGER-PART(-2.5)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION MAX(3, 9, 4) + FUNCTION ABS(-5)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION INTEGER-OF-DATE(20260927)"),
            line("    - FUNCTION INTEGER-OF-DATE(20260101)"),
            line("COMPUTE D = FUNCTION DATE-OF-INTEGER("),
            line("    FUNCTION INTEGER-OF-DATE(20240228) + 1)"),
            line("DISPLAY R ' ' D"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "000125}\n012345}\n[-12.50      ]\n0001I\n0003K\n0001D\n0026I 20240229\n");
}

fn cics_program(id: &str, data: &str, linkage: &str, procedure: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION.\n{procedure}"
    )
}

fn run_cics(source: &str, task: cics::Task, commarea: Option<&str>, clock: unit::Clock) -> (String, Result<(Ending, cics::Task), Abend>) {
    let mut harness = Harness::source(source).task(task).clock(clock);
    if let Some(commarea) = commarea {
        harness = harness.commarea(commarea);
    }
    let o = harness.run(Executor::Interpreter);
    (o.out, o.ending.map(|e| (e, o.task.unwrap())))
}

fn task(transid: &str) -> cics::Task {
    cics::Task { transid: transid.into(), termid: "T001".into(), userid: "USER01".into(), applid: "IRONWORK".into(), sysid: "IRON".into(), number: 7, ..Default::default() }
}

#[test]
fn cics_return_transid_carries_the_commarea_to_the_next_task() {
    let source = cics_program(
        "CICS1",
        "       01  WS-OUT PIC X(10) VALUE 'NEXT STATE'.\n",
        "       01  DFHCOMMAREA PIC X(10).\n",
        &[
            line("IF EIBCALEN = 0"),
            line("    DISPLAY 'FIRST ' EIBTRNID"),
            line("ELSE"),
            line("    DISPLAY 'AGAIN ' DFHCOMMAREA"),
            line("END-IF"),
            line("EXEC CICS RETURN TRANSID('ABCD') COMMAREA(WS-OUT)"),
            line("    LENGTH(10) END-EXEC."),
        ]
        .concat(),
    );
    let (out, ending) = run_cics(&source, task("TR01"), None, unit::Clock::System);
    let (_, t) = ending.unwrap();
    assert_eq!(out, "FIRST TR01\n");
    assert_eq!(t.next_transid.as_deref(), Some("ABCD"));
    assert_eq!(t.returned_commarea, Some(ebcdic("NEXT STATE")));
    let (out, ending) = run_cics(&source, task("ABCD"), Some("HELLO     "), unit::Clock::System);
    assert!(ending.is_ok());
    assert_eq!(out, "AGAIN HELLO     \n");
}

#[test]
fn cics_conditions_resp_handle_ignore_and_default_abend() {
    let data = "       01  WS-RESP PIC S9(8) COMP.\n       01  WS-DATA PIC X(8).\n       01  WS-LEN PIC S9(4) COMP VALUE 8.\n";
    let source = cics_program(
        "CICS2",
        data,
        "",
        &[
            "       MAIN-LINE.\n",
            &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA)"),
            &line("    LENGTH(WS-LEN) RESP(WS-RESP) END-EXEC"),
            &line("IF WS-RESP = DFHRESP(QIDERR) DISPLAY 'QIDERR' END-IF"),
            &line("EXEC CICS HANDLE CONDITION QIDERR(NO-QUEUE) END-EXEC"),
            &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA)"),
            &line("    LENGTH(WS-LEN) END-EXEC"),
            &line("DISPLAY 'NOT REACHED'."),
            "       NO-QUEUE.\n",
            &line("DISPLAY 'HANDLED'"),
            &line("EXEC CICS IGNORE CONDITION QIDERR END-EXEC"),
            &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA)"),
            &line("    LENGTH(WS-LEN) END-EXEC"),
            &line("IF EIBRESP = DFHRESP(QIDERR) DISPLAY 'IGNORED' END-IF"),
            &line("EXEC CICS HANDLE CONDITION QIDERR END-EXEC"),
            &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA)"),
            &line("    LENGTH(WS-LEN) END-EXEC"),
            &line("DISPLAY 'NOT REACHED EITHER'."),
        ]
        .concat(),
    );
    let (out, ending) = run_cics(&source, task("TR02"), None, unit::Clock::System);
    assert_eq!(out, "QIDERR\nHANDLED\nIGNORED\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "AEYH");
}

#[test]
fn cics_temporary_storage_and_transient_data() {
    let data = "       01  WS-DATA PIC X(8).\n       01  WS-SHORT PIC X(3).\n       01  WS-LEN PIC 9(4) COMP.\n       01  WS-ITEM PIC 9(4) COMP.\n       01  WS-NUM PIC 9(4) COMP.\n       01  WS-RESP PIC S9(8) COMP.\n";
    let source = cics_program(
        "CICS3",
        data,
        "",
        &[
            line("EXEC CICS WRITEQ TS QUEUE('SCRATCH') FROM('ALPHA')"),
            line("    LENGTH(5) ITEM(WS-ITEM) END-EXEC"),
            line("EXEC CICS WRITEQ TS QUEUE('SCRATCH') FROM('BRAVO')"),
            line("    LENGTH(5) END-EXEC"),
            line("EXEC CICS WRITEQ TS QUEUE('SCRATCH') FROM('CHARLIE')"),
            line("    LENGTH(7) NUMITEMS(WS-NUM) END-EXEC"),
            line("DISPLAY WS-ITEM ' ' WS-NUM"),
            line("MOVE 8 TO WS-LEN"),
            line("EXEC CICS READQ TS QUEUE('SCRATCH') INTO(WS-DATA)"),
            line("    LENGTH(WS-LEN) ITEM(2) END-EXEC"),
            line("DISPLAY '[' WS-DATA '] ' WS-LEN"),
            line("MOVE 8 TO WS-LEN"),
            line("EXEC CICS READQ TS QUEUE('SCRATCH') INTO(WS-DATA)"),
            line("    LENGTH(WS-LEN) NEXT END-EXEC"),
            line("DISPLAY '[' WS-DATA ']'"),
            line("MOVE 3 TO WS-LEN"),
            line("EXEC CICS READQ TS QUEUE('SCRATCH') INTO(WS-SHORT)"),
            line("    LENGTH(WS-LEN) ITEM(3) RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(LENGERR) DISPLAY 'LENGERR ' WS-LEN"),
            line("    ' ' WS-SHORT END-IF"),
            line("EXEC CICS WRITEQ TD QUEUE('LOGQ') FROM('FIRST') LENGTH(5)"),
            line("    END-EXEC"),
            line("EXEC CICS WRITEQ TD QUEUE('LOGQ') FROM('SECOND') LENGTH(6)"),
            line("    END-EXEC"),
            line("MOVE 8 TO WS-LEN"),
            line("EXEC CICS READQ TD QUEUE('LOGQ') INTO(WS-DATA)"),
            line("    LENGTH(WS-LEN) END-EXEC"),
            line("DISPLAY '[' WS-DATA ']'"),
            line("EXEC CICS DELETEQ TS QUEUE('SCRATCH') END-EXEC"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let (out, ending) = run_cics(&source, task("TR03"), None, unit::Clock::System);
    let (_, t) = ending.unwrap_or_else(|a| panic!("{a:?}"));
    assert_eq!(out, "0001 0003\n[BRAVO   ] 0005\n[CHARLIE ]\nLENGERR 0007 CHA\n[FIRST   ]\n");
    assert!(t.ts.is_empty());
    assert_eq!(t.td.get("LOGQ").map(|q| q.len()), Some(1));
}

#[test]
fn cics_link_passes_the_commarea_and_xctl_does_not_come_back() {
    let main = cics_program(
        "MAINP",
        "       01  WS-AREA PIC X(5) VALUE 'AAAAA'.\n       01  WS-RESP PIC S9(8) COMP.\n",
        "",
        &[
            line("EXEC CICS LINK PROGRAM('SUBP') COMMAREA(WS-AREA)"),
            line("    LENGTH(5) END-EXEC"),
            line("DISPLAY 'BACK ' WS-AREA"),
            line("EXEC CICS LINK PROGRAM('NOPROG') RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(PGMIDERR) DISPLAY 'PGMIDERR' END-IF"),
            line("EXEC CICS XCTL PROGRAM('LASTP') END-EXEC"),
            line("DISPLAY 'NOT REACHED'."),
        ]
        .concat(),
    );
    let sub = cics_program(
        "SUBP",
        "",
        "       01  DFHCOMMAREA PIC X(5).\n",
        &[line("DISPLAY 'SUB ' EIBCALEN ' ' DFHCOMMAREA"), line("MOVE 'BBBBB' TO DFHCOMMAREA"), line("EXEC CICS RETURN END-EXEC.")].concat(),
    );
    let last = cics_program("LASTP", "", "", &[line("DISPLAY 'LAST'"), line("EXEC CICS RETURN END-EXEC.")].concat());
    let source = format!("{main}       END PROGRAM MAINP.\n{sub}       END PROGRAM SUBP.\n{last}       END PROGRAM LASTP.\n");
    let (out, ending) = run_cics(&source, task("TR04"), None, unit::Clock::System);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "SUB 0005 AAAAA\nBACK BBBBB\nPGMIDERR\nLAST\n");
}

#[test]
fn cics_time_assign_and_abend() {
    let data = "       01  WS-ABS PIC S9(15) COMP-3.\n       01  WS-DATE PIC X(10).\n       01  WS-TIME PIC X(8).\n       01  WS-DOW PIC 9(8) COMP.\n       01  WS-USER PIC X(8).\n       01  WS-APPL PIC X(8).\n";
    let source = cics_program(
        "CICS5",
        data,
        "",
        &[
            "       MAIN-LINE.\n",
            &line("EXEC CICS ASKTIME ABSTIME(WS-ABS) END-EXEC"),
            &line("DISPLAY WS-ABS"),
            &line("EXEC CICS FORMATTIME ABSTIME(WS-ABS) YYYYMMDD(WS-DATE)"),
            &line("    DATESEP('/') TIME(WS-TIME) TIMESEP"),
            &line("    DAYOFWEEK(WS-DOW) END-EXEC"),
            &line("DISPLAY WS-DATE ' ' WS-TIME ' ' WS-DOW"),
            &line("EXEC CICS ASSIGN USERID(WS-USER) APPLID(WS-APPL) END-EXEC"),
            &line("DISPLAY WS-USER '|' WS-APPL"),
            &line("EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC"),
            &line("EXEC CICS ABEND ABCODE('XY12') END-EXEC"),
            &line("DISPLAY 'NOT REACHED'."),
            "       RECOVER.\n",
            &line("DISPLAY 'RECOVERED'"),
            &line("EXEC CICS ABEND ABCODE('XY34') CANCEL END-EXEC."),
        ]
        .concat(),
    );
    let (out, ending) = run_cics(&source, task("TR05"), None, unit::Clock::Fixed(1_790_514_309, 25));
    assert_eq!(out, "003999503109250\n2026/09/27 13:05:09 00000000\nUSER01  |IRONWORK\nRECOVERED\n");
    assert_eq!(ending.unwrap_err().code, "XY34");
}

#[test]
fn a_handle_abend_label_takes_an_abend_from_a_lower_level_and_reset_rearms_it() {
    let main = cics_program(
        "MAINP",
        "       01  WS-N PIC 9 VALUE 0.\n       01  WS-CODE PIC X(4).\n",
        "",
        &[
            "       MAIN-LINE.\n",
            &line("EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC"),
            &line("EXEC CICS LINK PROGRAM('BADP') END-EXEC"),
            &line("DISPLAY 'NOT REACHED'."),
            "       RECOVER.\n",
            &line("ADD 1 TO WS-N"),
            &line("EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC"),
            &line("DISPLAY 'RECOVERED ' WS-N ' ' WS-CODE"),
            &line("IF WS-N = 1"),
            &line("    EXEC CICS HANDLE ABEND RESET END-EXEC"),
            &line("    EXEC CICS ABEND ABCODE('XY12') END-EXEC"),
            &line("END-IF"),
            &line("EXEC CICS ABEND ABCODE('XY34') END-EXEC."),
        ]
        .concat(),
    );
    let bad = cics_program("BADP", "", "       01  DFHCOMMAREA PIC X(10).\n", &line("DISPLAY DFHCOMMAREA."));
    let source = format!("{main}       END PROGRAM MAINP.\n{bad}       END PROGRAM BADP.\n");
    let (out, ending) = run_cics(&source, task("TR08"), None, unit::Clock::System);
    assert_eq!(out, "RECOVERED 1 ASRA\nRECOVERED 2 XY12\n");
    assert_eq!(ending.unwrap_err().code, "XY34");
}

#[test]
fn a_handle_abend_program_is_linked_to_with_its_levels_commarea_and_returns_to_the_level_above() {
    let main = cics_program(
        "MAINP",
        "       01  WS-AREA PIC X(5) VALUE 'MIDDL'.\n",
        "",
        &[line("EXEC CICS LINK PROGRAM('MIDP') COMMAREA(WS-AREA) END-EXEC"), line("DISPLAY 'BACK IN MAIN'"), line("EXEC CICS RETURN END-EXEC.")].concat(),
    );
    let mid = cics_program(
        "MIDP",
        "       01  WS-RESP PIC S9(8) COMP.\n",
        "       01  DFHCOMMAREA PIC X(5).\n",
        &[
            line("EXEC CICS HANDLE ABEND PROGRAM('NOPROG') RESP(WS-RESP)"),
            line("    END-EXEC"),
            line("IF WS-RESP = DFHRESP(PGMIDERR) DISPLAY 'PGMIDERR' END-IF"),
            line("EXEC CICS HANDLE ABEND PROGRAM('EXITP') END-EXEC"),
            line("EXEC CICS LINK PROGRAM('BOTP') END-EXEC"),
            line("DISPLAY 'NOT REACHED'."),
        ]
        .concat(),
    );
    let bottom = cics_program(
        "BOTP",
        "",
        "",
        &[line("EXEC CICS HANDLE ABEND LABEL(OWN) END-EXEC"), line("EXEC CICS HANDLE ABEND CANCEL END-EXEC"), line("EXEC CICS ABEND ABCODE('BOT1') END-EXEC."), "       OWN.\n".into(), line("DISPLAY 'NOT REACHED'.")]
            .concat(),
    );
    let exit = cics_program(
        "EXITP",
        "       01  WS-CODE PIC X(4).\n",
        "       01  DFHCOMMAREA PIC X(5).\n",
        &[line("EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC"), line("DISPLAY 'EXIT ' DFHCOMMAREA ' ' WS-CODE"), line("EXEC CICS RETURN END-EXEC.")].concat(),
    );
    let source = format!("{main}       END PROGRAM MAINP.\n{mid}       END PROGRAM MIDP.\n{bottom}       END PROGRAM BOTP.\n{exit}       END PROGRAM EXITP.\n");
    let (out, ending) = run_cics(&source, task("TR09"), None, unit::Clock::System);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "PGMIDERR\nEXIT MIDDL BOT1\nBACK IN MAIN\n");
}

#[test]
fn push_handle_suspends_the_abend_exit_and_abend_cancel_passes_every_exit() {
    let with = |between: &[&str], abend: &str| {
        let mut body = vec!["       MAIN-LINE.\n".to_owned(), line("EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC")];
        body.extend(between.iter().map(|s| line(s)));
        body.extend([line(abend), "       RECOVER.\n".to_owned(), line("DISPLAY 'RECOVERED'"), line("EXEC CICS RETURN END-EXEC.")]);
        let (out, ending) = run_cics(&cics_program("PUSHP", "", "", &body.concat()), task("TR10"), None, unit::Clock::System);
        (out, ending.err().map(|a| a.code.to_string()))
    };
    assert_eq!(with(&["EXEC CICS PUSH HANDLE END-EXEC"], "EXEC CICS ABEND ABCODE('PSH1') END-EXEC."), (String::new(), Some("PSH1".into())));
    assert_eq!(with(&["EXEC CICS PUSH HANDLE END-EXEC", "EXEC CICS POP HANDLE END-EXEC"], "EXEC CICS ABEND ABCODE('POP1') END-EXEC."), ("RECOVERED\n".into(), None));
    assert_eq!(with(&[], "EXEC CICS ABEND ABCODE('CAN1') CANCEL END-EXEC."), (String::new(), Some("CAN1".into())));
    let both = cics_program("BOTH", "", "", &[line("EXEC CICS HANDLE ABEND LABEL(X) RESET END-EXEC."), "       X.\n".into(), line("GOBACK.")].concat());
    assert!(run_cics(&both, task("TR11"), None, unit::Clock::System).1.unwrap_err().message.contains("takes one of PROGRAM, LABEL, CANCEL and RESET"));
}

#[test]
fn a_handle_abend_label_is_a_go_to_that_leaves_the_performs_in_progress_armed() {
    let source = cics_program(
        "GOTOP",
        "       01  WS-N PIC 9 VALUE 0.\n",
        "",
        &[
            "       MAIN-LINE.\n",
            &line("EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC"),
            &line("PERFORM A THRU C"),
            &line("DISPLAY 'AFTER A THRU C'"),
            &line("EXEC CICS RETURN END-EXEC."),
            "       RECOVER.\n",
            &line("DISPLAY 'RECOVERED'"),
            &line("GO TO C."),
            "       A.\n",
            &line("DISPLAY 'A'"),
            &line("PERFORM C"),
            &line("DISPLAY 'BACK IN A'."),
            "       B.\n",
            &line("DISPLAY 'B'."),
            "       C.\n",
            &line("DISPLAY 'C ' WS-N"),
            &line("ADD 1 TO WS-N"),
            &line("IF WS-N = 1"),
            &line("    EXEC CICS ABEND ABCODE('C001') END-EXEC"),
            &line("END-IF."),
            "       Z.\n",
            &line("DISPLAY 'FELL THROUGH'."),
        ]
        .concat(),
    );
    let (out, ending) = run_cics(&source, task("TR12"), None, unit::Clock::System);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "A\nC 0\nRECOVERED\nC 1\nBACK IN A\nB\nC 2\nAFTER A THRU C\n");
}

#[test]
fn a_handle_abend_label_is_entered_by_a_go_to_at_the_handle_abend_command() {
    let lines = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. DBGH.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE.",
        "PROCEDURE DIVISION.",
        "DECLARATIVES.",
        "DBG SECTION.",
        "    USE FOR DEBUGGING ON RECOVER.",
        "DBG-1.",
        "    DISPLAY DEBUG-NAME(1:8) '|' DEBUG-CONTENTS(1:4)",
        "        '|' DEBUG-LINE.",
        "END DECLARATIVES.",
        "MAIN SECTION.",
        "MAIN-LINE.",
        "    EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC",
        "    PERFORM WORK.",
        "WORK.",
        "    EXEC CICS ABEND ABCODE('W001') END-EXEC.",
        "RECOVER.",
        "    EXEC CICS RETURN END-EXEC.",
    ];
    let source: String = lines.iter().map(|l| format!("       {l}\n")).collect();
    let handle = lines.iter().position(|l| l.contains("HANDLE ABEND")).unwrap() + 1;
    let o = Harness::source(&source).task(task("TR13")).flags(&["-debug"]).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}", o.ending);
    assert_eq!(o.out, format!("RECOVER |    |{handle:06}\n"));
}

/// The output and abend code of a task whose first program runs `body`, then RETURN, with
/// RECOVER showing the abend code. It can CALL or LINK OWNL, which takes its own abend at its
/// label, SETP and SETL, which set a PROGRAM and a LABEL exit and return, BADP, which abends,
/// MIDP, which CALLs SETL and abends, and MIDX and MIDL, which set a PROGRAM and a LABEL exit and
/// XCTL to LASTP, which abends; EXITP and EXITC, which shows its COMMAREA, are PROGRAM exits.
fn handle_abend_task(body: &[&str]) -> (String, Option<String>) {
    let code = "       01  WS-CODE PIC X(4).\n";
    let area = "       01  DFHCOMMAREA PIC X(5).\n";
    let mut procedure = vec!["       MAIN-LINE.\n".to_owned()];
    procedure.extend(body.iter().map(|s| line(s)));
    procedure.extend([line("EXEC CICS RETURN END-EXEC."), "       RECOVER.\n".into()]);
    procedure.extend(["EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC", "DISPLAY 'RECOVERED ' WS-CODE", "EXEC CICS RETURN END-EXEC."].map(line));
    let main = cics_program("MAINP", &format!("{code}       01  WS-PGM PIC X(8).\n"), "", &procedure.concat());
    let own = [
        line("EXEC CICS HANDLE ABEND LABEL(OWN) END-EXEC"),
        line("EXEC CICS ABEND ABCODE('OW01') END-EXEC."),
        "       OWN.\n".into(),
        line("EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC"),
        line("DISPLAY 'OWN ' WS-CODE"),
        line("GOBACK."),
    ];
    let programs = [
        ("OWNL", cics_program("OWNL", code, "", &own.concat())),
        ("SETP", cics_program("SETP", "", "", &[line("EXEC CICS HANDLE ABEND PROGRAM('EXITP') END-EXEC"), line("GOBACK.")].concat())),
        ("SETL", cics_program("SETL", "", "", &[line("EXEC CICS HANDLE ABEND LABEL(GONE) END-EXEC"), line("GOBACK."), "       GONE.\n".into(), line("DISPLAY 'NOT REACHED'.")].concat())),
        ("BADP", cics_program("BADP", "", "", &line("EXEC CICS ABEND ABCODE('BD01') END-EXEC."))),
        ("MIDP", cics_program("MIDP", "", "", &[line("CALL 'SETL'"), line("DISPLAY 'BACK IN MID'"), line("EXEC CICS ABEND ABCODE('MD01') END-EXEC.")].concat())),
        ("EXITP", cics_program("EXITP", code, "", &[line("EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC"), line("DISPLAY 'EXIT ' WS-CODE"), line("EXEC CICS RETURN END-EXEC.")].concat())),
        ("MIDX", cics_program("MIDX", "       01  WS-XC PIC X(2) VALUE 'XC'.\n", area, &[line("EXEC CICS HANDLE ABEND PROGRAM('EXITC') END-EXEC"), line("EXEC CICS XCTL PROGRAM('LASTP') COMMAREA(WS-XC)"), line("    LENGTH(2) END-EXEC.")].concat())),
        ("MIDL", cics_program("MIDL", "", "", &[line("EXEC CICS HANDLE ABEND LABEL(OWN) END-EXEC"), line("EXEC CICS XCTL PROGRAM('LASTP') END-EXEC."), "       OWN.\n".into(), line("DISPLAY 'NOT REACHED'.")].concat())),
        ("LASTP", cics_program("LASTP", "", "", &line("EXEC CICS ABEND ABCODE('LS01') END-EXEC."))),
        ("EXITC", cics_program("EXITC", code, area, &[line("EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC"), line("DISPLAY 'EXIT ' DFHCOMMAREA ' ' EIBCALEN ' ' WS-CODE"), line("EXEC CICS RETURN END-EXEC.")].concat())),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR14"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

#[test]
fn a_statically_called_program_shares_the_levels_abend_exit_and_takes_only_a_label_of_its_own() {
    let own = handle_abend_task(&["CALL 'OWNL'", "DISPLAY 'BACK IN MAIN'", "CALL 'SETP'", "EXEC CICS ABEND ABCODE('MN01') END-EXEC"]);
    assert_eq!(own, ("OWN OW01\nBACK IN MAIN\nEXIT MN01\n".into(), None));
    assert_eq!(handle_abend_task(&["EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC", "CALL 'BADP'"]), (String::new(), Some("APC2".into())));
    assert_eq!(handle_abend_task(&["CALL 'SETL'", "EXEC CICS ABEND ABCODE('MN02') END-EXEC"]), (String::new(), Some("APC2".into())));
    let above = handle_abend_task(&["EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC", "EXEC CICS LINK PROGRAM('MIDP') END-EXEC"]);
    assert_eq!(above, ("BACK IN MID\nRECOVERED APC2\n".into(), None));
}

#[test]
fn a_dynamic_call_suspends_the_callers_abend_exit_until_the_subprogram_returns() {
    let call = |program: &str, after: &[&str]| {
        let mut body = vec!["EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC".to_owned(), format!("MOVE '{program}' TO WS-PGM"), "CALL WS-PGM".to_owned()];
        body.extend(after.iter().map(|s| s.to_string()));
        handle_abend_task(&body.iter().map(String::as_str).collect::<Vec<_>>())
    };
    assert_eq!(call("BADP", &[]), (String::new(), Some("BD01".into())));
    assert_eq!(call("SETP", &["EXEC CICS ABEND ABCODE('MN03') END-EXEC"]), ("RECOVERED MN03\n".into(), None));
    assert_eq!(call("OWNL", &["DISPLAY 'BACK IN MAIN'"]), ("OWN OW01\nBACK IN MAIN\n".into(), None));
}

#[test]
fn xctl_keeps_the_levels_abend_exit_and_a_program_exit_gets_the_commarea_of_the_program_that_set_it() {
    let program = handle_abend_task(&["MOVE 'MIDDL' TO WS-PGM", "EXEC CICS LINK PROGRAM('MIDX') COMMAREA(WS-PGM)", "    LENGTH(5) END-EXEC", "DISPLAY 'BACK IN MAIN'"]);
    assert_eq!(program, ("EXIT MIDDL 0005 LS01\nBACK IN MAIN\n".into(), None));
    let label = handle_abend_task(&["EXEC CICS HANDLE ABEND LABEL(RECOVER) END-EXEC", "EXEC CICS LINK PROGRAM('MIDL') END-EXEC"]);
    assert_eq!(label, ("RECOVERED APC2\n".into(), None));
}

type LevelEnd = Result<(Option<String>, Option<Vec<u8>>), String>;

/// The output of a task whose first program runs `body`, then RETURN, with the TRANSID and
/// COMMAREA the task ended with, or its abend code. RETP RETURNs, RETT RETURNs TRANSID('NEXT')
/// COMMAREA('DONE'), RETX TRANSID('NXT2') and RETI TRANSID('NXT3') IMMEDIATE, RETT and RETI
/// showing INVREQ and RESP2; XCTP XCTLs to LASTX, which shows its COMMAREA; STOPR runs STOP RUN
/// and XCTS XCTLs to it; MIDC PERFORMs a paragraph that CALLs RETP, and CALLER CALLs the program
/// its COMMAREA names.
fn level_task(body: &[&str]) -> (String, LevelEnd) {
    let mut procedure: Vec<String> = body.iter().map(|s| line(s)).collect();
    procedure.push(line("EXEC CICS RETURN END-EXEC."));
    let main = cics_program("MAINP", "       01  WS-PGM PIC X(8).\n", "", &procedure.concat());
    let invreq = "IF WS-RESP = DFHRESP(INVREQ) DISPLAY 'INVREQ ' WS-R2 END-IF";
    let rett = ["EXEC CICS RETURN TRANSID('NEXT') COMMAREA(WS-OUT) LENGTH(4)", "    RESP(WS-RESP) RESP2(WS-R2) END-EXEC", invreq, "GOBACK."];
    let reti = ["EXEC CICS RETURN TRANSID('NXT3') IMMEDIATE", "    RESP(WS-RESP) RESP2(WS-R2) END-EXEC", invreq, "GOBACK."];
    let resp = "       01  WS-RESP PIC S9(8) COMP.\n       01  WS-R2 PIC S9(8) COMP.\n";
    let midc = ["       MID-LINE.\n".to_owned(), line("PERFORM CALL-RETP"), line("DISPLAY 'AFTER PERFORM'"), line("GOBACK."), "       CALL-RETP.\n".to_owned(), line("CALL 'RETP'"), line("DISPLAY 'AFTER CALL'.")];
    let programs = [
        ("RETP", cics_program("RETP", "", "", &["DISPLAY 'IN RETP'", "EXEC CICS RETURN END-EXEC", "DISPLAY 'AFTER RETURN'", "GOBACK."].map(line).concat())),
        ("RETT", cics_program("RETT", &format!("       01  WS-OUT PIC X(4) VALUE 'DONE'.\n{resp}"), "", &rett.map(line).concat())),
        ("RETX", cics_program("RETX", "", "", &["EXEC CICS RETURN TRANSID('NXT2') END-EXEC", "DISPLAY 'AFTER RETURN'", "GOBACK."].map(line).concat())),
        ("RETI", cics_program("RETI", resp, "", &reti.map(line).concat())),
        ("XCTP", cics_program("XCTP", "       01  WS-XC PIC X(2) VALUE 'XC'.\n", "", &["EXEC CICS XCTL PROGRAM('LASTX') COMMAREA(WS-XC)", "    LENGTH(2) END-EXEC", "DISPLAY 'AFTER XCTL'", "GOBACK."].map(line).concat())),
        ("LASTX", cics_program("LASTX", "", "       01  DFHCOMMAREA PIC X(2).\n", &["DISPLAY 'LAST ' DFHCOMMAREA ' ' EIBCALEN", "EXEC CICS RETURN END-EXEC."].map(line).concat())),
        ("STOPR", cics_program("STOPR", "", "", &["DISPLAY 'IN STOPR'", "STOP RUN."].map(line).concat())),
        ("XCTS", cics_program("XCTS", "", "", &["EXEC CICS XCTL PROGRAM('STOPR') END-EXEC", "DISPLAY 'AFTER XCTL'", "GOBACK."].map(line).concat())),
        ("MIDC", cics_program("MIDC", "", "", &midc.concat())),
        ("CALLER", cics_program("CALLER", "       01  WS-NAME PIC X(8).\n", "       01  DFHCOMMAREA PIC X(8).\n", &["MOVE DFHCOMMAREA TO WS-NAME", "CALL WS-NAME", "DISPLAY 'BACK IN CALLER'", "EXEC CICS RETURN END-EXEC."].map(line).concat())),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR15"), None, unit::Clock::System);
    (out, ending.map(|(_, t)| (t.next_transid, t.returned_commarea)).map_err(|a| a.code.to_string()))
}

/// `level_task` with CALLER LINKed to, CALLing `program`.
fn linked_caller(program: &str) -> (String, LevelEnd) {
    let name = format!("MOVE '{program}' TO WS-PGM");
    level_task(&[&name, "EXEC CICS LINK PROGRAM('CALLER') COMMAREA(WS-PGM)", "    LENGTH(8) END-EXEC", "DISPLAY 'BACK IN MAIN'"])
}

#[test]
fn return_in_a_called_program_ends_its_logical_level() {
    let ended = |out: &str| (out.to_owned(), Ok((None, None)));
    assert_eq!(level_task(&["CALL 'RETP'", "DISPLAY 'BACK IN MAIN'"]), ended("IN RETP\n"));
    let dynamic = ["MOVE 'RETP' TO WS-PGM", "CALL WS-PGM", "    NOT ON EXCEPTION DISPLAY 'NOT ON EXCEPTION'", "END-CALL", "DISPLAY 'BACK IN MAIN'"];
    assert_eq!(level_task(&dynamic), ended("IN RETP\n"));
    assert_eq!(level_task(&["CALL 'MIDC'", "DISPLAY 'BACK IN MAIN'"]), ended("IN RETP\n"));
    assert_eq!(level_task(&["EXEC CICS LINK PROGRAM('MIDC') END-EXEC", "DISPLAY 'BACK IN MAIN'"]), ended("IN RETP\nBACK IN MAIN\n"));
    assert_eq!(linked_caller("RETP"), ended("IN RETP\nBACK IN MAIN\n"));
}

#[test]
fn return_transid_and_commarea_in_a_called_program_belong_to_its_level() {
    assert_eq!(level_task(&["CALL 'RETT'", "DISPLAY 'BACK IN MAIN'"]), (String::new(), Ok((Some("NEXT".into()), Some(ebcdic("DONE"))))));
    assert_eq!(linked_caller("RETT"), ("INVREQ 00000002\nBACK IN CALLER\nBACK IN MAIN\n".into(), Ok((None, None))));
}

#[test]
fn return_below_the_first_level_names_the_next_transaction_and_refuses_what_belongs_to_cics() {
    let link = |program: &str| level_task(&[&format!("EXEC CICS LINK PROGRAM('{program}') END-EXEC"), "DISPLAY 'BACK IN MAIN'"]);
    assert_eq!(link("RETX"), ("BACK IN MAIN\n".into(), Ok((Some("NXT2".into()), None))));
    assert_eq!(link("RETI"), ("INVREQ 00000002\nBACK IN MAIN\n".into(), Ok((None, None))));
    assert_eq!(level_task(&["CALL 'RETI'", "DISPLAY 'BACK IN MAIN'"]), (String::new(), Ok((Some("NXT3".into()), None))));
    let replaced = level_task(&["EXEC CICS LINK PROGRAM('RETX') END-EXEC", "CALL 'RETT'"]);
    assert_eq!(replaced, (String::new(), Ok((Some("NEXT".into()), Some(ebcdic("DONE"))))));
}

type ReturnedLength = Result<(Option<String>, Option<usize>), String>;

/// The output of a task whose first program, given no COMMAREA, runs `body`, then RETURN, with the
/// TRANSID and the length of the COMMAREA the task ended with, or its abend code. WS-BIG is 32764
/// bytes, LK-AREA has no address until SET ADDRESS OF gives it one, LEN-ERR shows EIBRESP and
/// EIBRESP2, RETX, LINKed to, RETURNs TRANSID('NXT2'), and SHOWCA, LINKed or XCTLed to, shows an
/// EIBCALEN under 100.
fn return_length_task(body: &[&str]) -> (String, ReturnedLength) {
    let data = "       01  WS-BIG PIC X(32764) VALUE ALL 'B'.\n       01  WS-LEN PIC S9(4) COMP.\n       01  WS-RESP PIC S9(8) COMP.\n       01  WS-R2 PIC S9(8) COMP.\n";
    let linkage = "       01  DFHCOMMAREA PIC X(4).\n       01  LK-AREA PIC X(8).\n";
    let mut procedure = vec!["       MAIN-LINE.\n".to_owned()];
    procedure.extend(body.iter().map(|s| line(s)));
    procedure.extend([line("EXEC CICS RETURN END-EXEC."), "       LEN-ERR.\n".into(), line("DISPLAY 'HANDLED ' EIBRESP ' ' EIBRESP2"), line("EXEC CICS RETURN END-EXEC.")]);
    let main = cics_program("MAINP", data, linkage, &procedure.concat());
    let retx = cics_program("RETX", "", "", &line("EXEC CICS RETURN TRANSID('NXT2') END-EXEC."));
    let showca = ["IF EIBCALEN < 100 DISPLAY 'SHOWCA ' EIBCALEN", "ELSE DISPLAY 'SHOWCA LONG' END-IF", "EXEC CICS RETURN END-EXEC."];
    let showca = cics_program("SHOWCA", "", "", &showca.map(line).concat());
    let source = format!("{main}       END PROGRAM MAINP.\n{retx}       END PROGRAM RETX.\n{showca}       END PROGRAM SHOWCA.\n");
    let (out, ending) = run_cics(&source, task("TR21"), None, unit::Clock::System);
    (out, ending.map(|(_, t)| (t.next_transid, t.returned_commarea.map(|c| c.len()))).map_err(|a| a.code.to_string()))
}

#[test]
fn return_with_a_commarea_length_outside_0_to_32763_raises_lengerr() {
    let resp = "    RESP(WS-RESP) RESP2(WS-R2) END-EXEC";
    let lengerr = "IF WS-RESP = DFHRESP(LENGERR) DISPLAY 'LENGERR ' WS-R2 END-IF";
    let ret = |length: &str| return_length_task(&["EXEC CICS RETURN TRANSID('NEXT') COMMAREA(WS-BIG)", length, resp, lengerr]);
    let refused = |out: &str| (out.to_owned(), Ok((None, None)));
    assert_eq!(ret("    LENGTH(32763)"), (String::new(), Ok((Some("NEXT".into()), Some(32763)))));
    assert_eq!(ret("    LENGTH(0)"), (String::new(), Ok((Some("NEXT".into()), Some(0)))));
    assert_eq!(ret("    LENGTH(32764)"), refused("LENGERR 00000011\n"));
    assert_eq!(ret(""), refused("LENGERR 00000011\n"));
    let negative = ["MOVE -1 TO WS-LEN", "EXEC CICS RETURN COMMAREA(WS-BIG) LENGTH(WS-LEN)"];
    assert_eq!(return_length_task(&[negative[0], negative[1], resp, lengerr]), refused("LENGERR 00000011\n"));
    let handled = ["EXEC CICS HANDLE CONDITION LENGERR(LEN-ERR) END-EXEC", negative[0], negative[1], "    END-EXEC"];
    assert_eq!(return_length_task(&handled), refused("HANDLED 00000022 00000011\n"));
    assert_eq!(return_length_task(&[negative[0], negative[1], "    END-EXEC"]), (String::new(), Err("AEIV".into())));
    let cleared = ["EXEC CICS LINK PROGRAM('RETX') END-EXEC", negative[0], negative[1], resp, lengerr];
    assert_eq!(return_length_task(&cleared), refused("LENGERR 00000011\n"));
    assert_eq!(return_length_task(&["EXEC CICS LINK PROGRAM('RETX') END-EXEC"]), (String::new(), Ok((Some("NXT2".into()), None))));
    let zero = |length: &str| return_length_task(&["EXEC CICS RETURN TRANSID('NEXT') COMMAREA(LK-AREA)", length, resp, lengerr]);
    assert_eq!(zero(""), refused("LENGERR 00000026\n"));
    assert_eq!(zero("    LENGTH(0)"), (String::new(), Ok((Some("NEXT".into()), Some(0)))));
}

#[test]
fn link_and_xctl_raise_lengerr_for_a_commarea_length_outside_0_to_32763_or_at_address_zero() {
    let resp = "    RESP(WS-RESP) RESP2(WS-R2) END-EXEC";
    let lengerr = "IF WS-RESP = DFHRESP(LENGERR) DISPLAY 'LENGERR ' WS-R2 END-IF";
    let ended = |out: &str| (out.to_owned(), Ok((None, None)));
    for verb in ["LINK", "XCTL"] {
        let to = |area: &str| format!("EXEC CICS {verb} PROGRAM('SHOWCA') COMMAREA({area})");
        let run = |area: &str, length: &str| return_length_task(&[&to(area), length, resp, lengerr]);
        assert_eq!(run("WS-BIG", "    LENGTH(32763)"), ended("SHOWCA LONG\n"), "{verb}");
        assert_eq!(run("WS-BIG", "    LENGTH(32764)"), ended("LENGERR 00000011\n"), "{verb}");
        assert_eq!(run("WS-BIG", ""), ended("LENGERR 00000011\n"), "{verb}");
        assert_eq!(run("DFHCOMMAREA", ""), ended("LENGERR 00000026\n"), "{verb}");
        assert_eq!(run("LK-AREA", "    LENGTH(8)"), ended("LENGERR 00000026\n"), "{verb}");
        assert_eq!(run("LK-AREA", "    LENGTH(0)"), ended("SHOWCA 0000\n"), "{verb}");
        let set = "SET ADDRESS OF LK-AREA TO ADDRESS OF WS-BIG";
        assert_eq!(return_length_task(&[set, &to("LK-AREA"), resp, lengerr]), ended("SHOWCA 0008\n"), "{verb}");
        let big = to("WS-BIG");
        let negative = ["MOVE -1 TO WS-LEN", big.as_str(), "    LENGTH(WS-LEN)"];
        let handled = ["EXEC CICS HANDLE CONDITION LENGERR(LEN-ERR) END-EXEC", negative[0], negative[1], negative[2], "    END-EXEC"];
        assert_eq!(return_length_task(&handled), ended("HANDLED 00000022 00000011\n"), "{verb}");
        assert_eq!(return_length_task(&[negative[0], negative[1], negative[2], "    END-EXEC"]), (String::new(), Err("AEIV".into())), "{verb}");
        let missing = format!("EXEC CICS {verb} PROGRAM('NOPROG')");
        let pgmiderr = "IF WS-RESP = DFHRESP(PGMIDERR) DISPLAY 'PGMID ' WS-R2 END-IF";
        assert_eq!(return_length_task(&[&missing, resp, pgmiderr]), ended("PGMID 00000001\n"), "{verb}");
        assert_eq!(return_length_task(&[&missing, "    COMMAREA(WS-BIG)", resp, lengerr]), ended("LENGERR 00000011\n"), "{verb}");
    }
}

const READ_NO_QUEUE: [&str; 2] = ["EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA)", "    LENGTH(WS-LEN) END-EXEC"];

/// The output and abend code of a task whose first program runs `body`, then RETURN, with
/// MAIN-ERR a QIDERR label of its own. READP raises QIDERR with no handler of its own; SETH and
/// SETI HANDLE and IGNORE QIDERR and return; OWNH takes its own QIDERR at its label; POPP pops a
/// PUSH HANDLE, showing INVREQ when there is none, and POPR does too, then RETURNs or, after
/// ABEND1, abends; SHOWAB is a HANDLE ABEND PROGRAM.
fn condition_task(body: &[&str]) -> (String, Option<String>) {
    let data = "       01  WS-DATA PIC X(8).\n       01  WS-LEN PIC S9(4) COMP VALUE 8.\n       01  WS-PGM PIC X(8).\n       01  WS-RESP PIC S9(8) COMP.\n       01  WS-CODE PIC X(4).\n";
    let mut procedure = vec!["       MAIN-LINE.\n".to_owned()];
    procedure.extend(body.iter().map(|s| line(s)));
    procedure.extend([line("EXEC CICS RETURN END-EXEC."), "       MAIN-ERR.\n".into(), line("DISPLAY 'MAIN HANDLED'"), line("EXEC CICS RETURN END-EXEC.")]);
    let main = cics_program("MAINP", data, "", &procedure.concat());
    let read = || READ_NO_QUEUE.map(line).concat();
    let own = [line("EXEC CICS HANDLE CONDITION QIDERR(OWN-ERR) END-EXEC"), read(), line("DISPLAY 'NOT REACHED'."), "       OWN-ERR.\n".into(), line("DISPLAY 'OWN HANDLED'"), line("GOBACK.")];
    let pop = ["EXEC CICS POP HANDLE RESP(WS-RESP) END-EXEC", "IF WS-RESP = DFHRESP(INVREQ) DISPLAY 'NOTHING TO POP' END-IF", "GOBACK."];
    let show = ["EXEC CICS ASSIGN ABCODE(WS-CODE) END-EXEC", "DISPLAY 'ABEND ' WS-CODE", "EXEC CICS RETURN END-EXEC."];
    let popr = [line(pop[0]), line(pop[1]), read(), line("DISPLAY 'POPR GOES ON'"), line("EXEC CICS RETURN END-EXEC.")].concat();
    let abend1 = [line(pop[0]), line(pop[1]), line("EXEC CICS ABEND ABCODE('XC01') END-EXEC.")].concat();
    let programs = [
        ("READP", cics_program("READP", data, "", &[read(), line("DISPLAY 'READP GOES ON'"), line("GOBACK.")].concat())),
        ("SETH", cics_program("SETH", "", "", &[line("EXEC CICS HANDLE CONDITION QIDERR(SET-ERR) END-EXEC"), line("GOBACK."), "       SET-ERR.\n".into(), line("DISPLAY 'NOT REACHED'.")].concat())),
        ("SETI", cics_program("SETI", "", "", &[line("EXEC CICS IGNORE CONDITION QIDERR END-EXEC"), line("GOBACK.")].concat())),
        ("OWNH", cics_program("OWNH", data, "", &own.concat())),
        ("POPP", cics_program("POPP", "       01  WS-RESP PIC S9(8) COMP.\n", "", &pop.map(line).concat())),
        ("SHOWAB", cics_program("SHOWAB", "       01  WS-CODE PIC X(4).\n", "", &show.map(line).concat())),
        ("POPR", cics_program("POPR", data, "", &popr)),
        ("ABEND1", cics_program("ABEND1", data, "", &abend1)),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR16"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

/// `condition_task` with `before`, a CALL of `program`, statically or dynamically, and a READQ
/// that raises QIDERR.
fn condition_call(before: &[&str], program: &str, dynamic: bool) -> (String, Option<String>) {
    let call = if dynamic { vec![format!("MOVE '{program}' TO WS-PGM"), "CALL WS-PGM".to_owned()] } else { vec![format!("CALL '{program}'")] };
    let mut body: Vec<String> = before.iter().map(|s| s.to_string()).collect();
    body.extend(call);
    body.extend(READ_NO_QUEUE.map(str::to_owned));
    body.push("DISPLAY 'MAIN GOES ON'".into());
    condition_task(&body.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn a_static_call_shares_the_levels_condition_handlers_both_ways() {
    let ignore = "EXEC CICS IGNORE CONDITION QIDERR END-EXEC";
    let handle = "EXEC CICS HANDLE CONDITION QIDERR(MAIN-ERR) END-EXEC";
    assert_eq!(condition_call(&[ignore], "READP", false), ("READP GOES ON\nMAIN GOES ON\n".into(), None));
    assert_eq!(condition_call(&[handle], "READP", false), (String::new(), Some("APC2".into())));
    assert_eq!(condition_call(&[], "SETI", false), ("MAIN GOES ON\n".into(), None));
    assert_eq!(condition_call(&[], "SETH", false), (String::new(), Some("APC2".into())));
    assert_eq!(condition_call(&[handle], "OWNH", false), ("OWN HANDLED\n".into(), Some("APC2".into())));
    assert_eq!(condition_call(&[ignore, "EXEC CICS PUSH HANDLE END-EXEC"], "POPP", false), ("MAIN GOES ON\n".into(), None));
    assert_eq!(condition_call(&[], "POPP", false), ("NOTHING TO POP\n".into(), Some("AEYH".into())));
    let caught = condition_call(&["EXEC CICS HANDLE ABEND PROGRAM('SHOWAB') END-EXEC", handle], "READP", false);
    assert_eq!(caught, ("ABEND APC2\n".into(), None));
}

#[test]
fn a_dynamic_call_suspends_the_condition_handlers_until_the_subprogram_returns() {
    let ignore = "EXEC CICS IGNORE CONDITION QIDERR END-EXEC";
    let handle = "EXEC CICS HANDLE CONDITION QIDERR(MAIN-ERR) END-EXEC";
    assert_eq!(condition_call(&[ignore], "READP", true), (String::new(), Some("AEYH".into())));
    assert_eq!(condition_call(&[handle], "READP", true), (String::new(), Some("AEYH".into())));
    assert_eq!(condition_call(&[], "SETI", true), (String::new(), Some("AEYH".into())));
    assert_eq!(condition_call(&[ignore], "SETH", true), ("MAIN GOES ON\n".into(), None));
    assert_eq!(condition_call(&[handle], "OWNH", true), ("OWN HANDLED\nMAIN HANDLED\n".into(), None));
    assert_eq!(condition_call(&[ignore], "POPP", true), ("MAIN GOES ON\n".into(), None));
}

#[test]
fn stop_run_below_the_first_level_returns_to_the_program_that_linked_to_the_level() {
    let back = |out: &str| (out.to_owned(), Ok((None, None)));
    assert_eq!(level_task(&["EXEC CICS LINK PROGRAM('STOPR') END-EXEC", "DISPLAY 'BACK IN MAIN'"]), back("IN STOPR\nBACK IN MAIN\n"));
    assert_eq!(linked_caller("STOPR"), back("IN STOPR\nBACK IN MAIN\n"));
    assert_eq!(level_task(&["EXEC CICS LINK PROGRAM('XCTS') END-EXEC", "DISPLAY 'BACK IN MAIN'"]), back("IN STOPR\nBACK IN MAIN\n"));
    assert_eq!(level_task(&["CALL 'STOPR'", "DISPLAY 'BACK IN MAIN'"]), back("IN STOPR\n"));
    assert_eq!(level_task(&["EXEC CICS XCTL PROGRAM('STOPR') END-EXEC"]), back("IN STOPR\n"));
}

/// The output and abend code of a task whose first program runs `body`, then RETURN. COUNTER
/// counts its CALLs; TWICE CALLs it twice and HANDOFF once before XCTL to TWICE. SUB counts its
/// CALLs and, CALLed with the task's EXEC interface block, LINKs to SUBL with its count as the
/// COMMAREA when EIBCALEN is 0; SUBL shows the COMMAREA around a CALL of SUB.
fn run_unit_task(body: &[&str]) -> (String, Option<String>) {
    let mut procedure: Vec<String> = body.iter().map(|s| line(s)).collect();
    procedure.push(line("EXEC CICS RETURN END-EXEC."));
    let main = cics_program("MAINP", "", "", &procedure.concat());
    let count = "       01  N PIC 9 VALUE 0.\n";
    let sub = ["ADD 1 TO N", "IF EIBCALEN = 0", "    EXEC CICS LINK PROGRAM('SUBL') COMMAREA(N) LENGTH(1)", "    END-EXEC", "END-IF", "DISPLAY 'SUB ' N ' AT ' EIBCALEN", "GOBACK."];
    let subl = ["DISPLAY 'SUBL ' DFHCOMMAREA", "CALL 'SUB' USING DFHEIBLK", "DISPLAY 'SUBL ' DFHCOMMAREA", "EXEC CICS RETURN END-EXEC."];
    let programs = [
        ("COUNTER", cics_program("COUNTER", count, "", &["ADD 1 TO N", "DISPLAY 'COUNTER ' N", "GOBACK."].map(line).concat())),
        ("TWICE", cics_program("TWICE", "", "", &["CALL 'COUNTER'", "CALL 'COUNTER'", "EXEC CICS RETURN END-EXEC."].map(line).concat())),
        ("HANDOFF", cics_program("HANDOFF", "", "", &["CALL 'COUNTER'", "EXEC CICS XCTL PROGRAM('TWICE') END-EXEC."].map(line).concat())),
        ("SUB", cics_program("SUB", count, "", &sub.map(line).concat())),
        ("SUBL", cics_program("SUBL", "", "       01  DFHCOMMAREA PIC X.\n", &subl.map(line).concat())),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR17"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

#[test]
fn a_called_program_starts_afresh_in_each_run_unit_a_link_or_xctl_starts() {
    let counted = run_unit_task(&["CALL 'COUNTER'", "EXEC CICS LINK PROGRAM('TWICE') END-EXEC", "EXEC CICS LINK PROGRAM('HANDOFF') END-EXEC", "CALL 'COUNTER'"]);
    assert_eq!(counted, ("COUNTER 1\nCOUNTER 1\nCOUNTER 2\nCOUNTER 1\nCOUNTER 1\nCOUNTER 2\nCOUNTER 2\n".into(), None));
    let level = |n: u8| format!("SUBL {n}\nSUB 1 AT 0001\nSUBL {n}\nSUB {n} AT 0000\n");
    assert_eq!(run_unit_task(&["CALL 'SUB' USING DFHEIBLK", "CALL 'SUB' USING DFHEIBLK"]), (level(1) + &level(2), None));
}

#[test]
fn xctl_drops_the_programs_condition_handlers_and_keeps_the_levels_push_handle_stack() {
    let (ignore, push) = ("EXEC CICS IGNORE CONDITION QIDERR END-EXEC", "EXEC CICS PUSH HANDLE END-EXEC");
    let handle = "EXEC CICS HANDLE CONDITION QIDERR(MAIN-ERR) END-EXEC";
    let xctl = |to: &str| format!("EXEC CICS XCTL PROGRAM('{to}') END-EXEC");
    assert_eq!(condition_task(&[ignore, &xctl("READP")]), (String::new(), Some("AEYH".into())));
    assert_eq!(condition_task(&[handle, &xctl("READP")]), (String::new(), Some("AEYH".into())));
    assert_eq!(condition_task(&[&xctl("POPR")]), ("NOTHING TO POP\n".into(), Some("AEYH".into())));
    assert_eq!(condition_task(&[ignore, push, &xctl("POPR")]), (String::new(), Some("AEYH".into())));
    let exit = "EXEC CICS HANDLE ABEND PROGRAM('SHOWAB') END-EXEC";
    assert_eq!(condition_task(&[exit, &xctl("ABEND1")]), ("NOTHING TO POP\nABEND XC01\n".into(), None));
    assert_eq!(condition_task(&[exit, push, &xctl("ABEND1")]), ("ABEND XC01\n".into(), None));
    assert_eq!(condition_task(&[exit, push, push, &xctl("ABEND1")]), (String::new(), Some("XC01".into())));
}

/// The output and abend code of a task whose first program, MAINP, counts its activations in N,
/// shows the COMMAREA it was given and RETURNs, or with none runs `body` and shows N. BACKX XCTLs
/// to MAINP, CALLX does so from a CALL, LINKM LINKs to it, and CALLM CALLs it with the EXEC
/// interface block and its COMMAREA.
fn first_program_task(body: &[&str]) -> (String, Option<String>) {
    let again = ["ADD 1 TO N", "IF EIBCALEN > 0", "    DISPLAY 'MAIN AGAIN ' N ' ' DFHCOMMAREA", "    EXEC CICS RETURN END-EXEC", "END-IF"];
    let mut procedure: Vec<String> = again.iter().chain(body).map(|s| line(s)).collect();
    procedure.extend([line("DISPLAY 'MAIN ENDS ' N"), line("EXEC CICS RETURN END-EXEC.")]);
    let main = cics_program("MAINP", "       01  N PIC 9 VALUE 0.\n", "       01  DFHCOMMAREA PIC X(4).\n", &procedure.concat());
    let to_main = |verb: &str, area: &str| [line(&format!("EXEC CICS {verb} PROGRAM('MAINP') COMMAREA('{area}') LENGTH(4)")), line("    END-EXEC"), line("EXEC CICS RETURN END-EXEC.")].concat();
    let callm = ["CALL 'MAINP' USING DFHEIBLK DFHCOMMAREA", "DISPLAY 'BACK IN CALLM'", "EXEC CICS RETURN END-EXEC."].map(line).concat();
    let programs = [("BACKX", to_main("XCTL", "XCTL")), ("CALLX", to_main("XCTL", "CALL")), ("LINKM", to_main("LINK", "LINK")), ("CALLM", callm)];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, procedure) in programs {
        source.push_str(&format!("{}       END PROGRAM {id}.\n", cics_program(id, "", "       01  DFHCOMMAREA PIC X(4).\n", &procedure)));
    }
    let (out, ending) = run_cics(&source, task("TR19"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

#[test]
fn a_call_of_the_tasks_first_program_is_recursive_in_its_run_unit_and_fresh_in_another() {
    assert_eq!(first_program_task(&["CALL 'CALLM' USING DFHEIBLK"]), (String::new(), Some("4038".into())));
    let linked = first_program_task(&["EXEC CICS LINK PROGRAM('CALLM') COMMAREA('LINK')", "    LENGTH(4) END-EXEC", "ADD 1 TO N"]);
    assert_eq!(linked, ("MAIN AGAIN 1 LINK\nMAIN ENDS 2\n".into(), None));
}

#[test]
fn xctl_and_link_start_the_tasks_first_program_afresh_wherever_it_is() {
    assert_eq!(first_program_task(&["EXEC CICS XCTL PROGRAM('BACKX') END-EXEC"]), ("MAIN AGAIN 1 XCTL\n".into(), None));
    assert_eq!(first_program_task(&["CALL 'CALLX'", "DISPLAY 'NOT REACHED'"]), ("MAIN AGAIN 1 CALL\n".into(), None));
    assert_eq!(first_program_task(&["EXEC CICS LINK PROGRAM('LINKM') END-EXEC", "ADD 1 TO N"]), ("MAIN AGAIN 1 LINK\nMAIN ENDS 2\n".into(), None));
}

/// The output and abend code of a task whose first program, MAINP, has EXTERNAL record EXT-REC
/// and a CEEGTST block at HEAP-PTR, and runs `body`, then RETURN. EXTL shows EXT-REC, sets it and
/// CALLs EXTC, which shows it; HEAPL frees the block its COMMAREA points to and puts one of its
/// own there.
fn enclave_task(body: &[&str]) -> (String, Option<String>) {
    let ext = "       01  EXT-REC PIC X(4) EXTERNAL.\n";
    let heap = "       01  WS-HEAP PIC S9(9) BINARY VALUE 0.\n       01  WS-SIZE PIC S9(9) BINARY VALUE 16.\n       01  FC.\n           05 FC-SEV PIC 9(4) BINARY.\n           05 FC-MSG PIC 9(4) BINARY.\n           05 FILLER PIC X(8).\n";
    let mut procedure: Vec<String> = ["MOVE 'MAIN' TO EXT-REC", "CALL 'CEEGTST' USING WS-HEAP WS-SIZE HEAP-PTR FC", "SET AREA-PTR TO HEAP-PTR"].iter().chain(body).map(|s| line(s)).collect();
    procedure.push(line("EXEC CICS RETURN END-EXEC."));
    let main = cics_program("MAINP", &format!("{ext}{heap}       01  HEAP-PTR POINTER.\n       01  WS-AREA.\n           05 AREA-PTR POINTER.\n"), "", &procedure.concat());
    let extl = ["IF EXT-REC = LOW-VALUES DISPLAY 'EXTL FRESH'", "ELSE DISPLAY 'EXTL ' EXT-REC END-IF", "MOVE 'LINK' TO EXT-REC", "CALL 'EXTC'", "EXEC CICS RETURN END-EXEC."];
    let heapl = ["CALL 'CEEFRST' USING LK-PTR FC", "DISPLAY 'HEAPL FREES OUTER ' FC-MSG", "CALL 'CEEGTST' USING WS-HEAP WS-SIZE LK-PTR FC", "EXEC CICS RETURN END-EXEC."];
    let programs = [
        ("EXTL", cics_program("EXTL", ext, "", &extl.map(line).concat())),
        ("EXTC", cics_program("EXTC", ext, "", &["DISPLAY 'EXTC ' EXT-REC", "GOBACK."].map(line).concat())),
        ("HEAPL", cics_program("HEAPL", heap, "       01  DFHCOMMAREA.\n           05 LK-PTR POINTER.\n", &heapl.map(line).concat())),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR20"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

#[test]
fn each_run_unit_a_link_starts_has_external_data_and_heap_storage_of_its_own() {
    let link = "EXEC CICS LINK PROGRAM('EXTL') END-EXEC";
    let external = enclave_task(&[link, "DISPLAY 'MAIN ' EXT-REC", link, "CALL 'EXTC'"]);
    assert_eq!(external, ("EXTL FRESH\nEXTC LINK\nMAIN MAIN\nEXTL FRESH\nEXTC LINK\nEXTC MAIN\n".into(), None));
    let heap = [
        "EXEC CICS LINK PROGRAM('HEAPL') COMMAREA(WS-AREA) LENGTH(4)",
        "    END-EXEC",
        "CALL 'CEEFRST' USING AREA-PTR FC",
        "DISPLAY 'MAIN FREES INNER ' FC-MSG",
        "CALL 'CEEFRST' USING HEAP-PTR FC",
        "DISPLAY 'MAIN FREES OWN ' FC-MSG",
    ];
    assert_eq!(enclave_task(&heap), ("HEAPL FREES OUTER 0810\nMAIN FREES INNER 0810\nMAIN FREES OWN 0000\n".into(), None));
}

/// The output and abend code of a task whose first program runs `body`, then RETURN. RANDL shows
/// FUNCTION RANDOM's next value; RCL shows the RETURN-CODE it starts with, raises INVREQ and
/// RETURNs with RETURN-CODE 12, and RCS does the same with 4 and STOP RUN; RCX XCTLs to RCL with
/// RETURN-CODE 3; PTRL twice CALLs the procedure-pointer its COMMAREA holds, and PTRC counts its
/// CALLs.
fn run_unit_state_task(body: &[&str]) -> (String, Option<String>) {
    let data = "       01  N PIC 9(9).\n       01  WS-RESP PIC S9(8) COMP.\n       01  WS-R2 PIC S9(8) COMP.\n       01  WS-AREA.\n           05 WS-PP USAGE PROCEDURE-POINTER.\n";
    let mut procedure: Vec<String> = body.iter().map(|s| line(s)).collect();
    procedure.push(line("EXEC CICS RETURN END-EXEC."));
    let main = cics_program("MAINP", data, "", &procedure.concat());
    let rc = |id: &str, code: u8, end: &str| {
        let procedure = [format!("DISPLAY '{id} ' RETURN-CODE"), "EXEC CICS POP HANDLE RESP(WS-RESP) END-EXEC".into(), format!("MOVE {code} TO RETURN-CODE"), end.into()];
        cics_program(id, "       01  WS-RESP PIC S9(8) COMP.\n", "", &procedure.map(|s| line(&s)).concat())
    };
    let randl = ["COMPUTE N = FUNCTION RANDOM * 1000000000", "DISPLAY 'RANDL ' N", "EXEC CICS RETURN END-EXEC."];
    let pointer = "       01  DFHCOMMAREA.\n           05 LK-PP USAGE PROCEDURE-POINTER.\n";
    let programs = [
        ("RANDL", cics_program("RANDL", "       01  N PIC 9(9).\n", "", &randl.map(line).concat())),
        ("RCL", rc("RCL", 12, "EXEC CICS RETURN END-EXEC.")),
        ("RCS", rc("RCS", 4, "STOP RUN.")),
        ("RCX", cics_program("RCX", "", "", &["MOVE 3 TO RETURN-CODE", "EXEC CICS XCTL PROGRAM('RCL') END-EXEC."].map(line).concat())),
        ("PTRL", cics_program("PTRL", "", pointer, &["CALL LK-PP", "CALL LK-PP", "EXEC CICS RETURN END-EXEC."].map(line).concat())),
        ("PTRC", cics_program("PTRC", "       01  CALLS PIC 9 VALUE 0.\n", "", &["ADD 1 TO CALLS", "DISPLAY 'PTRC ' CALLS", "GOBACK."].map(line).concat())),
    ];
    let mut source = format!("{main}       END PROGRAM MAINP.\n");
    for (id, program) in programs {
        source.push_str(&format!("{program}       END PROGRAM {id}.\n"));
    }
    let (out, ending) = run_cics(&source, task("TR24"), None, unit::Clock::System);
    (out, ending.err().map(|a| a.code.to_string()))
}

#[test]
fn each_run_unit_a_link_or_xctl_starts_has_a_random_sequence_of_its_own() {
    let (seed, draw, show) = ("COMPUTE N = FUNCTION RANDOM(42)", "COMPUTE N = FUNCTION RANDOM * 1000000000", "DISPLAY 'MAIN ' N");
    let (seeded, _) = run_unit_state_task(&[seed, draw, show]);
    let linked = run_unit_state_task(&[seed, "EXEC CICS LINK PROGRAM('RANDL') END-EXEC", draw, show]);
    assert_eq!(linked, (format!("RANDL 000007826\n{seeded}"), None));
    assert_eq!(run_unit_state_task(&[seed, "EXEC CICS XCTL PROGRAM('RANDL') END-EXEC"]), ("RANDL 000007826\n".into(), None));
}

#[test]
fn a_link_gives_the_return_code_its_run_unit_ends_with_as_resp2_and_keeps_the_callers() {
    let shown = ["DISPLAY 'MAIN ' RETURN-CODE ' ' WS-R2", "DISPLAY 'EIB ' EIBRESP ' ' EIBRESP2"];
    let link = |program: &str| {
        let command = format!("EXEC CICS LINK PROGRAM('{program}') RESP(WS-RESP) RESP2(WS-R2)");
        run_unit_state_task(&["MOVE 5 TO RETURN-CODE", &command, "    END-EXEC", shown[0], shown[1]])
    };
    assert_eq!(link("RCL"), ("RCL 0000\nMAIN 0005 00000012\nEIB 00000000 00000012\n".into(), None));
    assert_eq!(link("RCS"), ("RCS 0000\nMAIN 0005 00000004\nEIB 00000000 00000004\n".into(), None));
    assert_eq!(link("RCX"), ("RCL 0000\nMAIN 0005 00000012\nEIB 00000000 00000012\n".into(), None));
    assert_eq!(run_unit_state_task(&["MOVE 5 TO RETURN-CODE", "EXEC CICS XCTL PROGRAM('RCL') END-EXEC"]), ("RCL 0000\n".into(), None));
}

#[test]
fn a_procedure_pointer_set_in_one_run_unit_enters_its_program_in_the_run_unit_calling_it() {
    let body = ["SET WS-PP TO ENTRY 'PTRC'", "CALL WS-PP", "EXEC CICS LINK PROGRAM('PTRL') COMMAREA(WS-AREA)", "    END-EXEC", "CALL WS-PP"];
    assert_eq!(run_unit_state_task(&body), ("PTRC 1\nPTRC 1\nPTRC 2\nPTRC 2\n".into(), None));
}

/// The output of a task whose first program runs `body`, then RETURN, and how many bytes of memory
/// its run unit is left with. LINKP CALLs COUNTED, whose WORKING-STORAGE of 10,001 bytes counts
/// its CALLs, and RETURNs.
fn released_task(body: &[&str]) -> (String, usize) {
    let mut procedure: Vec<String> = body.iter().map(|s| line(s)).collect();
    procedure.push(line("EXEC CICS RETURN END-EXEC."));
    let counted = cics_program("COUNTED", "       01  N PIC 9 VALUE 0.\n       01  BIG PIC X(10000).\n", "", &["ADD 1 TO N", "DISPLAY 'COUNTED ' N", "GOBACK."].map(line).concat());
    let linkp = cics_program("LINKP", "", "", &["CALL 'COUNTED'", "EXEC CICS RETURN END-EXEC."].map(line).concat());
    let source = format!("{}       END PROGRAM MAINP.\n{counted}       END PROGRAM COUNTED.\n{linkp}       END PROGRAM LINKP.\n", cics_program("MAINP", "", "", &procedure.concat()));
    let (out, ending) = run_cics(&source, task("TR22"), None, unit::Clock::System);
    assert!(ending.is_ok(), "{ending:?}");
    let mut programs = syntax::parse_all_with(&source, &syntax::copy::Libraries::default()).unwrap();
    let compiled = compile(programs.remove(0), &[]).unwrap();
    let (mut kept, mut sink, mut errors) = (None, Vec::new(), Vec::new());
    let library = unit::Library { programs, ..Default::default() };
    let (ran, _) = execute_task(&compiled, library, files::Dds::new(&[], false).unwrap(), task("TR22"), unit::Clock::System, None, &mut sink, &mut errors, None, &mut kept);
    assert!(ran.is_ok(), "{ran:?}");
    (out, kept.unwrap().mem.len())
}

#[test]
fn a_program_first_loaded_in_a_links_run_unit_is_released_when_it_ends() {
    let link = "EXEC CICS LINK PROGRAM('LINKP') END-EXEC";
    let (_, none) = released_task(&[]);
    let (out, once) = released_task(&[link]);
    assert_eq!(out, "COUNTED 1\n");
    assert!(once < none + 1_000, "{once} bytes after a LINK, {none} with none");
    assert_eq!(released_task(&[link, link, link]).1, once);
    let (out, called) = released_task(&[link, link, "CALL 'COUNTED'", link, "CALL 'COUNTED'"]);
    assert_eq!(out, "COUNTED 1\nCOUNTED 1\nCOUNTED 1\nCOUNTED 1\nCOUNTED 2\n");
    assert!((once + 10_001..once + 10_100).contains(&called), "{called} bytes after a CALL at the first level, {once} without");
}

#[test]
fn xctl_in_a_called_program_replaces_the_program_running_its_level() {
    assert_eq!(level_task(&["CALL 'XCTP'", "DISPLAY 'BACK IN MAIN'"]), ("LAST XC 0002\n".into(), Ok((None, None))));
    assert_eq!(linked_caller("XCTP"), ("LAST XC 0002\nBACK IN MAIN\n".into(), Ok((None, None))));
}

#[test]
fn a_program_check_in_a_cics_task_is_asra() {
    let source = cics_program("CICS7", "", "       01  DFHCOMMAREA PIC X(10).\n", &line("DISPLAY DFHCOMMAREA."));
    let (_, ending) = run_cics(&source, task("TR07"), None, unit::Clock::System);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "ASRA");
    assert!(abend.message.contains("S0C4"));
}

/// A Language Environment condition nothing handles ends a CICS task with transaction abend 4038
/// (assumption C454).
#[test]
fn a_language_environment_condition_in_a_cics_task_is_transaction_abend_4038() {
    let source = cics_program("CICS8", "       01  X COMP-2.\n", "", &line("COMPUTE X = FUNCTION RANDOM(-1)."));
    let (_, ending) = run_cics(&source, task("TR08"), None, unit::Clock::System);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "4038");
    assert_eq!(abend.message, "IGZ0163S Argument-1 for function RANDOM was less than zero. (-1) (U4038, which CICS reports as transaction abend 4038)");
}

#[test]
fn exec_cics_outside_a_task_names_the_harness() {
    let source = cics_program("CICS6", "", "", &line("EXEC CICS RETURN END-EXEC."));
    let (_, _, ending) = run_with(&source, &[]);
    assert!(ending.unwrap_err().message.contains("ironwork cics"));
}

#[test]
fn cics_file_control_reads_writes_rewrites_deletes_and_browses_a_ksds() {
    let path = temp("cics-ksds.txt");
    std::fs::write(&path, "001ALICE\n002BOB\n003CAROL\n").unwrap();
    let data = "       01  WS-REC.\n           05 WS-KEY PIC X(3).\n           05 WS-NAME PIC X(9).\n       01  WS-RID PIC X(3).\n       01  WS-RESP PIC S9(8) COMP.\n";
    let source = cics_program(
        "CICSF",
        data,
        "",
        &[
            line("MOVE '002' TO WS-RID"),
            line("EXEC CICS READ FILE('CUSTF') INTO(WS-REC) RIDFLD(WS-RID)"),
            line("    END-EXEC"),
            line("DISPLAY WS-NAME"),
            line("MOVE '009' TO WS-RID"),
            line("EXEC CICS READ FILE('CUSTF') INTO(WS-REC) RIDFLD(WS-RID)"),
            line("    RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(NOTFND) DISPLAY 'NOTFND' END-IF"),
            line("MOVE '0' TO WS-RID"),
            line("EXEC CICS READ FILE('CUSTF') INTO(WS-REC) RIDFLD(WS-RID)"),
            line("    KEYLENGTH(1) GENERIC GTEQ END-EXEC"),
            line("DISPLAY WS-RID ' ' WS-NAME"),
            line("MOVE '004DAVE     ' TO WS-REC"),
            line("EXEC CICS WRITE FILE('CUSTF') FROM(WS-REC) RIDFLD(WS-KEY)"),
            line("    END-EXEC"),
            line("EXEC CICS WRITE FILE('CUSTF') FROM(WS-REC) RIDFLD(WS-KEY)"),
            line("    RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(DUPREC) DISPLAY 'DUPREC' END-IF"),
            line("MOVE '003' TO WS-RID"),
            line("EXEC CICS READ FILE('CUSTF') INTO(WS-REC) RIDFLD(WS-RID)"),
            line("    UPDATE END-EXEC"),
            line("MOVE 'CAROLINE' TO WS-NAME"),
            line("EXEC CICS REWRITE FILE('CUSTF') FROM(WS-REC) END-EXEC"),
            line("EXEC CICS REWRITE FILE('CUSTF') FROM(WS-REC) RESP(WS-RESP)"),
            line("    END-EXEC"),
            line("IF WS-RESP = DFHRESP(INVREQ) DISPLAY 'INVREQ' END-IF"),
            line("MOVE '001' TO WS-RID"),
            line("EXEC CICS DELETE FILE('CUSTF') RIDFLD(WS-RID) END-EXEC"),
            line("MOVE LOW-VALUES TO WS-RID"),
            line("EXEC CICS STARTBR FILE('CUSTF') RIDFLD(WS-RID) GTEQ"),
            line("    END-EXEC"),
            line("PERFORM 4 TIMES"),
            line("    EXEC CICS READNEXT FILE('CUSTF') INTO(WS-REC)"),
            line("        RIDFLD(WS-RID) RESP(WS-RESP) END-EXEC"),
            line("    IF WS-RESP = DFHRESP(NORMAL)"),
            line("        DISPLAY 'NEXT ' WS-RID ' ' WS-NAME"),
            line("    ELSE"),
            line("        IF WS-RESP = DFHRESP(ENDFILE) DISPLAY 'END' END-IF"),
            line("    END-IF"),
            line("END-PERFORM"),
            line("EXEC CICS ENDBR FILE('CUSTF') END-EXEC"),
            line("EXEC CICS READ FILE('NOFILE') INTO(WS-REC) RIDFLD(WS-RID)"),
            line("    RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(FILENOTFOUND) DISPLAY 'FILENOTFOUND'"),
            line("END-IF"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let mut t = task("TRF1");
    let (name, def) = cics::parse_file(&format!("CUSTF={},KSDS,key=0:3,len=12,text", path.display())).unwrap();
    t.files.insert(name, def);
    let (out, ending) = run_cics(&source, t, None, unit::Clock::System);
    assert!(ending.is_ok(), "{ending:?}\n{out}");
    assert_eq!(
        out,
        "BOB      \nNOTFND\n001 ALICE    \nDUPREC\nINVREQ\nNEXT 002 BOB      \nNEXT 003 CAROLINE \nNEXT 004 DAVE     \nEND\nFILENOTFOUND\n"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "002BOB\n003CAROLINE\n004DAVE\n");
}

fn bms_line(text: &str, continued: bool) -> String {
    if continued { format!("{text:<71}X\n") } else { format!("{text}\n") }
}

/// Writes ORDSET, a mapset holding the one map ORDMAP, in `dir`.
fn ordset(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    let bms = [
        bms_line("ORDSET   DFHMSD TYPE=&SYSPARM,MODE=INOUT,LANG=COBOL,STORAGE=AUTO,", true),
        bms_line("               CTRL=(FREEKB,FRSET)", false),
        bms_line("ORDMAP   DFHMDI SIZE=(24,80),LINE=1,COLUMN=1", false),
        bms_line("         DFHMDF POS=(1,1),LENGTH=12,ATTRB=(ASKIP,BRT),", true),
        bms_line("               INITIAL='ORDER ENTRY'", false),
        bms_line("         DFHMDF POS=(3,1),LENGTH=9,ATTRB=ASKIP,INITIAL='CUSTOMER:'", false),
        bms_line("CUST     DFHMDF POS=(3,11),LENGTH=8,ATTRB=(UNPROT,IC)", false),
        bms_line("         DFHMDF POS=(3,20),LENGTH=1,ATTRB=ASKIP", false),
        bms_line("QTY      DFHMDF POS=(4,11),LENGTH=3,ATTRB=(UNPROT,NUM)", false),
        bms_line("         DFHMDF POS=(4,15),LENGTH=1,ATTRB=ASKIP", false),
        bms_line("MSG      DFHMDF POS=(6,1),LENGTH=30,ATTRB=(ASKIP,BRT)", false),
        bms_line("         DFHMSD TYPE=FINAL", false),
        bms_line("         END", false),
    ]
    .concat();
    std::fs::write(dir.join("ORDSET.bms"), bms).unwrap();
}

/// Runs `source`, with `dir` as its copy library, as a CICS task on a terminal that plays `script`,
/// on the VM with `vm`: what it displays, each screen it sends, and how it ends.
fn on_terminal(source: &str, dir: &std::path::Path, script: &str, vm: bool) -> (String, Vec<String>, Result<Ending, String>) {
    let libraries = syntax::copy::Libraries::new(vec![dir.to_path_buf()]);
    let mut programs = syntax::parse_all_with(source, &libraries).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let scripted = terminal::Scripted::new(24, 80, terminal::parse_script(script).unwrap(), compiled.options.code_page());
    let shown = scripted.shown.clone();
    let t = cics::Task { terminal: Some(Box::new(scripted)), ..task("ORD1") };
    let library = unit::Library { programs, copy: libraries, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let clock = unit::Clock::Fixed(1_790_514_309, 25);
    let ending = if vm {
        let code = crate::vm::lowered(&compiled).unwrap_or_else(|e| panic!("{e}"));
        match crate::vm::execute_cics(&compiled, &code, library, files::Dds::default(), t, clock, None, &mut out, &mut err, None, &mut None).0 {
            Ok(ending) => Ok(ending),
            Err(crate::vm::Halt::Abend(abend)) => Err(format!("{abend:?}")),
            Err(crate::vm::Halt::Unimplemented(what)) => Err(format!("not run yet: {what}")),
        }
    } else {
        compiled.execute_cics(library, files::Dds::default(), t, clock, &mut out, &mut err).map(|(ending, _)| ending).map_err(|abend| format!("{abend:?}"))
    };
    let screens = shown.borrow().clone();
    (String::from_utf8(out).unwrap(), screens, ending)
}

#[test]
fn bms_maps_send_and_receive_through_a_scripted_terminal() {
    let dir = temp("bms");
    ordset(&dir);
    let source = cics_program(
        "ORDERS",
        "           COPY ORDSET.\n           COPY DFHAID.\n       01  WS-RESP PIC S9(8) COMP.\n",
        "",
        &[
            line("MOVE LOW-VALUES TO ORDMAPO"),
            line("MOVE 'ENTER AN ORDER' TO MSGO"),
            line("EXEC CICS SEND MAP('ORDMAP') MAPSET('ORDSET') ERASE"),
            line("    END-EXEC"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET') END-EXEC"),
            line("DISPLAY 'CUST ' CUSTI ' L=' CUSTL ' QTY ' QTYI"),
            line("IF EIBAID = DFHENTER DISPLAY 'ENTER' END-IF"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET')"),
            line("    RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(MAPFAIL) DISPLAY 'MAPFAIL' END-IF"),
            line("IF EIBAID = DFHCLEAR DISPLAY 'CLEAR' END-IF"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let script = "type 3 12 ACME\ntype 4 12 7\nENTER\nCLEAR\n";
    let (out, screens, ending) = on_terminal(&source, &dir, script, false);
    assert!(ending.is_ok(), "{ending:?}\n{out}");
    assert_eq!(out, "CUST ACME     L=0004 QTY 007\nENTER\nMAPFAIL\nCLEAR\n");
    let rows: Vec<&str> = screens[0].lines().collect();
    assert_eq!(rows[0], " ORDER ENTRY");
    assert_eq!(rows[2], " CUSTOMER:");
    assert_eq!(rows[5], " ENTER AN ORDER");
    assert_eq!(on_terminal(&source, &dir, script, true), (out, screens, ending));
}

#[test]
fn bms_maps_sent_from_and_received_into_a_named_area_run_alike_on_the_vm() {
    let dir = temp("bms-vm");
    ordset(&dir);
    let source = cics_program(
        "ORDERS",
        "           COPY ORDSET.\n       01  WS-RESP PIC S9(8) COMP.\n",
        "",
        &[
            line("MOVE LOW-VALUES TO ORDMAPO"),
            line("MOVE 'ENTER AN ORDER' TO MSGO"),
            line("MOVE -1 TO QTYL"),
            line("EXEC CICS SEND MAP('ORDMAP') MAPSET('ORDSET') FROM(ORDMAPO)"),
            line("    ERASE CURSOR END-EXEC"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET')"),
            line("    INTO(ORDMAPI) END-EXEC"),
            line("DISPLAY 'CUST ' CUSTI ' L=' CUSTL ' QTY ' QTYI ' AID ' EIBAID"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET')"),
            line("    INTO(ORDMAPI) RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(MAPFAIL) DISPLAY 'MAPFAIL' END-IF"),
            line("EXEC CICS SEND CONTROL ERASE FREEKB END-EXEC"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let script = "type 3 12 ACME\ntype 4 12 7\nENTER\nCLEAR\n";
    let interpreted = on_terminal(&source, &dir, script, false);
    assert_eq!((interpreted.0.as_str(), &interpreted.2), ("CUST ACME     L=0004 QTY 007 AID '\nMAPFAIL\n", &Ok(Ending::Goback)));
    assert_eq!(on_terminal(&source, &dir, script, true), interpreted);
}

#[test]
fn a_receive_flag_in_the_attribute_byte_leaves_the_field_its_attrb() {
    let dir = temp("bms-flags");
    ordset(&dir);
    let source = cics_program(
        "ORDERS",
        "           COPY ORDSET.\n",
        "",
        &[
            line("MOVE LOW-VALUES TO ORDMAPO"),
            line("MOVE X'82' TO MSGA"),
            line("EXEC CICS SEND MAP('ORDMAP') MAPSET('ORDSET') ERASE"),
            line("    END-EXEC"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET') END-EXEC"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let script = "type 6 2 HELLO\nENTER\n";
    let (_, _, ending) = on_terminal(&source, &dir, script, false);
    assert!(ending.as_ref().is_err_and(|e| e.contains("row 6 column 2 is protected")), "{ending:?}");
    assert_eq!(on_terminal(&source, &dir, script, true).2, ending);
}

#[test]
fn receive_map_of_text_typed_on_an_unformatted_screen_is_mapfail() {
    let dir = temp("bms-unformatted");
    ordset(&dir);
    let source = cics_program(
        "ORDBLANK",
        "           COPY ORDSET.\n       01  WS-RESP PIC S9(8) COMP.\n",
        "",
        &[
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET')"),
            line("    INTO(ORDMAPI) RESP(WS-RESP) END-EXEC"),
            line("IF WS-RESP = DFHRESP(MAPFAIL) DISPLAY 'MAPFAIL' END-IF"),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let script = "home\nstring ORD1 ACME 7\nENTER\n";
    let interpreted = on_terminal(&source, &dir, script, false);
    assert_eq!((interpreted.0.as_str(), &interpreted.2), ("MAPFAIL\n", &Ok(Ending::Goback)));
    assert_eq!(on_terminal(&source, &dir, script, true), interpreted);
}

#[test]
fn records_of_different_lengths_with_no_record_clause_make_a_variable_file() {
    let data = temp("variable-records.dat");
    let source = file_program(
        "           SELECT F ASSIGN TO FDD.\n",
        "       FD  F.\n       01  SHORT-R PIC X(3).\n       01  LONG-R PIC X(6).\n",
        "",
        &[line("OPEN OUTPUT F"), line("WRITE SHORT-R FROM 'ABC'"), line("WRITE LONG-R FROM 'DEFGHI'"), line("CLOSE F"), line("GOBACK.")].concat(),
    );
    let (_, err, ending) = run_files(&source, &[format!("FDD={}", data.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(std::fs::metadata(&data).map(|m| m.len()).unwrap_or(0), 4 + 3 + 4 + 6);
    let _ = std::fs::remove_file(data);
}

#[test]
fn a_password_item_must_be_an_alphanumeric_item_of_working_storage() {
    let source = |item: &str| {
        file_program(
            "           SELECT K ASSIGN KDD ORGANIZATION INDEXED\n               RECORD KEY IS KK PASSWORD IS PW.\n",
            "       FD  K.\n       01  KR.\n           05 KK PIC X.\n",
            item,
            &line("GOBACK."),
        )
    };
    assert!(!compile_errors(&source("       01  PW PIC X(8).\n")).contains("PW"));
    assert!(compile_errors(&source("       01  PW PIC 9(8).\n")).contains("PW: the PASSWORD of K must be an alphabetic, alphanumeric or alphanumeric-edited item of WORKING-STORAGE"));
    assert!(compile_errors(&source("       01  OTHER PIC X(8).\n")).contains("PW is not defined"));
}

#[test]
fn record_varying_depending_on_gives_the_length_written_and_takes_the_length_read() {
    let data = temp("record-depending.dat");
    let source = file_program(
        "           SELECT F ASSIGN TO FDD FILE STATUS IS STAT.\n",
        "       FD  F RECORD IS VARYING IN SIZE FROM 1 TO 10\n           DEPENDING ON REC-LEN.\n       01  R PIC X(10).\n",
        "       01  REC-LEN PIC 9(4) COMP.\n       01  STAT PIC XX.\n       01  W PIC X(10) VALUE ALL '*'.\n",
        &[
            line("OPEN OUTPUT F"),
            line("MOVE 3 TO REC-LEN"),
            line("WRITE R FROM 'ABCDEFGHIJ'"),
            line("MOVE 5 TO REC-LEN"),
            line("WRITE R FROM 'KLMNOPQRST'"),
            line("MOVE 11 TO REC-LEN"),
            line("WRITE R"),
            line("DISPLAY STAT"),
            line("CLOSE F"),
            line("OPEN INPUT F"),
            line("READ F INTO W"),
            line("DISPLAY REC-LEN ' ' W '|'"),
            line("READ F"),
            line("DISPLAY REC-LEN ' ' R(1:REC-LEN)"),
            line("CLOSE F"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[format!("FDD={}", data.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "44\n0003 ABC       |\n0005 KLMNO\n");
    assert_eq!(std::fs::metadata(&data).map(|m| m.len()).unwrap_or(0), 4 + 3 + 4 + 5);
    let _ = std::fs::remove_file(data);
    let signed = source.replace("REC-LEN PIC 9(4)", "REC-LEN PIC S9(4)");
    assert!(compile_errors(&signed).contains("REC-LEN: the DEPENDING ON item of F must be an elementary unsigned integer"), "{}", compile_errors(&signed));
}

/// Each abbreviated combined relation condition and its unabbreviated form agree for every A, B, C
/// and D from 1 to 3: the Language Reference's examples (SC27-8713-03, p. 289, Table 31) and the
/// cases assumption C150 decides.
#[test]
fn abbreviated_combined_relations_mean_what_they_abbreviate() {
    let pairs = [
        ("A = B AND NOT < C OR D", "((A = B) AND (A NOT < C)) OR (A NOT < D)"),
        ("A NOT > B OR C", "(A NOT > B) OR (A NOT > C)"),
        ("NOT A = B OR C", "(NOT (A = B)) OR (A = C)"),
        ("NOT (A = B OR < C)", "NOT ((A = B) OR (A < C))"),
        ("NOT (A NOT = B AND C AND NOT D)", "NOT ((((A NOT = B) AND (A NOT = C)) AND (NOT (A NOT = D))))"),
        ("A > 1 AND <= B", "A > 1 AND A <= B"),
        ("A GREATER THAN B AND IS NOT LESS THAN C OR D", "(A > B AND A NOT < C) OR A NOT < D"),
        ("A = (B OR C AND NOT D)", "A = B OR A = C AND NOT A = D"),
        ("A NOT = (B OR C) AND D", "(A NOT = B OR A NOT = C) AND A NOT = D"),
        ("A = B AND (C OR < D) OR 2", "A = B AND (A = C OR A < D) OR A = 2"),
        ("A = B AND NOT NOT = C", "A = B AND NOT (A NOT = C)"),
        ("(A = B OR C) AND D-ON", "(A = B OR A = C) AND D = 3"),
    ];
    let mut body = vec!["       M.\n".to_owned(), line("PERFORM P VARYING A FROM 1 BY 1 UNTIL A > 3"), line("    AFTER B FROM 1 BY 1 UNTIL B > 3")];
    body.extend([line("    AFTER C FROM 1 BY 1 UNTIL C > 3"), line("    AFTER D FROM 1 BY 1 UNTIL D > 3"), line("DISPLAY LONG-TRUE ' ' SHORT-TRUE")]);
    body.extend([line("STOP RUN."), "       P.\n".to_owned()]);
    for (k, (short, long)) in pairs.iter().enumerate() {
        for (cond, count) in [(short, "S"), (long, "L")] {
            let (first, rest) = cond.split_at(if cond.len() > 40 { cond[..40].rfind(' ').unwrap() } else { cond.len() });
            body.extend([line(&format!("IF {first}")), line(&format!("   {rest}")), line(&format!("    ADD 1 TO {count} ({})", k + 1)), line("END-IF")]);
        }
    }
    let data = concat!(
        "       01  A PIC 9.\n       01  B PIC 9.\n       01  C PIC 9.\n       01  D PIC 9.\n           88 D-ON VALUE 3.\n",
        "       01  LONG-TRUE.\n           05 L PIC 99 OCCURS 12 VALUE 0.\n       01  SHORT-TRUE.\n           05 S PIC 99 OCCURS 12 VALUE 0.\n",
    );
    let out = run(&program("", data, &body.concat()));
    let (w, y) = out.trim_end().split_once(' ').unwrap();
    assert_eq!(w, y);
    assert!(w.as_bytes().chunks(2).all(|n| n != b"00" && n != b"81"), "{w}");
}
