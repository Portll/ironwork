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
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("HELLO|AB\n000000000K|00002|00005\n", Some(&Ending::Goback)), "{}", walked.err);
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
    assert_eq!(compile_errors(&fixed), "level 78 is not a data level");
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
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("0001\n0099 0002\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(RETURNING_OMITTED).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let parsed = syntax::parse_all_with(RETURNING_OMITTED, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap();
    let compiled = compile(parsed[1].clone(), &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let shown: Vec<(u32, u32, Option<&str>, Severity)> = compiled.diagnostics.iter().map(|m| (m.pos.line, m.pos.col, m.id, m.severity)).collect();
    assert_eq!(shown, [(18, 27, Some("IWX0009"), Severity::Warning)]);
    let strict = compile(syntax::parse_all_with(RETURNING_OMITTED, &syntax::copy::Libraries::default()).unwrap().remove(1), &[]).err().unwrap();
    assert_eq!(strict.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(), ["PROCEDURE DIVISION RETURNING OMITTED: not an 01 or 77 item of the LINKAGE SECTION"]);
}
