//! Every LIR type through the load-module codec, and a small lowered program encoded whole.

use std::collections::BTreeSet;
use std::fmt;

use ironwork_rt::lir::*;
use ironwork_rt::module::codec::{Decode, Encode, Writer, decode_all};
use ironwork_rt::module::{ModuleError, StringTable};
use ironwork_rt::picture::Sym;
use ironwork_rt::storage::Kind;
use ironwork_rt::vocab::{Figurative, Pos, SignClause, SignPosition};
use numeric::precision::{Fixed, Places};
use numeric::{Arith, Numproc, Options, SortKeys, Trunc, TruncCheck};
use zarch::check::ProgramCheck;
use zarch::hfp::Precision;
use zarch::wide::U256;

fn encoded<T: Encode>(value: &T) -> (Vec<u8>, StringTable) {
    let mut w = Writer::new();
    value.encode(&mut w);
    (w.take(), w.strings().clone())
}

/// Each value decodes to itself, and encodes again to the same bytes and strings.
fn round_trip<T: Encode + Decode + PartialEq + fmt::Debug>(values: &[T]) {
    for value in values {
        let (bytes, strings) = encoded(value);
        let decoded = decode_all::<T>("LIR", &bytes, &strings).unwrap_or_else(|e| panic!("{value:?}: {e}"));
        assert_eq!(&decoded, value);
        assert_eq!(encoded(&decoded), (bytes, strings), "{value:?}");
    }
}

/// `values` round-trip and between them carry every tag below `count`.
fn every_variant<T: Encode + Decode + PartialEq + fmt::Debug>(values: &[T], count: u8) {
    round_trip(values);
    let tags: BTreeSet<u8> = values.iter().map(|v| encoded(v).0[0]).collect();
    assert_eq!(tags, (0..count).collect(), "{}", std::any::type_name::<T>());
}

fn refused<T: Decode + fmt::Debug>(bytes: &[u8], strings: &StringTable) -> (usize, String) {
    match decode_all::<T>("LIR", bytes, strings) {
        Err(ModuleError::Malformed { offset, reason, .. }) => (offset, reason),
        other => panic!("{bytes:02X?} decoded as {other:?}"),
    }
}

fn fixed(value: i128, int: u32, dec: u32) -> Fixed {
    Fixed::new(value, Places::new(int, dec))
}

const SIGN: SignClause = SignClause { position: SignPosition::Trailing, separate: true };
const PACKED: StorePlan = StorePlan::Packed { digits: 9, scale: 2, signed: true };
const ODO: Odo = Odo { object: IntExpr::Item(3), max: 50, element: 12, check: true };
const REFMOD: RefMod = RefMod { start: IntExpr::Const(2), length: Some(IntExpr::Fixed { expr: 4, dmax: 0 }), check: false };

#[test]
fn the_borrowed_vocabulary_round_trips_with_every_tag() {
    let kinds = [
        Kind::Group,
        Kind::Alnum { justified: true },
        Kind::National,
        Kind::Zoned { digits: 5, scale: 2, signed: true, sign: Some(SIGN) },
        Kind::Packed { digits: 7, scale: 0, signed: false },
        Kind::Binary { digits: 9, scale: 1, signed: true, native: true },
        Kind::Float(Precision::Long),
        Kind::NumericEdited { edit: 3, digits: 6, scale: 2, blank_when_zero: true },
        Kind::AlnumEdited { edit: 1 },
        Kind::Pointer,
        Kind::Index,
        Kind::ObjectReference,
        Kind::ProgramPointer,
    ];
    every_variant(&kinds, 13);
    let syms = [
        Sym::Nine,
        Sym::Z,
        Sym::Star,
        Sym::FloatLead('$'),
        Sym::Float('+'),
        Sym::Sign('-'),
        Sym::Currency,
        Sym::Cr,
        Sym::Db,
        Sym::Point,
        Sym::Implied,
        Sym::Insert('/'),
        Sym::Char,
    ];
    every_variant(&syms, 13);
    let figuratives =
        [Figurative::Zero, Figurative::Space, Figurative::HighValue, Figurative::LowValue, Figurative::Quote, Figurative::Null];
    every_variant(&figuratives, 6);
    every_variant(&[SignPosition::Leading, SignPosition::Trailing], 2);
    round_trip(&[SIGN, SignClause { position: SignPosition::Leading, separate: false }]);
    every_variant(&[Precision::Short, Precision::Long, Precision::Extended], 3);
    let checks = [
        ProgramCheck::Specification,
        ProgramCheck::Data,
        ProgramCheck::FixedPointOverflow,
        ProgramCheck::FixedPointDivide,
        ProgramCheck::DecimalOverflow,
        ProgramCheck::DecimalDivide,
        ProgramCheck::HfpExponentOverflow,
        ProgramCheck::HfpExponentUnderflow,
        ProgramCheck::HfpSignificance,
        ProgramCheck::HfpDivide,
    ];
    every_variant(&checks, 10);
    every_variant(&[Arith::Compat, Arith::Extend], 2);
    every_variant(&[Trunc::Std, Trunc::Opt, Trunc::Bin], 3);
    every_variant(&[Numproc::Nopfd, Numproc::Pfd], 2);
    every_variant(&[TruncCheck::Report, TruncCheck::Silent], 2);
    every_variant(&[SortKeys::Dfsort, SortKeys::Strict], 2);
    round_trip(&[Options::default(), Options { arith: Arith::Extend, codepage: 37, dbcs: false, ..Options::default() }]);
    let wide = Fixed { negative: true, magnitude: U256 { hi: u128::MAX, lo: 1 << 100 }, places: Places::new(70, 7) };
    round_trip(&[fixed(0, 1, 0), fixed(-1_234_567, 5, 2), fixed(i128::MAX, 39, 0), wide]);
}

#[test]
fn kinds_and_options_have_load_module_s_bytes() {
    assert_eq!(encoded(&Kind::Zoned { digits: 5, scale: 2, signed: true, sign: Some(SIGN) }).0, [3, 5, 2, 1, 1, 1, 1]);
    assert_eq!(encoded(&Kind::Float(Precision::Extended)).0, [6, 2]);
    let options = Options { arith: Arith::Extend, trunc: Trunc::Opt, ..Options::default() };
    assert_eq!(encoded(&options).0, [0x01, 0x01, 0x00, 0xF4, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x01]);
    let (bytes, strings) = encoded(&(7u8, Options { codepage: 999, ..options }));
    let reason = "CODEPAGE(999) is not a page the tables carry".to_owned();
    assert_eq!(refused::<(u8, Options)>(&bytes, &strings), (1, reason));
}

#[test]
fn abend_codes_round_trip_with_every_tag() {
    let codes = [
        AbendCode::Check(ProgramCheck::Data),
        AbendCode::Protection,
        AbendCode::ModuleNotFound,
        AbendCode::Io(FileStatus::NotFound),
        AbendCode::Cics("AEIM".into()),
        AbendCode::User("U4038".into()),
        AbendCode::Ironwork,
        AbendCode::Exec,
        AbendCode::Sql,
        AbendCode::SqlReplay,
        AbendCode::Java,
        AbendCode::Signal(Signal::SortStopped),
    ];
    every_variant(&codes, 12);
    let signals = [Signal::DivideByZero, Signal::StopRun, Signal::GoBack, Signal::SortStopped, Signal::ClosedOutput];
    every_variant(&signals, 5);
    let statuses = [
        FileStatus::Success,
        FileStatus::SuccessDuplicate,
        FileStatus::SuccessWrongLength,
        FileStatus::SuccessOptional,
        FileStatus::AtEnd,
        FileStatus::RelativeKeyOverflow,
        FileStatus::SequenceError,
        FileStatus::DuplicateKey,
        FileStatus::NotFound,
        FileStatus::BoundaryViolation,
        FileStatus::PermanentError,
        FileStatus::FileNotFound,
        FileStatus::OpenModeUnsupported,
        FileStatus::AlreadyOpen,
        FileStatus::NotOpen,
        FileStatus::NoPriorRead,
        FileStatus::RecordLengthChanged,
        FileStatus::NoNextRecord,
        FileStatus::NotOpenInput,
        FileStatus::NotOpenOutput,
        FileStatus::NotOpenInputOutput,
    ];
    every_variant(&statuses, 21);
    round_trip(&[AbendText { code: AbendCode::Exec, message: 9 }]);
}

#[test]
fn program_shape_round_trips() {
    let options = ProgramOptions { options: Options::default(), ssrange: true, dynam: true, cards: vec!["TRUNC(OPT)".into(), "SSR".into()] };
    round_trip(&[options]);
    let storage = Storage {
        size: 3,
        image: vec![0x40, 0xF0, 0x0C],
        local_image: vec![0],
        init_reports: vec![4],
        init_abend: Some(0),
        linkage: vec![100, 8],
        using: vec![0, 1],
        returning: Some(1),
        file_areas: vec![(0, 80), (80, 132)],
    };
    round_trip(&[Storage::default(), storage]);
    let item = Item {
        name: Some(2),
        level: 5,
        parent: Some(0),
        offset: 4,
        size: 12,
        occurs: 50,
        dims: vec![(12, 50)],
        kind: Kind::Group,
        local: false,
        linkage: Some(1),
        redefines: None,
        depending_on: Some(3),
        keys: vec![(true, 4), (false, 5)],
        at: 7,
    };
    let filler = Item { name: None, parent: None, dims: vec![], depending_on: None, keys: vec![], redefines: Some(6), ..item.clone() };
    round_trip(&[item, filler]);
    round_trip(&[Paragraph { name: 1, is_section: true, entry: 0, section_end: 3, at: 2 }]);
    round_trip(&[Block { ops: vec![], end: Terminator::Jump(1) }, Block { ops: vec![Op::Nest, Op::Arith(0)], end: Terminator::Abend(0) }]);
    let plans = Plans {
        arith: vec![ArithPlan { dmax: 0, arith: Arith::Extend, prepass: vec![], steps: vec![], remainder: None, handled: false }],
        init: vec![InitPlan::Placeholder],
        display: vec![DisplayPlan::Placeholder],
        inspect: vec![InspectPlan::Placeholder],
        string: vec![StringPlan::Placeholder],
        unstring: vec![UnstringPlan::Placeholder],
        search_all: vec![SearchAllPlan::Placeholder],
        function: vec![FunctionPlan { func: Func::Trim, args: vec![0], side: Some(TrimSide::Leading), refmod: None, at: 1 }],
    };
    round_trip(&[Plans::default(), plans]);
    let services = Services {
        file_ops: vec![FileOp::Placeholder],
        files: vec![FileDesc::Placeholder],
        calls: vec![CallPlan::Placeholder],
        sorts: vec![SortPlan::Placeholder],
        releases: vec![ReleasePlan::Placeholder],
        returns: vec![ReturnPlan::Placeholder],
        invokes: vec![InvokePlan {
            receiver: Receiver::SelfRef,
            method: MethodName::New,
            args: vec![],
            returning: None,
            on_exception: false,
            not_on_exception: false,
        }],
        cics: vec![CicsCommand::Placeholder],
        sqlca: Sqlca::Placeholder,
    };
    round_trip(&[Services::default(), services]);
}

#[test]
fn a_slab_size_that_differs_from_its_image_is_malformed() {
    let storage = Storage { size: 4, image: vec![0; 3], ..Storage::default() };
    let (bytes, strings) = encoded(&storage);
    assert_eq!(refused::<Storage>(&bytes, &strings), (0, "an image of 3 bytes for a slab of 4".into()));
}

#[test]
fn places_round_trip_with_every_base() {
    let bases =
        [Base::Program, Base::Local, Base::Linkage(2), Base::ReturnCode, Base::Eib, Base::SelfRef, Base::JniEnv];
    every_variant(&bases, 7);
    let subscript = Subscript { stride: 12, value: IntExpr::Item(1), check: Some(50) };
    round_trip(&[subscript, Subscript { stride: 4, value: IntExpr::Const(-1), check: None }]);
    round_trip(&[ODO, Odo { check: false, ..ODO }]);
    round_trip(&[REFMOD, RefMod { length: None, ..REFMOD }]);
    let place = Place {
        base: Base::Linkage(0),
        offset: 16,
        len: 12,
        kind: Kind::Alnum { justified: false },
        subscripts: vec![subscript, subscript],
        odo: Some(ODO),
        refmod: Some(REFMOD),
        name: 3,
        at: 9,
    };
    let bare = Place { base: Base::ReturnCode, subscripts: vec![], odo: None, refmod: None, ..place.clone() };
    round_trip(&[place, bare]);
}

#[test]
fn values_and_conditions_round_trip_with_every_tag() {
    let operands = [Operand::Load(1), Operand::Const(2), Operand::LengthOf(3), Operand::AddressOf(4), Operand::Function(5)];
    every_variant(&operands, 5);
    let consts = [
        Const::Bytes(vec![0xC1, 0x40]),
        Const::National(vec![0x00, 0x41]),
        Const::Number(fixed(-125, 1, 2)),
        Const::Figurative(Figurative::HighValue),
        Const::All(vec![]),
    ];
    every_variant(&consts, 5);
    every_variant(&[IntExpr::Const(i64::MIN), IntExpr::Item(0), IntExpr::Fixed { expr: 2, dmax: 3 }], 3);
    let exprs = [Expr::Operand(Operand::Load(0)), Expr::Neg(0), Expr::Bin(0, BinOp::Div, 1), Expr::Pow(1, IntExpr::Const(2))];
    every_variant(&exprs, 4);
    let conds = [
        Cond::Rel { a: Comparand::Operand(Operand::Load(0)), op: RelOp::Ge, b: Comparand::Expr(3), how: Compare::Fixed },
        Cond::Class { place: 1, test: ByteClass::Packed { signed: true } },
        Cond::Sign { value: 2, test: SignTest::Negative },
        Cond::Name { subject: 0, values: vec![(1, None), (2, Some(3))], how: Compare::Alphanumeric },
        Cond::Not(0),
        Cond::And(0, 1),
        Cond::Or(1, 0),
        Cond::Counter(0),
        Cond::InTable { index: 4, count: Count::Odo(ODO) },
        Cond::Sql(SqlTest::NotFound),
    ];
    every_variant(&conds, 10);
    every_variant(&[Comparand::Operand(Operand::Const(0)), Comparand::Expr(1)], 2);
    let compares = [
        Compare::PackedPfd,
        Compare::Address,
        Compare::Float,
        Compare::Fixed,
        Compare::National,
        Compare::Alphanumeric,
        Compare::Refused(0),
    ];
    every_variant(&compares, 7);
    let classes = [ByteClass::Packed { signed: false }, ByteClass::Zoned { signed: true }, ByteClass::Digits, ByteClass::Alphabetic];
    every_variant(&classes, 4);
    every_variant(&[SignTest::Positive, SignTest::Negative, SignTest::Zero], 3);
    every_variant(&[Count::Fixed(10), Count::Odo(ODO)], 2);
    every_variant(&[BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Pow], 5);
    every_variant(&[RelOp::Eq, RelOp::Ne, RelOp::Lt, RelOp::Le, RelOp::Gt, RelOp::Ge], 6);
    every_variant(&[SqlTest::Error, SqlTest::NotFound, SqlTest::Warning], 3);
}

#[test]
fn arithmetic_plans_round_trip_with_every_tag() {
    let stores = [
        StorePlan::Zoned { digits: 5, scale: 0, signed: true, sign: Some(SIGN) },
        PACKED,
        StorePlan::Binary { digits: 4, scale: 0, signed: false, native: true, name: 2 },
        StorePlan::NumericEdited { edit: 0, digits: 7, scale: 2, blank_when_zero: false },
        StorePlan::Float(Precision::Short),
        StorePlan::Index,
        StorePlan::Refused(1),
    ];
    every_variant(&stores, 7);
    every_variant(&[Mode::Fixed, Mode::Float(Precision::Extended)], 2);
    let step = ArithStep { target: 1, expr: 2, mode: Mode::Fixed, store: PACKED, rounded: true, probe: vec![3, 4] };
    round_trip(std::slice::from_ref(&step));
    let remainder = RemainderPlan { target: 5, dividend: 0, divisor: 1, quotient_scale: 2, store: PACKED };
    round_trip(&[remainder]);
    round_trip(&[StepPlan { dmax: 2, store: stores[2] }]);
    let plan = ArithPlan {
        dmax: 2,
        arith: Arith::Compat,
        prepass: vec![1, 3, 5],
        steps: vec![step.clone(), ArithStep { mode: Mode::Float(Precision::Long), probe: vec![], ..step }],
        remainder: Some(remainder),
        handled: true,
    };
    round_trip(&[plan]);
}

#[test]
fn control_flow_round_trips_with_every_tag() {
    let move_plan = MovePlan::Numeric { from: NumericFrom::Value, store: PACKED };
    let ops = [
        Op::Move { from: Operand::Const(0), to: 1, plan: move_plan },
        Op::Initialize { target: 1, plan: 0 },
        Op::Arith(0),
        Op::SetAddress { record: 1, address: Operand::AddressOf(2) },
        Op::SetUpDown { target: 1, by: IntExpr::Const(-2), down: true, plan: StepPlan { dmax: 0, store: StorePlan::Index } },
        Op::Step { var: 1, by: 0, plan: StepPlan { dmax: 0, store: PACKED } },
        Op::SetInt { target: 1, value: IntExpr::Const(1) },
        Op::Inspect(0),
        Op::String(0),
        Op::Unstring(0),
        Op::SearchAll(0),
        Op::Nest,
        Op::Unnest(2),
        Op::SetTemp(1, IntExpr::Item(4)),
        Op::DecTemp(1),
        Op::Display(0),
        Op::Accept { target: 1, from: AcceptFrom::Date { four_digit_year: true }, plan: MovePlan::Alnum { image: Image::Bytes, justified: false } },
        Op::File(0),
        Op::Call(0),
        Op::Cancel(Operand::Load(3)),
        Op::Sort(0),
        Op::Release(0),
        Op::Return(0),
        Op::Report(ReportOp::Placeholder),
        Op::Invoke(0),
        Op::Cics(0),
        Op::Sql(1),
    ];
    every_variant(&ops, 27);
    every_variant(&[Step::Next, Step::Arm(2), Step::GoTo(3), Step::End(Ending::Goback)], 4);
    let terminators = [
        Terminator::Jump(1),
        Terminator::Branch { cond: 0, then: 1, otherwise: 2 },
        Terminator::Select(vec![3, 4, 5]),
        Terminator::ParagraphEnd { next: 2 },
        Terminator::GoTo(0),
        Terminator::Switch { value: IntExpr::Item(1), targets: vec![0, 2], otherwise: 6 },
        Terminator::PerformEnter { range: 0, ret: 7 },
        Terminator::ExitProgram { next: 8 },
        Terminator::End(Ending::StopRun),
        Terminator::Abend(0),
    ];
    every_variant(&terminators, 10);
    round_trip(&[Range { first: 1, last: 3, kind: RangeKind::Perform }, Range { first: 4, last: 2, kind: RangeKind::SortProcedure }]);
    every_variant(&[RangeKind::Perform, RangeKind::SortProcedure, RangeKind::UseBeforeReporting], 3);
    let kinds = [FrameKind::Main, FrameKind::Perform, FrameKind::SortProcedure, FrameKind::UseBeforeReporting { at: 4 }];
    every_variant(&kinds, 4);
    round_trip(&[Frame { first: 0, last: 9, kind: FrameKind::Main, ret: 0, depth: 0 }, Frame { first: 2, last: 3, kind: kinds[3], ret: 5, depth: 2 }]);
    every_variant(&[Ending::Goback, Ending::StopRun, Ending::EndOfProgram], 3);
    let accepts = [
        AcceptFrom::Sysin,
        AcceptFrom::Date { four_digit_year: false },
        AcceptFrom::Day { four_digit_year: true },
        AcceptFrom::DayOfWeek,
        AcceptFrom::Time,
    ];
    every_variant(&accepts, 5);
}

#[test]
fn statement_payloads_round_trip_with_every_tag() {
    let moves = [
        MovePlan::Alnum { image: Image::Digits { digits: 5 }, justified: true },
        MovePlan::AlnumEdited { image: Image::All, edit: 2, positions: 8 },
        MovePlan::National(NationalFrom::Decoded),
        MovePlan::Numeric { from: NumericFrom::DeEdit { edit: 1, digits: 7, scale: 2 }, store: PACKED },
        MovePlan::Float { from: FloatFrom::Fixed, precision: Precision::Short },
        MovePlan::Address,
        MovePlan::Index,
        MovePlan::Refused(3),
    ];
    every_variant(&moves, 8);
    every_variant(&[Image::Bytes, Image::All, Image::Figurative, Image::Digits { digits: 3 }], 4);
    every_variant(&[NationalFrom::Units, NationalFrom::Decoded, NationalFrom::Figurative], 3);
    let numeric = [
        NumericFrom::Value,
        NumericFrom::PackedCopy,
        NumericFrom::Float,
        NumericFrom::Zero,
        NumericFrom::Fill,
        NumericFrom::Zoned,
        NumericFrom::DeEdit { edit: 0, digits: 5, scale: 0 },
    ];
    every_variant(&numeric, 7);
    every_variant(&[FloatFrom::Float, FloatFrom::Fixed, FloatFrom::Zero], 3);
    let funcs = [
        Func::Char,
        Func::Ord,
        Func::NationalOf,
        Func::Length,
        Func::UpperCase,
        Func::LowerCase,
        Func::Reverse,
        Func::CurrentDate,
        Func::Numval,
        Func::NumvalC,
        Func::Trim,
        Func::Mod,
        Func::Rem,
        Func::Integer,
        Func::IntegerPart,
        Func::Abs,
        Func::Min,
        Func::Max,
        Func::IntegerOfDate,
        Func::DateOfInteger,
    ];
    every_variant(&funcs, 20);
    every_variant(&[TrimSide::Leading, TrimSide::Trailing], 2);
    round_trip(&[FunctionPlan { func: Func::Max, args: vec![0, 1, 2], side: None, refmod: Some(REFMOD), at: 5 }]);
    every_variant(&[Receiver::SelfRef, Receiver::Super, Receiver::Class(1), Receiver::Object(2)], 4);
    every_variant(&[MethodName::New, MethodName::Named(3), MethodName::Dynamic(4)], 3);
    let invoke = InvokePlan {
        receiver: Receiver::Class(1),
        method: MethodName::Named(2),
        args: vec![(Operand::Load(0), 3), (Operand::Const(1), 4)],
        returning: Some((5, 6)),
        on_exception: true,
        not_on_exception: false,
    };
    round_trip(&[invoke]);
    round_trip(&[InitPlan::Placeholder]);
    round_trip(&[DisplayPlan::Placeholder]);
    round_trip(&[InspectPlan::Placeholder]);
    round_trip(&[StringPlan::Placeholder]);
    round_trip(&[UnstringPlan::Placeholder]);
    round_trip(&[SearchAllPlan::Placeholder]);
    round_trip(&[FileOp::Placeholder]);
    round_trip(&[FileDesc::Placeholder]);
    round_trip(&[CallPlan::Placeholder]);
    round_trip(&[SortPlan::Placeholder]);
    round_trip(&[ReleasePlan::Placeholder]);
    round_trip(&[ReturnPlan::Placeholder]);
    round_trip(&[ReportOp::Placeholder]);
    round_trip(&[CicsCommand::Placeholder]);
    round_trip(&[SqlEntry::Placeholder]);
    round_trip(&[Sqlca::Placeholder]);
}

#[test]
fn debug_positions_are_differences_from_the_one_before() {
    let debug = Debug {
        sources: vec![1],
        positions: vec![Pos { file: 0, line: 5, col: 8 }, Pos { file: 1, line: 2, col: 12 }],
        ops: vec![vec![0, 1]],
    };
    assert_eq!(encoded(&debug).0, [1, 1, 2, 0, 10, 16, 2, 5, 8, 1, 2, 0, 1]);
    let far = Pos { file: u16::MAX, line: u32::MAX, col: 0 };
    round_trip(&[Debug::default(), debug, Debug { sources: vec![], positions: vec![far, Pos::default(), far], ops: vec![] }]);
    let none = StringTable::default();
    assert_eq!(refused::<Debug>(&[0, 1, 0, 1, 0, 0], &none), (3, "a line of -1".into()));
    assert_eq!(refused::<Debug>(&[0, 1, 0x80, 0x80, 0x08, 0, 0, 0], &none), (2, "a file of 65536".into()));
}

const WS_I: PlaceId = 0;
const WS_AMT_I: PlaceId = 1;
const WS_TOTAL: PlaceId = 2;

/// MAIN sets WS-I to 1 and performs ADD-PARA three times, stepping WS-I; ADD-PARA does
/// `ADD WS-AMT (WS-I) TO WS-TOTAL ROUNDED ON SIZE ERROR STOP RUN`.
fn payroll() -> Program {
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

#[test]
fn a_small_program_round_trips_byte_identically() {
    let program = payroll();
    let (bytes, strings) = encoded(&program);
    assert_eq!(strings.iter().collect::<Vec<_>>()[..3], ["SSRANGE", "PAYROLL", "PAYROLL.cbl"]);
    let decoded = decode_all::<Program>("LIR", &bytes, &strings).unwrap();
    assert_eq!(decoded, program);
    assert_eq!(encoded(&decoded), (bytes.clone(), strings.clone()));
    for len in 0..bytes.len() {
        assert!(decode_all::<Program>("LIR", &bytes[..len], &strings).is_err(), "{len}");
    }
}
