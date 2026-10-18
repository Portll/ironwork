use super::*;

const EXTENDED: &[&str] = &["--compliance=extended"];

/// What the program prints and how it ends, the same on the interpreter and the VM.
fn runs(source: &str, flags: &[&str], expected: &str) {
    let walked = Harness::source(source).flags(flags).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(source).flags(flags).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending), "{}", vm.err);
}

fn ids_under(source: &str, flags: &[&str]) -> Vec<String> {
    let flags: Vec<String> = flags.iter().map(|f| f.to_string()).collect();
    let compliance = if flags.is_empty() { numeric::Compliance::Strict } else { numeric::Compliance::Extended };
    match syntax::parse_with(source, &syntax::copy::Libraries::default().with_compliance(compliance)) {
        Err(e) => vec![e.id.unwrap_or_default().to_owned()],
        Ok(parsed) => match compile(parsed, &flags) {
            Ok(c) => c.diagnostics.iter().filter_map(|m| m.id.map(str::to_owned)).collect(),
            Err(errors) => errors.iter().filter_map(|m| m.id.map(str::to_owned)).collect(),
        },
    }
}

const TYPEDEFS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. TD.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 DOLLARS TYPEDEF PIC 9(5)V99.\n",
    "       01 PAIR TYPEDEF.\n",
    "          05 LO PIC 9(2) VALUE 0.\n",
    "          05 HI PIC 9(2) VALUE 0.\n",
    "       77 AMOUNT TYPE DOLLARS VALUE 12.34.\n",
    "       01 SPAN TYPE PAIR.\n",
    "       01 ROW.\n",
    "          05 CELL USAGE DOLLARS OCCURS 2.\n",
    "          05 BOUNDS TYPE PAIR.\n",
    "       PROCEDURE DIVISION.\n",
    "           ADD 1 TO AMOUNT.\n",
    "           MOVE 12 TO LO OF SPAN.\n",
    "           MOVE 34 TO HI OF BOUNDS.\n",
    "           MOVE 5.5 TO CELL(2).\n",
    "           DISPLAY AMOUNT ' ' SPAN ' ' CELL(2) ' ' HI OF BOUNDS.\n",
    "           GOBACK.\n",
);

#[test]
fn a_type_clause_copies_the_typedef_beneath_the_item() {
    // cobc 3.2's output.
    runs(TYPEDEFS, EXTENDED, "00013.34 1200 00005.50 34\n");
}

#[test]
fn typedef_is_read_by_strict_but_not_its_strong_form() {
    assert!(ids_under(TYPEDEFS, &[]).is_empty());
    let strong = TYPEDEFS.replace("PAIR TYPEDEF.", "PAIR TYPEDEF STRONG.");
    assert_eq!(ids_under(&strong, &[]), ["IWR0003"]);
    assert_eq!(ids_under(&strong, EXTENDED), ["IWX0114"]);
    let undefined = TYPEDEFS.replace("TYPE DOLLARS", "TYPE EUROS");
    assert_eq!(ids_under(&undefined, EXTENDED), ["IWC0341"]);
}

const EXTERNAL_AS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. EXTAS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 WRK-VAR PIC X(10) EXTERNAL.\n",
    "       01 MYVAR PIC X(10) EXTERNAL AS \"COB-VAR\".\n",
    "       01 EXT-VAR PIC X(10) EXTERNAL.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 'Extrn' TO WRK-VAR\n",
    "           MOVE 'Hello' TO MYVAR\n",
    "           MOVE SPACES TO EXT-VAR\n",
    "           CALL 'CALLEE'\n",
    "           DISPLAY '[' EXT-VAR ']'\n",
    "           GOBACK.\n",
    "       END PROGRAM EXTAS.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CALLEE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 PRG-VAR PIC X(10) EXTERNAL AS 'WRK-VAR'.\n",
    "       01 COB-VAR PIC X(10) EXTERNAL.\n",
    "       01 EXT-VAR PIC X(10) EXTERNAL.\n",
    "       PROCEDURE DIVISION.\n",
    "           IF PRG-VAR = 'Extrn' AND COB-VAR = 'Hello'\n",
    "               MOVE 'World' TO EXT-VAR\n",
    "           END-IF\n",
    "           EXIT PROGRAM.\n",
    "       END PROGRAM CALLEE.\n",
);

#[test]
fn external_as_shares_the_record_under_the_literal() {
    runs(EXTERNAL_AS, EXTENDED, "[World     ]\n");
    assert_eq!(ids_under(EXTERNAL_AS, EXTENDED), ["IWX0115"]);
    assert_eq!(ids_under(EXTERNAL_AS, &[]), ["IWC0342"]);
}

const ALGEBRAIC: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ALG.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 N-S999   PIC S999.\n",
    "       01 N-U999   PIC 999.\n",
    "       01 N-S9KB   PIC S9(4) BINARY.\n",
    "       01 N-S9C5   PIC S9(4) COMP-5.\n",
    "       01 N-99V999 PIC S99V9(3).\n",
    "       01 N-EDCR   PIC $**,**9.99BCR.\n",
    "       01 N-ED     PIC $**,**9.99.\n",
    "       01 N-BCS    USAGE BINARY-CHAR SIGNED.\n",
    "       01 N-BCU    USAGE BINARY-CHAR UNSIGNED.\n",
    "       01 N-PS     PIC SPP99.\n",
    "       01 N-PL     PIC 99PP.\n",
    "       01 SR       PIC +9(6).9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-S999) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-U999) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION HIGHEST-ALGEBRAIC(N-S9KB) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-S9C5) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-99V999) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-EDCR) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-ED) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION HIGHEST-ALGEBRAIC(N-ED) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION LOWEST-ALGEBRAIC(N-BCS) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION HIGHEST-ALGEBRAIC(N-BCU) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION HIGHEST-ALGEBRAIC(N-PS) TO SR DISPLAY SR\n",
    "           MOVE FUNCTION HIGHEST-ALGEBRAIC(N-PL) TO SR DISPLAY SR\n",
    "           GOBACK.\n",
);

#[test]
fn highest_and_lowest_algebraic_give_the_description_s_limits() {
    // cobc 3.2's output, run on the same description.
    runs(
        ALGEBRAIC,
        EXTENDED,
        "-000999.0000\n+000000.0000\n+009999.0000\n-032768.0000\n-000099.9990\n-099999.9900\n+000000.0000\n+099999.9900\n-000128.0000\n+000255.0000\n+000000.0099\n+999900.0000\n",
    );
    assert!(ids_under(ALGEBRAIC, &[]).iter().any(|i| i == "IWC0343"));
    assert_eq!(ids_under(ALGEBRAIC, EXTENDED).iter().filter(|i| *i == "IWX0116").count(), 12);
}

fn under(body: &str) -> String {
    format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n{body}")
}

#[test]
fn a_boolean_picture_is_one_byte_a_position() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 WS-A PIC A(4) VALUE 'ABCD'.\n",
        "       01 WS-B PIC 1(3).\n",
        "       01 WS-C PIC 1 VALUE 1.\n",
        "       PROCEDURE DIVISION.\n",
        "           MOVE WS-A(1:2) TO WS-B.\n",
        "           MOVE '10' TO WS-B\n",
        "           DISPLAY '[' WS-B '][' WS-C ']' FUNCTION LENGTH(WS-B)\n",
        "           GOBACK.\n",
    ));
    // cobc 3.2's output.
    runs(&source, EXTENDED, "[010][1]3\n");
    assert_eq!(ids_under(&source, &[]), ["IWC0261"]);
}

#[test]
fn display_may_name_its_device_after_no_advancing() {
    let source = under(concat!(
        "       PROCEDURE DIVISION.\n",
        "           DISPLAY 'A' WITH NO ADVANCING UPON SYSOUT\n",
        "           DISPLAY 'B' UPON SYSOUT\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "AB\n");
    assert!(ids_under(&source, EXTENDED).contains(&"IWX0117".to_owned()));
    assert_eq!(ids_under(&source, &[]), ["IWS0001"]);
}

#[test]
fn a_filler_record_of_an_external_file_is_read_under_extended() {
    let source = under(concat!(
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        "           SELECT F ASSIGN TO 'suite-filler.dat'.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD F IS EXTERNAL.\n",
        "       01 FILLER PIC X(10).\n",
        "       PROCEDURE DIVISION.\n",
        "           DISPLAY 'OK'\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "OK\n");
    assert_eq!(ids_under(&source, EXTENDED), ["IWX0119"]);
    assert_eq!(ids_under(&source, &[]), ["IWC0198"]);
}

#[test]
fn an_alphabet_for_national_is_left_out_under_extended() {
    let source = under(concat!(
        "       ENVIRONMENT DIVISION.\n",
        "       CONFIGURATION SECTION.\n",
        "       SPECIAL-NAMES.\n",
        "           ALPHABET UNI FOR NATIONAL IS UCS-4.\n",
        "       PROCEDURE DIVISION.\n",
        "           DISPLAY 'OK'\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "OK\n");
    assert_eq!(ids_under(&source, EXTENDED), ["IWX0140"]);
    assert_eq!(ids_under(&source, &[]), ["IWS0001"]);
}

#[test]
fn a_sort_collating_sequence_may_name_a_code_set() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 Z PIC X(10) VALUE 'd4b2e1a3c5'.\n",
        "       01 G.\n",
        "          02 TBL OCCURS 10.\n",
        "             03 X PIC X.\n",
        "       PROCEDURE DIVISION.\n",
        "           MOVE Z TO G\n",
        "           SORT TBL ASCENDING KEY X SEQUENCE EBCDIC\n",
        "           DISPLAY G\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "abcde12345\n");
    assert!(ids_under(&source, EXTENDED).contains(&"IWX0141".to_owned()));
    assert!(ids_under(&source, &[]).contains(&"IWC0204".to_owned()));
}

#[test]
fn system_offset_is_zero_in_the_formatted_functions() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 STR PIC X(30).\n",
        "       PROCEDURE DIVISION.\n",
        "           MOVE FUNCTION FORMATTED-TIME('hhmmss+hhmm', 45296, SYSTEM-OFFSET) TO STR\n",
        "           DISPLAY STR\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "123456+0000                   \n");
    assert!(ids_under(&source, EXTENDED).contains(&"IWX0142".to_owned()));
}

const START_LENGTH: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. STARTLEN.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT IXF ASSIGN TO 'IXDD'\n",
    "               ORGANIZATION IS INDEXED\n",
    "               ACCESS MODE IS DYNAMIC\n",
    "               RECORD KEY IS IX-KEY\n",
    "               FILE STATUS IS ST1.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD IXF.\n",
    "       01 IX-REC.\n",
    "          05 IX-KEY PIC X(4).\n",
    "          05 IX-VAL PIC X(4).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 ST1 PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT IXF\n",
    "           MOVE 'AAAA' TO IX-KEY MOVE 'ONE ' TO IX-VAL WRITE IX-REC\n",
    "           MOVE 'AAAB' TO IX-KEY MOVE 'TWO ' TO IX-VAL WRITE IX-REC\n",
    "           MOVE 'BBBA' TO IX-KEY MOVE 'THRE' TO IX-VAL WRITE IX-REC\n",
    "           CLOSE IXF\n",
    "           OPEN INPUT IXF\n",
    "           MOVE 'AAZZ' TO IX-KEY\n",
    "           START IXF KEY IS >= IX-KEY LENGTH 2\n",
    "           READ IXF NEXT RECORD\n",
    "           DISPLAY 'L1=' IX-KEY '|' IX-VAL '|' ST1\n",
    "           MOVE 'AAZZ' TO IX-KEY\n",
    "           START IXF KEY IS >= IX-KEY WITH LENGTH 2\n",
    "           READ IXF NEXT RECORD\n",
    "           DISPLAY 'L2=' IX-KEY '|' IX-VAL '|' ST1\n",
    "           MOVE 'AAZZ' TO IX-KEY\n",
    "           START IXF KEY IS >= IX-KEY\n",
    "           READ IXF NEXT RECORD\n",
    "           DISPLAY 'N1=' IX-KEY '|' IX-VAL '|' ST1\n",
    "           CLOSE IXF\n",
    "           GOBACK.\n",
);

#[test]
fn start_length_compares_the_first_characters_of_the_key() {
    let dir = temp("start-length");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let file = dir.join(format!("{name}.dat"));
        let _ = std::fs::remove_file(&file);
        // cobc 3.2's output.
        let ran = Harness::source(START_LENGTH).flags(EXTENDED).dds(&[format!("IXDD={}", file.display())]).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), ("L1=AAAA|ONE |00\nL2=AAAA|ONE |00\nN1=BBBA|THRE|00\n", Some(&Ending::Goback)), "{name}: {}", ran.err);
    }
    assert_eq!(ids_under(START_LENGTH, &[]), ["IWC0344"]);
    assert_eq!(ids_under(START_LENGTH, EXTENDED), ["IWX0143", "IWX0143"]);
}

const BOOLEAN_OPERATORS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BPREC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 R PIC S9(9) VALUE 0.\n",
    "       01 A PIC S9(4) COMP-5 VALUE 6.\n",
    "       01 B PIC S9(4) COMP-5 VALUE 3.\n",
    "       01 N PIC S9(4) COMP-5 VALUE -5.\n",
    "       PROCEDURE DIVISION.\n",
    "           COMPUTE R = 2 + 1 B-AND 1\n",
    "           DISPLAY \"a \" R\n",
    "           COMPUTE R = 1 B-AND 3 + 2\n",
    "           DISPLAY \"b \" R\n",
    "           COMPUTE R = 2 * 3 B-AND 6\n",
    "           DISPLAY \"c \" R\n",
    "           COMPUTE R = 6 B-AND 3 * 2\n",
    "           DISPLAY \"d \" R\n",
    "           COMPUTE R = 1 B-OR 2 B-AND 3\n",
    "           DISPLAY \"e \" R\n",
    "           COMPUTE R = 7 B-XOR 2 B-OR 1\n",
    "           DISPLAY \"f \" R\n",
    "           COMPUTE R = B-NOT A\n",
    "           DISPLAY \"g \" R\n",
    "           COMPUTE R = B-NOT A B-AND B\n",
    "           DISPLAY \"h \" R\n",
    "           COMPUTE R = N B-AND 255\n",
    "           DISPLAY \"i \" R\n",
    "           COMPUTE R = A B-AND 2.9\n",
    "           DISPLAY \"j \" R\n",
    "           COMPUTE R = - A B-OR 1\n",
    "           DISPLAY \"k \" R\n",
    "           COMPUTE R = A B-XOR N\n",
    "           DISPLAY \"m \" R\n",
    "           GOBACK.\n",
);

#[test]
fn boolean_operators_bind_as_multiplication_does() {
    // cobc 3.2's output under -std=mf.
    runs(BOOLEAN_OPERATORS, EXTENDED, "a +000000003\nb +000000003\nc +000000006\nd +000000004\ne +000000003\nf +000000005\ng +709551609\nh +000000001\ni +000000005\nj +000000004\nk +000000007\nm +000000003\n");
    assert!(ids_under(BOOLEAN_OPERATORS, EXTENDED).contains(&"IWX0144".to_owned()));
    assert!(ids_under(BOOLEAN_OPERATORS, &[]).iter().any(|i| i.starts_with("IWS")));
}

const BASED_LITERALS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. T.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 R PIC S9(9) VALUE 0.\n",
    "       01 V1 PIC 9(4) VALUE x#1F.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE h#10 TO R DISPLAY R\n",
    "           MOVE o#17 TO R DISPLAY R\n",
    "           MOVE b#101 TO R DISPLAY R\n",
    "           MOVE X#ff TO R DISPLAY R V1\n",
    "           GOBACK.\n",
);

#[test]
fn based_numeric_literals_are_read_under_extended() {
    runs(BASED_LITERALS, EXTENDED, "+000000016\n+000000015\n+000000005\n+0000002550031\n");
    assert!(ids_under(BASED_LITERALS, EXTENDED).iter().all(|i| i == "IWX0145"));
    assert!(!ids_under(BASED_LITERALS, &[]).is_empty());
}

const DISPLAYED_FUNCTIONS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. T.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'x ' FUNCTION INTEGER(-12.75)\n",
    "           DISPLAY 'x ' FUNCTION INTEGER-PART(-12.75)\n",
    "           DISPLAY 'x ' FUNCTION MOD(17 5)\n",
    "           DISPLAY 'x ' FUNCTION REM(17.5 5)\n",
    "           DISPLAY 'x ' FUNCTION REM(-17 5)\n",
    "           DISPLAY 'x ' FUNCTION SUM(1.5 2.25)\n",
    "           DISPLAY 'x ' FUNCTION SUM(0.1 0.2)\n",
    "           DISPLAY 'x ' FUNCTION SIGN(-12.75)\n",
    "           DISPLAY 'x ' FUNCTION FACTORIAL(5)\n",
    "           DISPLAY 'x ' FUNCTION FACTORIAL(15)\n",
    "           DISPLAY 'x ' FUNCTION INTEGER-OF-DATE(20240101)\n",
    "           DISPLAY 'x ' FUNCTION DATE-OF-INTEGER(150000)\n",
    "           DISPLAY 'x ' FUNCTION DAY-OF-INTEGER(150000)\n",
    "           DISPLAY 'x ' FUNCTION INTEGER(2147483647)\n",
    "           DISPLAY 'x ' FUNCTION INTEGER(-2147483648)\n",
    "           DISPLAY 'x ' FUNCTION INTEGER(-5000000000)\n",
    "           DISPLAY 'x ' FUNCTION SUM(0.0000000001 0)\n",
    "           DISPLAY 'x ' FUNCTION RANGE(1 5 3)\n",
    "           GOBACK.\n",
);

#[test]
fn display_shows_a_numeric_function_through_the_field_cobc_holds_it_in() {
    // cobc 3.2's output.
    runs(DISPLAYED_FUNCTIONS, EXTENDED, "x -000000013\nx -000000012\nx 000000002\nx 00000002.5\nx -000000002\nx 0000003.75\nx 00000000.3\nx -000000001\nx 000000120\nx 00000001307674368000\nx 000154498\nx 20110908\nx 2011251\nx 147483647\nx -00000000002147483648\nx -00000000005000000000\nx 0000000000.0000000001\nx 000000004\n");
    assert_eq!(ids_under(DISPLAYED_FUNCTIONS, EXTENDED).iter().filter(|i| *i == "IWX0149").count(), 18);
    assert!(ids_under(DISPLAYED_FUNCTIONS, &[]).iter().all(|i| i == "IWC0101"));
}

#[test]
fn a_class_condition_on_a_literal_tests_its_characters() {
    let source = under(concat!(
        "       ENVIRONMENT DIVISION.\n",
        "       CONFIGURATION SECTION.\n",
        "       SPECIAL-NAMES.\n",
        "           CLASS DIGITS IS '0' THROUGH '9'.\n",
        "       PROCEDURE DIVISION.\n",
        "           IF '7' IS DIGITS DISPLAY 'A-yes' ELSE DISPLAY 'A-no'.\n",
        "           IF '7X' IS DIGITS DISPLAY 'B-yes' ELSE DISPLAY 'B-no'.\n",
        "           IF '7X' IS NOT DIGITS DISPLAY 'C-yes' ELSE DISPLAY 'C-no'.\n",
        "           IF '12' IS NUMERIC DISPLAY 'E-yes' ELSE DISPLAY 'E-no'.\n",
        "           IF '1A' IS NUMERIC DISPLAY 'F-yes' ELSE DISPLAY 'F-no'.\n",
        "           IF 'ab' IS ALPHABETIC DISPLAY 'G-yes' ELSE DISPLAY 'G-no'.\n",
        "           GOBACK.\n",
    ));
    // cobc 3.2's output.
    runs(&source, EXTENDED, "A-yes\nB-no\nC-yes\nE-yes\nF-no\nG-yes\n");
    assert_eq!(ids_under(&source, EXTENDED).iter().filter(|i| *i == "IWX0147").count(), 6);
    assert_eq!(ids_under(&source, &[]), ["IWC0086", "IWC0086", "IWC0086"]);
}

#[test]
fn use_for_debugging_on_all_references_is_read_and_left_out() {
    let source = under(concat!(
        "       ENVIRONMENT DIVISION.\n",
        "       CONFIGURATION SECTION.\n",
        "       SOURCE-COMPUTER. X WITH DEBUGGING MODE.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 I PIC 9.\n",
        "       PROCEDURE DIVISION.\n",
        "       DECLARATIVES.\n",
        "       D SECTION.\n",
        "           USE FOR DEBUGGING ON ALL REFERENCES OF I.\n",
        "           DISPLAY DEBUG-NAME.\n",
        "       END DECLARATIVES.\n",
        "       MAIN SECTION.\n",
        "           MOVE 1 TO I\n",
        "           DISPLAY 'BODY'\n",
        "           GOBACK.\n",
    ));
    runs(&source, EXTENDED, "BODY\n");
    assert!(ids_under(&source, EXTENDED).contains(&"IWX0146".to_owned()));
    assert!(ids_under(&source, &[]).contains(&"IWS0067".to_owned()));
}

#[test]
fn typedef_goes_on_a_level_01_or_77_entry() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 T-REC.\n",
        "          05 T-A PIC X TYPEDEF.\n",
        "       PROCEDURE DIVISION.\n",
        "           GOBACK.\n",
    ));
    for flags in [&[][..], EXTENDED] {
        assert_eq!(ids_under(&source, flags), ["IWC0345"]);
    }
}

#[test]
fn a_perform_thru_out_of_a_declarative_is_a_hole_under_extended() {
    let source = under(concat!(
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        "           SELECT F1 ASSIGN TO 'straddle.dat' ORGANIZATION SEQUENTIAL.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD F1.\n",
        "       01 F1-REC PIC X(10).\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 N PIC 9(4) VALUE 0.\n",
        "       PROCEDURE DIVISION.\n",
        "       DECLARATIVES.\n",
        "       D-SEC SECTION.\n",
        "           USE AFTER STANDARD ERROR PROCEDURE ON F1.\n",
        "       D-P1.\n",
        "           ADD 1 TO N.\n",
        "       END DECLARATIVES.\n",
        "       MAIN-SEC SECTION.\n",
        "       MAIN-P.\n",
        "           DISPLAY 'BEFORE'\n",
        "           PERFORM D-P1 THRU MAIN-P2\n",
        "           DISPLAY 'N=' N\n",
        "           GOBACK.\n",
        "       MAIN-P2.\n",
        "           ADD 10 TO N.\n",
    ));
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(&source).flags(EXTENDED).run(executor);
        assert_eq!(o.out, "BEFORE\n", "{}", o.err);
        let ended = format!("{:?}", o.ending);
        assert!(ended.contains("IWR0078") && ended.contains("the statement at line 23 was reached"), "{ended}");
    }
    assert_eq!(ids_under(&source, EXTENDED), ["IWX0148"]);
    assert_eq!(ids_under(&source, &[]), ["IWC0015"]);
}

#[test]
fn stop_run_with_a_status_sets_return_code_alike_on_both_executors() {
    // cobc 3.2's exit statuses.
    for (statement, expected) in [
        ("STOP RUN ERROR", 1),
        ("STOP RUN NORMAL", 0),
        ("STOP RUN WITH NORMAL STATUS", 0),
        ("STOP RUN WITH ERROR", 1),
        ("STOP RUN ERROR STATUS 5", 5),
        ("STOP RUN WITH NORMAL STATUS N", 2),
        ("MOVE 7 TO RETURN-CODE STOP RUN NORMAL", 0),
        ("MOVE 7 TO RETURN-CODE STOP RUN WITH ERROR", 1),
        ("MOVE 7 TO RETURN-CODE STOP RUN", 7),
    ] {
        let source = under(&format!("       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 N PIC 9 VALUE 2.\n       PROCEDURE DIVISION.\n           {statement}.\n"));
        for executor in [Executor::Interpreter, Executor::Vm] {
            let o = Harness::source(&source).flags(EXTENDED).run(executor);
            assert_eq!((o.ending.as_ref().ok(), o.return_code), (Some(&Ending::StopRun), expected), "{statement}: {}", o.err);
        }
    }
    let source = under("       PROCEDURE DIVISION.\n           STOP RUN ERROR.\n");
    assert_eq!(ids_under(&source, &[]), ["IWC0346"]);
    assert_eq!(ids_under(&source, EXTENDED), ["IWX0150"]);
}

#[test]
fn xml_generate_suppress_when_names_no_item() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 P.\n",
        "          03 Q PIC 9(3) VALUE ZERO.\n",
        "          03 R PIC X(3) VALUE 'abc'.\n",
        "          03 S PIC X(3) VALUE SPACES.\n",
        "       01 OUT PIC X(100).\n",
        "       PROCEDURE DIVISION.\n",
        "           XML GENERATE OUT FROM P SUPPRESS WHEN SPACES\n",
        "           DISPLAY FUNCTION TRIM(OUT)\n",
        "           XML GENERATE OUT FROM P SUPPRESS WHEN ZERO\n",
        "           DISPLAY FUNCTION TRIM(OUT)\n",
        "           XML GENERATE OUT FROM P SUPPRESS WHEN SPACES OR ZERO\n",
        "           DISPLAY FUNCTION TRIM(OUT)\n",
        "           GOBACK.\n",
    ));
    // cobc 3.2's output.
    runs(&source, EXTENDED, "<P><Q>0</Q><R>abc</R></P>\n<P><R>abc</R><S> </S></P>\n<P><R>abc</R></P>\n");
    assert_eq!(ids_under(&source, EXTENDED).iter().filter(|i| *i == "IWX0151").count(), 3);
    assert!(ids_under(&source, &[]).contains(&"IWC0347".to_owned()));
}

#[test]
fn boolean_of_integer_compiles_and_ends_the_run_that_reaches_it() {
    let source = under(concat!(
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 A20 PIC X(20).\n",
        "       PROCEDURE DIVISION.\n",
        "           DISPLAY 'before'\n",
        "           MOVE FUNCTION BOOLEAN-OF-INTEGER(5 4) TO A20\n",
        "           DISPLAY 'after'\n",
        "           GOBACK.\n",
    ));
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(&source).flags(EXTENDED).run(executor);
        let ended = format!("{:?}", o.ending);
        assert_eq!(o.out, "before\n", "{}", o.err);
        assert!(ended.contains("IWR0092"), "{ended}");
    }
    assert_eq!(ids_under(&source, EXTENDED), ["IWX0152"]);
    assert_eq!(ids_under(&source, &[]), ["IWC0348"]);
}

const EXCEPTIONS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. EXC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 WS PIC S9(3) VALUE 7.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATUS \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATEMENT \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-FILE \"]\"\n",
    "           DISPLAY FUNCTION LENGTH(FUNCTION EXCEPTION-STATUS)\n",
    "           COMPUTE WS = 12345\n",
    "               ON SIZE ERROR DISPLAY \"SE\"\n",
    "           END-COMPUTE\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATUS \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATEMENT \"]\"\n",
    "           CALL \"NOPE\"\n",
    "              ON EXCEPTION DISPLAY \"NOCALL\"\n",
    "           END-CALL\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATUS \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           DISPLAY \"[\" FUNCTION EXCEPTION-STATEMENT \"]\"\n",
    "           GOBACK.\n",
);

const FILE_EXCEPTIONS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. EXC3.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT FILE-ONE ASSIGN TO 'EXC3DD' ORGANIZATION SEQUENTIAL\n",
    "               FILE STATUS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD FILE-ONE.\n",
    "       01 F-REC PIC X(4).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 FS PIC XX.\n",
    "       01 WS PIC S9(3) VALUE 7.\n",
    "       PROCEDURE DIVISION.\n",
    "       MAIN-PARA.\n",
    "           DISPLAY \"0[\" FUNCTION EXCEPTION-FILE \"]\"\n",
    "           DISPLAY \"0[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           DISPLAY \"0[\" FUNCTION EXCEPTION-STATEMENT \"]\"\n",
    "           DISPLAY \"0[\" FUNCTION EXCEPTION-STATUS \"]\"\n",
    "           OPEN INPUT FILE-ONE\n",
    "           DISPLAY \"1[\" FUNCTION EXCEPTION-FILE \"]\"\n",
    "           DISPLAY \"1[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           DISPLAY \"1[\" FUNCTION EXCEPTION-STATUS \"]\"\n",
    "           SET LAST EXCEPTION TO OFF\n",
    "           DISPLAY \"2[\" FUNCTION EXCEPTION-FILE \"]\"\n",
    "           DISPLAY \"2[\" FUNCTION EXCEPTION-LOCATION \"]\"\n",
    "           GOBACK.\n",
);

#[test]
fn exception_functions_report_the_last_exception_condition() {
    // cobc 3.2's output.
    runs(EXCEPTIONS, EXTENDED, "[                               ]\n[ ]\n[                               ]\n[00]\n000000031\nSE\n[EC-SIZE-OVERFLOW               ]\n[EXC; ; 0]\n[                               ]\nNOCALL\n[EC-PROGRAM-NOT-FOUND           ]\n[EXC; ; 0]\n[                               ]\n");
    let ids = ids_under(EXCEPTIONS, EXTENDED);
    assert_eq!(ids.iter().filter(|i| *i == "IWX0153").count(), 11);
    assert!(ids_under(EXCEPTIONS, &[]).contains(&"IWC0349".to_owned()));
}

#[test]
fn a_file_status_is_an_exception_condition_with_its_file() {
    let dir = temp("exception-file");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let o = Harness::source(FILE_EXCEPTIONS).flags(EXTENDED).dds(&[format!("EXC3DD={}", dir.join(format!("{name}.dat")).display())]).run(executor);
        assert_eq!((o.out.as_str(), o.ending.as_ref().ok()), ("0[00]\n0[ ]\n0[                               ]\n0[                               ]\n1[35FILE-ONE]\n1[EXC3; ; 0]\n1[EC-I-O-PERMANENT-ERROR         ]\n2[00]\n2[ ]\n", Some(&Ending::Goback)), "{name}: {}", o.err);
    }
}
