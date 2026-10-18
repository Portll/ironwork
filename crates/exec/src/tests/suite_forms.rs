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
