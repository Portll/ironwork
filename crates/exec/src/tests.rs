use super::*;
use crate::testing::{Executor, Harness, compile_errors, ebcdic, line};
use compile::refused;
use numeric::Options;
use std::collections::BTreeMap;
use syntax::ast::Stmt;
use syntax::{Error, Severity};

mod collating;
mod corresponding;
mod data;
mod data_division;
mod declaratives;
mod diagnostics;
mod intrinsic;
mod json;
mod json_parse;
mod linage;
mod numcheck;
mod oo;
mod parmcheck;
mod printer;
mod procedure;
mod quote_currency_nsymbol;
mod report;
mod sort;
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

#[test]
fn a_vsam_open_is_00_under_either_vsamopenfs_setting_as_no_open_verifies_a_file() {
    let path = temp("vsamopenfs.ksds");
    let source = |card: &str, body: &[&str]| {
        let program = file_program(
            "           SELECT K-FILE ASSIGN TO KDD ORGANIZATION INDEXED\n               RECORD KEY K-KEY FILE STATUS IS FS.\n           SELECT X-FILE ASSIGN TO NODD.\n",
            "       FD  K-FILE.\n       01  K-REC.\n           05 K-KEY PIC X(4).\n       FD  X-FILE.\n       01  X-REC PIC X.\n",
            "       01  FS PIC XX.\n",
            &body.iter().map(|l| line(l)).collect::<String>(),
        );
        format!("{card}{program}")
    };
    let dds = [format!("KDD={}", path.display())];
    let abends = ["OPEN OUTPUT K-FILE", "MOVE 'K001' TO K-KEY", "WRITE K-REC", "OPEN INPUT X-FILE", "GOBACK."];
    let reopens = ["OPEN I-O K-FILE", "DISPLAY FS", "READ K-FILE", "DISPLAY FS ' ' K-KEY", "GOBACK."];
    for card in ["       CBL VSAMOPENFS(COMPAT)\n", "       CBL VS(S)\n"] {
        let (_, _, ending) = run_files(&source(card, &abends), &dds);
        assert_eq!(ending.unwrap_err().code, "IO-35", "{card}");
        let (out, err, ending) = run_files(&source(card, &reopens), &dds);
        assert!(ending.is_ok(), "{ending:?} {err}");
        assert_eq!(out, "00\n00 K001\n", "{card}: the abend left the data set closed, so there is nothing to verify");
    }
    std::fs::remove_file(&path).unwrap();
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

#[test]
fn an_unhandled_io_failure_ends_the_run() {
    let source = file_program("           SELECT X-F ASSIGN TO NODD.\n", "       FD  X-F.\n       01  X-REC PIC X.\n", "", &[line("OPEN INPUT X-F"), line("GOBACK.")].concat());
    let (_, _, ending) = run_files(&source, &[]);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "IO-35");
    assert!(abend.message.contains("--dd NODD=path"));
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
    assert_eq!(abend.code, "IO-23");
    assert!(abend.message.contains("no record with that key"), "{}", abend.message);
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
    assert!(abend.message.starts_with("EXEC DLI GN was reached"), "{}", abend.message);
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

/// Mutated programs may be refused, but must never panic the reader, lexer, parser or compiler.
/// `IRONWORK_FUZZ_ITERATIONS` raises the count for a longer run.
#[test]
fn mutated_programs_never_panic_the_front_end() {
    let iterations: usize = std::env::var("IRONWORK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000);
    let corpus: Vec<String> = oracle::programs()
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
        .collect();
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
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert_eq!(ending.as_ref().map(|e| e.1), Ok(4), "{ending:?} {err}");
    assert_eq!(out, "XYZ BBB 0107 0004\n0201\n0101\n");
}

#[test]
fn a_missing_program_takes_on_exception_or_abends_s806() {
    let body = [
        line("MOVE 'NOPE' TO NAME"),
        line("CALL NAME ON EXCEPTION DISPLAY 'NOT FOUND' END-CALL"),
        line("CALL 'NOPE2'"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", "       01  NAME PIC X(8).\n", &body);
    let (out, _, ending) = run_unit(&source, vec![], "");
    assert_eq!(out, "NOT FOUND\n");
    assert_eq!(ending.unwrap_err().code, "S806");
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
fn a_program_check_in_a_cics_task_is_asra() {
    let source = cics_program("CICS7", "", "       01  DFHCOMMAREA PIC X(10).\n", &line("DISPLAY DFHCOMMAREA."));
    let (_, ending) = run_cics(&source, task("TR07"), None, unit::Clock::System);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "ASRA");
    assert!(abend.message.contains("S0C4"));
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

#[test]
fn bms_maps_send_and_receive_through_a_scripted_terminal() {
    let dir = temp("bms");
    std::fs::create_dir_all(&dir).unwrap();
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
    let libraries = syntax::copy::Libraries::new(vec![dir.clone()]);
    let mut programs = syntax::parse_all_with(&source, &libraries).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let page = compiled.options.code_page();
    let script = terminal::parse_script("type 3 12 ACME\ntype 4 12 7\nENTER\nCLEAR\n").unwrap();
    let scripted = terminal::Scripted::new(24, 80, script, page);
    let shown = scripted.shown.clone();
    let t = cics::Task { terminal: Some(Box::new(scripted)), ..task("ORD1") };
    let library = unit::Library { programs, copy: libraries, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ending = compiled.execute_cics(library, files::Dds::default(), t, unit::Clock::System, &mut out, &mut err);
    let out = String::from_utf8(out).unwrap();
    assert!(ending.is_ok(), "{ending:?}\n{out}");
    assert_eq!(out, "CUST ACME     L=0004 QTY 007\nENTER\nMAPFAIL\nCLEAR\n");
    let screen = shown.borrow()[0].clone();
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows[0], " ORDER ENTRY");
    assert_eq!(rows[2], " CUSTOMER:");
    assert_eq!(rows[5], " ENTER AN ORDER");
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
