//! The sample program the LIR and load-module tests share.

use ironwork_rt::lir::*;
use ironwork_rt::storage::Kind;
use ironwork_rt::vocab::{BinOp, Pos};
use numeric::precision::{Fixed, Places};
use numeric::{Arith, Options};

fn fixed(value: i128, int: u32, dec: u32) -> Fixed {
    Fixed::new(value, Places::new(int, dec))
}

const WS_I: PlaceId = 0;
const WS_AMT_I: PlaceId = 1;
const WS_TOTAL: PlaceId = 2;

/// MAIN sets WS-I to 1 and performs ADD-PARA three times, stepping WS-I; ADD-PARA does
/// `ADD WS-AMT (WS-I) TO WS-TOTAL ROUNDED ON SIZE ERROR STOP RUN`.
pub fn payroll() -> Program {
    let symbols = ["PAYROLL", "PAYROLL.cbl", "MAIN", "ADD-PARA", "WS-TABLE", "WS-AMT", "WS-I", "WS-TOTAL"];
    let binary = Kind::Binary { digits: 4, scale: 0, signed: false, native: false };
    let packed = |digits| Kind::Packed { digits, scale: 2, signed: true };
    let item = |name, level, parent, offset, size, kind, at| Item {
        name: Some(name),
        level,
        parent,
        offset,
        size,
        occurs: 1,
        dims: vec![],
        kind,
        local: false,
        linkage: None,
        redefines: None,
        depending_on: None,
        keys: vec![],
        at,
    };
    let amount = Item { occurs: 10, dims: vec![(4, 10)], ..item(5, 5, Some(0), 0, 4, packed(7), 1) };
    let items = vec![item(4, 1, None, 0, 40, Kind::Group, 0), amount, item(6, 1, None, 40, 2, binary, 2), item(7, 1, None, 42, 5, packed(9), 3)];

    let mut image = [0x00, 0x00, 0x00, 0x0C].repeat(10);
    image.extend([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C]);
    let storage = Storage { size: 47, image, ..Storage::default() };

    let place = |offset, len, kind, subscripts, name, at| Place {
        base: Base::Program,
        offset,
        len,
        kind,
        subscripts,
        odo: None,
        refmod: None,
        name,
        at,
    };
    let subscript = Subscript { stride: 4, value: IntExpr::Item(WS_I), check: Some(10) };
    let places = vec![
        place(40, 2, binary, vec![], 6, 11),
        place(0, 4, packed(7), vec![subscript], 5, 9),
        place(42, 5, packed(9), vec![], 7, 10),
    ];

    let step_i = StepPlan { dmax: 0, store: StorePlan::Binary { digits: 4, scale: 0, signed: false, native: false, name: 6 } };
    let blocks = vec![
        Block {
            ops: vec![Op::SetInt { target: WS_I, value: IntExpr::Const(1) }, Op::Nest, Op::SetTemp(0, IntExpr::Const(3))],
            end: Terminator::Jump(1),
        },
        Block { ops: vec![], end: Terminator::Branch { cond: 0, then: 2, otherwise: 4 } },
        Block { ops: vec![Op::DecTemp(0)], end: Terminator::PerformEnter { range: 0, ret: 3 } },
        Block { ops: vec![Op::Step { var: WS_I, by: 3, plan: step_i }], end: Terminator::Jump(1) },
        Block { ops: vec![Op::Unnest(1)], end: Terminator::End(Ending::StopRun) },
        Block { ops: vec![Op::Arith(0)], end: Terminator::Select(vec![6, 7]) },
        Block { ops: vec![], end: Terminator::Jump(8) },
        Block { ops: vec![], end: Terminator::End(Ending::StopRun) },
        Block { ops: vec![], end: Terminator::ParagraphEnd { next: 2 } },
    ];

    let add = ArithPlan {
        dmax: 2,
        arith: Arith::Compat,
        prepass: vec![WS_AMT_I],
        steps: vec![ArithStep {
            target: WS_TOTAL,
            expr: 2,
            mode: Mode::Fixed,
            store: StorePlan::Packed { digits: 9, scale: 2, signed: true },
            rounded: true,
            probe: vec![WS_AMT_I],
        }],
        remainder: None,
        handled: true,
    };

    let at = |line, col| Pos { file: 0, line, col };
    let debug = Debug {
        sources: vec![1],
        positions: vec![
            at(3, 8),
            at(4, 12),
            at(5, 8),
            at(6, 8),
            at(8, 8),
            at(9, 12),
            at(10, 12),
            at(11, 8),
            at(12, 12),
            at(12, 16),
            at(12, 36),
            at(9, 30),
        ],
        ops: vec![vec![5; 4], vec![5], vec![5; 2], vec![5; 2], vec![5, 6], vec![8; 2], vec![8], vec![8], vec![7]],
    };

    Program {
        id: 0,
        options: ProgramOptions { options: Options::default(), ssrange: true, dynam: false, cards: vec!["SSRANGE".into()] },
        initial: false,
        recursive: false,
        storage,
        items,
        paragraphs: vec![
            Paragraph { name: 2, is_section: false, entry: 0, section_end: 1, at: 4 },
            Paragraph { name: 3, is_section: false, entry: 5, section_end: 1, at: 7 },
        ],
        procedure_start: 0,
        ranges: vec![Range { first: 1, last: 1, kind: RangeKind::Perform }],
        blocks,
        places,
        exprs: vec![
            Expr::Operand(Operand::Load(WS_TOTAL)),
            Expr::Operand(Operand::Load(WS_AMT_I)),
            Expr::Bin(0, BinOp::Add, 1),
            Expr::Operand(Operand::Const(0)),
        ],
        conds: vec![Cond::Counter(0)],
        consts: vec![Const::Number(fixed(1, 1, 0))],
        plans: Plans { arith: vec![add], ..Plans::default() },
        services: Services::default(),
        sql: vec![],
        abends: vec![],
        edits: vec![],
        symbols: symbols.map(String::from).to_vec(),
        debug,
    }
}
