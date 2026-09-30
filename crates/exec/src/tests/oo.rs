use super::*;

/// The options IBM compiles object-oriented COBOL with; RENT and DBCS are its defaults.
const OO_CARD: &str = "       CBL THREAD,DLL\n";

/// A class definition: its REPOSITORY entries, then FACTORY and OBJECT paragraphs as written.
fn class(head: &str, repository: &[&str], parts: &str) -> String {
    let entries: Vec<String> = repository.iter().map(|e| format!("           CLASS {e}")).collect();
    format!(
        "{OO_CARD}       IDENTIFICATION DIVISION.\n       CLASS-ID. {head}.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n{}.\n{parts}       END CLASS {}.\n",
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
        "{OO_CARD}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLIENT RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n{}.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{body}",
        entries.join("\n")
    )
}

/// Runs the first program of `main` with the rest of its programs, and `classes`, in its run unit.
fn run_oo(main: &str, classes: &[String]) -> (String, String, Result<(Ending, i16), Abend>) {
    let o = Harness::source(main).classes(classes).clock(unit::Clock::Fixed(0, 0)).run(Executor::Interpreter);
    (o.out, o.err, o.ending.map(|e| (e, o.return_code)))
}

fn errors(source: &str) -> String {
    compile_errors(source)
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
        OO_CARD,
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
            OO_CARD,
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REPORTS RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n",
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

const ENVIRONMENT: [&str; 2] = ["SET ADDRESS OF JNIENV TO JNIENVPTR", "SET ADDRESS OF JNINATIVEINTERFACE TO JNIENV"];

/// A class that keeps an Account in its instance data: as a method receives it, makes it, or makes
/// it global; and SELF.
fn holder() -> String {
    let jni = "       LINKAGE SECTION.\n           COPY JNI.\n";
    let account = "       01  A USAGE OBJECT REFERENCE Account.\n";
    let balance = "       LINKAGE SECTION.\n       01  B PIC S9(9) BINARY.\n";
    class(
        "Holder INHERITS Base",
        &["Base IS \"java.lang.Object\"", "Account IS \"Account\"", "Holder IS \"Holder\""],
        &part(
            "OBJECT",
            "       01  KEPT USAGE OBJECT REFERENCE Account.\n       01  ME USAGE OBJECT REFERENCE Holder.\n",
            &[
                method("keep", &format!("       LINKAGE SECTION.\n{account}"), " USING BY VALUE A", &["SET KEPT TO A."]),
                method("keepGlobal", &format!("{jni}{account}"), " USING BY VALUE A", &[ENVIRONMENT[0], ENVIRONMENT[1], "CALL NewGlobalRef USING BY VALUE JNIENVPTR A", "    RETURNING KEPT."]),
                method("drop", jni, "", &[ENVIRONMENT[0], ENVIRONMENT[1], "CALL DeleteGlobalRef USING BY VALUE JNIENVPTR KEPT."]),
                method("make", "", "", &["INVOKE Account NEW RETURNING KEPT."]),
                method("remember", "", "", &["SET ME TO SELF."]),
                method("balance", balance, " RETURNING B", &["INVOKE KEPT \"getBalance\" RETURNING B."]),
                method("again", balance, " RETURNING B", &["INVOKE ME \"balance\" RETURNING B."]),
                method("kept", "       LINKAGE SECTION.\n       01  R USAGE OBJECT REFERENCE Account.\n", " RETURNING R", &["SET R TO KEPT."]),
            ],
        ),
    )
}

/// A client whose line 23 is the first statement after it makes a Holder, H, and an Account, A1.
fn holder_client(body: &[&str]) -> (String, Result<(Ending, i16), Abend>) {
    let data = [ACCOUNT_DATA, "       01  H USAGE OBJECT REFERENCE Holder.\n"].concat();
    let body = [&["INVOKE Holder NEW RETURNING H", "INVOKE Account NEW RETURNING A1"], body, &["GOBACK."]].concat();
    let main = client(&["Account IS \"Account\"", "Holder IS \"Holder\""], &data, &body);
    let (out, _, ending) = run_oo(&main, &[account(), holder()]);
    (out, ending)
}

fn abend(ending: Result<(Ending, i16), Abend>) -> String {
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "IRONWORK", "{abend:?}");
    abend.message
}

#[test]
fn a_local_reference_a_method_keeps_expires_when_the_method_returns() {
    let invoked = |m: &str| format!("method \"{m}\" of Holder, invoked at line 23 of CLIENT");
    let message = abend(holder_client(&["INVOKE H \"keep\" USING BY VALUE A1", "INVOKE H \"balance\" RETURNING BAL"]).1);
    let expected = format!(
        "INVOKE KEPT \"getBalance\": KEPT holds a local reference to an Account object, received as argument 1 by {}; it expired when {}, returned, and IBM leaves using it unpredictable (see J17)",
        invoked("keep"),
        invoked("keep")
    );
    assert_eq!(message, expected);
    let message = abend(holder_client(&["INVOKE H \"make\"", "INVOKE H \"balance\" RETURNING BAL"]).1);
    assert!(message.contains(&format!("KEPT holds a local reference to an Account object, made by INVOKE Account NEW at line 50 of Holder.make; it expired when {}, returned", invoked("make"))), "{message}");
    let message = abend(holder_client(&["INVOKE H \"remember\"", "INVOKE H \"again\" RETURNING BAL"]).1);
    assert!(message.starts_with(&format!("INVOKE ME \"balance\": ME holds a local reference to a Holder object, SELF of {}; it expired when", invoked("remember"))), "{message}");
    let message = abend(holder_client(&["INVOKE H \"keep\" USING BY VALUE A1", "INVOKE H \"kept\" RETURNING A2"]).1);
    assert!(message.starts_with("INVOKE H \"kept\": R, the RETURNING item of method \"kept\", holds a local reference"), "{message}");
}

#[test]
fn a_global_reference_lasts_until_deleted_and_a_returned_one_is_the_invokers_own() {
    let (out, ending) = holder_client(&[
        "INVOKE H \"keepGlobal\" USING BY VALUE A1",
        "INVOKE H \"balance\" RETURNING BAL",
        "MOVE BAL TO SHOWN DISPLAY SHOWN",
        "INVOKE H \"kept\" RETURNING A2",
        "IF A1 = A2 DISPLAY 'SAME OBJECT' END-IF",
        "INVOKE H \"drop\"",
        "INVOKE A2 \"getBalance\" RETURNING BAL",
        "MOVE BAL TO SHOWN DISPLAY SHOWN",
        "INVOKE H \"balance\" RETURNING BAL",
    ]);
    assert_eq!(out, " 100\nSAME OBJECT\n 100\n");
    let message = abend(ending);
    assert!(message.contains("KEPT holds a global reference to an Account object, made by NewGlobalRef at line 34 of Holder.keepGlobal; it was deleted by DeleteGlobalRef at line 45 of Holder.drop"), "{message}");
}

/// A client with the JNI's function table in its LINKAGE SECTION.
fn jni_client(body: &[&str]) -> (String, Result<(Ending, i16), Abend>) {
    let data = [ACCOUNT_DATA, "       01  B USAGE OBJECT REFERENCE Account.\n       01  N PIC S9(9) COMP-5.\n       LINKAGE SECTION.\n           COPY JNI.\n"].concat();
    let main = client(&["Account IS \"Account\""], &data, &[&ENVIRONMENT[..], body, &["GOBACK."]].concat());
    let (out, _, ending) = run_oo(&main, &[account()]);
    (out, ending)
}

#[test]
fn jni_services_make_delete_and_frame_references() {
    let (out, ending) = jni_client(&[
        "INVOKE Account NEW RETURNING A1",
        "CALL NewGlobalRef USING BY VALUE JNIENVPTR A1 RETURNING A2",
        "CALL GetObjectRefType USING BY VALUE JNIENVPTR A1 RETURNING N",
        "DISPLAY N",
        "CALL GetObjectRefType USING BY VALUE JNIENVPTR A2 RETURNING N",
        "DISPLAY N",
        "CALL GetObjectRefType USING BY VALUE JNIENVPTR U RETURNING N",
        "DISPLAY N",
        "IF A1 = A2 DISPLAY 'ONE OBJECT' END-IF",
        "CALL NewLocalRef USING BY VALUE JNIENVPTR A2 RETURNING U",
        "CALL DeleteGlobalRef USING BY VALUE JNIENVPTR A2",
        "INVOKE U \"getBalance\" RETURNING BAL",
        "CALL DeleteLocalRef USING BY VALUE JNIENVPTR A1",
        "IF A1 NOT = NULL DISPLAY 'NOT NULL' END-IF",
        "CALL PushLocalFrame USING BY VALUE JNIENVPTR 4 RETURNING N",
        "INVOKE Account NEW RETURNING A1",
        "INVOKE Account \"open\" RETURNING B",
        "CALL PopLocalFrame USING BY VALUE JNIENVPTR A1 RETURNING A1",
        "INVOKE A1 \"getBalance\" RETURNING BAL",
        "MOVE BAL TO SHOWN DISPLAY SHOWN",
        "INVOKE B \"getBalance\" RETURNING BAL",
    ]);
    assert_eq!(out, "000000000A\n000000000B\n000000000{\nONE OBJECT\nNOT NULL\n 100\n");
    let message = abend(ending);
    assert!(
        message.starts_with("INVOKE B \"getBalance\": B holds a local reference to an Account object, returned by method \"open\" of Account, invoked at line 41 of CLIENT; it expired when PopLocalFrame at line 42 of CLIENT freed its frame"),
        "{message}"
    );
    let deleted = abend(jni_client(&["INVOKE Account NEW RETURNING A1", "SET A2 TO A1", "CALL DeleteLocalRef USING BY VALUE JNIENVPTR A1", "IF A2 = A1 CONTINUE END-IF"]).1);
    assert!(deleted.starts_with("A2 = A1: A2 holds a local reference to an Account object, made by INVOKE Account NEW at line 25 of CLIENT; it was deleted by DeleteLocalRef at line 27 of CLIENT"), "{deleted}");
    let wrong_kind = abend(jni_client(&["INVOKE Account NEW RETURNING A1", "CALL NewGlobalRef USING BY VALUE JNIENVPTR A1 RETURNING A2", "CALL DeleteLocalRef USING BY VALUE JNIENVPTR A2"]).1);
    assert!(wrong_kind.contains("A2 holds a global reference, which DeleteLocalRef does not delete"), "{wrong_kind}");
    let unpushed = abend(jni_client(&["CALL PopLocalFrame USING BY VALUE JNIENVPTR U RETURNING U"]).1);
    assert!(unpushed.contains("no frame PushLocalFrame pushed is open"), "{unpushed}");
}

#[test]
fn a_program_that_is_not_a_method_makes_its_references_in_the_running_methods_frame() {
    let maker = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAKER RECURSIVE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  M USAGE OBJECT REFERENCE Account.\n       01  B PIC S9(9) BINARY.\n       01  SHOWN PIC ZZZ9.\n       PROCEDURE DIVISION.\n";
    let maker = [
        maker,
        &line("IF M = NULL INVOKE Account NEW RETURNING M DISPLAY 'MADE'"),
        &line("ELSE INVOKE M \"getBalance\" RETURNING B MOVE B TO SHOWN"),
        &line("    DISPLAY 'USED ' SHOWN END-IF"),
        &line("GOBACK."),
        "       END PROGRAM MAKER.\n",
    ]
    .concat();
    let caller = class("Caller INHERITS Base", &["Base IS \"java.lang.Object\""], &part("OBJECT", "", &[method("call", "", "", &["CALL 'MAKER'."])]));
    let main = |body: &[&str]| {
        let data = "       01  C USAGE OBJECT REFERENCE Caller.\n";
        [client(&["Caller"], data, &[body, &["GOBACK."]].concat()), "       END PROGRAM CLIENT.\n".into(), maker.clone()].concat()
    };
    let (out, err, ending) = run_oo(&main(&["CALL 'MAKER'", "CALL 'MAKER'"]), &[account(), caller.clone()]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "MADE\nUSED  100\n");
    let (out, _, ending) = run_oo(&main(&["INVOKE Caller NEW RETURNING C", "INVOKE C \"call\"", "CALL 'MAKER'"]), &[account(), caller]);
    assert_eq!(out, "MADE\n");
    let message = abend(ending);
    assert!(
        message.starts_with("INVOKE M \"getBalance\": M holds a local reference to an Account object, made by INVOKE Account NEW at line 29 of MAKER; it expired when method \"call\" of CALLER, invoked at line 13 of CLIENT, returned"),
        "{message}"
    );
}

#[test]
fn object_oriented_programs_need_the_options_ibm_compiles_them_with() {
    let with = |card: &str, id: &str| errors(&client(&["Account IS \"Account\""], ACCOUNT_DATA, &["INVOKE Account NEW RETURNING A1", "GOBACK."]).replacen(OO_CARD, card, 1).replacen("CLIENT RECURSIVE", id, 1));
    let needs = |missing: &str| format!("warning: program CLIENT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: {missing} missing from its CBL or PROCESS cards (see J13 and J19)");
    assert_eq!(with(OO_CARD, "CLIENT RECURSIVE"), "");
    assert_eq!(with("", "CLIENT"), needs("THREAD, DLL"));
    assert_eq!(with("       CBL THREAD\n", "CLIENT RECURSIVE"), needs("DLL"));
    assert_eq!(with("       PROCESS DLL\n", "CLIENT"), needs("THREAD"));
    assert_eq!(with("       CBL THREAD,DLL,NODBCS\n", "CLIENT RECURSIVE"), needs("DBCS"));
    assert_eq!(with("       CBL NORENT\n", "CLIENT"), needs("THREAD, DLL, RENT"));
    assert!(with("       CBL THREAD,DLL,NORENT\n", "CLIENT RECURSIVE").starts_with("warning: NORENT conflicts with THREAD and DLL, which IBM compiles only as RENT"));
    assert_eq!(with(OO_CARD, "CLIENT"), "program CLIENT is compiled with THREAD, which requires RECURSIVE in its PROGRAM-ID paragraph");
    assert_eq!(with(OO_CARD, "CLIENT RECURSIVE INITIAL"), "program CLIENT is INITIAL, which THREAD does not allow");
    let nested = [client(&["Account IS \"Account\""], ACCOUNT_DATA, &["GOBACK."]), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n           GOBACK.\n       END PROGRAM INNER.\n       END PROGRAM CLIENT.\n".into()].concat();
    assert_eq!(errors(&nested), "program CLIENT contains program INNER, and THREAD does not allow nested programs");
    let bare = account().replacen(OO_CARD, "", 1);
    assert!(errors(&bare).starts_with("warning: class ACCOUNT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: THREAD, DLL missing"), "{}", errors(&bare));
}

#[test]
fn a_program_and_class_without_the_options_run_with_a_warning_unless_warnings_block() {
    let program = client(&["Account IS \"Account\""], ACCOUNT_DATA, &["INVOKE Account NEW RETURNING A1", "INVOKE A1 \"getBalance\" RETURNING BAL", "MOVE BAL TO SHOWN DISPLAY SHOWN", "GOBACK."]);
    let bare = program.replacen(OO_CARD, "", 1).replacen("CLIENT RECURSIVE", "CLIENT", 1);
    let compiled = compile(syntax::parse(&bare).unwrap(), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(compiled.diagnostics.iter().map(|d| (d.severity, syntax::return_code(std::slice::from_ref(d)))).collect::<Vec<_>>(), [(syntax::Severity::Warning, 4)]);
    let bare_class = account().replacen(OO_CARD, "", 1);
    let (out, err, ending) = run_oo(&bare, std::slice::from_ref(&bare_class));
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, " 100\n");
    let refused = compile(syntax::parse(&bare).unwrap(), &["-warnings-block".into()]).err().expect("refused");
    assert_eq!(refused.iter().map(|d| d.severity).collect::<Vec<_>>(), [syntax::Severity::Warning]);
    let blocked = Harness::source(&program).classes(&[bare_class]).flags(&["-warnings-block"]).run(Executor::Interpreter);
    let message = blocked.ending.unwrap_err().message;
    assert!(message.starts_with("class Account does not compile: Account: warning: class ACCOUNT uses object-oriented syntax") && message.contains("THREAD, DLL missing"), "{message}");
}

#[test]
fn thread_needs_recursive_and_refuses_initial_and_file_sorts() {
    let t = |card: &str, head: &str, body: &[&str]| {
        let text = [
            card,
            &format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {head}.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n"),
            "           SELECT S ASSIGN TO SORTWK1.\n           SELECT F ASSIGN TO FDD.\n           SELECT G ASSIGN TO GDD.\n       DATA DIVISION.\n       FILE SECTION.\n",
            "       SD  S.\n       01  S-REC PIC X(4).\n       FD  F.\n       01  F-REC PIC X(4).\n       FD  G.\n       01  G-REC PIC X(4).\n",
            "       WORKING-STORAGE SECTION.\n       01  T.\n           05 E PIC X OCCURS 3.\n       PROCEDURE DIVISION.\n",
            &body.iter().map(|l| line(l)).collect::<String>(),
            &line("GOBACK."),
        ]
        .concat();
        errors(&text)
    };
    let sort = ["SORT S ON ASCENDING KEY S-REC", "    USING F GIVING G"];
    let merge = ["MERGE S ON ASCENDING KEY S-REC", "    USING F G GIVING G"];
    assert_eq!(t("", "T", &sort), "");
    assert_eq!(t("", "T IS INITIAL", &merge), "");
    assert_eq!(t("       CBL THREAD\n", "T", &[]), "program T is compiled with THREAD, which requires RECURSIVE in its PROGRAM-ID paragraph");
    assert_eq!(t("       CBL THREAD\n", "T RECURSIVE INITIAL", &[]), "program T is INITIAL, which THREAD does not allow");
    assert_eq!(t("       CBL THREAD\n", "T RECURSIVE", &["SORT E ON ASCENDING KEY E"]), "");
    assert_eq!(t("       CBL THREAD\n", "T RECURSIVE", &sort), "SORT of a file is not allowed in a program compiled with THREAD");
    assert_eq!(t("       CBL THREAD\n", "T RECURSIVE", &merge), "MERGE is not allowed in a program compiled with THREAD");
}

#[test]
fn a_program_that_reaches_java_only_through_the_jni_needs_no_thread_or_dll() {
    let text = [
        "       CBL LIST,MAP,XREF\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. BRIDGE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Hist IS \"com.acme.Hist\".\n",
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  CLASS-REF PIC 9(18) COMP-5 VALUE 0.\n       01  CNAME PIC X(14) VALUE Z'com/acme/Hist'.\n       LINKAGE SECTION.\n           COPY JNI.\n       PROCEDURE DIVISION.\n",
        &line(ENVIRONMENT[0]),
        &line(ENVIRONMENT[1]),
        &line("CALL FindClass USING BY VALUE JNIENVPTR ADDRESS OF CNAME"),
        &line("    RETURNING CLASS-REF"),
        &line("GOBACK."),
    ]
    .concat();
    assert_eq!(errors(&text), "");
}
