use super::*;

/// A class definition: its REPOSITORY entries, then FACTORY and OBJECT paragraphs as written.
fn class(head: &str, repository: &[&str], parts: &str) -> String {
    let entries: Vec<String> = repository.iter().map(|e| format!("           CLASS {e}")).collect();
    format!(
        "       IDENTIFICATION DIVISION.\n       CLASS-ID. {head}.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n{}.\n{parts}       END CLASS {}.\n",
        entries.join("\n"),
        head.split_whitespace().next().unwrap()
    )
}

fn part(kind: &str, data: &str, methods: &[String]) -> String {
    let data = if data.is_empty() { String::new() } else { format!("       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}") };
    format!("       IDENTIFICATION DIVISION.\n       {kind}.\n{data}       PROCEDURE DIVISION.\n{}       END {kind}.\n", methods.concat())
}

/// One method: its name, DATA DIVISION sections, PROCEDURE DIVISION header phrase and body.
fn method(name: &str, data: &str, header: &str, body: &[&str]) -> String {
    let data = if data.is_empty() { String::new() } else { format!("       DATA DIVISION.\n{data}") };
    let body: String = body.iter().map(|l| line(l)).collect();
    format!("       IDENTIFICATION DIVISION.\n       METHOD-ID. \"{name}\".\n{data}       PROCEDURE DIVISION{header}.\n{body}       END METHOD \"{name}\".\n")
}

fn client(repository: &[&str], data: &str, body: &[&str]) -> String {
    let entries: Vec<String> = repository.iter().map(|e| format!("           CLASS {e}")).collect();
    let body: String = body.iter().map(|l| line(l)).collect();
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLIENT RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n{}.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{body}",
        entries.join("\n")
    )
}

/// Runs `main` with `classes` among the programs of its run unit.
fn run_oo(main: &str, classes: &[String]) -> (String, String, Result<(Ending, i16), Abend>) {
    let first = syntax::parse(main).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(first, &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let programs = classes.iter().map(|c| syntax::parse(c).unwrap_or_else(|e| panic!("{e}\n{c}"))).collect();
    let library = unit::Library { programs, ..Default::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ending = compiled.execute(library, files::Dds::default(), None, unit::Clock::Fixed(0, 0), &mut out, &mut err);
    (String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap(), ending)
}

fn errors(source: &str) -> String {
    let parsed = syntax::parse(source).unwrap_or_else(|e| panic!("{e}"));
    compile(parsed, &[]).err().map(|e| e.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

fn account() -> String {
    class(
        "Account INHERITS Base",
        &["Base IS \"java.lang.Object\"", "Account IS \"Account\""],
        &[
            part(
                "FACTORY",
                "       01  OPENED PIC S9(9) BINARY VALUE 0.\n",
                &[
                    method(
                        "open",
                        "       LINKAGE SECTION.\n       01  OPENING USAGE OBJECT REFERENCE Account.\n",
                        " RETURNING OPENING",
                        &["INVOKE Account NEW RETURNING OPENING", "ADD 1 TO OPENED."],
                    ),
                    method("opened", "       LINKAGE SECTION.\n       01  N PIC S9(9) BINARY.\n", " RETURNING N", &["MOVE OPENED TO N."]),
                ],
            ),
            part(
                "OBJECT",
                "       01  BALANCE PIC S9(9) BINARY VALUE 100.\n       01  LINKED USAGE OBJECT REFERENCE Account.\n",
                &[
                    method("credit", "       LINKAGE SECTION.\n       01  AMOUNT PIC S9(9) BINARY.\n", " USING BY VALUE AMOUNT", &["ADD AMOUNT TO BALANCE."]),
                    method(
                        "credit",
                        "       LINKAGE SECTION.\n       01  SMALL PIC S9(4) COMP-5.\n",
                        " USING BY VALUE SMALL",
                        &["DISPLAY 'SHORT CREDIT'", "ADD SMALL TO BALANCE."],
                    ),
                    method("getBalance", "       LINKAGE SECTION.\n       01  RESULT PIC S9(9) BINARY.\n", " RETURNING RESULT", &["MOVE BALANCE TO RESULT."]),
                ],
            ),
        ]
        .concat(),
    )
}

const ACCOUNT_DATA: &str = "       01  A1 USAGE OBJECT REFERENCE Account.\n       01  A2 USAGE OBJECT REFERENCE Account.\n       01  U USAGE OBJECT REFERENCE.\n       01  AMOUNT PIC S9(9) BINARY.\n       01  SMALL PIC S9(4) BINARY VALUE 3.\n       01  BAL PIC S9(9) BINARY.\n       01  SHOWN PIC ZZZ9.\n       01  MNAME PIC X(20).\n";

#[test]
fn instances_keep_their_own_data_and_factory_data_is_shared() {
    let main = client(
        &["Account IS \"Account\""],
        ACCOUNT_DATA,
        &[
            "INVOKE Account \"open\" RETURNING A1",
            "INVOKE Account \"open\" RETURNING A2",
            "MOVE 25 TO AMOUNT",
            "INVOKE A1 \"credit\" USING BY VALUE AMOUNT",
            "INVOKE A2 \"credit\" USING BY VALUE 7",
            "INVOKE A2 \"credit\" USING BY VALUE SMALL",
            "INVOKE A1 \"getBalance\" RETURNING BAL",
            "MOVE BAL TO SHOWN DISPLAY 'A1 ' SHOWN",
            "SET U TO A2",
            "MOVE 'getBalance' TO MNAME",
            "INVOKE U MNAME RETURNING BAL",
            "MOVE BAL TO SHOWN DISPLAY 'A2 ' SHOWN",
            "INVOKE Account NEW RETURNING A1",
            "INVOKE Account \"opened\" RETURNING BAL",
            "MOVE BAL TO SHOWN DISPLAY 'OPENED ' SHOWN",
            "GOBACK.",
        ],
    );
    let (out, err, ending) = run_oo(&main, &[account()]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "SHORT CREDIT\nA1  125\nA2  110\nOPENED    2\n");
}

fn animals() -> Vec<String> {
    let animal = class(
        "Animal INHERITS Base",
        &["Base IS \"java.lang.Object\""],
        &part(
            "OBJECT",
            "       01  NAME PIC X(6) VALUE 'ANIMAL'.\n",
            &[
                method("speak", "", "", &["DISPLAY 'SOUND OF ' NAME."]),
                method("describe", "", "", &["DISPLAY 'I AM ' NAME", "INVOKE SELF \"speak\"."]),
            ],
        ),
    );
    let dog = class(
        "Dog INHERITS Animal",
        &["Animal"],
        &part(
            "OBJECT",
            "       01  NAME PIC X(6) VALUE 'DOG'.\n       01  BARKS PIC 9 VALUE 0.\n",
            &[method(
                "speak",
                "       WORKING-STORAGE SECTION.\n       01  NAME PIC X(6) VALUE 'REX'.\n",
                "",
                &["ADD 1 TO BARKS", "DISPLAY 'WOOF ' BARKS ' FROM ' NAME", "INVOKE SUPER \"speak\"."],
            )],
        ),
    );
    vec![animal, dog]
}

#[test]
fn inheritance_overrides_dispatch_on_the_object_and_super_reaches_the_parent() {
    let main = client(
        &["Dog", "Animal"],
        "       01  D USAGE OBJECT REFERENCE Dog.\n       01  A USAGE OBJECT REFERENCE Animal.\n",
        &["INVOKE Dog NEW RETURNING D", "INVOKE D \"describe\"", "INVOKE D \"speak\"", "INVOKE Animal NEW RETURNING A", "INVOKE A \"describe\"", "GOBACK."],
    );
    let (out, err, ending) = run_oo(&main, &animals());
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "I AM ANIMAL\nWOOF 1 FROM REX   \nSOUND OF ANIMAL\nWOOF 2 FROM REX   \nSOUND OF ANIMAL\nI AM ANIMAL\nSOUND OF ANIMAL\n");
}

#[test]
fn set_null_comparisons_and_the_exception_phrases() {
    let data = [ACCOUNT_DATA, "       01  B USAGE OBJECT REFERENCE Account.\n"].concat();
    let main = client(
        &["Account IS \"Account\""],
        &data,
        &[
            "SET A1 TO NULL",
            "IF A1 = NULL DISPLAY 'NULL' END-IF",
            "INVOKE Account NEW RETURNING A1",
            "SET B TO A1",
            "IF A1 = B AND B NOT = NULL DISPLAY 'SAME' END-IF",
            "INVOKE Account NEW RETURNING B",
            "IF A1 NOT = B DISPLAY 'DIFFERENT' END-IF",
            "INVOKE A1 \"nosuch\"",
            "    ON EXCEPTION DISPLAY 'NO METHOD' END-INVOKE",
            "INVOKE A1 \"credit\" USING BY VALUE 1",
            "    ON EXCEPTION DISPLAY 'NEVER'",
            "    NOT ON EXCEPTION DISPLAY 'CREDITED'",
            "END-INVOKE",
            "INVOKE A1 \"getBalance\" RETURNING SMALL",
            "    ON EXCEPTION DISPLAY 'WRONG TYPE' END-INVOKE",
            "SET A1 TO NULL",
            "INVOKE A1 \"credit\" USING BY VALUE 1",
            "DISPLAY 'NOT REACHED'",
            "GOBACK.",
        ],
    );
    let (out, _, ending) = run_oo(&main, &[account()]);
    assert_eq!(out, "NULL\nSAME\nDIFFERENT\nNO METHOD\nCREDITED\nWRONG TYPE\n");
    assert!(ending.unwrap_err().message.contains("NULL"));
    let missing = client(&["Account IS \"Account\""], ACCOUNT_DATA, &["INVOKE Account NEW RETURNING A1", "INVOKE A1 \"close\"", "GOBACK."]);
    let abend = run_oo(&missing, &[account()]).2.unwrap_err();
    assert_eq!(abend.code, "U4038");
    assert!(abend.message.contains("\"close\""), "{}", abend.message);
}

#[test]
fn a_java_class_is_checked_and_reaching_it_ends_the_run_naming_class_and_method() {
    let data = [
        ACCOUNT_DATA,
        "       01  S USAGE OBJECT REFERENCE JString.\n       01  O USAGE OBJECT REFERENCE JObject.\n",
        "       01  FLAG PIC X.\n           88 FLAG-FALSE VALUE X'00'.\n           88 FLAG-TRUE VALUE X'01' THRU X'FF'.\n",
    ]
    .concat();
    let repository = ["Account IS \"Account\"", "JString IS \"java.lang.String\"", "JObject IS \"java.lang.Object\""];
    let main = client(
        &repository,
        &data,
        &[
            "INVOKE Account NEW RETURNING A1",
            "SET O TO A1",
            "INVOKE A1 \"equals\" USING BY VALUE O RETURNING FLAG",
            "IF FLAG-TRUE DISPLAY 'EQUAL' END-IF",
            "INVOKE A1 \"toString\" RETURNING S",
            "GOBACK.",
        ],
    );
    let (out, _, ending) = run_oo(&main, &[account()]);
    assert_eq!(out, "EQUAL\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "JAVA");
    assert!(abend.message.contains("\"toString\"") && abend.message.contains("java.lang.Object"), "{}", abend.message);
    let string = client(&repository, &data, &["INVOKE JString NEW USING BY VALUE 'X' RETURNING S", "GOBACK."]);
    let abend = run_oo(&string, &[]).2.unwrap_err();
    assert_eq!(abend.code, "JAVA");
    assert!(abend.message.contains("NEW") && abend.message.contains("java.lang.String"), "{}", abend.message);
    let checked = client(&repository, &data, &["INVOKE S \"length\" RETURNING BAL", "GOBACK."]);
    assert_eq!(errors(&checked), "");
    let exception = class("MyError INHERITS JavaException", &["JavaException IS \"java.lang.Exception\""], "");
    let thrower = client(&["MyError"], "       01  E USAGE OBJECT REFERENCE MyError.\n", &["INVOKE MyError NEW RETURNING E", "GOBACK."]);
    let abend = run_oo(&thrower, &[exception]).2.unwrap_err();
    assert!(abend.code == "JAVA" && abend.message.contains("java.lang.Exception"), "{abend:?}");
}

#[test]
fn method_storage_persists_local_storage_does_not_and_invoke_keeps_return_code() {
    let counter = class(
        "Counter INHERITS Base",
        &["Base IS \"java.lang.Object\""],
        &part(
            "OBJECT",
            "",
            &[
                method(
                    "tick",
                    "       WORKING-STORAGE SECTION.\n       01  CALLS PIC 9(4) VALUE 0.\n       LOCAL-STORAGE SECTION.\n       01  FRESH PIC 9(4) VALUE 5.\n",
                    "",
                    &["ADD 1 TO CALLS FRESH", "MOVE 9 TO RETURN-CODE", "DISPLAY CALLS ' ' FRESH", "EXIT METHOD", "DISPLAY 'NEVER'."],
                ),
                method("halt", "", "", &["STOP RUN."]),
            ],
        ),
    );
    let main = client(
        &["Counter"],
        "       01  C1 USAGE OBJECT REFERENCE Counter.\n       01  C2 USAGE OBJECT REFERENCE Counter.\n",
        &["MOVE 3 TO RETURN-CODE", "INVOKE Counter NEW RETURNING C1", "INVOKE Counter NEW RETURNING C2", "INVOKE C1 \"tick\"", "INVOKE C2 \"tick\"", "INVOKE C1 \"halt\"", "DISPLAY 'AFTER'", "GOBACK."],
    );
    let (out, err, ending) = run_oo(&main, &[counter]);
    assert_eq!(ending, Ok((Ending::StopRun, 3)), "{err}");
    assert_eq!(out, "0001 0006\n0002 0006\n");
}

#[test]
fn jni_reference_services_run_and_the_rest_need_the_jvm() {
    let main = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. JNIUSER RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n",
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        ACCOUNT_DATA,
        "       01  FLAG PIC X.\n           88 FLAG-TRUE VALUE X'01' THRU X'FF'.\n       01  CNAME PIC X(8) VALUE Z'Account'.\n",
        "       LINKAGE SECTION.\n           COPY JNI.\n       PROCEDURE DIVISION.\n",
        &line("SET ADDRESS OF JNIENV TO JNIENVPTR"),
        &line("SET ADDRESS OF JNINATIVEINTERFACE TO JNIENV"),
        &line("INVOKE Account NEW RETURNING A1"),
        &line("CALL NewGlobalRef USING BY VALUE JNIENVPTR A1"),
        &line("    RETURNING A2"),
        &line("CALL IsSameObject USING BY VALUE JNIENVPTR A1 A2"),
        &line("    RETURNING FLAG"),
        &line("IF FLAG-TRUE DISPLAY 'SAME OBJECT' END-IF"),
        &line("IF CNAME = X'C1838396A495A300' DISPLAY 'Z LITERAL' END-IF"),
        &line("CALL FindClass USING BY VALUE JNIENVPTR CNAME"),
        &line("    RETURNING U"),
        &line("GOBACK."),
    ]
    .concat();
    let (out, _, ending) = run_oo(&main, &[account()]);
    assert_eq!(out, "SAME OBJECT\nZ LITERAL\n");
    let abend = ending.unwrap_err();
    assert!(abend.code == "JAVA" && abend.message.contains("FindClass"), "{abend:?}");
}

#[test]
fn a_class_is_found_in_the_program_libraries_and_checks_alone() {
    let dir = temp("classlib");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Account.cbl"), account()).unwrap();
    let main = client(&["Account IS \"Account\""], ACCOUNT_DATA, &["INVOKE Account NEW RETURNING A1", "INVOKE A1 \"getBalance\" RETURNING BAL", "MOVE BAL TO SHOWN DISPLAY SHOWN", "GOBACK."]);
    let (out, err, ending) = run_unit(&main, vec![dir], "");
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, " 100\n");
    let compiled = compile(syntax::parse(&account()).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let refused = compiled.run(&mut out, &mut err).unwrap_err();
    assert!(refused.message.contains("class definition"), "{}", refused.message);
    let refused = compiled.execute_cics(unit::Library::default(), files::Dds::default(), task("ACCT"), unit::Clock::Fixed(0, 0), &mut out, &mut err).unwrap_err();
    assert!(refused.message.contains("class definition"), "{}", refused.message);
}

#[test]
fn what_enterprise_cobol_refuses_is_refused() {
    let data = [ACCOUNT_DATA, "       01  TEXT PIC X(10).\n       01  P POINTER.\n"].concat();
    let refused = |body: &[&str]| errors(&client(&["Account IS \"Account\""], &data, body));
    assert!(refused(&["MOVE A1 TO A2."]).contains("object reference"));
    assert!(refused(&["DISPLAY A1."]).contains("object reference"));
    assert!(refused(&["IF A1 > A2 CONTINUE END-IF."]).contains("equal or not equal"));
    assert!(refused(&["IF A1 = TEXT CONTINUE END-IF."]).contains("compares with"));
    assert!(refused(&["SET P TO A1."]).contains("its own kind"));
    assert!(refused(&["SET A1 TO P."]).contains("another object reference"));
    assert!(refused(&["INVOKE A1 \"m\" USING BY VALUE TEXT."]).contains("not a type Java shares"));
    assert!(refused(&["INVOKE A1 \"m\" USING BY VALUE 1.5."]).contains("not an argument Java takes"));
    assert!(refused(&["INVOKE A1 MNAME."]).contains("universal"));
    assert!(refused(&["INVOKE Account NEW."]).contains("RETURNING"));
    assert!(refused(&["INVOKE Account NEW RETURNING BAL."]).contains("not an object reference"));
    assert!(refused(&["INVOKE BAL \"m\"."]).contains("not an object reference"));
    assert!(refused(&["INVOKE Nowhere \"m\"."]).contains("NOWHERE"));
    assert!(refused(&["INVOKE SELF \"m\"."]).contains("only in a method"));
    assert!(refused(&["EXIT METHOD."]).contains("only in a method"));
    assert!(refused(&["EXEC CICS RETURN END-EXEC."]).contains("EXEC CICS"));
    assert!(errors(&client(&["Account IS \"Account\""], "       01  X OBJECT REFERENCE Other.\n", &["GOBACK."])).contains("REPOSITORY"));
    let bad_class = |methods: &[String], repository: &[&str]| errors(&class("Bad INHERITS Base", repository, &part("OBJECT", "       01  D PIC X.\n", methods)));
    let base = ["Base IS \"java.lang.Object\""];
    assert!(bad_class(&[method("m", "       LINKAGE SECTION.\n       01  L PIC S9(9) BINARY.\n", " USING L", &["CONTINUE."])], &base).contains("BY VALUE"));
    assert!(bad_class(&[method("m", "       LINKAGE SECTION.\n       01  L PIC X(4).\n", " USING BY VALUE L", &["CONTINUE."])], &base).contains("not a type Java shares"));
    assert!(bad_class(&[method("m", "", "", &["EXIT PROGRAM."])], &base).contains("EXIT PROGRAM"));
    assert!(bad_class(&[method("m", "", "", &["SET ADDRESS OF D TO NULL."])], &base).contains("WORKING-STORAGE"));
    assert!(bad_class(&[method("m", "", "", &["EXEC SQL COMMIT END-EXEC."])], &base).contains("EXEC"));
    assert!(bad_class(&[method("m", "", "", &["CONTINUE."]), method("m", "", "", &["CONTINUE."])], &base).contains("same parameter types"));
    assert!(bad_class(&[], &["Other IS \"x.Y\""]).contains("REPOSITORY"));
}

#[test]
fn a_report_takes_no_object_reference_as_source_or_control() {
    let report = |control: &str, source: &str| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REPORTS.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n",
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT P ASSIGN TO PDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  P REPORT IS R.\n",
            "       WORKING-STORAGE SECTION.\n       01  A USAGE OBJECT REFERENCE Account.\n       01  K PIC X.\n       REPORT SECTION.\n",
            &format!("       RD  R CONTROLS {control}.\n       01  D TYPE DE LINE PLUS 1.\n           05 COLUMN 1 PIC X(8) SOURCE {source}.\n"),
            "       PROCEDURE DIVISION.\n",
            &line("GOBACK."),
        ]
        .concat()
    };
    assert_eq!(errors(&report("K", "K")), "");
    assert!(errors(&report("K", "A")).contains("A is an object reference"));
    assert!(errors(&report("A", "K")).contains("A is an object reference"));
}

#[test]
fn a_callable_service_runs_in_a_method_and_cee3dmp_names_the_method() {
    let dumper = class(
        "Dumper INHERITS Base",
        &["Base IS \"java.lang.Object\""],
        &part(
            "OBJECT",
            "",
            &[method(
                "dump",
                "       WORKING-STORAGE SECTION.\n       01  HEADING PIC X(80) VALUE 'IN A METHOD'.\n       01  OPTS PIC X(255) VALUE SPACES.\n       01  FC PIC X(12).\n",
                "",
                &["CALL 'CEE3DMP' USING HEADING OPTS FC", "IF FC = LOW-VALUES DISPLAY 'CEE000' END-IF."],
            )],
        ),
    );
    let main = client(&["Dumper"], "       01  D USAGE OBJECT REFERENCE Dumper.\n", &["INVOKE Dumper NEW RETURNING D", "INVOKE D \"dump\"", "GOBACK."]);
    let (out, err, ending) = run_oo(&main, &[dumper]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "CEE000\n");
    assert!(err.ends_with("  CLIENT\n  DUMPER.dump\n"), "{err}");
}

/// Classes and a client for the front-end fuzzer to mutate.
pub(super) fn fuzz_seeds() -> Vec<String> {
    let client = client(
        &["Account IS \"Account\"", "JString IS \"java.lang.String\""],
        &[ACCOUNT_DATA, "       01  S USAGE OBJECT REFERENCE JString.\n"].concat(),
        &[
            "INVOKE Account \"open\" RETURNING A1",
            "INVOKE A1 \"credit\" USING BY VALUE 7 LENGTH OF BAL",
            "    ON EXCEPTION CONTINUE END-INVOKE",
            "SET U TO A1",
            "IF U = A1 AND A2 NOT = NULL SET A2 TO NULL END-IF",
            "INVOKE U MNAME RETURNING S",
            "    NOT ON EXCEPTION DISPLAY 'Y' END-INVOKE",
            "GOBACK.",
        ],
    );
    let mut seeds = vec![account(), client];
    seeds.extend(animals());
    seeds
}

#[test]
fn factory_methods_are_inherited_and_self_in_one_is_its_own_class() {
    let maker = class(
        "Maker INHERITS Base",
        &["Base IS \"java.lang.Object\""],
        &part(
            "FACTORY",
            "       01  BUILT PIC 9 VALUE 0.\n",
            &[
                method("kind", "", "", &["DISPLAY 'MAKER ' BUILT."]),
                method("build", "", "", &["ADD 1 TO BUILT", "INVOKE SELF \"kind\"."]),
            ],
        ),
    );
    let sub = class("SubMaker INHERITS Maker", &["Maker"], &part("FACTORY", "", &[method("kind", "", "", &["DISPLAY 'SUBMAKER'", "INVOKE SUPER \"kind\"."])]));
    let main = client(&["SubMaker", "Maker"], "", &["INVOKE SubMaker \"build\"", "INVOKE SubMaker \"kind\"", "INVOKE Maker \"build\"", "GOBACK."]);
    let (out, err, ending) = run_oo(&main, &[maker, sub]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "MAKER 1\nSUBMAKER\nMAKER 1\nMAKER 2\n");
}
