//! GnuCOBOL's ANY NUMERIC parameter under `--compliance extended`: the callee reads the class,
//! digits, scale, sign and usage of the argument each CALL passes.

use super::*;

const EXTENDED: &[&str] = &["--compliance=extended"];

const ANY_NUMERIC_CALLS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ANYNMAIN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  Z4    PIC 9(4) VALUE 1234.\n",
    "       01  SZ    PIC S9(3)V99 VALUE -12.34.\n",
    "       01  PK    PIC S9(5)V9 COMP-3 VALUE 123.4.\n",
    "       01  BN    PIC S9(4) COMP VALUE -77.\n",
    "       01  LSEP  PIC S9(3) SIGN LEADING SEPARATE VALUE 5.\n",
    "       01  AL    PIC X(4) VALUE '4321'.\n",
    "       PROCEDURE DIVISION.\n",
    "           CALL 'ANYNSUB' USING Z4\n",
    "           CALL 'ANYNSUB' USING SZ\n",
    "           CALL 'ANYNSUB' USING PK\n",
    "           CALL 'ANYNSUB' USING BN\n",
    "           CALL 'ANYNSUB' USING LSEP\n",
    "           CALL 'ANYNSUB' USING AL\n",
    "           CALL 'ANYNSUB' USING BY CONTENT 42\n",
    "           CALL 'ANYNSUB' USING BY CONTENT -42\n",
    "           CALL 'ANYNSUB' USING BY CONTENT 1.25\n",
    "           CALL 'ANYNSUB' USING BY CONTENT 3000000000\n",
    "           CALL 'ANYNSUB' USING OMITTED\n",
    "           STOP RUN.\n",
    "       END PROGRAM ANYNMAIN.\n",
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ANYNSUB.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  R   PIC S9(9)V99.\n",
    "       01  N   PIC 9(3).\n",
    "       01  W   PIC S9(5) COMP VALUE 100.\n",
    "       LINKAGE SECTION.\n",
    "       01  V   ANY NUMERIC.\n",
    "       PROCEDURE DIVISION USING OPTIONAL V.\n",
    "           IF V OMITTED\n",
    "               DISPLAY 'OMITTED'\n",
    "               GOBACK\n",
    "           END-IF\n",
    "           IF V NOT NUMERIC\n",
    "               DISPLAY 'NOT NUMERIC'\n",
    "           ELSE\n",
    "               DISPLAY 'NUMERIC [' V ']'\n",
    "           END-IF\n",
    "           MOVE V TO R\n",
    "           MOVE V TO N\n",
    "           DISPLAY 'R ' R ' N ' N\n",
    "           IF V < 0 DISPLAY 'NEGATIVE' END-IF\n",
    "           IF V < W DISPLAY 'BELOW W' ELSE DISPLAY 'NOT BELOW W' END-IF\n",
    "           COMPUTE R = V * 2 + 1\n",
    "           DISPLAY 'COMPUTED ' R\n",
    "           ADD V 1 TO R\n",
    "           DISPLAY 'ADDED ' R\n",
    "           EVALUATE TRUE\n",
    "               WHEN V POSITIVE DISPLAY 'POSITIVE'\n",
    "               WHEN V ZERO DISPLAY 'ZERO'\n",
    "               WHEN OTHER DISPLAY 'OTHER'\n",
    "           END-EVALUATE\n",
    "           GOBACK.\n",
);

/// A program whose ANYNSUB reads its parameter in the way the test names, with the caller passing a
/// zoned item.
fn callee_with(statements: &str) -> String {
    let callee = &ANY_NUMERIC_CALLS[ANY_NUMERIC_CALLS.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. ANYNSUB").unwrap()..];
    callee.replace("           GOBACK.\n", &format!("{statements}           GOBACK.\n"))
}

fn diagnostics(source: &str) -> Vec<(u32, u32, Option<&'static str>, Severity)> {
    let flags: Vec<String> = EXTENDED.iter().map(|f| f.to_string()).collect();
    let parsed = syntax::parse_with(source, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap_or_else(|e| panic!("{e}"));
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
fn any_numeric_reads_each_arguments_field_alike_on_both_executors() {
    // cobc 3.2's output.
    let expected = "NUMERIC [1234]\nR +000001234.00 N 234\nNOT BELOW W\nCOMPUTED +000002469.00\nADDED +000003704.00\nPOSITIVE\nNUMERIC [-012.34]\nR -000000012.34 N 012\nNEGATIVE\nBELOW W\nCOMPUTED -000000023.68\nADDED -000000035.02\nOTHER\nNUMERIC [+00123.4]\nR +000000123.40 N 123\nNOT BELOW W\nCOMPUTED +000000247.80\nADDED +000000372.20\nPOSITIVE\nNUMERIC [-0077]\nR -000000077.00 N 077\nNEGATIVE\nBELOW W\nCOMPUTED -000000153.00\nADDED -000000229.00\nOTHER\nNUMERIC [+005]\nR +000000005.00 N 005\nBELOW W\nCOMPUTED +000000011.00\nADDED +000000017.00\nPOSITIVE\nNUMERIC [4321]\nR +000004321.00 N 321\nNOT BELOW W\nCOMPUTED +000008643.00\nADDED +000012965.00\nPOSITIVE\nNUMERIC [+000000042]\nR +000000042.00 N 042\nBELOW W\nCOMPUTED +000000085.00\nADDED +000000128.00\nPOSITIVE\nNUMERIC [-000000042]\nR -000000042.00 N 042\nNEGATIVE\nBELOW W\nCOMPUTED -000000083.00\nADDED -000000124.00\nOTHER\nNUMERIC [00000000000000000125]\nR +000000125.00 N 125\nNOT BELOW W\nCOMPUTED +000000251.00\nADDED +000000377.00\nPOSITIVE\nNUMERIC [00000000003000000000]\nR +000000000.00 N 000\nNOT BELOW W\nCOMPUTED +000000001.00\nADDED +000000002.00\nPOSITIVE\nOMITTED\n";
    let walked = Harness::source(ANY_NUMERIC_CALLS).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(ANY_NUMERIC_CALLS).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn any_numeric_is_refused_under_strict_and_in_the_uses_ironwork_does_not_read() {
    let called = callee_with("");
    assert_eq!(diagnostics(&called).first().map(|d| (d.0, d.1, d.2)), Some((9, 8, Some("IWX0120"))));
    let refused = syntax::parse(&called).unwrap_err();
    assert_eq!((refused.pos.line, refused.pos.col, refused.id), (9, 16, Some("IWC0350")), "{refused}");
    for (use_, why) in [
        ("           MOVE 9 TO V\n", "a MOVE to it"),
        ("           ADD 1 TO V\n", "an ADD to it"),
        ("           CALL 'OTHER' USING V\n", "a CALL passing it on"),
        ("           MOVE FUNCTION LENGTH(V) TO N\n", "FUNCTION LENGTH of it"),
        ("           MOVE V TO N\n           ADD V TO R\n", "ADD of it alone to an item"),
        ("           SUBTRACT V FROM R\n", "SUBTRACT of it alone from an item"),
    ] {
        let source = callee_with(use_);
        let found = diagnostics(&source);
        assert!(found.iter().any(|d| d.2 == Some("IWR0095")), "{why}: {found:?}");
    }
    let returned = called.replace("USING OPTIONAL V.", "USING V RETURNING V.");
    assert!(diagnostics(&returned).iter().any(|d| d.2 == Some("IWR0095")));
}

#[test]
fn any_numeric_reads_an_argument_that_is_not_numeric_only_as_ironwork_can() {
    let source = ANY_NUMERIC_CALLS.replace("CALL 'ANYNSUB' USING Z4\n", "CALL 'ANYNSUB' USING ED\n").replace("       01  Z4    PIC 9(4) VALUE 1234.\n", "       01  ED    PIC ZZ9.99 VALUE 5.5.\n");
    let walked = Harness::source(&source).flags(EXTENDED).run(Executor::Interpreter);
    let vm = Harness::source(&source).flags(EXTENDED).run(Executor::Vm);
    assert!(walked.out.starts_with("NOT NUMERIC\n"), "{}", walked.out);
    assert_eq!((walked.out.as_str(), format!("{:?}", walked.ending)), (vm.out.as_str(), format!("{:?}", vm.ending)));
    assert!(format!("{:?}", walked.ending).contains("IWR0096"), "{:?}", walked.ending);
}
