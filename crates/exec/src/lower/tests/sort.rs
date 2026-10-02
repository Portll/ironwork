use super::*;
use rt::lir::{Count, FromMove, RangeKind, SenderCheck, SortIo, SortPlan};

fn sort_program(collating: &str, data: &str, procedure: &str) -> String {
    let configuration = if collating.is_empty() {
        String::new()
    } else {
        format!(
            "       CONFIGURATION SECTION.\n       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS BACK.\n       SPECIAL-NAMES. ALPHABET BACK IS 'Z' THROUGH 'A'\n           ALPHABET {collating} IS STANDARD-1.\n"
        )
    };
    [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n",
        &configuration,
        "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT S-FILE ASSIGN TO SORTWK1.\n",
        "           SELECT IN-F ASSIGN TO INDD.\n           SELECT OUT-F ASSIGN TO OUTDD.\n",
        "       DATA DIVISION.\n       FILE SECTION.\n",
        "       FD  IN-F.\n       01  IN-REC PIC X(5).\n",
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-KEY PIC X(2).\n           05 S-AMT PIC S9(5) COMP-3.\n",
        "       FD  OUT-F.\n       01  OUT-REC PIC X(5).\n",
        "       WORKING-STORAGE SECTION.\n",
        data,
        "       PROCEDURE DIVISION.\n",
        procedure,
    ]
    .concat()
}

fn file_sorts(p: &Program) -> Vec<&rt::lir::FileSort> {
    p.services.sorts.iter().filter_map(|s| if let SortPlan::File(f) = s { Some(f) } else { None }).collect()
}

fn file(p: &Program, name: &str) -> u16 {
    p.services.files.iter().position(|f| p.symbols[f.name as usize] == name).unwrap() as u16
}

fn named(p: &Program, q: u32) -> &str {
    &p.symbols[p.places[q as usize].name as usize]
}

#[test]
fn a_file_sort_names_its_sd_files_and_registers_and_places_its_keys_in_the_sd_s_records() {
    let body = [
        line("SORT S-FILE ON ASCENDING KEY S-KEY DESCENDING KEY S-AMT"),
        line("    USING IN-F GIVING OUT-F"),
        line("MERGE S-FILE ON DESCENDING KEY S-KEY USING IN-F OUT-F"),
        line("    GIVING OUT-F"),
        line("GOBACK."),
    ]
    .concat();
    let p = lowered(&sort_program("", "", &body));
    assert_eq!(ops(&p).filter(|op| matches!(op, Op::Sort(_))).count(), 2);
    let sorts = file_sorts(&p);
    let (sort, merge) = (sorts[0], sorts[1]);
    let (sd, input, output) = (file(&p, "S-FILE"), file(&p, "IN-F"), file(&p, "OUT-F"));
    assert_eq!((sort.sd, sort.merge, merge.merge), (sd, false, true));
    assert_eq!((&sort.input, &sort.output), (&Some(SortIo::Files(vec![input])), &Some(SortIo::Files(vec![output]))));
    assert_eq!(merge.input, Some(SortIo::Files(vec![input, output])));
    assert_eq!((named(&p, sort.sort_return), named(&p, sort.sort_control)), ("SORT-RETURN", "SORT-CONTROL"));
    let keys: Vec<_> = sort.keys.keys.iter().map(|k| (k.ascending, k.offset, k.len, k.collated)).collect();
    assert_eq!(keys, [(true, 0, 2, true), (false, 2, 3, false)]);
    assert_eq!(sort.keys.collating, None);
    assert!(p.services.files[sort.sd as usize].fixed);
}

#[test]
fn procedures_are_sort_ranges_of_the_whole_program_and_release_and_return_name_their_places() {
    let body = [
        "       MAIN-LINE.\n",
        &line("SORT S-FILE ON ASCENDING KEY S-KEY"),
        &line("    INPUT PROCEDURE FEED THRU FEED-END OUTPUT PROCEDURE SHOW"),
        &line("GOBACK."),
        "       FEED.\n",
        &line("MOVE 'AB' TO S-KEY RELEASE S-REC"),
        &line("RELEASE S-REC FROM W"),
        &line("GO TO AWAY."),
        "       FEED-END.\n",
        &line("EXIT."),
        "       SHOW.\n",
        &line("RETURN S-FILE INTO W AT END DISPLAY 'END'"),
        &line("    NOT AT END DISPLAY W END-RETURN."),
        "       AWAY.\n",
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&sort_program("", "       01  W PIC X(5).\n", &body));
    let sort = file_sorts(&p)[0];
    let (Some(SortIo::Procedure(input)), Some(SortIo::Procedure(output))) = (&sort.input, &sort.output) else { panic!("{sort:?}") };
    let (input, output) = (p.ranges[*input as usize], p.ranges[*output as usize]);
    assert_eq!((input.first as usize, input.last as usize, input.kind), (paragraph(&p, "FEED"), paragraph(&p, "FEED-END"), RangeKind::SortProcedure));
    assert_eq!((output.first, output.last), (paragraph(&p, "SHOW") as u32, paragraph(&p, "SHOW") as u32));
    let n = p.paragraphs.len() as u32;
    assert_eq!(input.region(n), (0, n - 1));
    for end in ["FEED-END", "SHOW"] {
        assert!(p.paragraphs[paragraph(&p, end)].abandoned.is_some(), "{end}");
        let next = paragraph(&p, end) as u32 + 1;
        assert!(p.blocks.iter().any(|b| b.end == Terminator::ParagraphEnd { next }), "{end}");
    }
    // The procedure's region is every paragraph, so a GO TO never leaves its frame.
    assert!(!p.blocks.iter().any(|b| matches!(b.end, Terminator::GoTo(_))));
    let [plain, from] = &p.services.releases[..] else { panic!("{:?}", p.services.releases) };
    assert_eq!((named(&p, plain.record), plain.file, plain.from), ("S-REC", Some(file(&p, "S-FILE")), None));
    let Some(FromMove { from: LirOperand::Load(w), to, plan: MovePlan::Alnum { .. }, check: SenderCheck::None }) = from.from else { panic!("{from:?}") };
    assert_eq!((named(&p, w), named(&p, to), named(&p, from.sort_return)), ("W", "S-REC", "SORT-RETURN"));
    let returned = &p.services.returns[0];
    let Some((into, MovePlan::Alnum { .. })) = returned.into else { panic!("{returned:?}") };
    assert_eq!((returned.file, named(&p, into), p.symbols[returned.name as usize].as_str()), (Some(file(&p, "S-FILE")), "W", "S-FILE"));
    let at = p.blocks.iter().find(|b| b.ops.iter().any(|op| matches!(op, Op::Return(_)))).unwrap();
    assert!(matches!(&at.end, Terminator::Select(arms) if arms.len() == 2));
}

#[test]
fn release_from_a_record_holding_its_own_odo_object_moves_into_it_whole_and_releases_it_as_it_stands() {
    let source = sort_program("", "       01  SRC PIC X(6) VALUE '3ABCDE'.\n", "").replace(
        "       01  S-REC.\n           05 S-KEY PIC X(2).\n           05 S-AMT PIC S9(5) COMP-3.\n",
        "       01  S-REC.\n           05 S-KEY PIC 9.\n           05 S-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON S-KEY.\n",
    );
    let body = ["       MAIN-LINE.\n", &line("SORT S-FILE ON ASCENDING KEY S-KEY INPUT PROCEDURE FEED"), &line("    GIVING OUT-F"), &line("GOBACK."), "       FEED.\n", &line("RELEASE S-REC FROM SRC.")].concat();
    let p = lowered(&format!("{source}{body}"));
    let release = &p.services.releases[0];
    let to = release.from.unwrap().to;
    assert_eq!((p.places[to as usize].odo.is_none(), p.places[release.record as usize].odo.is_some()), (true, true));
}

#[test]
fn a_table_sort_counts_its_elements_then_locates_the_first_and_keys_them_within_the_element() {
    let data = concat!(
        "       01  T.\n           05 N PIC 9 VALUE 3.\n           05 E OCCURS 1 TO 5 DEPENDING ON N ASCENDING KEY IS E-K.\n",
        "              10 E-V PIC 9.\n              10 E-K PIC X(2).\n",
    );
    let body = [line("SORT E"), line("SORT E ON DESCENDING KEY E-V COLLATING SEQUENCE ASCII"), line("GOBACK.")].concat();
    let p = lowered(&sort_program("ASCII", data, &body));
    let tables: Vec<_> = p.services.sorts.iter().filter_map(|s| if let SortPlan::Table(t) = s { Some(t) } else { None }).collect();
    let (occurs, named_keys) = (tables[0], tables[1]);
    assert!(matches!(&occurs.count, Count::Odo(o) if o.max == 5 && o.element == 3));
    let first = &p.places[occurs.first as usize];
    assert_eq!((first.subscripts.len(), &first.subscripts[0].value, occurs.stride), (1, &IntExpr::Const(1), 3));
    let keys = |t: &rt::lir::TableSort| t.keys.keys.iter().map(|k| (k.ascending, k.offset, k.len)).collect::<Vec<_>>();
    assert_eq!((keys(occurs), keys(named_keys)), (vec![(true, 1, 2)], vec![(false, 0, 1)]));
    // A table SORT takes only its own COLLATING SEQUENCE, not the program's.
    assert_eq!(occurs.keys.collating, None);
    assert!(named_keys.keys.collating.is_some());
    let file = lowered(&sort_program("ASCII", "", &[line("SORT S-FILE ON ASCENDING KEY S-KEY USING IN-F GIVING OUT-F"), line("GOBACK.")].concat()));
    assert_eq!(file_sorts(&file)[0].keys.collating.as_deref(), Some(&compiled(&sort_program("ASCII", "", &line("GOBACK."))).collating.positions()));
}
