use super::*;

/// Every form of the Communication feature: a CD with clauses, a CD with a record, a debugging
/// procedure for a CD, and the six statements.
const COMMUNICATION: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. COMMS.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       CONFIGURATION SECTION.\n",
    "       SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  MSG PIC X(20).\n",
    "       COMMUNICATION SECTION.\n",
    "       CD  IN-CD FOR INPUT\n",
    "           STATUS KEY IS IN-STATUS\n",
    "           COUNT IN-COUNT.\n",
    "       CD  OUT-CD FOR OUTPUT.\n",
    "       01  OUT-AREA.\n",
    "           05  OUT-COUNT PIC 9(4).\n",
    "           05  FILLER PIC X(19).\n",
    "       PROCEDURE DIVISION.\n",
    "       DECLARATIVES.\n",
    "       WATCH SECTION.\n",
    "           USE FOR DEBUGGING ON IN-CD.\n",
    "       END DECLARATIVES.\n",
    "       MAIN SECTION.\n",
    "       BEGIN.\n",
    "           ENABLE INPUT IN-CD KEY \"PASS\".\n",
    "           ACCEPT IN-CD COUNT.\n",
    "           RECEIVE IN-CD MESSAGE INTO MSG\n",
    "               NO DATA DISPLAY \"NONE\".\n",
    "           IF IN-STATUS = \"00\" DISPLAY IN-COUNT.\n",
    "           MOVE 1 TO OUT-COUNT.\n",
    "           SEND OUT-CD FROM MSG WITH EMI.\n",
    "           PURGE OUT-CD.\n",
    "           DISABLE INPUT IN-CD WITH KEY \"PASS\".\n",
    "           GOBACK.\n",
);

fn diagnostics(source: &str, compliance: numeric::Compliance) -> Vec<(u32, u32, Option<&'static str>, Severity)> {
    let flags: Vec<String> = if compliance == numeric::Compliance::Extended { vec!["--compliance=extended".into()] } else { Vec::new() };
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
fn each_communication_item_is_refused_where_it_stands_as_not_enterprise_cobol() {
    let refused = |line, col| (line, col, Some("IWS0102"), Severity::Severe);
    let expected = [refused(9, 8), refused(10, 8), refused(13, 8), refused(20, 33), refused(24, 12), refused(25, 12), refused(26, 12), refused(30, 12), refused(31, 12), refused(32, 12)];
    assert_eq!(diagnostics(COMMUNICATION, numeric::Compliance::Strict), expected);
    assert_eq!(diagnostics(COMMUNICATION, numeric::Compliance::Extended), expected);
    let parsed = syntax::parse_with(COMMUNICATION, &syntax::copy::Libraries::default()).unwrap();
    let errors = compile(parsed, &[]).err().unwrap();
    assert_eq!(errors[0].to_string(), "9:8: IWS0102-S the Communication feature (COMMUNICATION SECTION) is not part of Enterprise COBOL, which does not compile it");
}

#[test]
fn the_features_verbs_are_not_reserved_in_a_program_without_a_cd() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. WORDS.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  SEND PIC X(5) VALUE \"MAIL\".\n",
        "       01  PURGE PIC 9 VALUE 3.\n",
        "       PROCEDURE DIVISION.\n",
        "           DISPLAY SEND PURGE\n",
        "           MOVE \"POST\" TO SEND\n",
        "           DISPLAY SEND.\n",
        "           GOBACK.\n",
    );
    for executor in [Executor::Interpreter, Executor::Vm] {
        let ran = Harness::source(source).run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.is_ok()), ("MAIL 3\nPOST \n", true), "{}", ran.err);
    }
}
