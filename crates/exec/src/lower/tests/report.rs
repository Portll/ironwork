use super::*;
use rt::lir::{RangeKind, ReportOp, ReportWriter};
use rt::module::codec::{Encode, Writer};
use rt::report::FieldContent;

fn cobol(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

const SALES: &[&str] = &[
    "IDENTIFICATION DIVISION.",
    "PROGRAM-ID. R.",
    "ENVIRONMENT DIVISION.",
    "INPUT-OUTPUT SECTION.",
    "FILE-CONTROL.",
    "    SELECT RPT ASSIGN TO RPTDD.",
    "DATA DIVISION.",
    "FILE SECTION.",
    "FD  RPT REPORT IS SALES.",
    "WORKING-STORAGE SECTION.",
    "01  DEPT PIC X(2) VALUE 'AA'.",
    "01  AMT PIC 9(3) VALUE 5.",
    "01  CALLS PIC 9 VALUE 0.",
    "REPORT SECTION.",
    "RD  SALES CONTROL IS DEPT.",
    "01  ROW TYPE DE LINE PLUS 1.",
    "    05 R-AMT COLUMN 1 PIC 9(3) SOURCE AMT.",
    "    05 COLUMN 5 PIC 9(4) SOURCE AMT * 2.",
    "    05 COLUMN 10 VALUE 'X'.",
    "01  TYPE CF DEPT LINE PLUS 1.",
    "    05 COLUMN 1 PIC Z(4)9 SUM R-AMT.",
    "PROCEDURE DIVISION.",
    "DECLARATIVES.",
    "ROW-USE SECTION.",
    "    USE BEFORE REPORTING ROW.",
    "ROW-PARA.",
    "    ADD 1 TO CALLS.",
    "END DECLARATIVES.",
    "MAIN SECTION.",
    "MAIN-PARA.",
    "    OPEN OUTPUT RPT",
    "    INITIATE SALES",
    "    GENERATE ROW",
    "    GENERATE SALES",
    "    TERMINATE SALES",
    "    CLOSE RPT",
    "    GOBACK.",
];

#[test]
fn the_report_model_is_in_services_with_comparands_places_constants_and_a_range_per_use_procedure() {
    let p = lowered(&cobol(SALES));
    let writer = &p.services.report;
    let [report] = &writer.reports[..] else { panic!("{writer:?}") };
    let row = report.groups.iter().position(|g| g.name.as_deref() == Some("ROW")).unwrap() as u32;
    let reported: Vec<ReportOp> = ops(&p).filter_map(|op| if let Op::Report(r) = op { Some(*r) } else { None }).collect();
    assert_eq!(reported, [ReportOp::Initiate(0), ReportOp::Generate { report: 0, detail: Some(row) }, ReportOp::Generate { report: 0, detail: None }, ReportOp::Terminate(0)]);
    let name = |q: usize| p.symbols[p.places[q].name as usize].as_str();
    assert_eq!(name(report.controls[0].reference as usize), "DEPT");
    let group = &report.groups[row as usize];
    let fields = &group.lines[0].fields;
    let FieldContent::Source(Comparand::Operand(LirOperand::Load(amt))) = fields[0].content else { panic!("{:?}", fields[0]) };
    assert_eq!(name(amt as usize), "AMT");
    assert!(matches!(fields[1].content, FieldContent::Source(Comparand::Expr { mode: Mode::Fixed, .. })));
    let FieldContent::Value(x) = fields[2].content else { panic!("{:?}", fields[2]) };
    assert_eq!(p.consts[x as usize], Const::Bytes(crate::testing::ebcdic("X")));
    // Every data item the writer names is a static place of the slab.
    let items = fields.iter().map(|f| f.item).chain(report.sums.iter().map(|s| s.total)).chain([report.page_counter, report.line_counter, report.state]);
    for q in items {
        let place = &p.places[q];
        assert!(place.base == Base::Program && place.moved.is_empty() && place.subscripts.is_empty() && place.odo.is_empty() && place.refmod.is_none(), "{place:?}");
    }
    assert_eq!(name(fields[0].item), "R-AMT");
    let r = group.declarative.unwrap();
    let range = p.ranges[r as usize];
    assert_eq!((range.first as usize, range.kind), (paragraph(&p, "ROW-USE"), RangeKind::UseBeforeReporting));
    assert!(p.paragraphs[range.last as usize].abandoned.is_some());
    assert_eq!(end_of(&p, "ROW-PARA"), Terminator::ParagraphEnd { next: range.last + 1 });
}

#[test]
fn a_program_without_reports_holds_an_empty_writer_in_two_bytes() {
    let p = lowered(&program("", "", &line("GOBACK.")));
    assert_eq!(p.services.report, ReportWriter::default());
    let mut w = Writer::new();
    p.services.report.encode(&mut w);
    assert_eq!(w.take().len(), 2);
}
