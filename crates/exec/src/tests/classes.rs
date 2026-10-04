use super::*;

fn program(classes: &str, data: &str, procedure: &str) -> String {
    [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLS.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n",
        classes,
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        data,
        "       PROCEDURE DIVISION.\n",
        procedure,
    ]
    .concat()
}

const CLASSES: &str = concat!(
    "           CLASS HEX-DIGIT IS '0' THRU '9' 'A' THROUGH 'F'\n",
    "           CLASS VOWEL 'AEIOU'\n",
    "           CLASS BY-ORDINAL IS 198 THRU 194 X'40'.\n",
);

/// Tests each value against each class, writing a 1 or a 0 per class, and a contained program
/// testing through its container's class-names.
const PROCEDURE: &str = concat!(
    "           MOVE 'C0FE' TO X PERFORM SHOW\n",
    "           MOVE 'AEIO' TO X PERFORM SHOW\n",
    "           MOVE 'AB E' TO X PERFORM SHOW\n",
    "           MOVE 'abcd' TO X PERFORM SHOW\n",
    "           MOVE 1234 TO Z\n",
    "           IF Z IS HEX-DIGIT AND Z NOT VOWEL DISPLAY 'Z' END-IF\n",
    "           MOVE 'BAD ' TO X\n",
    "           CALL 'INNER'\n",
    "           GOBACK.\n",
    "       SHOW.\n",
    "           MOVE '000' TO S\n",
    "           IF X HEX-DIGIT MOVE '1' TO S(1:1) END-IF\n",
    "           IF X IS VOWEL MOVE '1' TO S(2:1) END-IF\n",
    "           IF X IS NOT BY-ORDINAL CONTINUE\n",
    "           ELSE MOVE '1' TO S(3:1) END-IF\n",
    "           DISPLAY X ' ' S.\n",
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n",
    "           IF X BY-ORDINAL DISPLAY 'INNER' END-IF\n",
    "           GOBACK.\n",
    "       END PROGRAM INNER.\n       END PROGRAM CLS.\n",
);

const DATA: &str = "       01  X PIC X(4) GLOBAL.\n       01  S PIC X(3).\n       01  Z PIC 9(4).\n";

#[test]
fn a_class_name_holds_its_characters_ranges_and_ordinals_on_both_executors() {
    let source = program(CLASSES, DATA, PROCEDURE);
    let walked = Harness::source(&source).run(Executor::Interpreter);
    assert_eq!(
        (walked.out.as_str(), walked.ending.as_ref().ok()),
        ("C0FE 100\nAEIO 010\nAB E 001\nabcd 000\nZ\nINNER\n", Some(&Ending::Goback)),
        "{}",
        walked.err
    );
    let vm = Harness::source(&source).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn a_class_clause_takes_characters_or_ordinals_and_tests_a_display_item() {
    let errors = compile_errors(&program(
        "           CLASS TWO 'AB' THRU 'Z'\n           CLASS NONE IS 0 257\n           CLASS MIXED 'A' THRU 197.\n",
        "       01  P PIC S9(4) COMP-3.\n",
        "           IF P TWO DISPLAY 'P' END-IF\n           GOBACK.\n",
    ));
    for expected in [
        "CLASS TWO: an alphanumeric literal of a THROUGH phrase is one character",
        "CLASS NONE: 0 is not an ordinal number from 1 to 256",
        "CLASS NONE: 257 is not an ordinal number from 1 to 256",
        "CLASS MIXED: the literals of a THROUGH phrase are both numeric or both alphanumeric",
        "class-name TWO tests a data item of USAGE DISPLAY, and P is not one",
    ] {
        assert!(errors.contains(expected), "{expected}\n{errors}");
    }
}
