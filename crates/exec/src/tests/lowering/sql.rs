use super::*;
use crate::sql::HostType;
use rt::abend::AbendCode;
use rt::lir::{HostPlace, SqlStatement, SqlTest, SqlcaField};

const DATA: &str = concat!(
    "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
    "       01 WS-ID    PIC S9(9) COMP VALUE 7.\n",
    "       01 WS-NAME  PIC X(10).\n",
    "       01 WS-AMT   PIC S9(5)V99 COMP-3 VALUE 0.\n",
    "       01 WS-IND   PIC S9(4) COMP VALUE 0.\n",
    "       01 E-AMT    PIC -9(5).99.\n",
);

fn sql_ops(p: &Program) -> Vec<u32> {
    ops(p).filter_map(|op| if let Op::Sql(k) = op { Some(*k) } else { None }).collect()
}

fn var_name<'p>(p: &'p Program, h: &HostPlace) -> &'p str {
    symbol(p, p.places[h.var as usize].name)
}

/// Each WHENEVER branch from block `from` on: its test, and the end of the block it takes.
fn whenever(p: &Program, from: usize) -> Vec<(SqlTest, Terminator)> {
    let mut out = Vec::new();
    let mut b = from;
    while let Terminator::Branch { cond, then, otherwise } = p.blocks[b].end {
        let lir::Cond::Sql(test) = p.conds[cond as usize] else { break };
        out.push((test, p.blocks[then as usize].end.clone()));
        b = otherwise as usize;
    }
    out
}

fn block_of(p: &Program, ordinal: u32) -> usize {
    p.blocks.iter().position(|b| b.ops.contains(&Op::Sql(ordinal))).unwrap()
}

#[test]
fn every_block_has_its_entry_by_ordinal_and_whenever_branches_after_each_statement() {
    let procedure = [
        "       MAIN-LINE.\n",
        &line("EXEC SQL WHENEVER SQLERROR GO TO FAILED END-EXEC."),
        &line("EXEC SQL WHENEVER NOT FOUND GO TO NONE END-EXEC."),
        &line("EXEC SQL SELECT NAME, AMT INTO :WS-NAME, :WS-AMT:WS-IND"),
        &line("         FROM T WHERE ID = :WS-ID END-EXEC."),
        &line("EXEC SQL WHENEVER NOT FOUND CONTINUE END-EXEC."),
        &line("EXEC SQL DELETE FROM T END-EXEC."),
        &line("GOBACK."),
        "       FAILED.\n",
        &line("DISPLAY 'FAILED'."),
        "       NONE.\n",
        &line("DISPLAY 'NONE'."),
    ]
    .concat();
    let p = lowered(&program("", DATA, &procedure));
    let verbs: Vec<(u32, &str)> = p.sql.iter().map(|e| (e.ordinal, symbol(&p, e.verb))).collect();
    // INCLUDE is expanded as COPY is, and takes no ordinal.
    assert_eq!(verbs, [(1, "WHENEVER"), (2, "WHENEVER"), (3, "SELECT"), (4, "WHENEVER"), (5, "DELETE")]);
    assert!(p.sql.iter().filter(|e| symbol(&p, e.verb) != "SELECT" && symbol(&p, e.verb) != "DELETE").all(|e| e.statement == SqlStatement::Declaration && symbol(&p, e.text).is_empty()));
    assert_eq!(sql_ops(&p), [3, 5]);
    let select = &p.sql[2];
    assert_eq!((symbol(&p, select.text), select.fingerprint), ("SELECT NAME, AMT FROM T WHERE ID = ?", crate::sql::fingerprint("SELECT NAME, AMT FROM T WHERE ID = ?")));
    let SqlStatement::Query { inputs, into } = &select.statement else { panic!("{:?}", select.statement) };
    assert_eq!(inputs.iter().map(|h| (var_name(&p, h), h.ty.clone())).collect::<Vec<_>>(), [("WS-ID", Ok(HostType::Integer { signed: true }))]);
    assert_eq!(into.iter().map(|h| var_name(&p, h)).collect::<Vec<_>>(), ["WS-NAME", "WS-AMT"]);
    assert!(into[0].indicator.is_none() && into[1].indicator.is_some_and(|(q, at)| symbol(&p, p.places[q as usize].name) == "WS-IND" && at == 0));
    let (failed, none) = (p.paragraphs[paragraph(&p, "FAILED")].entry, p.paragraphs[paragraph(&p, "NONE")].entry);
    assert_eq!(whenever(&p, block_of(&p, 3)), [(SqlTest::Error, Terminator::Jump(failed)), (SqlTest::NotFound, Terminator::Jump(none))]);
    assert_eq!(whenever(&p, block_of(&p, 5)), [(SqlTest::Error, Terminator::Jump(failed))]);
    let fields: Vec<SqlcaField> = p.services.sqlca.fields.iter().map(|f| f.0).collect();
    assert!(fields.contains(&SqlcaField::Code) && fields.contains(&SqlcaField::ErrD(3)) && fields.contains(&SqlcaField::Warn(10)), "{fields:?}");
    let errd = p.services.sqlca.fields.iter().find(|f| f.0 == SqlcaField::ErrD(3)).unwrap();
    assert!(matches!(&p.places[errd.1 as usize].subscripts[..], [lir::Subscript { value: IntExpr::Const(3), .. }]));
}

#[test]
fn a_whenever_go_to_out_of_a_perform_range_is_a_transfer_and_one_to_no_procedure_abends_when_taken() {
    let procedure = [
        "       MAIN-LINE.\n",
        &line("PERFORM QUERY"),
        &line("GOBACK."),
        "       QUERY.\n",
        &line("EXEC SQL WHENEVER SQLERROR GO TO FAILED END-EXEC."),
        &line("EXEC SQL WHENEVER SQLWARNING GO TO NOWHERE END-EXEC."),
        &line("EXEC SQL DELETE FROM T END-EXEC."),
        "       FAILED.\n",
        &line("DISPLAY 'FAILED'."),
    ]
    .concat();
    let p = lowered(&program("", DATA, &procedure));
    let branches = whenever(&p, block_of(&p, 3));
    assert_eq!(branches[0], (SqlTest::Error, Terminator::GoTo(paragraph(&p, "FAILED") as u32)));
    let (SqlTest::Warning, Terminator::Abend(a)) = branches[1] else { panic!("{branches:?}") };
    let text = &p.abends[a as usize];
    assert!(text.code == AbendCode::Ironwork && symbol(&p, text.message).contains("NOWHERE") && text.at.is_none(), "{text:?}");
}

#[test]
fn a_host_structure_is_its_members_with_the_indicator_array_s_elements_and_an_untyped_item_keeps_its_abend() {
    let data = concat!(
        "       01 WS-ROW.\n          05 R-NAME PIC X(5).\n          05 R-ID   PIC S9(9) COMP.\n          05 R-AMT  PIC S9(5)V99 COMP-3.\n",
        "       01 WS-INDS.\n          05 WS-IND PIC S9(4) COMP OCCURS 3.\n       01 E-AMT PIC -9(5).99.\n",
    );
    let body = [line("EXEC SQL SELECT NAME, ID, AMT INTO :WS-ROW:WS-INDS FROM T"), line("         WHERE AMT = :E-AMT END-EXEC."), line("GOBACK.")].concat();
    let p = lowered(&program("", data, &body));
    let SqlStatement::Query { inputs, into } = &p.sql[0].statement else { panic!("{:?}", p.sql[0].statement) };
    let members: Vec<_> = into.iter().map(|h| (h.member, h.indicator.map(|(_, at)| at))).collect();
    assert_eq!(members, [(Some((0, 5)), Some(0)), (Some((5, 4)), Some(2)), (Some((9, 4)), Some(4))]);
    assert!(into.iter().all(|h| h.var == into[0].var && var_name(&p, h) == "WS-ROW"));
    let [HostPlace { ty: Err(a), .. }] = inputs[..] else { panic!("{inputs:?}") };
    let text = &p.abends[a as usize];
    assert_eq!((&text.code, symbol(&p, text.message)), (&AbendCode::Exec, "EXEC SQL SELECT: E-AMT: this USAGE or PICTURE has no SQL type"));
    assert_eq!(text.at.map(|at| p.debug.positions[at as usize].line), Some(13));
    assert!(p.services.sqlca.fields.is_empty());
}

#[test]
fn a_whenever_label_two_sections_have_is_the_one_in_the_sql_statement_s_section() {
    let section = |name: &str| {
        let whenever = line("EXEC SQL WHENEVER NOT FOUND GO TO NONE END-EXEC.");
        format!("       {name} SECTION.\n{whenever}{}       NONE.\n{}", line("EXEC SQL DELETE FROM T END-EXEC."), line("GOBACK."))
    };
    let p = lowered(&program("", DATA, &[section("S1"), section("S2")].concat()));
    let (first, second) = (p.paragraphs[1].entry, p.paragraphs[3].entry);
    assert_eq!(whenever(&p, block_of(&p, 2)), [(SqlTest::NotFound, Terminator::Jump(first))]);
    assert_eq!(whenever(&p, block_of(&p, 4)), [(SqlTest::NotFound, Terminator::Jump(second))]);
}
