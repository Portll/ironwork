use super::*;

const EXTENDED: &[&str] = &["--compliance=extended"];

/// A free-form program using every extension `--compliance extended` reads.
const EXTENDED_PROGRAM: &str = concat!(
    "*> free form, read as Micro Focus and GnuCOBOL read it\n",
    "IDENTIFICATION DIVISION.\n",
    "PROGRAM-ID. EXT.\n",
    "DATA DIVISION.\n",
    "WORKING-STORAGE SECTION.\n",
    "78 MAX-LEN VALUE 5.\n",
    "01 GREETING CONSTANT AS \"HEL\" & \"LO\".\n",
    "01 WS-NAME PIC X(MAX-LEN) VALUE GREETING.\n",
    "01 WS-COUNT BINARY-LONG VALUE -7.\n",
    "01 WS-SHORT USAGE BINARY-SHORT UNSIGNED.\n",
    "01 WS-TABLE.\n",
    "   05 WS-CELL OCCURS MAX-LEN TIMES PIC 9 VALUE ZERO.\n",
    "PROCEDURE DIVISION.\n",
    "MAIN-PARA.\n",
    "* a comment line in column 1\n",
    "    DISPLAY WS-NAME \"|\" X\"C1\" & \"B\"\n",
    "    IF WS-COUNT <> MAX-LEN\n",
    "        ADD MAX-LEN TO WS-COUNT\n",
    "    END-IF\n",
    "    COMPUTE WS-SHORT = 65535 + 3\n",
    "    MOVE MAX-LEN TO WS-CELL(MAX-LEN)\n",
    "    PERFORM SHOW-PARA\n",
    "    GOBACK.\n",
    "SHOW-PARA.\n",
    "    DISPLAY WS-COUNT \"|\" WS-SHORT \"|\" WS-TABLE.\n",
);

#[test]
fn an_extended_program_runs_alike_on_the_interpreter_and_the_vm() {
    let walked = Harness::source(EXTENDED_PROGRAM).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("HELLO|AB\n-0000000002|00002|00005\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(EXTENDED_PROGRAM).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn each_extension_is_a_warning_naming_it_and_where_it_is() {
    let parsed = syntax::parse_with(EXTENDED_PROGRAM, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let shown: Vec<(u32, u32, Option<&str>)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.pos.col, m.id)).collect();
    assert_eq!(
        shown,
        [(1, 7, "IWX0001"), (6, 1, "IWX0002"), (7, 1, "IWX0002"), (7, 31, "IWX0004"), (9, 13, "IWX0005"), (10, 19, "IWX0005"), (16, 31, "IWX0004"), (17, 17, "IWX0003")].map(|(line, col, id)| (line, col, Some(id)))
    );
    assert!(compiled.diagnostics.iter().all(|m| m.severity == Severity::Warning));
    assert_eq!(syntax::return_code(&compiled.diagnostics), 4);
    assert_eq!(compiled.options.compliance, numeric::Compliance::Extended);
}

#[test]
fn strict_refuses_the_extended_program_as_before() {
    assert!(syntax::parse(EXTENDED_PROGRAM).is_err_and(|e| !e.message.contains("IWX")));
    let fixed = program("", "       78  N VALUE 1.\n", &line("GOBACK."));
    assert_eq!(compile_errors(&fixed), "IWC0034-S level 78 is not a data level");
}

/// A caller and a subprogram whose header ends RETURNING OMITTED, GnuCOBOL's program that returns
/// no item.
const RETURNING_OMITTED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CALLER.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  R PIC 9(4) VALUE 99.\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'VOIDSUB'\n",
    "           DISPLAY RETURN-CODE\n",
    "           CALL 'VOIDSUB' RETURNING R\n",
    "           DISPLAY R ' ' RETURN-CODE\n",
    "           GOBACK.\n",
    "       END PROGRAM CALLER.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. VOIDSUB.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9 VALUE 0.\n",
    "       PROCEDURE DIVISION RETURNING OMITTED.\n",
    "           ADD 1 TO N\n",
    "           MOVE N TO RETURN-CODE\n",
    "           GOBACK.\n",
    "       END PROGRAM VOIDSUB.\n",
);

#[test]
fn a_program_returning_omitted_returns_its_return_code_and_no_item() {
    let walked = Harness::source(RETURNING_OMITTED).flags(EXTENDED).run(Executor::Interpreter);
    // The item takes the RETURN-CODE, as cobc 3.2 gives it; cobc keeps its caller's RETURN-CODE
    // after a program RETURNING OMITTED, where ironwork gives it the called program's (C491).
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("+0001\n0002 +0002\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(RETURNING_OMITTED).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let parsed = syntax::parse_all_with(RETURNING_OMITTED, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed[1].clone(), &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let shown: Vec<(u32, u32, Option<&str>, Severity)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.pos.col, m.id, m.severity)).collect();
    assert_eq!(shown, [(18, 27, Some("IWX0009"), Severity::Warning)]);
    let strict = compile(syntax::parse_all_with(RETURNING_OMITTED, &syntax::copy::Libraries::default()).unwrap().remove(1), &[]).err().unwrap();
    assert_eq!(strict.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(), ["PROCEDURE DIVISION RETURNING OMITTED: not an 01 or 77 item of the LINKAGE SECTION"]);
}

/// A program reading its job step's PARM as Micro Focus and GnuCOBOL read their command line.
const COMMAND_LINE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ARGS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  CL PIC X(20).\n",
    "       01  N  PIC 9(3).\n",
    "       01  V  PIC X(6).\n",
    "       01  K  PIC 9 VALUE 3.\n",
    "       PROCEDURE DIVISION.\n",
    "           ACCEPT CL FROM COMMAND-LINE\n",
    "           ACCEPT N FROM ARGUMENT-NUMBER\n",
    "           DISPLAY '[' CL '] ' N\n",
    "           PERFORM 4 TIMES\n",
    "               MOVE ALL '*' TO V\n",
    "               ACCEPT V FROM ARGUMENT-VALUE\n",
    "                  ON EXCEPTION DISPLAY 'NONE LEFT'\n",
    "                  NOT ON EXCEPTION DISPLAY V\n",
    "               END-ACCEPT\n",
    "           END-PERFORM\n",
    "           DISPLAY K UPON ARGUMENT-NUMBER\n",
    "           ACCEPT V FROM ARGUMENT-VALUE\n",
    "           DISPLAY 'THIRD ' V\n",
    "           DISPLAY 9 UPON ARGUMENT-NUMBER\n",
    "           ACCEPT V FROM ARGUMENT-VALUE\n",
    "           DISPLAY 'LAST ' V\n",
    "           DISPLAY 0 UPON ARGUMENT-NUMBER\n",
    "           ACCEPT V FROM ARGUMENT-VALUE EXCEPTION DISPLAY 'NO WORD 0'\n",
    "           GOBACK.\n",
);

/// Assumption C442: the PARM's program arguments, before the last slash when runtime options follow
/// it, are the command line, and its blank-separated words the arguments.
#[test]
fn extended_reads_the_command_line_and_arguments_from_the_parm_alike_on_both_executors() {
    let run = |executor, parm: Option<&str>| {
        let harness = Harness::source(COMMAND_LINE).flags(EXTENDED);
        let o = match parm {
            Some(p) => harness.parm(p),
            None => harness,
        }
        .run(executor);
        assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
        o.out
    };
    let given = run(Executor::Interpreter, Some("alpha be  0042 zed/RPTOPTS(ON)"));
    assert_eq!(given, "[alpha be  0042 zed  ] 004\nalpha \nbe    \n0042  \nzed   \nTHIRD 0042  \nLAST zed   \nNO WORD 0\n");
    assert_eq!(run(Executor::Vm, Some("alpha be  0042 zed/RPTOPTS(ON)")), given);
    let none = run(Executor::Interpreter, None);
    assert_eq!(none, "[                    ] 000\nNONE LEFT\nNONE LEFT\nNONE LEFT\nNONE LEFT\nTHIRD ******\nLAST ******\nNO WORD 0\n");
    assert_eq!(run(Executor::Vm, None), none);
}

#[test]
fn the_command_line_is_a_warning_under_extended_and_refused_under_strict() {
    let extended = syntax::parse_with(COMMAND_LINE, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(extended, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let ids: Vec<(u32, Option<&str>, Severity)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.id, m.severity)).collect();
    let (terminators, ids): (Vec<_>, Vec<_>) = ids.into_iter().partition(|m| m.1 == Some("IWX0013"));
    assert_eq!(ids, [10, 11, 15, 20, 21, 23, 24, 26, 27].map(|line| (line, Some("IWX0010"), Severity::Warning)));
    assert_eq!(terminators, [(18, Some("IWX0013"), Severity::Warning)], "END-ACCEPT");
    let strict = compile(syntax::parse(COMMAND_LINE).unwrap(), &[]).err().unwrap();
    let (terminators, refused): (Vec<_>, Vec<_>) = strict.iter().filter(|e| e.severity == Severity::Severe).partition(|e| e.id == Some("IWS0097"));
    assert_eq!(terminators.iter().map(|e| e.pos.line).collect::<Vec<_>>(), [18], "END-ACCEPT, which Enterprise COBOL does not reserve");
    let refused: Vec<&str> = refused.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(refused.len(), 9, "{refused:?}");
    assert!(refused[0].starts_with("ACCEPT ... FROM COMMAND-LINE: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's"), "{refused:?}");
    assert!(refused.contains(&"DISPLAY UPON ARGUMENT-NUMBER: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it"), "{refused:?}");
    let misused = program("", "       01  V PIC X(6).\n", &[line("ACCEPT V FROM SYSIN ON EXCEPTION DISPLAY 'X' END-ACCEPT"), line("DISPLAY V UPON ARGUMENT-NUMBER"), line("GOBACK.")].concat());
    let parsed = syntax::parse_with(&misused, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let errors = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).err().unwrap();
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.starts_with("ACCEPT ... ON EXCEPTION: of the ACCEPT statements, only ACCEPT ... FROM ARGUMENT-VALUE")), "{messages:?}");
    assert!(messages.iter().any(|m| m.starts_with("DISPLAY UPON ARGUMENT-NUMBER: it shows one numeric item or literal")), "{messages:?}");
}

/// A FETCH whose INTO list names one host variable without its colon.
const COLONLESS_INTO: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. COLONS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  WS-A PIC X(4).\n",
    "       01  WS-B PIC X(4).\n",
    "           EXEC SQL DECLARE C1 CURSOR FOR SELECT A, B FROM T END-EXEC\n",
    "       PROCEDURE DIVISION.\n",
    "           EXEC SQL FETCH C1 INTO :WS-A, WS-B END-EXEC\n",
    "           GOBACK.\n",
);

#[test]
fn an_into_name_without_its_colon_is_refused_under_strict_and_read_under_extended() {
    let strict = compile(syntax::parse(COLONLESS_INTO).unwrap(), &[]).err().unwrap();
    assert_eq!(strict.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(), ["EXEC SQL FETCH: WS-B in the INTO list has no colon, which Db2 requires before every host variable"]);
    let parsed = syntax::parse_with(COLONLESS_INTO, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let shown: Vec<(u32, Option<&str>, Severity)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.id, m.severity)).collect();
    assert_eq!(shown, [(9, Some("IWX0011"), Severity::Warning)]);
}

/// A program using the five forms Enterprise COBOL flags that Micro Focus and GnuCOBOL read: a
/// numeric VALUE for a numeric-edited item, VALUES for a data item, names of more than 30
/// characters that differ only after the 30th, END-DISPLAY, and statements in Area A.
const FLAGGED_FORMS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. FLAGGED.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  AMOUNT PIC ZZ9.99 VALUE 12.5.\n",
    "       01  FLAG PIC X VALUES 'Y'.\n",
    "       01  CUSTOMER-ACCOUNT-BALANCE-TOTAL-A PIC 9(5) VALUE 42.\n",
    "       01  CUSTOMER-ACCOUNT-BALANCE-TOTAL-B PIC 9(5) VALUE 7.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY AMOUNT '|' FLAG END-DISPLAY\n",
    "           DISPLAY CUSTOMER-ACCOUNT-BALANCE-TOTAL-A '|'\n",
    "               CUSTOMER-ACCOUNT-BALANCE-TOTAL-B\n",
    "       DISPLAY 'AREA A'\n",
    "       GOBACK.\n",
);

fn diagnostics_under(source: &str, compliance: numeric::Compliance) -> Vec<(u32, u32, Option<&'static str>, Severity)> {
    let flags: Vec<String> = if compliance == numeric::Compliance::Extended { EXTENDED.iter().map(|f| f.to_string()).collect() } else { Vec::new() };
    let parsed = syntax::parse_with(source, &syntax::copy::Libraries::default().with_compliance(compliance)).unwrap_or_else(|e| panic!("{e}"));
    let mut shown: Vec<_> = match compile(parsed, &flags) {
        Ok(compiled) => compiled.diagnostics,
        Err(errors) => errors,
    }
    .iter()
    .map(|m| (m.pos.line, m.pos.col, m.id, m.severity))
    .collect();
    shown.sort();
    shown
}

#[test]
fn the_forms_ibm_flags_run_alike_on_the_interpreter_and_the_vm_under_extended() {
    let walked = Harness::source(FLAGGED_FORMS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (" 12.50|Y\n00042|00007\nAREA A\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(FLAGGED_FORMS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn extended_warns_of_each_form_and_strict_gives_ibms_severity() {
    let warning = |line, col, id| (line, col, Some(id), Severity::Warning);
    let extended = [warning(5, 8, "IWX0012"), warning(6, 23, "IWX0014"), warning(7, 12, "IWX0015"), warning(8, 12, "IWX0015"), warning(10, 36, "IWX0013"), warning(11, 20, "IWX0015"), warning(12, 16, "IWX0015"), warning(13, 8, "IWX0017"), warning(14, 8, "IWX0017")];
    assert_eq!(diagnostics_under(FLAGGED_FORMS, numeric::Compliance::Extended), extended);
    let strict = diagnostics_under(FLAGGED_FORMS, numeric::Compliance::Strict);
    let given = |id: &str| strict.iter().filter(|m| m.2 == Some(id)).map(|m| (m.0, m.3)).collect::<Vec<_>>();
    assert_eq!(given("IWC0292"), [(5, Severity::Severe)], "IGYGR1080-S: {strict:?}");
    assert_eq!(given("IWS0098"), [(6, Severity::Severe)]);
    assert_eq!(given("IWS0097"), [(10, Severity::Severe)]);
    assert_eq!(given("IWS0099"), [7, 8, 11, 12].map(|line| (line, Severity::Error)), "IGYDS0023-E");
    assert_eq!(given("IWS0100"), [(13, Severity::Error), (14, Severity::Error)], "IGYPS0009-E");
    assert_eq!(given("IWC0002"), [(11, Severity::Severe), (12, Severity::Severe)], "the two names are one name in their first 30 characters");
}

const DIGITS_AS_CHARACTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. DIGITS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9(3) VALUE 5.\n",
    "       01  S PIC S9(3) VALUE 5.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY '[' FUNCTION TRIM(N) '|' FUNCTION REVERSE(N) ']'\n",
    "           IF FUNCTION UPPER-CASE(N) = '005' DISPLAY 'EQ' END-IF\n",
    "           GOBACK.\n",
);

/// cobc 3.2 gives the output this test expects; a signed item's characters are not its digits,
/// and stay refused.
#[test]
fn extended_reads_an_unsigned_integers_digits_as_a_character_functions_argument() {
    let walked = Harness::source(DIGITS_AS_CHARACTERS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("[005|500]\nEQ\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(DIGITS_AS_CHARACTERS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warning = |line, col| (line, col, Some("IWX0018"), Severity::Warning);
    assert_eq!(diagnostics_under(DIGITS_AS_CHARACTERS, numeric::Compliance::Extended), [warning(8, 24), warning(8, 45), warning(9, 15)]);
    let strict: Vec<_> = diagnostics_under(DIGITS_AS_CHARACTERS, numeric::Compliance::Strict).into_iter().map(|m| (m.0, m.2, m.3)).collect();
    assert_eq!(strict, [8, 8, 9].map(|line| (line, Some("IWC0297"), Severity::Severe)));
    let signed = DIGITS_AS_CHARACTERS.replace("REVERSE(N)", "REVERSE(S)");
    let refused: Vec<_> = diagnostics_under(&signed, numeric::Compliance::Extended).into_iter().filter(|m| m.3 == Severity::Severe).map(|m| m.2).collect();
    assert_eq!(refused, [Some("IWC0297")]);
}

/// Enterprise COBOL compiles a statement in Area A, or a name of more than 30 characters, with an
/// error (return code 8) and runs it under its default NOCOMPILE(S), the statement read as though
/// it began in Area B and the name as its first 30 characters.
#[test]
fn strict_runs_what_ibm_compiles_with_an_error() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. AREAS.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  CUSTOMER-ACCOUNT-BALANCE-TOTAL-AMOUNT PIC 9(3) VALUE 5.\n",
        "       PROCEDURE DIVISION.\n",
        "       MAIN-PARA.\n",
        "       DISPLAY CUSTOMER-ACCOUNT-BALANCE-TOTAL-AMOUNT\n",
        "           PERFORM NEXT-PARA\n",
        "       GOBACK.\n",
        "       NEXT-PARA.\n",
        "       EXIT.\n",
    );
    let error = |line, col, id| (line, col, Some(id), Severity::Error);
    let strict = [error(5, 12, "IWS0099"), error(8, 8, "IWS0100"), error(8, 16, "IWS0099"), error(10, 8, "IWS0100"), error(12, 8, "IWS0100")];
    assert_eq!(diagnostics_under(source, numeric::Compliance::Strict), strict);
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(source).run(executor);
        assert_eq!((o.out.as_str(), o.ending.as_ref().ok()), ("005\n", Some(&Ending::Goback)), "{}", o.err);
    }
}

/// A name in EXEC SQL or EXEC CICS is read as the program's declarations are, cut to its first 30
/// characters under strict.
#[test]
fn a_long_name_in_exec_sql_or_cics_names_what_its_declaration_names() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. SQLLONG.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
        "       01  CUSTOMER-ACCOUNT-BALANCE-TOTAL-AMOUNT PIC S9(7) COMP-3.\n",
        "       PROCEDURE DIVISION.\n",
        "           EXEC SQL\n",
        "             SELECT BAL INTO :CUSTOMER-ACCOUNT-BALANCE-TOTAL-AMOUNT\n",
        "               FROM ACCT WHERE ID = 1\n",
        "           END-EXEC\n",
        "           EXEC CICS WRITEQ TS QUEUE('Q1')\n",
        "                FROM(CUSTOMER-ACCOUNT-BALANCE-TOTAL-AMOUNT) END-EXEC\n",
        "           GOBACK.\n",
    );
    assert_eq!(diagnostics_under(source, numeric::Compliance::Strict), [(6, 12, Some("IWS0099"), Severity::Error)]);
    assert_eq!(diagnostics_under(source, numeric::Compliance::Extended), [(6, 12, Some("IWX0015"), Severity::Warning)]);
}

/// BINARY-CHAR items stored past their byte, and one in a group beside a character.
const BINARY_CHAR: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BCHAR.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  S BINARY-CHAR.\n",
    "       01  U BINARY-CHAR UNSIGNED.\n",
    "       01  T USAGE BINARY-CHAR SIGNED VALUE 100.\n",
    "       01  G.\n",
    "           05  G1 BINARY-CHAR UNSIGNED VALUE 65.\n",
    "           05  G2 PIC X VALUE 'Z'.\n",
    "       01  N PIC 9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE -1 TO S\n",
    "           MOVE 300 TO U\n",
    "           ADD 50 TO T\n",
    "           DISPLAY S ' ' U ' ' T\n",
    "           COMPUTE N = S * 10 + U\n",
    "           MOVE 255 TO U\n",
    "           ADD 1 TO U\n",
    "           COMPUTE S = FUNCTION LENGTH(G)\n",
    "           DISPLAY N ' ' U ' ' S ' ' G1\n",
    "           GOBACK.\n",
);

#[test]
fn binary_char_is_one_byte_alike_on_both_executors_and_shown_as_cobc_shows_it() {
    let run = |executor, flags: &[&str]| {
        let o = Harness::source(BINARY_CHAR).flags(flags).run(executor);
        assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
        o.out
    };
    let ibm = run(Executor::Interpreter, EXTENDED);
    assert_eq!(ibm, "-001 044 -106\n0034 000 +002 065\n");
    assert_eq!(run(Executor::Vm, EXTENDED), ibm);
    let gnucobol = ["--compliance=extended", "--dialect=gnucobol"];
    let shown = run(Executor::Interpreter, &gnucobol);
    assert_eq!(shown, "-001 044 -106\n0034 000 +002 065\n");
    assert_eq!(run(Executor::Vm, &gnucobol), shown);
}

#[test]
fn binary_char_is_a_warning_naming_its_range_under_extended_and_refused_under_strict() {
    let parsed = syntax::parse_with(BINARY_CHAR, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let shown: Vec<(u32, Option<&str>, Severity)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.id, m.severity)).collect();
    assert_eq!(shown, [5, 6, 7, 9].map(|line| (line, Some("IWX0016"), Severity::Warning)));
    assert!(compiled.diagnostics[1].message.ends_with("U is one byte of binary, 0 to 255"), "{}", compiled.diagnostics[1].message);
    let strict = compile(syntax::parse(BINARY_CHAR).unwrap(), &[]).err().unwrap();
    let refused: Vec<(u32, Option<&str>)> = strict.iter().map(|e| (e.pos.line, e.id)).collect();
    assert_eq!(refused, [5, 6, 7, 9].map(|line| (line, Some("IWC0293"))));
    let pictured = program("", "       01  P PIC 99 BINARY-CHAR.\n", &line("GOBACK."));
    let parsed = syntax::parse_with(&pictured, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let errors = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).err().unwrap();
    assert!(errors.iter().any(|e| e.id == Some("IWC0294")), "{errors:?}");
}

/// GnuCOBOL's binary usages given values past their bytes and below zero.
const GNUCOBOL_BINARY: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. INTS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  A SIGNED-SHORT.\n",
    "       01  B UNSIGNED-SHORT.\n",
    "       01  D UNSIGNED-INT.\n",
    "       01  F UNSIGNED-LONG.\n",
    "       01  G BINARY-INT.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE -1 TO B D F\n",
    "           MOVE 70000 TO A\n",
    "           MOVE 2147483647 TO G\n",
    "           ADD 1 TO G\n",
    "           DISPLAY A ' ' B ' ' D ' ' F ' ' G\n",
    "           GOBACK.\n",
);

#[test]
fn gnucobols_binary_usages_keep_their_bytes_as_cobc_does_on_both_executors() {
    let flags = ["--compliance=extended", "--dialect=gnucobol"];
    let walked = Harness::source(GNUCOBOL_BINARY).flags(&flags).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("+04464 00001 0000000001 00000000000000000001 -2147483648\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(GNUCOBOL_BINARY).flags(&flags).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

/// Tables declared at level 01 and 77, with records on each side of them.
const TOP_LEVEL_TABLES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. OCC01.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  BEFORE-T PIC X(3) VALUE 'ABC'.\n",
    "       01  T OCCURS 3 TIMES.\n",
    "           05  T-NAME PIC X(4).\n",
    "           05  T-NUM  PIC 9(2).\n",
    "       77  C PIC 9 OCCURS 4 VALUE 7.\n",
    "       01  AFTER-T PIC X(3) VALUE 'XYZ'.\n",
    "       01  I PIC 9.\n",
    "       PROCEDURE DIVISION.\n",
    "           PERFORM VARYING I FROM 1 BY 1 UNTIL I > 3\n",
    "               MOVE 'NM' TO T-NAME(I)\n",
    "               COMPUTE T-NUM(I) = I * 11\n",
    "           END-PERFORM\n",
    "           MOVE 5 TO C(2)\n",
    "           DISPLAY T(1) '|' T(3) '|' T-NUM(2) '|' C(1) C(2) C(4)\n",
    "           DISPLAY BEFORE-T AFTER-T\n",
    "           GOBACK.\n",
);

#[test]
fn a_table_at_level_01_or_77_is_a_record_of_its_own_alike_on_both_executors() {
    let walked = Harness::source(TOP_LEVEL_TABLES).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("NM  11|NM  33|22|757\nABCXYZ\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(TOP_LEVEL_TABLES).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    assert_eq!(diagnostics_under(TOP_LEVEL_TABLES, numeric::Compliance::Extended), [(6, 8, Some("IWX0019"), Severity::Warning), (9, 8, Some("IWX0019"), Severity::Warning)]);
    assert_eq!(diagnostics_under(TOP_LEVEL_TABLES, numeric::Compliance::Strict), [(6, 8, Some("IWC0027"), Severity::Severe)]);
}

/// A level-66 entry inside its record, the entries after it continuing the record, and one
/// after the next record's last entry.
const RENAMES_INSIDE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. REN66.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  DW.\n",
    "           05  DW-BP.\n",
    "               10  DW-BP-YY    PIC 9(02) VALUE 24.\n",
    "               10  DW-BP-MM    PIC 9(02) VALUE 10.\n",
    "               10  DW-BP-CYCLE PIC 9(02) VALUE 7.\n",
    "           66  DW-BP-YYMM RENAMES DW-BP-YY THRU DW-BP-MM.\n",
    "           05  DW-LEAP-SW      PIC X(01) VALUE 'Y'.\n",
    "               88  DW-IS-LEAP  VALUE 'Y'.\n",
    "           05  DW-TAIL         PIC X(02) VALUE 'ZZ'.\n",
    "       01  K.\n",
    "           05  K-A PIC X VALUE 'A'.\n",
    "           05  K-B PIC X VALUE 'B'.\n",
    "           66  K-AB RENAMES K-A THRU K-B.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY DW-BP-YYMM '|' DW-LEAP-SW '|' LENGTH OF DW '|' DW\n",
    "           MOVE '9912' TO DW-BP-YYMM\n",
    "           IF DW-IS-LEAP DISPLAY DW '|' K-AB END-IF\n",
    "           GOBACK.\n",
);

#[test]
fn a_level_66_entry_inside_its_record_is_read_after_the_record_alike_on_both_executors() {
    let walked = Harness::source(RENAMES_INSIDE).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("2410|Y|000000009|241007YZZ\n991207YZZ|AB\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(RENAMES_INSIDE).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    assert_eq!(diagnostics_under(RENAMES_INSIDE, numeric::Compliance::Extended), [(10, 12, Some("IWX0032"), Severity::Warning)]);
    assert_eq!(diagnostics_under(RENAMES_INSIDE, numeric::Compliance::Strict), [(11, 12, Some("IWC0035"), Severity::Severe)]);
}

/// Positioned DISPLAYs and ACCEPTs on the screen: AT a number and an item, LINE and COL, ERASE,
/// UPDATE and SECURE fields, and a function key ON EXCEPTION.
const SCREEN: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SCR.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  NAME PIC X(8) VALUE 'ALICE'.\n",
    "       01  QTY  PIC 9(3) VALUE 7.\n",
    "       01  PW   PIC X(4).\n",
    "       01  WPOS PIC 9(4) VALUE 0510.\n",
    "       01  L    PIC 99 VALUE 7.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'ROW1COL1' AT 0101\n",
    "           DISPLAY 'XXXXXXXXXX' AT 0201\n",
    "           DISPLAY 'AB' AT 0205 WITH ERASE EOL\n",
    "           DISPLAY NAME AT WPOS\n",
    "           DISPLAY 'P' 'Q' 'R' LINE L COL 10\n",
    "           DISPLAY 'HI' AT 0801 WITH HIGHLIGHT FOREGROUND-COLOR 2\n",
    "           ACCEPT NAME AT 1001\n",
    "           ACCEPT QTY AT 1101 WITH UPDATE\n",
    "           ACCEPT PW AT 1201 WITH SECURE\n",
    "               ON EXCEPTION DISPLAY 'KEY' AT 1301\n",
    "               NOT ON EXCEPTION DISPLAY 'ENTER' AT 1301\n",
    "           END-ACCEPT\n",
    "           DISPLAY NAME '|' QTY '|' PW\n",
    "           GOBACK.\n",
);

#[test]
fn positioned_display_and_accept_use_one_screen_alike_on_both_executors() {
    let script = "string BOB\nENTER\nstring 42\nENTER\nstring PW\nPF3\n";
    let walked = Harness::source(SCREEN).flags(EXTENDED).screens(script).run(Executor::Interpreter);
    let first = "ROW1COL1\nXXXXAB\n\n\n         ALICE\n\n         PQR\nHI";
    let expected = format!(
        "BOB     |042|PW  \n--- screen 1 ---\n{first}\n--- screen 2 ---\n{first}\n\nBOB\n007\n--- screen 3 ---\n{first}\n\nBOB\n042\n--- screen 4 ---\n{first}\n\nBOB\n042\n**\nKEY\n"
    );
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected.as_str(), Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(SCREEN).flags(EXTENDED).screens(script).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn an_accept_with_no_key_left_ends_the_run_and_strict_refuses_the_screen_by_name() {
    let short = Harness::source(SCREEN).flags(EXTENDED).screens("string BOB\nENTER\n").run(Executor::Interpreter);
    assert_eq!(short.ending.as_ref().err().map(|a| a.message.as_str()), Some("ACCEPT: the screen has no more operator input"));
    let strict = diagnostics_under(SCREEN, numeric::Compliance::Strict);
    assert_eq!(strict.iter().filter(|d| d.2 == Some("IWC0298")).count(), 11, "{strict:?}");
    let extended = diagnostics_under(SCREEN, numeric::Compliance::Extended);
    assert!(extended.iter().all(|d| d.3 == Severity::Warning), "{extended:?}");
    assert_eq!(extended.iter().filter(|d| d.2 == Some("IWX0020")).count(), 11, "{extended:?}");
}

/// A SCREEN SECTION screen: VALUE literals, FROM, USING and TO fields, an edited PICTURE, and
/// LINE and COL PLUS relative to the entry before.
const SCREEN_SECTION: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SCR2.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  CUST   PIC X(6) VALUE 'ACME'.\n",
    "       01  AMT    PIC S9(4)V99 VALUE 1234.5.\n",
    "       01  QTY    PIC 999 VALUE 12.\n",
    "       01  NOTE   PIC X(5).\n",
    "       SCREEN SECTION.\n",
    "       01  ORDER-SCREEN.\n",
    "           05  BLANK SCREEN.\n",
    "           05  LINE 2 COL 3 VALUE 'CUSTOMER'.\n",
    "           05  COL PLUS 2 PIC X(6) FROM CUST.\n",
    "           05  LINE PLUS 1 COL 3 VALUE 'AMOUNT'.\n",
    "           05  COL 14 PIC Z,ZZ9.99 FROM AMT.\n",
    "           05  LINE 5 COL 3 VALUE 'QTY'.\n",
    "           05  COL 14 PIC ZZ9 USING QTY.\n",
    "           05  LINE 6 COL 3 VALUE 'NOTE'.\n",
    "           05  COL 14 PIC X(5) TO NOTE.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY ORDER-SCREEN\n",
    "           ACCEPT ORDER-SCREEN\n",
    "           DISPLAY 'QTY=' QTY ' NOTE=' NOTE AT 0801\n",
    "           GOBACK.\n",
);

#[test]
fn a_screen_section_screen_is_shown_and_read_alike_on_both_executors() {
    let script = "string 7\ntab\nstring HELLO\nENTER\n";
    let walked = Harness::source(SCREEN_SECTION).flags(EXTENDED).screens(script).run(Executor::Interpreter);
    let shown = "\n  CUSTOMER ACME\n  AMOUNT     1,234.50\n\n  QTY";
    let expected = format!("--- screen 1 ---\n{shown}         12\n  NOTE\n--- screen 2 ---\n{shown}          7\n  NOTE       HELLO\n\nQTY=007 NOTE=HELLO\n");
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected.as_str(), Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(SCREEN_SECTION).flags(EXTENDED).screens(script).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned = diagnostics_under(SCREEN_SECTION, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.2 == Some("IWX0020")).map(|d| d.0).collect::<Vec<_>>(), [21, 22, 23], "{warned:?}");
    let strict = diagnostics_under(SCREEN_SECTION, numeric::Compliance::Strict);
    assert_eq!(strict.iter().find(|d| d.2 == Some("IWC0298")).map(|d| d.0), Some(10), "{strict:?}");
}

/// The environment: ACCEPT ... FROM ENVIRONMENT a literal and an item, a variable not set, SET
/// ENVIRONMENT, and DISPLAY UPON ENVIRONMENT-NAME and ENVIRONMENT-VALUE.
const ENVIRONMENT: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ENV1.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  V    PIC X(10) VALUE 'UNCHANGED'.\n",
    "       01  N    PIC 9(4) VALUE 7.\n",
    "       01  NM   PIC X(8) VALUE 'IW_TWO'.\n",
    "       PROCEDURE DIVISION.\n",
    "           ACCEPT V FROM ENVIRONMENT 'IW_ONE'\n",
    "           DISPLAY '1[' V ']'\n",
    "           MOVE 'UNCHANGED' TO V\n",
    "           ACCEPT V FROM ENVIRONMENT 'IW_MISSING'\n",
    "               ON EXCEPTION DISPLAY '2 EXC [' V ']'\n",
    "               NOT ON EXCEPTION DISPLAY '2 OK [' V ']'\n",
    "           END-ACCEPT\n",
    "           ACCEPT N FROM ENVIRONMENT NM\n",
    "           DISPLAY '3[' N ']'\n",
    "           SET ENVIRONMENT 'IW_SET' TO 'SETVAL'\n",
    "           ACCEPT V FROM ENVIRONMENT 'IW_SET'\n",
    "           DISPLAY '4[' V ']'\n",
    "           DISPLAY 'IW_ONE' UPON ENVIRONMENT-NAME\n",
    "           ACCEPT V FROM ENVIRONMENT-VALUE\n",
    "           DISPLAY '5[' V ']'\n",
    "           DISPLAY 'IW_NEW' UPON ENVIRONMENT-NAME\n",
    "           DISPLAY 'NEWVAL' UPON ENVIRONMENT-VALUE\n",
    "           ACCEPT V FROM ENVIRONMENT 'IW_NEW'\n",
    "           DISPLAY '6[' V ']'\n",
    "           MOVE 'UNCHANGED' TO V\n",
    "           DISPLAY 'IW_NONE' UPON ENVIRONMENT-NAME\n",
    "           ACCEPT V FROM ENVIRONMENT-VALUE\n",
    "              ON EXCEPTION DISPLAY '7 EXC [' V ']'\n",
    "           END-ACCEPT\n",
    "           GOBACK.\n",
);

#[test]
fn the_environment_is_read_and_set_alike_on_both_executors() {
    let run = |executor| {
        let o = Harness::source(ENVIRONMENT).flags(EXTENDED).env("IW_ONE", "hello").env("IW_TWO", "0042").run(executor);
        assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
        o.out
    };
    let walked = run(Executor::Interpreter);
    assert_eq!(walked, "1[hello     ]\n2 EXC [          ]\n3[0042]\n4[SETVAL    ]\n5[hello     ]\n6[NEWVAL    ]\n7 EXC [          ]\n");
    assert_eq!(run(Executor::Vm), walked);
    let warned = diagnostics_under(ENVIRONMENT, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.2 == Some("IWX0021")).map(|d| d.0).collect::<Vec<_>>(), [9, 12, 16, 18, 19, 21, 22, 24, 25, 26, 29, 30], "{warned:?}");
    let strict = diagnostics_under(ENVIRONMENT, numeric::Compliance::Strict);
    let refused: Vec<(u32, Option<&str>)> = strict.iter().filter(|d| d.2 != Some("IWS0097")).map(|d| (d.0, d.2)).collect();
    assert_eq!(refused[..4], [(9, Some("IWS0055")), (12, Some("IWS0055")), (16, Some("IWS0055")), (18, Some("IWS0061"))], "{strict:?}");
}

/// Files with LOCK MODE and SHARING, and the READ, WRITE and REWRITE phrases and UNLOCK that go
/// with them.
const LOCKS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. LOCKS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT KF ASSIGN TO 'kf.dat'\n",
    "               ORGANIZATION INDEXED ACCESS DYNAMIC\n",
    "               RECORD KEY IS K-ID\n",
    "               LOCK MODE IS MANUAL WITH LOCK ON MULTIPLE RECORDS\n",
    "               FILE STATUS IS FS.\n",
    "           SELECT SF ASSIGN TO 'sf.dat'\n",
    "               ORGANIZATION LINE SEQUENTIAL\n",
    "               LOCK MODE AUTOMATIC\n",
    "               SHARING WITH ALL OTHER.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD  KF.\n",
    "       01  K-REC.\n",
    "           05  K-ID   PIC 9(3).\n",
    "           05  K-NAME PIC X(5).\n",
    "       FD  SF.\n",
    "       01  S-REC PIC X(10).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  FS PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT KF\n",
    "           MOVE 1 TO K-ID MOVE 'ALPHA' TO K-NAME\n",
    "           WRITE K-REC WITH NO LOCK\n",
    "           MOVE 2 TO K-ID MOVE 'BETA' TO K-NAME\n",
    "           WRITE K-REC\n",
    "           CLOSE KF\n",
    "           OPEN I-O KF\n",
    "           MOVE 2 TO K-ID\n",
    "           READ KF WITH LOCK KEY IS K-ID\n",
    "           DISPLAY 'READ ' FS ' ' K-NAME\n",
    "           MOVE 'GAMMA' TO K-NAME\n",
    "           REWRITE K-REC WITH LOCK\n",
    "           UNLOCK KF RECORDS\n",
    "           MOVE 1 TO K-ID\n",
    "           READ KF WITH NO LOCK\n",
    "           DISPLAY 'READ ' FS ' ' K-NAME\n",
    "           CLOSE KF\n",
    "           OPEN OUTPUT SF\n",
    "           MOVE 'LINE' TO S-REC\n",
    "           WRITE S-REC\n",
    "           CLOSE SF\n",
    "           OPEN INPUT SF\n",
    "           READ SF NEXT RECORD\n",
    "           DISPLAY 'SEQ ' S-REC\n",
    "           CLOSE SF\n",
    "           GOBACK.\n",
);

#[test]
fn locking_phrases_change_nothing_a_run_unit_does_alike_on_both_executors() {
    let dir = temp("locks");
    std::fs::create_dir_all(&dir).unwrap();
    let dds = [format!("KF.DAT={}", dir.join("kf.dat").display()), format!("SF.DAT={}:text", dir.join("sf.dat").display())];
    let walked = Harness::source(LOCKS).flags(EXTENDED).dds(&dds).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("READ 00 BETA \nREAD 00 ALPHA\nSEQ LINE      \n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(LOCKS).flags(EXTENDED).dds(&dds).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned = diagnostics_under(LOCKS, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.2 == Some("IWX0022")).map(|d| d.0).collect::<Vec<_>>(), [9, 13, 14, 28, 34, 37, 38, 40], "{warned:?}");
    let strict = syntax::parse(&program("", "       01  A PIC X(4).\n", &line("READ F WITH LOCK."))).unwrap_err();
    assert_eq!((strict.id, strict.message.as_str()), (Some("IWC0299"), "READ ... WITH LOCK: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it"));
}

/// INSPECT ... TRAILING, tallying and replacing, alone, with BEFORE and AFTER, and beside LEADING.
const INSPECT_TRAILING: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. INSP.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  T    PIC X(12) VALUE 'AB  CD  XX  '.\n",
    "       01  U    PIC X(10) VALUE '00123000  '.\n",
    "       01  N    PIC 99 VALUE 0.\n",
    "       01  M    PIC 99 VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           INSPECT T TALLYING N FOR TRAILING SPACES\n",
    "           DISPLAY 'N=' N\n",
    "           INSPECT U TALLYING M FOR TRAILING '0' TRAILING ' '\n",
    "           DISPLAY 'M=' M\n",
    "           INSPECT T REPLACING TRAILING SPACES BY '*'\n",
    "           DISPLAY 'T=' T\n",
    "           MOVE 0 TO N\n",
    "           INSPECT U TALLYING N FOR TRAILING '0' AFTER '1'\n",
    "           DISPLAY 'N2=' N\n",
    "           MOVE '00123000' TO U\n",
    "           INSPECT U REPLACING TRAILING '0' BY '9' BEFORE INITIAL ' '\n",
    "           DISPLAY 'U=' U\n",
    "           MOVE 'XXAB' TO T\n",
    "           INSPECT T REPLACING LEADING 'X' BY 'Y' TRAILING ' ' BY '-'\n",
    "           DISPLAY 'T2=' T\n",
    "           GOBACK.\n",
);

#[test]
fn inspect_trailing_takes_the_run_at_the_end_of_its_region_alike_on_both_executors() {
    let walked = Harness::source(INSPECT_TRAILING).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("N=02\nM=02\nT=AB  CD  XX**\nN2=00\nU=00123999  \nT2=YYAB--------\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(INSPECT_TRAILING).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned = diagnostics_under(INSPECT_TRAILING, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.2 == Some("IWX0023")).count(), 7, "{warned:?}");
}

/// GnuCOBOL's CALL ... RETURNING OMITTED, NOTHING and NULL, with exception phrases and a program
/// that is not there, before a subprogram that sets RETURN-CODE to 3.
const CALL_RETURNING_OMITTED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. OMITCALL.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  D PIC 9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           CALL 'SETS3'\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'PLAIN ' D\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           CALL 'SETS3' RETURNING OMITTED\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'OMITTED ' D\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           CALL 'SETS3' RETURNING NOTHING\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'NOTHING ' D\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           IF D = 7\n",
    "               CALL 'SETS3' RETURNING NULL\n",
    "                   ON EXCEPTION MOVE 99 TO D\n",
    "                   NOT ON EXCEPTION MOVE RETURN-CODE TO D\n",
    "                       DISPLAY 'NULL ' D\n",
    "               END-CALL\n",
    "           END-IF\n",
    "           CALL 'NOSUCH' RETURNING OMITTED\n",
    "               ON EXCEPTION DISPLAY 'MISSING'\n",
    "           END-CALL\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'AFTER MISSING ' D\n",
    "           MOVE 0 TO RETURN-CODE\n",
    "           GOBACK.\n",
    "       END PROGRAM OMITCALL.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SETS3.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 3 TO RETURN-CODE\n",
    "           GOBACK.\n",
    "       END PROGRAM SETS3.\n",
);

#[test]
fn call_returning_omitted_leaves_the_callers_return_code_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "PLAIN 0003\nOMITTED 0007\nNOTHING 0007\nNULL 0007\nMISSING\nAFTER MISSING 0007\n";
    let walked = Harness::source(CALL_RETURNING_OMITTED).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(CALL_RETURNING_OMITTED).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn call_returning_omitted_is_refused_under_strict_and_warned_under_extended() {
    let caller = CALL_RETURNING_OMITTED.split_inclusive("END PROGRAM OMITCALL.\n").next().unwrap();
    let at = [(12, 35), (16, 35), (21, 39), (27, 36)];
    let warned = diagnostics_under(caller, numeric::Compliance::Extended);
    assert_eq!(warned, at.map(|(line, col)| (line, col, Some("IWX0024"), Severity::Warning)));
    let refused = diagnostics_under(caller, numeric::Compliance::Strict);
    assert_eq!(refused, at.map(|(line, col)| (line, col, Some("IWC0300"), Severity::Severe)));
}

#[test]
fn call_returning_nothing_names_an_item_a_program_declares_nothing() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. NAMED.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  NOTHING PIC S9(4) BINARY VALUE 5.\n",
        "       PROCEDURE DIVISION.\n",
        "           CALL 'SETS3' RETURNING NOTHING\n",
        "           GOBACK.\n",
    );
    assert_eq!(diagnostics_under(source, numeric::Compliance::Strict), []);
    assert_eq!(diagnostics_under(source, numeric::Compliance::Extended), []);
    let compiled = compile(syntax::parse(source).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let Stmt::Call(call) = &compiled.program.paragraphs[0].statements[0] else { panic!("{:?}", compiled.program.paragraphs[0].statements) };
    assert_eq!(call.returning.as_ref().map(|r| r.name.as_str()), Some("NOTHING"));
}

/// COMP-X items of a numeric and an alphanumeric PICTURE stored past their digits, and a PIC XX
/// COMP-5 item past its bytes.
const COMP_X: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. COMPX.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  V2  PIC 99 COMP-X.\n",
    "       01  N7  PIC 9(7) COMP-X.\n",
    "       01  X1  PIC X COMP-X.\n",
    "       01  X2  PIC XX COMP-X VALUE 258.\n",
    "       01  F2  PIC XX COMP-5.\n",
    "       01  X8  PIC X(8) COMP-X.\n",
    "       01  X8R REDEFINES X8 PIC X(8).\n",
    "       01  A6  PIC X(6).\n",
    "       01  L   PIC 9.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 250 TO V2\n",
    "           ADD 10 TO V2 ON SIZE ERROR DISPLAY 'SIZE' END-ADD\n",
    "           COMPUTE V2 = V2 + 5\n",
    "           MOVE 300 TO X1\n",
    "           MOVE 70000 TO F2\n",
    "           COMPUTE L = FUNCTION LENGTH(N7)\n",
    "           DISPLAY V2 ' ' X1 ' ' X2 ' ' F2 ' ' L\n",
    "           MOVE V2 TO A6\n",
    "           MOVE HIGH-VALUES TO X8R\n",
    "           DISPLAY A6 ' ' X8 ' ' FUNCTION HEX-OF(X2)\n",
    "           GOBACK.\n",
);

#[test]
fn comp_x_is_limited_by_its_bytes_and_shown_in_its_digits_alike_on_both_executors() {
    let walked = Harness::source(COMP_X).flags(EXTENDED).run(Executor::Interpreter);
    assert!(walked.ending.is_ok(), "{:?}\n{}", walked.ending, walked.err);
    assert_eq!(walked.out, "SIZE\n55 44 0258 04464 3\n55     8446744073709551615 0102\n");
    let vm = Harness::source(COMP_X).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn comp_x_and_pic_x_comp_5_are_warnings_naming_their_bytes_under_extended_and_refused_under_strict() {
    let warned = diagnostics_under(COMP_X, numeric::Compliance::Extended);
    let ids: Vec<(u32, Option<&str>)> = warned.iter().map(|d| (d.0, d.2)).collect();
    assert_eq!(ids, [(5, Some("IWX0025")), (6, Some("IWX0025")), (7, Some("IWX0025")), (8, Some("IWX0025")), (9, Some("IWX0026")), (10, Some("IWX0025"))]);
    let parsed = syntax::parse_with(COMP_X, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let messages: Vec<&str> = compiled.diagnostics.iter().map(|m| m.message.as_str()).collect();
    assert!(messages[1].ends_with("N7 is 3 bytes of binary, 0 to 16777215, shown in 7 digits"), "{}", messages[1]);
    assert!(messages[4].ends_with("F2 is 2 bytes of binary, 0 to 65535"), "{}", messages[4]);
    let refused = diagnostics_under(COMP_X, numeric::Compliance::Strict);
    let ids: Vec<(u32, Option<&str>)> = refused.iter().map(|d| (d.0, d.2)).collect();
    assert_eq!(ids, [(5, Some("IWC0301")), (6, Some("IWC0301")), (7, Some("IWC0301")), (8, Some("IWC0301")), (9, Some("IWC0302")), (10, Some("IWC0301"))]);
    let nine = program("", "       01  P PIC X(9) COMP-X.\n", &line("GOBACK."));
    assert!(diagnostics_under(&nine, numeric::Compliance::Extended).iter().any(|d| d.2 == Some("IWC0303")));
}

/// FLOAT-SHORT and FLOAT-LONG items beside COMP-1 and COMP-2 ones, shown by DISPLAY.
const GNUCOBOL_FLOATS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. FLOATS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  C1 COMP-1 VALUE 1.5.\n",
    "       01  FS FLOAT-SHORT VALUE -0.001.\n",
    "       01  FL USAGE IS FLOAT-LONG.\n",
    "       01  Z  COMP-2 VALUE 0.\n",
    "       01  N  PIC S9(5)V99.\n",
    "       PROCEDURE DIVISION.\n",
    "           COMPUTE FL = 1 / 3\n",
    "           DISPLAY C1 ' ' FS\n",
    "           DISPLAY FL ' ' Z\n",
    "           COMPUTE FL = 12345.678\n",
    "           MOVE FL TO N\n",
    "           DISPLAY N\n",
    "           GOBACK.\n",
);

#[test]
fn float_short_and_float_long_are_comp_1_and_comp_2_shown_as_ibm_shows_them_on_both_executors() {
    let walked = Harness::source(GNUCOBOL_FLOATS).flags(EXTENDED).run(Executor::Interpreter);
    assert!(walked.ending.is_ok(), "{:?}\n{}", walked.ending, walked.err);
    assert_eq!(walked.out, " .15000000E 01 -.99999993E-03\n .33333333333333333E 00  .00000000000000000E 00\n+12345.68\n");
    let vm = Harness::source(GNUCOBOL_FLOATS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned = diagnostics_under(GNUCOBOL_FLOATS, numeric::Compliance::Extended);
    assert_eq!(warned, [(6, 15, Some("IWX0027"), Severity::Warning), (7, 24, Some("IWX0027"), Severity::Warning)]);
    let refused = syntax::parse(GNUCOBOL_FLOATS).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (6, Some("IWS0101")), "{refused}");
}

/// PERFORM FOREVER inline, left by EXIT PERFORM, and naming a paragraph, left by GO TO.
const PERFORM_FOREVER: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. PF.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 99 VALUE 0.\n",
    "       01  M PIC 99 VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           PERFORM FOREVER\n",
    "               ADD 1 TO N\n",
    "               IF N > 3 EXIT PERFORM END-IF\n",
    "               DISPLAY 'N ' N\n",
    "           END-PERFORM\n",
    "           PERFORM BUMP FOREVER\n",
    "           DISPLAY 'AFTER BUMP ' M\n",
    "           STOP RUN.\n",
    "       BUMP.\n",
    "           ADD 1 TO M\n",
    "           IF M > 2 DISPLAY 'M ' M GO TO DONE END-IF.\n",
    "       DONE.\n",
    "           DISPLAY 'DONE ' M\n",
    "           STOP RUN.\n",
);

#[test]
fn perform_forever_repeats_until_left_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "N 01\nN 02\nN 03\nM 03\nDONE 03\n";
    let walked = Harness::source(PERFORM_FOREVER).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!(walked.out, expected, "{:?}\n{}", walked.ending, walked.err);
    let vm = Harness::source(PERFORM_FOREVER).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned = diagnostics_under(PERFORM_FOREVER, numeric::Compliance::Extended);
    assert_eq!(warned, [(8, 20, Some("IWX0028"), Severity::Warning), (13, 25, Some("IWX0028"), Severity::Warning)]);
}

#[test]
fn perform_forever_beside_a_paragraph_named_forever_is_refused_under_extended() {
    let named = PERFORM_FOREVER.replace("       DONE.\n", "       FOREVER.\n").replace("GO TO DONE", "GO TO FOREVER");
    let refused = diagnostics_under(&named, numeric::Compliance::Extended);
    assert!(refused.iter().any(|d| (d.0, d.2) == (19, Some("IWC0304"))), "{refused:?}");
}

#[test]
fn accept_from_lines_and_columns_give_the_screen_size_under_extended() {
    let data = "       01  L PIC 999.\n       01  C PIC 999.\n       01  X PIC X(4).\n";
    let body = [line("ACCEPT L FROM LINES"), line("ACCEPT C FROM COLUMNS END-ACCEPT"), line("ACCEPT X FROM LINES"), line("DISPLAY L ' ' C ' [' X ']'"), line("GOBACK.")].concat();
    let source = program("", data, &body);
    let walked = Harness::source(&source).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!(walked.out, "024 080 [24  ]\n", "{:?}\n{}", walked.ending, walked.err);
    let vm = Harness::source(&source).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    assert_eq!(diagnostics_under(&source, numeric::Compliance::Extended).iter().filter(|d| d.2 == Some("IWX0029")).count(), 3);
    let refused = syntax::parse(&source).unwrap_err();
    assert_eq!(refused.id, Some("IWS0060"), "{refused}");
}

/// A line-sequential report's headings written BEFORE ADVANCING, as ACAS writes its first page's.
const BEFORE_ADVANCING: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BEFADV.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT P ASSIGN TO 'rpt.txt'\n",
    "               ORGANIZATION LINE SEQUENTIAL.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD  P.\n",
    "       01  R PIC X(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT P\n",
    "           MOVE 'AAAA' TO R WRITE R BEFORE 1\n",
    "           MOVE 'BBBB' TO R WRITE R BEFORE 2\n",
    "           MOVE 'CCCC' TO R WRITE R AFTER 1\n",
    "           MOVE 'DD' TO R WRITE R BEFORE PAGE\n",
    "           MOVE 'EEEE' TO R WRITE R\n",
    "           CLOSE P\n",
    "           GOBACK.\n",
);

#[test]
fn before_advancing_on_a_line_sequential_file_writes_the_line_then_moves_alike_on_both_executors() {
    let dir = temp("before-advancing");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rpt.txt");
    let dds = [format!("RPT.TXT={}:text", path.display())];
    let walked = Harness::source(BEFORE_ADVANCING).flags(EXTENDED).dds(&dds).run(Executor::Interpreter);
    assert_eq!(walked.ending.as_ref().ok(), Some(&Ending::Goback), "{}", walked.err);
    let written = std::fs::read(&path).unwrap();
    assert_eq!(String::from_utf8_lossy(&written), "AAAA\nBBBB\n\n\nCCCC\rDD\n\x0c\nEEEE\n");
    std::fs::remove_file(&path).unwrap();
    let vm = Harness::source(BEFORE_ADVANCING).flags(EXTENDED).dds(&dds).run(Executor::Vm);
    assert_eq!(vm.ending, walked.ending);
    assert_eq!(std::fs::read(&path).unwrap(), written);
    let warned = diagnostics_under(BEFORE_ADVANCING, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.2 == Some("IWX0030")).map(|d| d.0).collect::<Vec<_>>(), [14, 15, 17], "{warned:?}");
    assert!(diagnostics_under(BEFORE_ADVANCING, numeric::Compliance::Strict).iter().any(|d| d.2 == Some("IWC0145")));
}

/// FUNCTION MODULE-CALLER-ID in a main program and two levels of CALL below it.
const MODULE_CALLER_ID: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. MAINP.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  X PIC X(10).\n",
    "       01  N PIC 9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION MODULE-CALLER-ID TO X\n",
    "           COMPUTE N = FUNCTION LENGTH(FUNCTION MODULE-CALLER-ID)\n",
    "           DISPLAY 'MAIN [' X '] ' N\n",
    "           CALL 'SUBP'\n",
    "           STOP RUN.\n",
    "       END PROGRAM MAINP.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SUBP.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'SUB [' FUNCTION MODULE-CALLER-ID ']'\n",
    "           IF FUNCTION MODULE-CALLER-ID = 'MAINP' DISPLAY 'EQ' END-IF\n",
    "           CALL 'SUBQ'\n",
    "           GOBACK.\n",
    "       END PROGRAM SUBP.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SUBQ.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'SUBQ [' FUNCTION MODULE-CALLER-ID ']'\n",
    "           GOBACK.\n",
    "       END PROGRAM SUBQ.\n",
);

#[test]
fn module_caller_id_names_the_calling_program_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "MAIN [          ] 0000\nSUB [MAINP]\nEQ\nSUBQ [SUBP]\n";
    let walked = Harness::source(MODULE_CALLER_ID).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!(walked.out, expected, "{:?}\n{}", walked.ending, walked.err);
    let vm = Harness::source(MODULE_CALLER_ID).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let main = MODULE_CALLER_ID.split_inclusive("END PROGRAM MAINP.\n").next().unwrap();
    let warned = diagnostics_under(main, numeric::Compliance::Extended);
    assert_eq!(warned, [(8, 17, Some("IWX0031"), Severity::Warning), (9, 40, Some("IWX0031"), Severity::Warning)]);
    let refused = diagnostics_under(main, numeric::Compliance::Strict);
    assert_eq!(refused.iter().map(|d| (d.0, d.2)).collect::<Vec<_>>(), [(8, Some("IWC0305")), (9, Some("IWC0305"))]);
}

/// GOBACK RETURNING a literal, GOBACK GIVING a LINKAGE item, and STOP RUN RETURNING an item.
const STOP_RETURNING: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RETCALL.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9(3) VALUE 42.\n",
    "       01  D PIC 9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'GIVES4'\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'GIVES4 ' D\n",
    "           MOVE 300 TO N\n",
    "           CALL 'GIVESN' USING N\n",
    "           MOVE RETURN-CODE TO D\n",
    "           DISPLAY 'GIVESN ' D\n",
    "           IF D = 300\n",
    "               STOP RUN RETURNING N\n",
    "           END-IF\n",
    "           DISPLAY 'NOT HERE'\n",
    "           GOBACK.\n",
    "       END PROGRAM RETCALL.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. GIVES4.\n",
    "       PROCEDURE DIVISION.\n",
    "           GOBACK RETURNING 4.\n",
    "       END PROGRAM GIVES4.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. GIVESN.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01  L PIC 9(3).\n",
    "       PROCEDURE DIVISION USING L.\n",
    "           GOBACK GIVING L.\n",
    "       END PROGRAM GIVESN.\n",
);

#[test]
fn stop_run_and_goback_returning_set_return_code_alike_on_both_executors() {
    // cobc 3.2's output; its exit status is 300 modulo 256.
    let expected = "GIVES4 0004\nGIVESN 0300\n";
    let walked = Harness::source(STOP_RETURNING).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok(), walked.return_code), (expected, Some(&Ending::StopRun), 300), "{}", walked.err);
    let vm = Harness::source(STOP_RETURNING).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending, vm.return_code), (walked.out, walked.ending, walked.return_code));
}

#[test]
fn stop_run_and_goback_returning_are_refused_under_strict_and_warned_under_extended() {
    let (main, called) = STOP_RETURNING.split_at(STOP_RETURNING.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. GIVES4").unwrap());
    assert_eq!(diagnostics_under(main, numeric::Compliance::Extended), [(16, 25, Some("IWX0033"), Severity::Warning)]);
    assert_eq!(diagnostics_under(called, numeric::Compliance::Extended), [(4, 19, Some("IWX0033"), Severity::Warning)]);
    let refused = syntax::parse(main).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (16, 25, Some("IWC0306")), "{refused}");
    let refused = syntax::parse(called).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (4, 19, Some("IWC0306")), "{refused}");
}

/// A parameter passed, passed OMITTED, and not passed at all, tested with [NOT] OMITTED.
const OMITTED_PARAMETERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. OMITMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  A PIC X(3) VALUE 'AAA'.\n",
    "       01  B PIC X(3) VALUE 'BBB'.\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'OMITSUB' USING A B\n",
    "           CALL 'OMITSUB' USING A OMITTED\n",
    "           CALL 'OMITSUB' USING OMITTED B\n",
    "           CALL 'OMITSUB' USING A\n",
    "           CALL 'OMITSUB' USING BY REFERENCE OMITTED OMITTED\n",
    "           STOP RUN.\n",
    "       END PROGRAM OMITMAIN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. OMITSUB.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01  P1 PIC X(3).\n",
    "       01  P2 PIC X(3).\n",
    "       PROCEDURE DIVISION USING P1 P2.\n",
    "           IF P1 OMITTED\n",
    "               DISPLAY 'P1 OMITTED' WITH NO ADVANCING\n",
    "           ELSE\n",
    "               DISPLAY 'P1 ' P1 WITH NO ADVANCING\n",
    "           END-IF\n",
    "           IF P2 IS NOT OMITTED\n",
    "               DISPLAY ' P2 ' P2\n",
    "           ELSE\n",
    "               DISPLAY ' P2 OMITTED'\n",
    "           END-IF\n",
    "           IF (P1 OMITTED) OR (P2 OMITTED)\n",
    "               DISPLAY 'ONE IS OMITTED'\n",
    "           END-IF\n",
    "           GOBACK.\n",
    "       END PROGRAM OMITSUB.\n",
);

#[test]
fn omitted_tests_a_parameter_passed_omitted_or_not_passed_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "P1 AAA P2 BBB\nP1 AAA P2 OMITTED\nONE IS OMITTED\nP1 OMITTED P2 BBB\nONE IS OMITTED\nP1 AAA P2 OMITTED\nONE IS OMITTED\nP1 OMITTED P2 OMITTED\nONE IS OMITTED\n";
    let walked = Harness::source(OMITTED_PARAMETERS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(OMITTED_PARAMETERS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn omitted_is_refused_under_strict_and_warned_under_extended() {
    let called = &OMITTED_PARAMETERS[OMITTED_PARAMETERS.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OMITSUB").unwrap()..];
    let at = [(8, 18), (13, 25), (18, 19), (18, 35)];
    assert_eq!(diagnostics_under(called, numeric::Compliance::Extended), at.map(|(line, col)| (line, col, Some("IWX0034"), Severity::Warning)));
    let refused = syntax::parse(called).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (8, 18, Some("IWC0307")), "{refused}");
}

/// ANY LENGTH passed items of three lengths and a BY CONTENT literal.
const ANY_LENGTH_CALLS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ANYMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  SHORT-ONE PIC X(3) VALUE 'ABC'.\n",
    "       01  LONG-ONE  PIC X(10) VALUE 'ABCDEFGHIJ'.\n",
    "       01  GROUPED.\n",
    "           05 G1 PIC X(2) VALUE 'GG'.\n",
    "           05 G2 PIC 9(3) VALUE 123.\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'ANYSUB' USING SHORT-ONE\n",
    "           CALL 'ANYSUB' USING LONG-ONE\n",
    "           CALL 'ANYSUB' USING GROUPED\n",
    "           CALL 'ANYSUB' USING BY CONTENT 'LITERAL'\n",
    "           DISPLAY 'AFTER ' SHORT-ONE '|' LONG-ONE '|' GROUPED\n",
    "           STOP RUN.\n",
    "       END PROGRAM ANYMAIN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ANYSUB.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9(4).\n",
    "       LINKAGE SECTION.\n",
    "       01  L PIC X ANY LENGTH.\n",
    "       PROCEDURE DIVISION USING L.\n",
    "           MOVE FUNCTION LENGTH(L) TO N\n",
    "           DISPLAY 'LEN ' N ' [' L '] ' L(2:1)\n",
    "           IF L = 'ABC'\n",
    "               DISPLAY 'EQUALS ABC'\n",
    "           END-IF\n",
    "           MOVE 'XY' TO L\n",
    "           DISPLAY 'MOVED [' L ']'\n",
    "           GOBACK.\n",
    "       END PROGRAM ANYSUB.\n",
);

/// A function's ANY LENGTH parameter passed items and a reference-modified item.
const ANY_LENGTH_FUNCTION: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       FUNCTION-ID. FIRSTCH.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01  S PIC X ANY LENGTH.\n",
    "       01  R PIC X(12).\n",
    "       PROCEDURE DIVISION USING S RETURNING R.\n",
    "           MOVE FUNCTION LENGTH(S) TO R\n",
    "           MOVE S(1:1) TO R(12:1)\n",
    "           GOBACK.\n",
    "       END FUNCTION FIRSTCH.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. FMAIN.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       CONFIGURATION SECTION.\n",
    "       REPOSITORY.\n",
    "           FUNCTION FIRSTCH.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  W5 PIC X(5) VALUE 'HELLO'.\n",
    "       01  W9 PIC X(9) VALUE 'WORLDWIDE'.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY '[' FUNCTION FIRSTCH(W5) ']'\n",
    "           DISPLAY '[' FUNCTION FIRSTCH(W9) ']'\n",
    "           DISPLAY '[' FUNCTION FIRSTCH(W9(3:4)) ']'\n",
    "           STOP RUN.\n",
    "       END PROGRAM FMAIN.\n",
);

#[test]
fn any_length_takes_each_arguments_length_alike_on_both_executors() {
    // cobc 3.2's output.
    for (source, expected) in [(ANY_LENGTH_CALLS, "LEN 0003 [ABC] B\nEQUALS ABC\nMOVED [XY ]\nLEN 0010 [ABCDEFGHIJ] B\nMOVED [XY        ]\nLEN 0005 [GG123] G\nMOVED [XY   ]\nLEN 0007 [LITERAL] I\nMOVED [XY     ]\nAFTER XY |XY        |XY   \n"), (ANY_LENGTH_FUNCTION, "[000000005  H]\n[000000009  W]\n[000000004  R]\n")] {
        let walked = Harness::source(source).flags(EXTENDED).run(Executor::Interpreter);
        assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
        let vm = Harness::source(source).flags(EXTENDED).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
}

#[test]
fn any_length_is_refused_under_strict_and_where_ironwork_does_not_read_it() {
    let called = &ANY_LENGTH_CALLS[ANY_LENGTH_CALLS.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. ANYSUB").unwrap()..];
    assert_eq!(diagnostics_under(called, numeric::Compliance::Extended), [(7, 8, Some("IWX0035"), Severity::Warning), (9, 17, Some("IWX0008"), Severity::Warning)]);
    let refused = syntax::parse(called).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (7, 20, Some("IWC0308")), "{refused}");
    let returned = called.replace("USING L.", "USING L RETURNING L.");
    assert_eq!(diagnostics_under(&returned, numeric::Compliance::Extended).first().map(|d| (d.0, d.2)), Some((7, Some("IWR0076"))));
    let numeric = called.replace("PIC X ANY LENGTH", "PIC 9 ANY LENGTH");
    assert_eq!(diagnostics_under(&numeric, numeric::Compliance::Extended).first().map(|d| (d.0, d.2)), Some((7, Some("IWR0076"))));
    let literal = ANY_LENGTH_FUNCTION.replace("FIRSTCH(W5)", "FIRSTCH(123)");
    let libraries = syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended);
    let main = syntax::parse_all_with(&literal, &libraries).unwrap().into_iter().find(|p| p.id == "FMAIN").unwrap();
    let flags: Vec<String> = EXTENDED.iter().map(|f| f.to_string()).collect();
    let Err(refused) = compile(main, &flags) else { panic!("the numeric literal argument compiled") };
    assert_eq!(refused.iter().map(|e| (e.pos.line, e.id)).collect::<Vec<_>>(), [(23, Some("IWR0076"))]);
}

/// Micro Focus's file forms: SELECT with no FILE-CONTROL header, ASSIGN TO DISK an undeclared
/// name, an FD with no FILE SECTION header, START KEY NOT GREATER, LESS and <=, READ PREVIOUS
/// after them, and PERFORM UNTIL EXIT.
const MICRO_FOCUS_FILES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. MFFORMS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       CONFIGURATION SECTION.\n",
    "           SELECT KF ASSIGN TO DISK KF-NAME\n",
    "               ORGANIZATION IS INDEXED\n",
    "               ACCESS MODE IS DYNAMIC\n",
    "               RECORD KEY IS KF-KEY\n",
    "               FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FD  KF.\n",
    "       01  KF-REC.\n",
    "           05 KF-KEY  PIC X(3).\n",
    "           05 KF-DATA PIC X(5).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  FS PIC XX.\n",
    "       01  N  PIC 9 VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 'KFDD' TO KF-NAME\n",
    "           OPEN OUTPUT KF\n",
    "           MOVE 'AAA' TO KF-KEY  MOVE 'one' TO KF-DATA  WRITE KF-REC\n",
    "           MOVE 'CCC' TO KF-KEY  MOVE 'three' TO KF-DATA  WRITE KF-REC\n",
    "           MOVE 'EEE' TO KF-KEY  MOVE 'five' TO KF-DATA  WRITE KF-REC\n",
    "           CLOSE KF\n",
    "           OPEN INPUT KF\n",
    "           MOVE 'DDD' TO KF-KEY\n",
    "           START KF KEY IS NOT GREATER KF-KEY\n",
    "           DISPLAY 'START NOT GREATER DDD ' FS\n",
    "           READ KF PREVIOUS\n",
    "           DISPLAY 'PREVIOUS ' FS ' ' KF-REC\n",
    "           READ KF PREVIOUS\n",
    "           DISPLAY 'PREVIOUS ' FS ' ' KF-REC\n",
    "           READ KF PREVIOUS\n",
    "           DISPLAY 'PREVIOUS ' FS ' ' KF-REC\n",
    "           MOVE 'CCC' TO KF-KEY\n",
    "           START KF KEY IS LESS THAN KF-KEY\n",
    "           DISPLAY 'START LESS CCC ' FS\n",
    "           READ KF NEXT\n",
    "           DISPLAY 'NEXT ' FS ' ' KF-REC\n",
    "           MOVE 'CCC' TO KF-KEY\n",
    "           START KF KEY IS <= KF-KEY\n",
    "           DISPLAY 'START <= CCC ' FS\n",
    "           READ KF PREVIOUS\n",
    "           DISPLAY 'PREVIOUS ' FS ' ' KF-REC\n",
    "           MOVE 'AAA' TO KF-KEY\n",
    "           START KF KEY IS LESS KF-KEY\n",
    "           DISPLAY 'START LESS AAA ' FS\n",
    "           PERFORM UNTIL EXIT\n",
    "               ADD 1 TO N\n",
    "               IF N > 2 EXIT PERFORM END-IF\n",
    "           END-PERFORM\n",
    "           DISPLAY 'N ' N\n",
    "           CLOSE KF\n",
    "           STOP RUN.\n",
);

#[test]
fn micro_focus_file_forms_run_alike_on_both_executors() {
    // cobc 3.2's output, its BDB handler giving the indexed file.
    let expected = "START NOT GREATER DDD 00\nPREVIOUS 00 CCCthree\nPREVIOUS 00 AAAone  \nPREVIOUS 10 AAAone  \nSTART LESS CCC 00\nNEXT 00 AAAone  \nSTART <= CCC 00\nPREVIOUS 00 CCCthree\nSTART LESS AAA 23\nN 3\n";
    let dir = temp("micro-focus-files");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let file = dir.join(format!("{name}.dat"));
        let _ = std::fs::remove_file(&file);
        let ran = Harness::source(MICRO_FOCUS_FILES).flags(EXTENDED).dds(&[format!("KFDD={}", file.display())]).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{name}: {}", ran.err);
    }
}

#[test]
fn micro_focus_file_forms_are_warned_under_extended_and_refused_under_strict() {
    let warned: Vec<_> = diagnostics_under(MICRO_FOCUS_FILES, numeric::Compliance::Extended).into_iter().filter(|d| d.2.is_some_and(|id| id >= "IWX0036")).map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(5, Some("IWX0039")), (5, Some("IWX0103")), (11, Some("IWX0037")), (27, Some("IWX0036")), (36, Some("IWX0036")), (41, Some("IWX0036")), (46, Some("IWX0036")), (48, Some("IWX0038"))]);
    let refused = syntax::parse(MICRO_FOCUS_FILES).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (11, Some("IWC0310")), "{refused}");
    let until_exit = MICRO_FOCUS_FILES.replace("       DATA DIVISION.\n       FD", "       DATA DIVISION.\n       FILE SECTION.\n       FD");
    let refused = syntax::parse(&until_exit).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (49, Some("IWC0309")), "{refused}");
}

/// Micro Focus's split keys: a prime key joining two items out of record order, and an alternate
/// key with duplicates joining two others, written, read in each key's order, started on and read
/// by key.
const SPLIT_KEYS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SPLITKEY.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT SK ASSIGN TO SKDD\n",
    "               ORGANIZATION IS INDEXED\n",
    "               ACCESS MODE IS DYNAMIC\n",
    "               RECORD KEY IS SK-PRIME = SK-B SK-A\n",
    "               ALTERNATE RECORD KEY IS SK-ALT = SK-C SK-A\n",
    "                   WITH DUPLICATES\n",
    "               FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD  SK.\n",
    "       01  SK-REC.\n",
    "           05 SK-A PIC X(2).\n",
    "           05 SK-C PIC X(1).\n",
    "           05 SK-B PIC X(2).\n",
    "           05 SK-D PIC X(3).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  FS PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT SK\n",
    "           MOVE 'a1' TO SK-A\n",
    "           MOVE 'z' TO SK-C\n",
    "           MOVE 'b2' TO SK-B\n",
    "           MOVE 'one' TO SK-D\n",
    "           WRITE SK-REC\n",
    "           DISPLAY 'WRITE ' FS\n",
    "           MOVE 'a2' TO SK-A\n",
    "           MOVE 'y' TO SK-C\n",
    "           MOVE 'b1' TO SK-B\n",
    "           MOVE 'two' TO SK-D\n",
    "           WRITE SK-REC\n",
    "           DISPLAY 'WRITE ' FS\n",
    "           MOVE 'a0' TO SK-A\n",
    "           MOVE 'z' TO SK-C\n",
    "           MOVE 'b2' TO SK-B\n",
    "           MOVE 'thr' TO SK-D\n",
    "           WRITE SK-REC\n",
    "           DISPLAY 'WRITE ' FS\n",
    "           MOVE 'a1' TO SK-A\n",
    "           MOVE 'x' TO SK-C\n",
    "           MOVE 'b2' TO SK-B\n",
    "           MOVE 'dup' TO SK-D\n",
    "           WRITE SK-REC\n",
    "           DISPLAY 'WRITE DUP PRIME ' FS\n",
    "           CLOSE SK\n",
    "           OPEN INPUT SK\n",
    "           PERFORM 3 TIMES\n",
    "               READ SK NEXT\n",
    "               DISPLAY 'BY PRIME ' FS ' ' SK-REC\n",
    "           END-PERFORM\n",
    "           MOVE 'b2' TO SK-B MOVE 'a0' TO SK-A\n",
    "           START SK KEY IS > SK-PRIME\n",
    "           DISPLAY 'START > ' FS\n",
    "           READ SK NEXT\n",
    "           DISPLAY 'NEXT ' FS ' ' SK-REC\n",
    "           MOVE 'z' TO SK-C MOVE SPACES TO SK-A\n",
    "           START SK KEY IS NOT < SK-ALT\n",
    "           DISPLAY 'START ALT ' FS\n",
    "           PERFORM 3 TIMES\n",
    "               READ SK NEXT\n",
    "               DISPLAY 'BY ALT ' FS ' ' SK-REC\n",
    "           END-PERFORM\n",
    "           MOVE 'y' TO SK-C MOVE 'a2' TO SK-A\n",
    "           READ SK KEY IS SK-ALT\n",
    "           DISPLAY 'READ KEY ALT ' FS ' ' SK-REC\n",
    "           MOVE 'b2' TO SK-B MOVE 'a1' TO SK-A\n",
    "           READ SK KEY IS SK-PRIME\n",
    "           DISPLAY 'READ KEY PRIME ' FS ' ' SK-REC\n",
    "           CLOSE SK\n",
    "           STOP RUN.\n",
);

#[test]
fn split_keys_order_start_and_read_alike_on_both_executors() {
    // cobc 3.2's output, its BDB handler giving the indexed file.
    let expected = "WRITE 00\nWRITE 00\nWRITE 00\nWRITE DUP PRIME 22\nBY PRIME 00 a2yb1two\nBY PRIME 00 a0zb2thr\nBY PRIME 00 a1zb2one\nSTART > 00\nNEXT 00 a1zb2one\nSTART ALT 00\nBY ALT 00 a0zb2thr\nBY ALT 00 a1zb2one\nBY ALT 10 a1zb2one\nREAD KEY ALT 00 a2yb1two\nREAD KEY PRIME 00 a1zb2one\n";
    let dir = temp("split-keys");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let file = dir.join(format!("{name}.dat"));
        let _ = std::fs::remove_file(&file);
        let ran = Harness::source(SPLIT_KEYS).flags(EXTENDED).dds(&[format!("SKDD={}", file.display())]).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{name}: {}", ran.err);
    }
}

#[test]
fn split_keys_are_warned_under_extended_and_refused_under_strict() {
    let warned: Vec<_> = diagnostics_under(SPLIT_KEYS, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0040")).map(|d| (d.0, d.1)).collect();
    assert_eq!(warned, [(9, 39), (10, 47)]);
    let refused = syntax::parse(SPLIT_KEYS).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (9, 39, Some("IWC0311")), "{refused}");
    let outside = SPLIT_KEYS.replace("SK-C SK-A\n", "SK-C FS\n");
    let refused = diagnostics_under(&outside, numeric::Compliance::Extended);
    assert!(refused.iter().any(|d| (d.0, d.2) == (10, Some("IWC0090"))), "{refused:?}");
}

#[test]
fn a_load_module_carries_the_split_keys_after_the_lir_records_and_prints_them() {
    let parsed = syntax::parse_with(SPLIT_KEYS, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap_or_else(|e| panic!("{e}"));
    let Ok(compiled) = compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()) else { panic!("SPLIT_KEYS did not compile") };
    let lowered = crate::lower::lower(&compiled).unwrap_or_else(|e| panic!("{e:?}"));
    let keys = lowered.services.files[0].keys.clone().unwrap();
    let span = |offset, len| rt::lir::RecordSpan { offset, len };
    assert_eq!(keys.split, [(0, vec![span(3, 2), span(0, 2)]), (1, vec![span(2, 1), span(0, 2)])]);
    assert_eq!((keys.prime, keys.alternates[0].0), (span(3, 4), span(2, 3)));
    let printed = rt::lir::Listing::of(&lowered).to_string();
    assert!(printed.contains(" split 0 = +3 len 2, +0 len 2 split 1 = +2 len 1, +0 len 2"), "{printed}");
    let bytes = rt::module::write(std::slice::from_ref(&lowered));
    let module = rt::module::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(module.programs[0].services.files[0].keys.as_ref().map(|k| &k.split), Some(&keys.split), "the LIR section's end carries them");
}

/// FUNCTION STORED-CHAR-LENGTH of alphanumeric, all-space, national and literal arguments.
const STORED_CHAR_LENGTH: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SCL.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  A PIC X(10) VALUE \"AB  C\".\n",
    "       01  B PIC X(4) VALUE SPACES.\n",
    "       01  N PIC N(4) VALUE N\"XY\".\n",
    "       01  L PIC 9(4).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION STORED-CHAR-LENGTH(A) TO L DISPLAY L\n",
    "           MOVE FUNCTION STORED-CHAR-LENGTH(B) TO L DISPLAY L\n",
    "           MOVE FUNCTION STORED-CHAR-LENGTH(N) TO L DISPLAY L\n",
    "           MOVE FUNCTION STORED-CHAR-LENGTH(\"XYZ  \") TO L DISPLAY L\n",
    "           STOP RUN.\n",
);

/// Sentences ended with two periods, in the DATA and PROCEDURE DIVISIONs, and a period alone.
const DOUBLED_PERIODS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. DOTS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9 VALUE 1..\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY N..\n",
    "           DISPLAY \"TWO\".\n",
    "           .\n",
    "           STOP RUN.\n",
);

#[test]
fn stored_char_length_and_doubled_periods_run_alike_on_both_executors() {
    // cobc 3.2's output, but for the national argument: cobc, whose NATIONAL handling it calls
    // unfinished, gives 4 there, and ironwork its two characters.
    for (source, expected) in [(STORED_CHAR_LENGTH, "0005\n0000\n0002\n0003\n"), (DOUBLED_PERIODS, "1\nTWO\n")] {
        let walked = Harness::source(source).flags(EXTENDED).run(Executor::Interpreter);
        assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
        let vm = Harness::source(source).flags(EXTENDED).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
}

#[test]
fn stored_char_length_and_doubled_periods_are_warned_under_extended_and_refused_under_strict() {
    let warned: Vec<_> = diagnostics_under(STORED_CHAR_LENGTH, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0042")).map(|d| d.0).collect();
    assert_eq!(warned, [10, 11, 12, 13]);
    let refused = diagnostics_under(STORED_CHAR_LENGTH, numeric::Compliance::Strict);
    assert_eq!(refused.iter().filter(|d| d.2 == Some("IWC0312")).count(), 4, "{refused:?}");
    let warned: Vec<_> = diagnostics_under(DOUBLED_PERIODS, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0041")).map(|d| (d.0, d.1)).collect();
    assert_eq!(warned, [(5, 27), (7, 21)]);
    let refused = syntax::parse(DOUBLED_PERIODS).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (5, Some("IWS0026")), "{refused}");
}

/// DELETE FILE on an open file, a closed one, and one already deleted.
const DELETE_FILE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. DELFILE.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT F ASSIGN TO DELDD\n",
    "               ORGANIZATION IS LINE SEQUENTIAL\n",
    "               FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD  F.\n",
    "       01  F-REC PIC X(5).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  FS PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT F\n",
    "           MOVE 'hello' TO F-REC WRITE F-REC\n",
    "           DELETE FILE F\n",
    "           DISPLAY 'DELETE OPEN ' FS\n",
    "           CLOSE F\n",
    "           DELETE FILE F\n",
    "           DISPLAY 'DELETE ' FS\n",
    "           OPEN INPUT F\n",
    "           DISPLAY 'OPEN AFTER ' FS\n",
    "           DELETE FILE F\n",
    "           DISPLAY 'DELETE AGAIN ' FS\n",
    "           STOP RUN.\n",
);

#[test]
fn delete_file_removes_a_closed_files_data_set_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "DELETE OPEN 41\nDELETE 00\nOPEN AFTER 35\nDELETE AGAIN 35\n";
    let dir = temp("delete-file");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let file = dir.join(format!("{name}.txt"));
        let ran = Harness::source(DELETE_FILE).flags(EXTENDED).dds(&[format!("DELDD={}:text", file.display())]).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{name}: {}", ran.err);
        assert!(!file.exists(), "{name}: the data set is gone");
    }
    let warned: Vec<_> = diagnostics_under(DELETE_FILE, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0043")).map(|d| d.0).collect();
    assert_eq!(warned, [18, 21, 25]);
    let refused = syntax::parse(DELETE_FILE).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (18, Some("IWC0313")), "{refused}");
}

/// PROGRAM-POINTER items set by SET ... TO ENTRY and called, compared and moved.
const PROGRAM_POINTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. PPMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  PP USAGE PROGRAM-POINTER.\n",
    "       01  PQ PROGRAM-POINTER.\n",
    "       01  N  PIC 9 VALUE 1.\n",
    "       PROCEDURE DIVISION.\n",
    "           IF PP = NULL DISPLAY 'NULL AT START' END-IF\n",
    "           SET PP TO ENTRY 'PPSUB'\n",
    "           CALL PP USING N\n",
    "           SET PQ TO PP\n",
    "           CALL PQ USING N\n",
    "           DISPLAY 'N ' N\n",
    "           IF PP = PQ DISPLAY 'SAME' END-IF\n",
    "           STOP RUN.\n",
    "       END PROGRAM PPMAIN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. PPSUB.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01  L PIC 9.\n",
    "       PROCEDURE DIVISION USING L.\n",
    "           ADD 1 TO L\n",
    "           DISPLAY 'IN PPSUB ' L\n",
    "           GOBACK.\n",
    "       END PROGRAM PPSUB.\n",
);

/// A last data description entry with no period before PROCEDURE DIVISION.
const NO_PERIOD: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. NOPERIOD.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  A PIC 9 VALUE 7.\n",
    "       01  B PIC 99.99\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 1.5 TO B\n",
    "           DISPLAY A ' ' B\n",
    "           STOP RUN.\n",
);

/// BASED records given storage by SET ADDRESS OF.
const BASED_ITEMS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BASEDP.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  REC BASED.\n",
    "           05 R-A PIC X(3).\n",
    "           05 R-N PIC 9(3).\n",
    "       01  OTHER-REC PIC X(6) BASED.\n",
    "       01  P USAGE POINTER.\n",
    "       01  HOLDER PIC X(6).\n",
    "       PROCEDURE DIVISION.\n",
    "           IF ADDRESS OF REC = NULL DISPLAY 'NO STORAGE' END-IF\n",
    "           SET ADDRESS OF REC TO ADDRESS OF HOLDER\n",
    "           MOVE 'ABC' TO R-A MOVE 42 TO R-N\n",
    "           DISPLAY 'REC ' REC\n",
    "           SET P TO ADDRESS OF REC\n",
    "           SET ADDRESS OF OTHER-REC TO P\n",
    "           DISPLAY 'OTHER ' OTHER-REC\n",
    "           \n",
    "           STOP RUN.\n",
);

#[test]
fn program_pointers_missing_periods_and_based_items_run_alike_on_both_executors() {
    // cobc 3.2's output.
    for (source, expected) in [(PROGRAM_POINTERS, "NULL AT START\nIN PPSUB 2\nIN PPSUB 3\nN 3\nSAME\n"), (NO_PERIOD, "7 01.50\n"), (BASED_ITEMS, "NO STORAGE\nREC ABC042\nOTHER ABC042\n")] {
        let walked = Harness::source(source).flags(EXTENDED).run(Executor::Interpreter);
        assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
        let vm = Harness::source(source).flags(EXTENDED).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
}

#[test]
fn program_pointers_missing_periods_and_based_items_are_warned_under_extended_and_refused_under_strict() {
    let warned = |source: &str, id: &str| diagnostics_under(source, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some(id)).map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(warned(PROGRAM_POINTERS, "IWX0044"), [5, 6]);
    assert_eq!(warned(BASED_ITEMS, "IWX0046"), [5, 8]);
    for (source, line, id) in [(PROGRAM_POINTERS, 5, "IWC0314"), (BASED_ITEMS, 5, "IWC0316")] {
        let refused = syntax::parse(source).unwrap_err();
        assert_eq!((refused.pos.line, refused.id), (line, Some(id)), "{refused}");
    }
    for (compliance, id, severity) in [(numeric::Compliance::Strict, "IWS0105", Severity::Error), (numeric::Compliance::Extended, "IWX0093", Severity::Warning)] {
        let assumed: Vec<_> = diagnostics_under(NO_PERIOD, compliance).into_iter().filter(|d| d.2 == Some(id)).map(|d| (d.0, d.3)).collect();
        assert_eq!(assumed, [(7, severity)], "{compliance:?}");
    }
    let nested = BASED_ITEMS.replace("05 R-A PIC X(3).", "05 R-A PIC X(3) BASED.");
    assert!(diagnostics_under(&nested, numeric::Compliance::Extended).iter().any(|d| (d.0, d.2) == (6, Some("IWR0077"))));
}

/// GnuCOBOL's ALLOCATE of a BASED record and FREE of the record.
const FREE_RECORD: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. FREEREC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  REC BASED.\n",
    "           05 R-A PIC X(3).\n",
    "       PROCEDURE DIVISION.\n",
    "           ALLOCATE REC\n",
    "           MOVE 'ABC' TO R-A\n",
    "           DISPLAY 'REC ' REC\n",
    "           FREE REC\n",
    "           IF ADDRESS OF REC = NULL DISPLAY 'FREED' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn free_of_a_based_record_releases_its_storage_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "REC ABC\nFREED\n";
    let walked = Harness::source(FREE_RECORD).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(FREE_RECORD).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(FREE_RECORD, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0047")).map(|d| (d.0, d.1)).collect();
    assert_eq!(warned, [(11, 17)]);
    let linkage = FREE_RECORD.replace("       WORKING-STORAGE SECTION.\n       01  REC BASED.", "       LINKAGE SECTION.\n       01  REC.");
    let refused = syntax::parse(&linkage).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (11, Some("IWC0317")), "{refused}");
}

/// Conditional compilation, >>TURN and a >>D debugging line in fixed form, with debugging mode.
const DIRECTIVES_FIXED: &str = concat!(
    "       >>DEFINE MODE AS 'TEST'\n",
    "       >>DEFINE LEVEL AS 3\n",
    "       >>TURN EC-SIZE CHECKING ON\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. DIRS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       CONFIGURATION SECTION.\n",
    "       SOURCE-COMPUTER. X WITH DEBUGGING MODE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  N PIC 9 VALUE 1.\n",
    "       PROCEDURE DIVISION.\n",
    "       >>IF MODE = 'TEST'\n",
    "           DISPLAY 'MODE IS TEST'\n",
    "       >>ELSE\n",
    "           DISPLAY 'MODE IS NOT TEST'\n",
    "       >>END-IF\n",
    "       >>IF UNSET IS DEFINED\n",
    "           DISPLAY 'UNSET DEFINED'\n",
    "       >>ELIF LEVEL > 2\n",
    "           DISPLAY 'LEVEL ABOVE 2'\n",
    "           >>IF LEVEL = 3\n",
    "           DISPLAY 'LEVEL 3'\n",
    "           >>END-IF\n",
    "       >>ELSE\n",
    "           DISPLAY 'ELSE'\n",
    "       >>END-IF\n",
    "       >>IF NOTHING NOT DEFINED\n",
    "           DISPLAY 'NOTHING NOT DEFINED'\n",
    "       >>END-IF\n",
    "       >>D DISPLAY 'DEBUG LINE'\n",
    "           DISPLAY 'END'\n",
    "           STOP RUN.\n",
);

/// Conditional compilation and >>D debugging lines in free form, one a word cobc ignores.
const DIRECTIVES_FREE: &str = concat!(
    ">>SOURCE FORMAT IS FREE\n",
    "IDENTIFICATION DIVISION.\n",
    "PROGRAM-ID. FREEDIR.\n",
    "ENVIRONMENT DIVISION.\n",
    "CONFIGURATION SECTION.\n",
    "SOURCE-COMPUTER. X WITH DEBUGGING MODE.\n",
    "PROCEDURE DIVISION.\n",
    ">>IF docpass NOT DEFINED\n",
    "    DISPLAY \"no docpass\"\n",
    ">>ELSE\n",
    "    DISPLAY \"docpass\"\n",
    ">>END-IF\n",
    ">>D DISPLAY \"dbg one\"\n",
    ">>Ddisplay \"dbg two\"\n",
    "    STOP RUN.\n",
);

#[test]
fn compiler_directives_choose_the_lines_compiled_alike_on_both_executors() {
    // cobc 3.2's output.
    for (source, expected) in [(DIRECTIVES_FIXED, "MODE IS TEST\nLEVEL ABOVE 2\nLEVEL 3\nNOTHING NOT DEFINED\nDEBUG LINE\nEND\n"), (DIRECTIVES_FREE, "no docpass\ndbg one\n")] {
        let walked = Harness::source(source).flags(EXTENDED).run(Executor::Interpreter);
        assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
        let vm = Harness::source(source).flags(EXTENDED).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
    let warned: Vec<_> = diagnostics_under(DIRECTIVES_FREE, numeric::Compliance::Extended).into_iter().filter(|d| d.2.is_some_and(|id| id >= "IWX0048")).map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(8, Some("IWX0048")), (13, Some("IWX0050")), (14, Some("IWX0049"))]);
}

/// INITIALISE, and an inline PERFORM VARYING with an AFTER phrase.
const INITIALISE_AND_AFTER: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BATCH3.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  REC.\n",
    "           05 R-A PIC X(3) VALUE 'ABC'.\n",
    "           05 R-N PIC 9(3) VALUE 42.\n",
    "       01  D2 PIC 9V9 VALUE 7.\n",
    "       01  I PIC 9.\n",
    "       01  J PIC 9.\n",
    "       PROCEDURE DIVISION.\n",
    "           INITIALISE REC\n",
    "           DISPLAY 'REC [' REC ']'\n",
    "           PERFORM VARYING I FROM 1 BY 1 UNTIL I > 2\n",
    "                   AFTER J FROM 1 BY 1 UNTIL J > 3\n",
    "               DISPLAY I J\n",
    "           END-PERFORM\n",
    "           STOP RUN.\n",
);

#[test]
fn initialise_and_an_inline_perform_with_after_run_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "REC [   000]\n11\n12\n13\n21\n22\n23\n";
    let walked = Harness::source(INITIALISE_AND_AFTER).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(INITIALISE_AND_AFTER).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(INITIALISE_AND_AFTER, numeric::Compliance::Extended).into_iter().filter(|d| matches!(d.2, Some("IWX0051" | "IWX0052"))).map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(12, Some("IWX0052")), (14, Some("IWX0051"))]);
    let refused = syntax::parse(&INITIALISE_AND_AFTER.replace("INITIALISE", "INITIALIZE")).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (14, Some("IWS0058")), "{refused}");
}

/// FUNCTION CONCATENATE of an item, a literal and a number, of a function's value, and its length.
const CONCATENATE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CONCAT.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  A PIC X(5) VALUE \"AB\".\n",
    "       01  N PIC 9(3) VALUE 42.\n",
    "       01  R PIC X(20).\n",
    "       01  L PIC 9(9).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION CONCATENATE(A \"-\" N) TO R\n",
    "           DISPLAY \"[\" R \"]\"\n",
    "           DISPLAY FUNCTION CONCATENATE(\"X\" FUNCTION TRIM(A) \"Y\")\n",
    "           COMPUTE L = FUNCTION LENGTH(FUNCTION CONCATENATE(A A))\n",
    "           DISPLAY L\n",
    "           STOP RUN.\n",
);

#[test]
fn concatenate_joins_its_arguments_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "[AB   -042           ]\nXABY\n000000010\n";
    let walked = Harness::source(CONCATENATE).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(CONCATENATE).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(CONCATENATE, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0053")).map(|d| d.0).collect();
    assert_eq!(warned, [10, 12, 13]);
    let refused = diagnostics_under(CONCATENATE, numeric::Compliance::Strict);
    assert_eq!(refused.iter().filter(|d| d.2 == Some("IWC0319")).count(), 3, "{refused:?}");
}

/// PROCEDURE DIVISION CHAINING of three items, one numeric.
const CHAINING: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CHAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  X            PIC X VALUE '-'.\n",
    "       01  ABCD         PIC X(4) VALUE 'ZZZZ'.\n",
    "       01  NUM          PIC 9 VALUE 7.\n",
    "       PROCEDURE DIVISION CHAINING X ABCD NUM.\n",
    "           DISPLAY '[' X '][' ABCD '][' NUM ']'\n",
    "           STOP RUN.\n",
);

#[test]
fn chaining_moves_each_run_argument_into_its_item_alike_on_both_executors() {
    // cobc 3.2's output for each command line.
    for (parm, expected) in [("", "[-][ZZZZ][7]\n"), ("A", "[A][ZZZZ][7]\n"), ("AB CDEFG 5", "[A][CDEF][5]\n"), ("A B 123", "[A][B   ][1]\n")] {
        let walked = Harness::source(CHAINING).flags(EXTENDED).parm(parm).run(Executor::Interpreter);
        assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{parm}: {}", walked.err);
        let vm = Harness::source(CHAINING).flags(EXTENDED).parm(parm).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
    let warned: Vec<_> = diagnostics_under(CHAINING, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0055")).map(|d| (d.0, d.1)).collect();
    assert_eq!(warned, [(8, 27)]);
    let refused = syntax::parse(CHAINING).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (8, Some("IWC0320")), "{refused}");
}

/// DISPLAY UPON SYSERR, with and without NO ADVANCING, between DISPLAYs of the output.
const SYSERR: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SYSERRP.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY \"TO OUT\"\n",
    "           DISPLAY \"TO ERR \" 42 UPON SYSERR\n",
    "           DISPLAY \"AGAIN\" UPON SYSERR WITH NO ADVANCING\n",
    "           DISPLAY \"OUT END\"\n",
    "           STOP RUN.\n",
);

#[test]
fn display_upon_syserr_writes_standard_error_alike_on_both_executors() {
    // cobc 3.2's standard output and standard error.
    for executor in [Executor::Interpreter, Executor::Vm] {
        let ran = Harness::source(SYSERR).flags(EXTENDED).run(executor);
        assert_eq!((ran.out.as_str(), ran.err.as_str(), ran.ending.as_ref().ok()), ("TO OUT\nOUT END\n", "TO ERR 42\nAGAIN", Some(&Ending::StopRun)));
    }
    let warned: Vec<_> = diagnostics_under(SYSERR, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0056")).map(|d| d.0).collect();
    assert_eq!(warned, [5, 6]);
    assert!(diagnostics_under(SYSERR, numeric::Compliance::Strict).iter().any(|d| (d.0, d.2) == (5, Some("IWC0073"))));
}

/// COB-CRT-STATUS before any screen ACCEPT, after one PF1 ends, in the exception phrase of one CLEAR
/// ends, and after one ENTER ends.
const CRT_STATUS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CRTST.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  REPLY PIC X(3).\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'BEFORE ' COB-CRT-STATUS\n",
    "           ACCEPT REPLY AT 0101\n",
    "           DISPLAY 'F1 ' COB-CRT-STATUS\n",
    "           ACCEPT REPLY AT 0101\n",
    "               ON EXCEPTION DISPLAY 'IN HANDLER ' COB-CRT-STATUS\n",
    "           END-ACCEPT\n",
    "           ACCEPT REPLY AT 0101\n",
    "           DISPLAY 'ENTER ' COB-CRT-STATUS ' ' REPLY\n",
    "           STOP RUN.\n",
);

#[test]
fn cob_crt_status_holds_the_key_that_ended_a_screen_accept_alike_on_both_executors() {
    // The codes GnuCOBOL's screenio.cpy gives F1, Esc and ENTER.
    let script = "PF1\nCLEAR\nstring abc\nENTER\n";
    let shown = |out: &str| out.lines().filter(|l| ["BEFORE", "F1", "IN HANDLER", "ENTER"].iter().any(|w| l.contains(w))).map(str::trim_end).collect::<Vec<_>>().join("|");
    let walked = Harness::source(CRT_STATUS).flags(EXTENDED).screens(script).run(Executor::Interpreter);
    assert_eq!(shown(&walked.out), "BEFORE 0000|F1 1001|IN HANDLER 2005|ENTER 0000 abc", "{}", walked.err);
    let vm = Harness::source(CRT_STATUS).flags(EXTENDED).screens(script).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(CRT_STATUS, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0057")).map(|d| d.0).collect();
    assert_eq!(warned, [7]);
}

/// PICTUREs of 20 and 27 digit positions, and one of 18 that carries.
const WIDE_PICTURES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. WIDE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 A PIC 9(20) VALUE 12345678901234567890.\n",
    "       01 B PIC 9(25)V99.\n",
    "       01 C PIC 9(18) VALUE 999999999999999999.\n",
    "       PROCEDURE DIVISION.\n",
    "           ADD 1 TO A\n",
    "           COMPUTE B = A * 1000 + 0.5\n",
    "           ADD 1 TO C\n",
    "           DISPLAY A ' ' B ' ' C\n",
    "           GOBACK.\n",
);

#[test]
fn a_picture_past_18_digits_compiles_the_program_with_arith_extend_under_extended() {
    let walked = Harness::source(WIDE_PICTURES).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("12345678901234567891 0012345678901234567891000.50 000000000000000000\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(WIDE_PICTURES).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    assert_eq!(diagnostics_under(WIDE_PICTURES, numeric::Compliance::Extended), [(5, 8, Some("IWX0060"), Severity::Warning)]);
    assert!(diagnostics_under(WIDE_PICTURES, numeric::Compliance::Strict).iter().any(|d| d.2 == Some("IWC0069")));
    let card = format!("       CBL ARITH(COMPAT)\n{WIDE_PICTURES}");
    assert!(diagnostics_under(&card, numeric::Compliance::Extended).iter().any(|d| d.2 == Some("IWC0069")));
    let past_31 = WIDE_PICTURES.replace("9(25)V99", "9(30)V99");
    let shown = diagnostics_under(&past_31, numeric::Compliance::Extended);
    assert!(shown.iter().any(|d| d.0 == 6 && d.3 == Severity::Severe), "{shown:?}");
}

/// FUNCTION SUBSTITUTE and SUBSTITUTE-CASE: several pairs, the order they are tried in, a number's
/// digits and a value's length.
const SUBSTITUTE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SUBST.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 T PIC X(20) VALUE \"abcabc xyz\".\n",
    "       01 N PIC 9(9).\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(T \"abc\" \"Q\") \"]\"\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(T \"b\" \"BBB\" \"xyz\" \"!\") \"]\"\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE-CASE(T \"ABC\" \"z\") \"]\"\n",
    "           COMPUTE N = FUNCTION LENGTH(FUNCTION SUBSTITUTE(T \"abc\" \"Q\"))\n",
    "           DISPLAY N\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(\"abc\" \"ab\" \"X\" \"abc\" \"Y\") \"]\"\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(\"abc\" \"abc\" \"Y\" \"ab\" \"X\") \"]\"\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(\"aaa\" \"aa\" \"b\") \"]\"\n",
    "           DISPLAY \"[\" FUNCTION SUBSTITUTE(\"a1a\" 1 \"x\") \"]\"\n",
    "           GOBACK.\n",
);

#[test]
fn substitute_replaces_the_first_pair_found_at_each_position_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "[QQ xyz          ]\n[aBBBcaBBBc !          ]\n[zz xyz          ]\n000000016\n[Xc]\n[Y]\n[ba]\n[axa]\n";
    let walked = Harness::source(SUBSTITUTE).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(SUBSTITUTE).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(SUBSTITUTE, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0062")).map(|d| d.0).collect();
    assert_eq!(warned, [8, 9, 10, 11, 13, 14, 15, 16]);
    let refused = diagnostics_under(SUBSTITUTE, numeric::Compliance::Strict);
    assert_eq!(refused.iter().filter(|d| d.2 == Some("IWC0321")).count(), 8, "{refused:?}");
}

/// Zoned, packed and binary items of each sign and scale, a separate sign each side, PICTURE P
/// each side and a zoned item holding letters.
const NUMBERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. NUMBERS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 A PIC S9(3)V99 VALUE -12.5.\n",
    "       01 B PIC S9(3) COMP-3 VALUE -12.\n",
    "       01 C PIC 9(3)V9 COMP-3 VALUE 1.5.\n",
    "       01 D PIC S9(4) COMP VALUE -7.\n",
    "       01 E PIC S9(3)V99 SIGN LEADING SEPARATE VALUE -1.25.\n",
    "       01 F PIC 9(3)PP VALUE 12300.\n",
    "       01 G PIC PPP99 VALUE 0.00012.\n",
    "       01 H PIC S9(3)V99 COMP-5 VALUE 3.5.\n",
    "       01 K PIC SV99 VALUE -.5.\n",
    "       01 L PIC S9(3)V99 SIGN TRAILING SEPARATE VALUE 1.25.\n",
    "       01 M PIC S9(3) VALUE 12.\n",
    "       01 N PIC 9V99 BINARY VALUE 1.5.\n",
    "       01 Q PIC X(3) VALUE \"abc\".\n",
    "       01 R REDEFINES Q PIC 9(3).\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY A '|' B '|' C '|' D '|' E '|' F\n",
    "           DISPLAY G '|' H '|' K '|' L '|' M '|' N '|' R\n",
    "           GOBACK.\n",
);

#[test]
fn display_shows_numbers_as_the_compliance_level_or_the_flag_says_alike_on_both_executors() {
    // cobc 3.2's default dialect and -std=ibm-strict.
    let cobc = "-012.50|-012|001.5|-0007|-001.25|12300\n.00012|+0000000350|-.50|001.25+|+012|1.50|abc\n";
    let ibm_strict = "01250-|-012|0015|-00007|-00125|123\n12|+0000000350|50-|00125+|012+|00150|abc\n";
    let ibm = "0125}|01K|0015|000P|-00125|123\n12|0000000350|5}|00125+|01B|150|abc\n";
    for (flags, shown) in [
        (&["--compliance=extended"][..], cobc),
        (&["--compliance=extended", "--relaxed"], cobc),
        (&["--dialect=gnucobol"], ibm_strict),
        (&["--compliance=extended", "--dialect=gnucobol"], ibm_strict),
        (&[], ibm),
        (&["--compliance=extended", "--numeric-display=ibm"], ibm),
        (&["--numeric-display=cobc"], cobc),
    ] {
        let walked = Harness::source(NUMBERS).flags(flags).run(Executor::Interpreter);
        assert_eq!(walked.out, shown, "{flags:?}: {}", walked.err);
        let vm = Harness::source(NUMBERS).flags(flags).run(Executor::Vm);
        assert_eq!(vm.out, walked.out, "{flags:?}");
    }
    let card = format!("       CBL DISPSIGN(COMPAT)\n{NUMBERS}");
    assert_eq!(Harness::source(&card).flags(EXTENDED).run(Executor::Interpreter).out, ibm);
}

/// z390's short and long hexadecimal floating point beside COMP-2.
const Z390_FLOATS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. Z390F.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       77 E FLOAT-HEX-7 VALUE 1.5.\n",
    "       77 D FLOAT-HEX-15 VALUE 123456789012345.\n",
    "       77 C COMP-2 VALUE 123456789012345.\n",
    "       PROCEDURE DIVISION.\n",
    "           COMPUTE E = E * 2\n",
    "           DISPLAY E ' ' D\n",
    "           IF D = C DISPLAY 'SAME' END-IF\n",
    "           GOBACK.\n",
);

#[test]
fn float_hex_7_and_15_are_comp_1_and_comp_2_alike_on_both_executors() {
    let walked = Harness::source(Z390_FLOATS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (" .30000000E 01  .12345678901234500E 15\nSAME\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(Z390_FLOATS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    assert_eq!(diagnostics_under(Z390_FLOATS, numeric::Compliance::Extended), [(5, 13, Some("IWX0066"), Severity::Warning), (6, 13, Some("IWX0066"), Severity::Warning)]);
    let refused = syntax::parse(Z390_FLOATS).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (5, Some("IWS0107")), "{refused}");
}

/// An inline PERFORM a period ends, CALL STATIC ... GIVING, and NUMBER-OF-CALL-PARAMETERS in the
/// main program and in programs called with one and two arguments.
const CALL_FORMS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CALLFORM.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 Z PIC 9 VALUE 0.\n",
    "       01 A PIC X(5).\n",
    "       01 R PIC S9(9) BINARY.\n",
    "       PROCEDURE DIVISION.\n",
    "           PERFORM VARYING Z FROM 1 BY 1 UNTIL Z > 2\n",
    "            IF Z = 1\n",
    "              DISPLAY 'ONE'\n",
    "            END-IF.\n",
    "           DISPLAY 'AFTER ' Z\n",
    "           CALL STATIC 'SUB1' USING A GIVING R\n",
    "           CALL 'SUB2' USING A A\n",
    "           CALL 'SUB2' USING A\n",
    "           DISPLAY 'MAIN ' NUMBER-OF-CALL-PARAMETERS\n",
    "           STOP RUN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SUB1.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01 X PIC X.\n",
    "       PROCEDURE DIVISION USING X.\n",
    "           DISPLAY 'SUB1 ' NUMBER-OF-CALL-PARAMETERS\n",
    "           GOBACK.\n",
    "       END PROGRAM SUB1.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SUB2.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01 X PIC X.\n",
    "       01 Y PIC X.\n",
    "       PROCEDURE DIVISION USING X Y.\n",
    "           DISPLAY 'SUB2 ' NUMBER-OF-CALL-PARAMETERS\n",
    "           GOBACK.\n",
    "       END PROGRAM SUB2.\n",
    "       END PROGRAM CALLFORM.\n",
);

#[test]
fn call_static_giving_call_parameters_and_a_period_ending_perform_run_as_cobc_runs_them() {
    // cobc 3.2 -std=mf's output; its default dialect refuses the PERFORM the period ends.
    let walked = Harness::source(CALL_FORMS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("ONE\nAFTER 3\nSUB1 +000000001\nSUB2 +000000002\nSUB2 +000000001\nMAIN +000000000\n", Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(CALL_FORMS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(CALL_FORMS, numeric::Compliance::Extended).into_iter().map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(12, Some("IWX0079")), (14, Some("IWX0076")), (14, Some("IWX0077")), (17, Some("IWX0078"))]);
    let refused = |text: &str| syntax::parse(text).unwrap_err().id;
    assert_eq!(refused(CALL_FORMS), Some("IWS0108"));
    let ended = CALL_FORMS.replace("            END-IF.\n", "            END-IF\n           END-PERFORM\n");
    assert_eq!(refused(&ended), Some("IWC0326"));
    assert_eq!(refused(&ended.replace("CALL STATIC", "CALL")), Some("IWC0327"));
}

/// A procedure-pointer MOVEd to another and to a table entry, CALLed through, and a line written
/// UPON STDERR.
const POINTER_MOVES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. PTRMOVE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 P1 PROGRAM-POINTER.\n",
    "       01 P2 PROGRAM-POINTER.\n",
    "       01 T.\n",
    "          05 PS PROGRAM-POINTER OCCURS 2.\n",
    "       PROCEDURE DIVISION.\n",
    "           SET P1 TO ENTRY 'HELLO'\n",
    "           MOVE P1 TO P2\n",
    "           MOVE P2 TO PS(2)\n",
    "           CALL PS(2)\n",
    "           DISPLAY 'to stderr' UPON STDERR\n",
    "           IF P1 = PS(2) DISPLAY 'SAME' END-IF\n",
    "           STOP RUN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. HELLO.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'HELLO'\n",
    "           GOBACK.\n",
    "       END PROGRAM HELLO.\n",
    "       END PROGRAM PTRMOVE.\n",
);

#[test]
fn a_moved_procedure_pointer_is_set_and_upon_stderr_writes_standard_error() {
    let walked = Harness::source(POINTER_MOVES).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("HELLO\nSAME\n", Some(&Ending::StopRun)), "{}", walked.err);
    assert!(walked.err.contains("to stderr"), "{}", walked.err);
    let vm = Harness::source(POINTER_MOVES).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending, vm.err.contains("to stderr")), (walked.out, walked.ending, true));
    let warned: Vec<_> = diagnostics_under(POINTER_MOVES, numeric::Compliance::Extended).into_iter().filter(|d| d.2 != Some("IWX0044")).map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(11, Some("IWX0080")), (12, Some("IWX0080")), (14, Some("IWX0056"))]);
}

/// A program whose paragraph and data names are Japanese, as a UTF-8 source of GnuCOBOL's writes
/// them.
const WIDE_WORDS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. WIDE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  件数 PIC 9(3) VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "       0000-主処理.\n",
    "           PERFORM 1000-加算 3 TIMES\n",
    "           DISPLAY 'COUNT ' 件数\n",
    "           GOBACK.\n",
    "       1000-加算.\n",
    "           ADD 1 TO 件数.\n",
);

#[test]
fn under_extended_a_word_may_hold_letters_beyond_latin_1() {
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(WIDE_WORDS).flags(&["--compliance=extended"]).run(executor);
        assert_eq!((o.out.as_str(), o.ending), ("COUNT 003\n", Ok(Ending::Goback)), "{}", o.err);
    }
    let warned: Vec<_> = diagnostics_under(WIDE_WORDS, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0094")).map(|d| d.0).collect();
    assert_eq!(warned, [5, 7, 8], "one warning for each word, at its first use");
    let refused = syntax::parse(WIDE_WORDS).unwrap_err();
    assert_eq!((refused.pos.line, refused.id), (5, Some("IWS0021")), "{refused}");
}

/// Entries whose PICTURE GnuCOBOL takes from the VALUE, and one missing its period before the next
/// level number. The output is cobc 3.2's.
const VALUE_PICTURES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. VALPIC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 W-ONE PIC X(5) VALUE 'ab'\n",
    "       01 G.\n",
    "          05 N VALUE \"xyz\".\n",
    "          05 FILLER VALUE SPACES.\n",
    "          05 FILLER VALUE ALL \"ab\".\n",
    "       01 L PIC 9.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION LENGTH(G) TO L\n",
    "           DISPLAY '[' G ']' L\n",
    "           MOVE 'Q' TO N\n",
    "           DISPLAY '[' G '][' W-ONE ']'\n",
    "           GOBACK.\n",
);

#[test]
fn under_extended_a_value_gives_a_picture_and_a_missing_period_before_a_level_is_assumed() {
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(VALUE_PICTURES).flags(EXTENDED).run(executor);
        assert_eq!((o.out.as_str(), o.ending), ("[xyz ab]6\n[Q   ab][ab   ]\n", Ok(Ending::Goback)), "{}", o.err);
    }
    let extended = diagnostics_under(VALUE_PICTURES, numeric::Compliance::Extended);
    let ids = |line: u32| extended.iter().filter(|d| d.0 == line).filter_map(|d| d.2).collect::<Vec<_>>();
    assert_eq!((ids(6), ids(7), ids(8), ids(9)), (vec!["IWX0095"], vec!["IWX0096"], vec!["IWX0096"], vec!["IWX0096"]), "{extended:?}");
    let strict = diagnostics_under(VALUE_PICTURES, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| (d.0, d.2, d.3) == (6, Some("IWS0120"), Severity::Error)), "{strict:?}");
    assert!(strict.iter().any(|d| (d.0, d.2) == (7, Some("IWC0235"))), "{strict:?}");
}

/// A main program that CALLs one with no RETURNING phrase RETURNING an item, then runs past its
/// last statement.
const PAST_THE_END: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RETMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 R PIC S9(9) BINARY VALUE 5.\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'RETSUB' RETURNING R\n",
    "           DISPLAY 'R=' R.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RETSUB.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           GOBACK.\n",
    "       END PROGRAM RETSUB.\n",
    "       END PROGRAM RETMAIN.\n",
);

#[test]
fn a_gnucobol_target_gives_returning_the_return_code_and_ends_a_run_past_the_end_as_goback() {
    for flags in [EXTENDED, &["--dialect=gnucobol"][..]] {
        let walked = Harness::source(PAST_THE_END).flags(flags).run(Executor::Interpreter);
        assert!(walked.out.starts_with("R=+") && walked.out.ends_with("7\n"), "{flags:?}: {}{}", walked.out, walked.err);
        assert_eq!(walked.ending.as_ref().ok(), Some(&Ending::Goback), "{flags:?}: {}", walked.err);
        let vm = Harness::source(PAST_THE_END).flags(flags).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending), "{flags:?}");
    }
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(PAST_THE_END).run(executor);
        assert_eq!(ibm.out, "R=000000005\n", "{name}");
        assert!(ibm.ending.as_ref().is_err_and(|a| a.message.starts_with("IGZ0037S")), "{name}: {:?}", ibm.ending);
    }
}

/// A CALL ... RETURNING of a program that sets RETURN-CODE and returns an item.
const RETURNING_KEEPS_RETURN_CODE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RCMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 R PIC S9(9) BINARY VALUE 5.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 3 TO RETURN-CODE\n",
    "           CALL 'RCSUB' RETURNING R\n",
    "           DISPLAY R ' ' RETURN-CODE\n",
    "           MOVE 0 TO RETURN-CODE\n",
    "           STOP RUN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RCSUB.\n",
    "       DATA DIVISION.\n",
    "       LINKAGE SECTION.\n",
    "       01 Q PIC S9(9) BINARY.\n",
    "       PROCEDURE DIVISION RETURNING Q.\n",
    "           MOVE 7 TO RETURN-CODE\n",
    "           MOVE 9 TO Q\n",
    "           GOBACK.\n",
    "       END PROGRAM RCSUB.\n",
    "       END PROGRAM RCMAIN.\n",
);

#[test]
fn a_call_with_returning_leaves_the_return_code_as_it_was_under_ibm() {
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ran = Harness::source(RETURNING_KEEPS_RETURN_CODE).run(executor);
        assert_eq!(ran.out, "000000009 0003\n", "{name}: {}", ran.err);
    }
}

/// GnuCOBOL forms of real programs: a period right after PROGRAM-ID, INSPECT of a literal, a
/// DISPLAY that clears the screen with no AT, and ACCEPT FROM ESCAPE KEY.
const GNUCOBOL_FORMS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID.FORMS.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 N PIC 9(2) VALUE 0.\n",
    "       01 K PIC 9(4) VALUE 9999.\n",
    "       PROCEDURE DIVISION.\n",
    "           INSPECT 'HOLA HOY' TALLYING N FOR ALL 'HO'\n",
    "           DISPLAY 'N=' N\n",
    "           GOBACK.\n",
    "       SCREENS.\n",
    "           DISPLAY SPACES ERASE SCREEN\n",
    "           DISPLAY SPACE BLANK SCREEN\n",
    "           ACCEPT K FROM ESCAPE KEY.\n",
);

#[test]
fn under_extended_gnucobol_forms_of_real_programs_compile_and_run() {
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(GNUCOBOL_FORMS).flags(EXTENDED).run(executor);
        assert_eq!((o.out.as_str(), o.ending), ("N=02\n", Ok(Ending::Goback)), "{}", o.err);
    }
    let extended = diagnostics_under(GNUCOBOL_FORMS, numeric::Compliance::Extended);
    let at = |line: u32| extended.iter().filter(|d| d.0 == line).filter_map(|d| d.2).collect::<Vec<_>>();
    assert_eq!((at(2), at(8), at(14)), (vec!["IWX0100"], vec!["IWX0099"], vec!["IWX0098"]), "{extended:?}");
    assert!(extended.iter().all(|d| d.3 == Severity::Warning), "{extended:?}");
    let refused = syntax::parse(GNUCOBOL_FORMS).unwrap_err();
    assert_eq!(refused.id, Some("IWS0026"), "{refused}");
    let replacing = GNUCOBOL_FORMS.replace("TALLYING N FOR ALL 'HO'", "REPLACING ALL 'HO' BY 'XX'");
    let error = syntax::parse_all_with(&replacing, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap_err();
    assert_eq!(error.id, Some("IWS0121"), "{error}");
}

/// FUNCTION ALL INTRINSIC with GnuCOBOL's CONCAT, and DISPLAY and ACCEPT (line, column).
const CONCAT_AND_POSITIONS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CONCATP.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       CONFIGURATION SECTION.\n",
    "       REPOSITORY.\n",
    "           FUNCTION ALL INTRINSIC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 W PIC X(8).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE CONCAT('AB' 'CD') TO W\n",
    "           DISPLAY W\n",
    "           MOVE FUNCTION CONCAT('EF' 'G') TO W\n",
    "           DISPLAY W\n",
    "           GOBACK.\n",
    "       SCREENS.\n",
    "           DISPLAY (23, 40) 'TEXT'\n",
    "           ACCEPT (23, 57) W WITH UPDATE.\n",
);

#[test]
fn under_extended_concat_is_concatenate_and_a_parenthesised_position_is_at() {
    for executor in [Executor::Interpreter, Executor::Vm] {
        let o = Harness::source(CONCAT_AND_POSITIONS).flags(EXTENDED).run(executor);
        assert_eq!((o.out.as_str(), o.ending), ("ABCD    \nEFG     \n", Ok(Ending::Goback)), "{}", o.err);
    }
    let extended = diagnostics_under(CONCAT_AND_POSITIONS, numeric::Compliance::Extended);
    let at = |line: u32| extended.iter().filter(|d| d.0 == line).filter_map(|d| d.2).collect::<Vec<_>>();
    assert!(at(11).contains(&"IWX0101") && at(13).contains(&"IWX0101"), "{extended:?}");
    assert!(at(17).contains(&"IWX0102") && at(18).contains(&"IWX0102"), "{extended:?}");
    assert!(extended.iter().all(|d| d.3 == Severity::Warning), "{extended:?}");
}

/// GnuCOBOL's ASSIGN names: one no entry declares that the program uses, one after ASSIGN TO DISK
/// that the program never sets, and a constant defined after the ASSIGN.
const ASSIGN_NAMES: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ASSIGNS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT AF ASSIGN TO OutName FILE STATUS IS FS.\n",
    "           SELECT BF ASSIGN TO DISK BDD FILE STATUS IS FS.\n",
    "           SELECT CF ASSIGN DYNAMIC C-NAME FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD AF.\n",
    "       01 AREC PIC X(5).\n",
    "       FD BF.\n",
    "       01 BREC PIC X(5).\n",
    "       FD CF.\n",
    "       01 CREC PIC X(5).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 FS PIC XX.\n",
    "       78 C-NAME VALUE 'CDD'.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY '[' OutName(1:8) ']'\n",
    "           MOVE 'ADD' TO OutName\n",
    "           OPEN OUTPUT AF BF CF\n",
    "           DISPLAY 'FS=' FS\n",
    "           MOVE 'one' TO AREC  WRITE AREC\n",
    "           MOVE 'two' TO BREC  WRITE BREC\n",
    "           MOVE 'three' TO CREC  WRITE CREC\n",
    "           CLOSE AF BF CF\n",
    "           STOP RUN.\n",
);

#[test]
fn under_extended_assign_names_are_declared_or_take_a_later_constant() {
    let dir = temp("assign-names");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let files = ["ADD", "BDD", "CDD"].map(|dd| dir.join(format!("{name}-{dd}")));
        let dds: Vec<String> = ["ADD", "BDD", "CDD"].iter().zip(&files).map(|(dd, f)| format!("{dd}={}", f.display())).collect();
        let ran = Harness::source(ASSIGN_NAMES).flags(EXTENDED).dds(&dds).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), ("[OutName ]\nFS=00\n", Some(&Ending::StopRun)), "{name}: {}", ran.err);
        let page = numeric::Options::default().code_page();
        let written: Vec<String> = files.iter().map(|f| page.decode(&std::fs::read(f).unwrap()).to_string()).collect();
        assert_eq!(written, ["one  ", "two  ", "three"], "{name}");
    }
    let warned: Vec<_> = diagnostics_under(ASSIGN_NAMES, numeric::Compliance::Extended).into_iter().filter(|d| d.2.is_some_and(|id| id >= "IWX0103")).map(|d| (d.0, d.2)).collect();
    assert_eq!(warned, [(6, Some("IWX0103")), (7, Some("IWX0103")), (8, Some("IWX0104"))]);
    let plain = ASSIGN_NAMES.replace("           SELECT CF ASSIGN DYNAMIC C-NAME FILE STATUS IS FS.\n", "           SELECT CF ASSIGN TO CDD FILE STATUS IS FS.\n").replace("       78 C-NAME VALUE 'CDD'.\n", "");
    let strict = diagnostics_under(&plain, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| (d.0, d.2) == (21, Some("IWC0001"))), "{strict:?}");
}

/// An indexed file whose RECORD KEY and ALTERNATE RECORD KEY names WORKING-STORAGE declares too.
const KEYS_IN_RECORDS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. KEYS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT KF ASSIGN TO KFDD\n",
    "               ORGANIZATION INDEXED ACCESS DYNAMIC\n",
    "               RECORD KEY KF-KEY\n",
    "               ALTERNATE RECORD KEY KF-ALT WITH DUPLICATES\n",
    "               FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD KF.\n",
    "       01 KF-REC.\n",
    "          05 KF-KEY PIC X(3).\n",
    "          05 KF-ALT PIC X(2).\n",
    "          05 KF-DATA PIC X(5).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 FS PIC XX.\n",
    "       01 WS-REC.\n",
    "          05 KF-KEY PIC X(3).\n",
    "          05 KF-ALT PIC X(2).\n",
    "          05 KF-DATA PIC X(5).\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN OUTPUT KF\n",
    "           MOVE 'BBBzzTWO' TO KF-REC\n",
    "           WRITE KF-REC\n",
    "           MOVE 'AAAyyONE' TO KF-REC\n",
    "           WRITE KF-REC\n",
    "           CLOSE KF\n",
    "           OPEN INPUT KF\n",
    "           MOVE 'AAA' TO KF-KEY OF WS-REC\n",
    "           MOVE 'BBB' TO KF-KEY OF KF-REC\n",
    "           READ KF KEY IS KF-KEY OF KF-REC\n",
    "           DISPLAY FS ' ' KF-REC\n",
    "           MOVE 'yy' TO KF-ALT OF KF-REC\n",
    "           READ KF KEY IS KF-ALT OF KF-REC\n",
    "           DISPLAY FS ' ' KF-REC\n",
    "           CLOSE KF\n",
    "           STOP RUN.\n",
);

#[test]
fn under_extended_a_key_named_outside_the_records_too_is_the_records_item() {
    // cobc 3.2's output, under -std=default, mf and ibm alike.
    let expected = "00 BBBzzTWO  \n00 AAAyyONE  \n";
    let dir = temp("keys-in-records");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, executor) in [("interpreter", Executor::Interpreter), ("vm", Executor::Vm)] {
        let file = dir.join(format!("{name}.dat"));
        let _ = std::fs::remove_file(&file);
        let ran = Harness::source(KEYS_IN_RECORDS).flags(EXTENDED).dds(&[format!("KFDD={}", file.display())]).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{name}: {}", ran.err);
    }
    let warned: Vec<_> = diagnostics_under(KEYS_IN_RECORDS, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0105")).map(|d| d.0).collect();
    assert_eq!(warned, [8, 9]);
    let strict = diagnostics_under(KEYS_IN_RECORDS, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| (d.0, d.2) == (8, Some("IWC0002"))), "{strict:?}");
}

/// GnuCOBOL's ACCEPT OMITTED, plain and with ON EXCEPTION.
const ACCEPT_OMITTED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. OMIT.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY 'PRESS A KEY' AT 0101\n",
    "           ACCEPT OMITTED\n",
    "           DISPLAY 'DONE' AT 0201\n",
    "           ACCEPT OMITTED\n",
    "               ON EXCEPTION DISPLAY 'PF' AT 0301\n",
    "           END-ACCEPT\n",
    "           GOBACK.\n",
);

#[test]
fn under_extended_accept_omitted_waits_for_a_key_alike_on_both_executors() {
    let script = "string IGNORED\nENTER\nPF3\n";
    let walked = Harness::source(ACCEPT_OMITTED).flags(EXTENDED).screens(script).run(Executor::Interpreter);
    let expected = "--- screen 1 ---\nPRESS A KEY\n--- screen 2 ---\nPRESS A KEY\nDONE\n--- screen 3 ---\nPRESS A KEY\nDONE\nPF\n";
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(ACCEPT_OMITTED).flags(EXTENDED).screens(script).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let warned: Vec<_> = diagnostics_under(ACCEPT_OMITTED, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0106")).map(|d| d.0).collect();
    assert_eq!(warned, [5, 7]);
    let strict = diagnostics_under(ACCEPT_OMITTED, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| (d.0, d.2) == (5, Some("IWC0001"))), "{strict:?}");
}

/// A SORT keyed on a WORKING-STORAGE item, which marks bytes 3 and 4 of its own record.
const SORT_KEY_ELSEWHERE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. KEYED.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT SF ASSIGN TO SORTWK.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       SD SF.\n",
    "       01 SREC PIC X(6).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 WREC.\n",
    "          05 W-A PIC XX.\n",
    "          05 W-K PIC XX.\n",
    "          05 W-B PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           SORT SF ON ASCENDING KEY W-K\n",
    "               INPUT PROCEDURE FEED\n",
    "               OUTPUT PROCEDURE DRAIN THRU DRAIN-X\n",
    "           STOP RUN.\n",
    "       FEED.\n",
    "           MOVE 'a1z9c3' TO SREC  RELEASE SREC\n",
    "           MOVE 'b2y8d4' TO SREC  RELEASE SREC\n",
    "           MOVE 'c3x7e5' TO SREC  RELEASE SREC.\n",
    "       DRAIN.\n",
    "           RETURN SF AT END GO TO DRAIN-X.\n",
    "           DISPLAY SREC\n",
    "           GO TO DRAIN.\n",
    "       DRAIN-X.\n",
    "           EXIT.\n",
);

#[test]
fn under_extended_a_sort_key_outside_the_records_keys_its_own_bytes() {
    // cobc 3.2's output, under -std=default, mf and ibm alike.
    for executor in [Executor::Interpreter, Executor::Vm] {
        let ran = Harness::source(SORT_KEY_ELSEWHERE).flags(EXTENDED).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), ("c3x7e5\nb2y8d4\na1z9c3\n", Some(&Ending::StopRun)), "{}", ran.err);
    }
    let warned: Vec<_> = diagnostics_under(SORT_KEY_ELSEWHERE, numeric::Compliance::Extended).into_iter().filter(|d| d.2 == Some("IWX0107")).map(|d| d.0).collect();
    assert_eq!(warned, [17]);
    let strict = diagnostics_under(SORT_KEY_ELSEWHERE, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| (d.0, d.2) == (17, Some("IWC0211"))), "{strict:?}");
    let past = SORT_KEY_ELSEWHERE.replace("       01 SREC PIC X(6).\n", "       01 SREC PIC X(3).\n");
    let refused = diagnostics_under(&past, numeric::Compliance::Extended);
    assert!(refused.iter().any(|d| (d.0, d.2) == (17, Some("IWC0211"))), "{refused:?}");
}
