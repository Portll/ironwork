use super::*;
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

const EXTENDED: &[&str] = &["--compliance=extended"];

/// A program whose file takes its DD name from WS-DD at each OPEN, its first names coming from
/// SYSIN: two DDs the run has, a path, and a name no DD has.
fn reader(assign: &str) -> String {
    [
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. T.\n",
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        &format!("           SELECT IN-FILE ASSIGN {assign}\n"),
        "               FILE STATUS FS.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD  IN-FILE.\n",
        "       01  IN-REC PIC X(6).\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  WS-DD PIC X(12).\n",
        "       01  FS PIC XX.\n",
        "       PROCEDURE DIVISION.\n",
        "           PERFORM 4 TIMES\n",
        "               ACCEPT WS-DD\n",
        "               OPEN INPUT IN-FILE\n",
        "               IF FS = '00'\n",
        "                   READ IN-FILE\n",
        "                   DISPLAY WS-DD ' ' IN-REC\n",
        "                   CLOSE IN-FILE\n",
        "               ELSE\n",
        "                   DISPLAY WS-DD ' ' FS\n",
        "               END-IF\n",
        "           END-PERFORM.\n",
        "           GOBACK.\n",
    ]
    .concat()
}

const NAMES: &str = "first\nSECOND\n/etc/passwd\nNODD\n";

fn data_sets(name: &str) -> Vec<String> {
    let dir = temp(name);
    std::fs::create_dir_all(&dir).unwrap();
    let (first, second) = (dir.join("first.txt"), dir.join("second.txt"));
    std::fs::write(&first, "AAA\n").unwrap();
    std::fs::write(&second, "BBB\n").unwrap();
    vec![format!("FIRST={}:text", first.display()), format!("SECOND={}:text", second.display())]
}

#[test]
fn under_extended_each_open_takes_the_dd_name_from_the_item_on_both_executors() {
    let dds = data_sets("assign-item");
    let walked = Harness::source(&reader("TO WS-DD")).flags(EXTENDED).dds(&dds).sysin(NAMES).run(Executor::Interpreter);
    assert_eq!(walked.out, "first        AAA   \nSECOND       BBB   \n/etc/passwd  35\nNODD         35\n", "{}", walked.err);
    assert!(walked.ending.is_ok(), "{:?}", walked.ending);
    let vm = Harness::source(&reader("TO WS-DD")).flags(EXTENDED).dds(&dds).sysin(NAMES).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    for form in ["USING WS-DD", "TO DYNAMIC WS-DD"] {
        let o = Harness::source(&reader(form)).flags(EXTENDED).dds(&dds).sysin(NAMES).run(Executor::Interpreter);
        assert_eq!(o.out, "first        AAA   \nSECOND       BBB   \n/etc/passwd  35\nNODD         35\n", "{form}: {}", o.err);
    }
}

#[test]
fn the_lir_carries_the_item_with_the_select_and_prints_it() {
    let compiled = extended_compile(&reader("TO WS-DD")).unwrap_or_else(|e| panic!("{e:?}"));
    let lowered = crate::lower::lower(&compiled).unwrap_or_else(|e| panic!("{e:?}"));
    let file = &lowered.services.files[0];
    assert_eq!(file.assign_item.map(|a| (a.select.line, lowered.places[a.place as usize].len)), Some((6, 12)));
    let printed = rt::lir::Listing::of(&lowered).to_string();
    assert!(printed.lines().any(|l| l.starts_with("file 0 IN-FILE assign WS-DD") && l.contains(" assign-item WS-DD @")), "{printed}");
    let module = rt::module::read(&rt::module::write(std::slice::from_ref(&lowered))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(module.programs[0].services.files[0].assign_item, file.assign_item, "the LIR section's end carries it");
    let plain = crate::lower::lower(&extended_compile(&reader("TO INDD")).unwrap()).unwrap();
    let bytes = rt::module::write(std::slice::from_ref(&plain));
    assert_eq!(rt::module::read(&bytes).unwrap().programs[0].services.files[0].assign_item, None);
}

#[test]
fn under_strict_the_name_is_a_dd_name_and_using_or_dynamic_is_refused() {
    let dir = temp("assign-strict");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ws.txt"), "WSDD\n").unwrap();
    let dds = [format!("WS-DD={}:text", dir.join("ws.txt").display())];
    let o = Harness::source(&reader("TO WS-DD")).dds(&dds).sysin("first\n").run(Executor::Interpreter);
    assert!(o.out.starts_with("first        WSDD  \n"), "{}{}", o.out, o.err);
    for (form, message) in [("USING WS-DD", "ASSIGN USING or DYNAMIC WS-DD: a Micro Focus and GnuCOBOL form; --compliance extended reads it"), ("TO DYNAMIC WS-DD", "ASSIGN USING or DYNAMIC WS-DD: a Micro Focus and GnuCOBOL form; --compliance extended reads it")] {
        let errors = compile(syntax::parse(&reader(form)).unwrap(), &[]).err().unwrap_or_else(|| panic!("{form} compiled"));
        assert!(errors.iter().any(|e| e.message == message), "{form}: {errors:?}");
    }
}

fn extended_compile(source: &str) -> Result<Compiled, Vec<syntax::Error>> {
    let parsed = syntax::parse_with(source, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap_or_else(|e| panic!("{e}"));
    compile(parsed, &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>())
}

#[test]
fn the_item_is_named_with_a_warning_and_must_be_alphanumeric() {
    let compiled = extended_compile(&reader("TO WS-DD")).unwrap_or_else(|e| panic!("{e:?}"));
    let warning = compiled.diagnostics.iter().find(|m| m.id == Some("IWX0007")).expect("IWX0007-W");
    assert_eq!((warning.pos.line, warning.severity), (6, Severity::Warning));
    assert!(warning.message.ends_with("each OPEN of IN-FILE takes its DD name from WS-DD"), "{}", warning.message);
    assert!(extended_compile(&reader("TO INDD")).unwrap().diagnostics.iter().all(|m| m.id != Some("IWX0007")), "a name that is no data item stays a DD name");
    let numeric = reader("USING FS").replace("01  FS PIC XX.", "01  FS PIC 99.");
    let errors = extended_compile(&numeric).err().unwrap();
    assert!(errors.iter().any(|e| e.message == "ASSIGN FS: the item holding the file's name must be alphanumeric or a group"), "{errors:?}");
    let errors = extended_compile(&reader("USING NO-SUCH")).err().unwrap();
    assert!(errors.iter().any(|e| e.message == "ASSIGN NO-SUCH: not a data item"), "{errors:?}");
}

/// A SORT whose USING and GIVING files take their DD names from items, and whose SD names an item
/// no DD has, the GIVING file then read back.
fn sorter(card: &str) -> String {
    [
        card,
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. T.\n",
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        "           SELECT IN-FILE ASSIGN TO IN-NAME.\n",
        "           SELECT SORT-FILE ASSIGN TO SORT-NAME.\n",
        "           SELECT OUT-FILE ASSIGN TO OUT-NAME FILE STATUS FS.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD  IN-FILE.\n",
        "       01  IN-REC PIC X(6).\n",
        "       SD  SORT-FILE.\n",
        "       01  SORT-REC PIC X(6).\n",
        "       FD  OUT-FILE.\n",
        "       01  OUT-REC PIC X(6).\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  IN-NAME PIC X(8) VALUE 'UNSORTED'.\n",
        "       01  SORT-NAME PIC X(8) VALUE 'NOSUCH'.\n",
        "       01  OUT-NAME PIC X(8).\n",
        "       01  FS PIC XX.\n",
        "       PROCEDURE DIVISION.\n",
        "           MOVE 'SORTED' TO OUT-NAME\n",
        "           SORT SORT-FILE ON ASCENDING KEY SORT-REC\n",
        "               USING IN-FILE GIVING OUT-FILE\n",
        "           DISPLAY 'SORT ' FS\n",
        "           OPEN INPUT OUT-FILE\n",
        "           PERFORM 3 TIMES\n",
        "               READ OUT-FILE\n",
        "               DISPLAY OUT-REC\n",
        "           END-PERFORM\n",
        "           CLOSE OUT-FILE\n",
        "           GOBACK.\n",
    ]
    .concat()
}

#[test]
fn a_sort_opens_its_using_and_giving_files_by_the_names_their_items_hold() {
    for card in ["", "       CBL FASTSRT\n"] {
        let dir = temp(if card.is_empty() { "assign-sort" } else { "assign-sort-fastsrt" });
        std::fs::create_dir_all(&dir).unwrap();
        let (unsorted, sorted) = (dir.join("unsorted.txt"), dir.join("sorted.txt"));
        std::fs::write(&unsorted, "CCC\nAAA\nBBB\n").unwrap();
        let dds = [format!("UNSORTED={}:text", unsorted.display()), format!("SORTED={}:text", sorted.display())];
        let compiled = extended_compile(&sorter(card)).unwrap_or_else(|e| panic!("{e:?}"));
        let named: Vec<&str> = compiled.diagnostics.iter().filter(|m| m.id == Some("IWX0007")).map(|m| m.message.as_str()).collect();
        assert!(named.len() == 2 && named.iter().all(|m| !m.contains("SORT-FILE")), "{named:?}");
        let walked = Harness::source(&sorter(card)).flags(EXTENDED).dds(&dds).run(Executor::Interpreter);
        assert_eq!(walked.out, "SORT 00\nAAA   \nBBB   \nCCC   \n", "{card}{}", walked.err);
        assert!(walked.ending.is_ok(), "{:?}", walked.ending);
        let vm = Harness::source(&sorter(card)).flags(EXTENDED).dds(&dds).run(Executor::Vm);
        assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    }
}

/// What `pick` takes from each event a traced run of `source` raises.
fn observed<T: 'static>(source: &str, sysin: &str, dds: &[String], pick: fn(unit::Event<'_>) -> Option<T>) -> Vec<T> {
    let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default().with_compliance(numeric::Compliance::Extended)).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &EXTENDED.iter().map(|f| f.to_string()).collect::<Vec<_>>()).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| recorder.borrow_mut().extend(pick(e)));
    let library = unit::Library { programs, trace_input: true, ..Default::default() };
    let sysin = Some(Box::new(Cursor::new(sysin.as_bytes().to_vec())) as Box<dyn std::io::BufRead>);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ended = compiled.execute_observed(library, files::Dds::new(dds, false).unwrap(), sysin, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer));
    assert!(ended.is_ok(), "{ended:?} {}", String::from_utf8_lossy(&err));
    seen.take()
}

/// Each dynamic-file-path sink a traced run of `source` reached: its line, whether input may be
/// in the name, and the name.
fn file_sinks(source: &str, sysin: &str, dds: &[String]) -> Vec<(u32, Option<bool>, String)> {
    observed(source, sysin, dds, |e| match e {
        unit::Event::Sink { kind: "dynamic-file-path", line, input, operand, .. } => Some((line, input, operand.to_string())),
        _ => None,
    })
}

#[test]
fn an_open_of_a_file_already_open_leaves_it_on_the_dd_it_was_opened_on() {
    let source = [
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. T.\n",
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        "           SELECT IN-FILE ASSIGN TO WS-DD FILE STATUS FS.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD  IN-FILE.\n",
        "       01  IN-REC PIC X(6).\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  WS-DD PIC X(8) VALUE 'FIRST'.\n",
        "       01  FS PIC XX.\n",
        "       PROCEDURE DIVISION.\n",
        "           OPEN INPUT IN-FILE.\n",
        "           MOVE 'SECOND' TO WS-DD.\n",
        "           OPEN INPUT IN-FILE.\n",
        "           DISPLAY FS.\n",
        "           CLOSE IN-FILE.\n",
        "           GOBACK.\n",
    ]
    .concat();
    let dds = data_sets("assign-reopen");
    let events = observed(&source, "", &dds, |e| match e {
        unit::Event::Open { dd, .. } => Some(format!("open {dd}")),
        unit::Event::Close { dd, .. } => Some(format!("close {dd}")),
        _ => None,
    });
    assert_eq!(events, ["open FIRST", "close FIRST"]);
    let walked = Harness::source(&source).flags(EXTENDED).dds(&dds).run(Executor::Interpreter);
    assert_eq!(walked.out, "41\n", "{}", walked.err);
    let vm = Harness::source(&source).flags(EXTENDED).dds(&dds).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

#[test]
fn the_input_trace_has_each_files_name_at_its_select_with_its_own_input() {
    let source = [
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. T.\n",
        "       ENVIRONMENT DIVISION.\n",
        "       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n",
        "           SELECT A-FILE ASSIGN TO A-NAME.\n",
        "           SELECT B-FILE ASSIGN TO B-NAME.\n",
        "       DATA DIVISION.\n",
        "       FILE SECTION.\n",
        "       FD  A-FILE.\n",
        "       01  A-REC PIC X(6).\n",
        "       FD  B-FILE.\n",
        "       01  B-REC PIC X(6).\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01  A-NAME PIC X(8).\n",
        "       01  B-NAME PIC X(8) VALUE 'FIRST'.\n",
        "       PROCEDURE DIVISION.\n",
        "           ACCEPT A-NAME.\n",
        "           OPEN INPUT A-FILE B-FILE.\n",
        "           CLOSE A-FILE B-FILE.\n",
        "           GOBACK.\n",
    ]
    .concat();
    let dds = data_sets("assign-trace");
    assert_eq!(file_sinks(&source, "SECOND\n", &dds), [(6, Some(true), "SECOND".into()), (7, Some(false), "FIRST".into())]);
    let walked = Harness::source(&source).flags(EXTENDED).dds(&dds).sysin("SECOND\n").run(Executor::Interpreter);
    let vm = Harness::source(&source).flags(EXTENDED).dds(&dds).sysin("SECOND\n").run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}

/// An indexed file and a sequential one, each ASSIGNed with a label before its DD name.
const LABELLED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. T.\n",
    "       ENVIRONMENT DIVISION.\n",
    "       INPUT-OUTPUT SECTION.\n",
    "       FILE-CONTROL.\n",
    "           SELECT KF ASSIGN TO DA-MASTER\n",
    "               ORGANIZATION IS INDEXED ACCESS MODE IS DYNAMIC\n",
    "               RECORD KEY IS K-KEY FILE STATUS IS FS.\n",
    "           SELECT SF ASSIGN TO TAPE-NAMES FILE STATUS IS FS.\n",
    "       DATA DIVISION.\n",
    "       FILE SECTION.\n",
    "       FD  KF.\n",
    "       01  K-REC.\n",
    "           05 K-KEY  PIC X(4).\n",
    "           05 K-DATA PIC X(4).\n",
    "       FD  SF.\n",
    "       01  S-REC PIC X(6).\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01  FS PIC XX.\n",
    "       PROCEDURE DIVISION.\n",
    "           OPEN INPUT KF SF\n",
    "           MOVE '0002' TO K-KEY\n",
    "           READ KF KEY IS K-KEY\n",
    "           READ SF\n",
    "           DISPLAY FS ' ' K-DATA ' ' S-REC\n",
    "           CLOSE KF SF\n",
    "           GOBACK.\n",
);

#[test]
fn an_assign_label_documents_the_device_and_the_dd_is_the_name_after_it_on_both_executors() {
    let dir = temp("assign-label");
    std::fs::create_dir_all(&dir).unwrap();
    let (master, names, written) = (dir.join("master.txt"), dir.join("names.txt"), dir.join("written.txt"));
    std::fs::write(&master, "0001AAAA\n0002BBBB\n").unwrap();
    std::fs::write(&names, "ALPHA \n").unwrap();
    std::fs::write(&written, "0002WWWW\n").unwrap();
    let dds = [format!("MASTER={}:text", master.display()), format!("NAMES={}:text", names.display())];
    let walked = Harness::source(LABELLED).dds(&dds).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.is_ok()), ("00 BBBB ALPHA \n", true), "{}", walked.err);
    let vm = Harness::source(LABELLED).dds(&dds).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    let both = [dds[0].clone(), dds[1].clone(), format!("DA-MASTER={}:text", written.display())];
    let o = Harness::source(LABELLED).dds(&both).run(Executor::Interpreter);
    assert_eq!(o.out, "00 WWWW ALPHA \n", "a DD named as ASSIGN writes it is found first: {}", o.err);
}
