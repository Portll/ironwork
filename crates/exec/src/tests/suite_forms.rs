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
