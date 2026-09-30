//! Every LIR type through the load-module codec, and a small lowered program encoded whole.

use std::collections::BTreeSet;
use std::fmt;

mod common;

use common::payroll;
use ironwork_rt::abend::{AbendCode, Ending, FileStatus, Signal};
use ironwork_rt::files::Format;
use ironwork_rt::lir::*;
use ironwork_rt::module::codec::{Decode, Encode, Writer, decode_all};
use ironwork_rt::module::{ModuleError, StringTable};
use ironwork_rt::picture::Sym;
use ironwork_rt::sql::{HostType, fingerprint};
use ironwork_rt::storage::Kind;
use ironwork_rt::vocab::{AcceptFrom, BinOp, Figurative, InspectMode, OpenMode, Pos, RelOp, SignClause, SignPosition};
use numeric::precision::{Fixed, Places};
use numeric::options::{Compile, FastsrtAdvPrint, Stop, Warnings};
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
const REFMOD: RefMod = RefMod { start: IntExpr::Const(2), length: Some(IntExpr::Fixed { expr: 4, dmax: 0, prepass: Vec::new() }), check: false };
const ALNUM: MovePlan = MovePlan::Alnum { image: Image::Bytes, justified: false };
const FILL: MovePlan = MovePlan::Alnum { image: Image::Figurative, justified: false };
const TALLY: StepPlan = StepPlan { dmax: 0, store: PACKED };
const FLOAT_EXPR: Comparand = Comparand::Expr { expr: 1, dmax: 0, mode: Mode::Float(Precision::Long), prepass: Vec::new() };
const INTEGER: HostType = HostType::Integer { signed: true };

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

/// Every field set away from its default, so a field the codec skipped decodes as the default and fails.
#[test]
fn options_round_trip_with_every_field_off_its_default() {
    let every = Options {
        arith: Arith::Extend,
        trunc: Trunc::Bin,
        numproc: Numproc::Pfd,
        codepage: 1047,
        trunc_check: TruncCheck::Silent,
        fastsrt: true,
        fastsrt_adv_print: FastsrtAdvPrint::Include,
        sort_keys: SortKeys::Strict,
        adv: false,
        thread: true,
        dll: true,
        rent: false,
        dbcs: false,
        warnings: Warnings::Block,
        compile: Some(Compile::Until(Stop::E)),
        dynam: true,
        debug: true,
    };
    round_trip(&[every]);
    let each = [
        Options { fastsrt_adv_print: FastsrtAdvPrint::Include, ..Options::default() },
        Options { warnings: Warnings::Block, ..Options::default() },
        Options { compile: Some(Compile::SyntaxOnly), ..Options::default() },
        Options { dynam: true, ..Options::default() },
        Options { debug: true, ..Options::default() },
    ];
    round_trip(&each);
    for options in each {
        assert_ne!(encoded(&options).0, encoded(&Options::default()).0);
    }
    every_variant(&[FastsrtAdvPrint::Exclude, FastsrtAdvPrint::Include], 2);
    every_variant(&[Warnings::Proceed, Warnings::Block], 2);
    every_variant(&[Compile::Full, Compile::Until(Stop::W), Compile::SyntaxOnly], 3);
    every_variant(&[Stop::W, Stop::E, Stop::S], 3);
}

#[test]
fn kinds_and_options_have_load_module_s_bytes() {
    assert_eq!(encoded(&Kind::Zoned { digits: 5, scale: 2, signed: true, sign: Some(SIGN) }).0, [3, 5, 2, 1, 1, 1, 1]);
    assert_eq!(encoded(&Kind::Float(Precision::Extended)).0, [6, 2]);
    let options = Options { arith: Arith::Extend, trunc: Trunc::Opt, ..Options::default() };
    assert_eq!(encoded(&options).0, [0x01, 0x01, 0x00, 0xF4, 0x08, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00]);
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
    let signals = [Signal::StopRun, Signal::GoBack, Signal::SortStopped, Signal::ClosedOutput, Signal::DeclarativeExit];
    every_variant(&signals, 5);
    every_variant(&[Ending::Goback, Ending::StopRun, Ending::EndOfProgram], 3);
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
    round_trip(&[AbendText { code: AbendCode::Exec, message: 9, at: None }, AbendText { code: AbendCode::Ironwork, message: 2, at: Some(4) }]);
}

/// A sequence that puts X'C1' and X'81' first, sharing a position, then every other byte in EBCDIC order.
fn letters_first() -> Sequence {
    let order: Vec<u8> = [0xC1, 0x81].into_iter().chain((0..=255u8).filter(|b| ![0xC1, 0x81].contains(b))).collect();
    let mut positions = Box::new([0u8; 256]);
    for (k, &b) in order.iter().enumerate() {
        positions[usize::from(b)] = k.saturating_sub(1) as u8;
    }
    let characters = std::iter::once(0xC1).chain(order[2..].iter().copied()).collect();
    Sequence { positions, characters, high_value: 0xFF, low_value: 0xC1 }
}

#[test]
fn program_shape_round_trips() {
    let cards = vec!["TRUNC(OPT)".into(), "SSR".into()];
    let options = ProgramOptions { options: Options::default(), ssrange: true, cards, collating: Collating::Native };
    let sequenced = ProgramOptions { collating: Collating::Sequence(letters_first()), ..options.clone() };
    round_trip(&[options, sequenced]);
    every_variant(&[Collating::Native, Collating::Sequence(letters_first())], 2);
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
    round_trip(&[Paragraph { name: 1, is_section: true, entry: 0, section_end: 3, priority: 50, at: 2 }]);
    round_trip(&[Block { ops: vec![], end: Terminator::Jump(1) }, Block { ops: vec![Op::Nest, Op::Arith(0)], end: Terminator::Abend(0) }]);
    let plans = Plans {
        arith: vec![ArithPlan { dmax: 0, arith: Arith::Extend, prepass: vec![], steps: vec![], remainder: None, handled: false }],
        init: vec![InitPlan { fields: vec![InitField { offset: 0, len: 2, value: Figurative::Null, store: FILL }] }],
        display: vec![DisplayPlan { items: vec![DisplayItem::Text(0)], no_advancing: false }],
        inspect: vec![InspectPlan { target: 0, tallying: vec![], replacing: vec![], converting: None }],
        string: vec![StringPlan { into: 0, pointer: None, sources: vec![] }],
        unstring: vec![UnstringPlan { source: 0, pointer: None, delimiters: vec![], into: vec![], tallying: None }],
        search_all: vec![SearchAllPlan { index: 0, store: StorePlan::Index, count: Count::Fixed(5), keys: vec![] }],
        function: vec![FunctionPlan {
            func: Func::Trim,
            args: vec![Comparand::Operand(Operand::Load(0))],
            integer: None,
            side: Some(TrimSide::Leading),
            refmod: None,
            arity: None,
            at: 1,
        }],
    };
    round_trip(&[Plans::default(), plans]);
    let services = Services {
        file_ops: vec![FileOp { file: 0, verb: FileVerb::Delete, phrase: Some(Phrase { on: true, not_on: false }), end_of_page: None }],
        files: vec![master()],
        calls: vec![CallPlan {
            target: CallTarget::Named { name: 1, le: None },
            args: vec![],
            returning: None,
            on_exception: false,
            not_on_exception: false,
        }],
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
        sqlca: Sqlca { fields: vec![(SqlcaField::Code, 0, INTEGER)] },
        entries: vec![EntryPoint { name: 2, paragraph: 1, block: 4, using: vec![0, 1] }],
        class: Some(Box::new(account())),
    };
    round_trip(&[Services::default(), services]);
}

/// A class with FACTORY data, OBJECT data and a method of each, its programs the sample program.
fn account() -> Class {
    let part = ClassPart { data: payroll(), records: vec![0, 40] };
    let method = Method { name: 3, factory: false, params: vec![4], returns: Some(5), own_records: 1, code: payroll() };
    let open = Method { factory: true, params: vec![], returns: None, own_records: 0, ..method.clone() };
    Class { external: 1, parent: 2, factory: Some(part.clone()), object: Some(part), methods: vec![method, open] }
}

#[test]
fn class_definitions_and_entry_points_round_trip() {
    let bare = Class { factory: None, object: None, methods: vec![], ..account() };
    round_trip(&[account(), bare]);
    round_trip(&[EntryPoint { name: 0, paragraph: 0, block: 0, using: vec![] }, EntryPoint { name: 7, paragraph: 2, block: 9, using: vec![3] }]);
    let mut program = payroll();
    program.services.class = Some(Box::new(account()));
    round_trip(std::slice::from_ref(&program));
}

#[test]
fn a_class_inside_a_class_s_method_is_malformed() {
    let mut inner = payroll();
    inner.services.class = Some(Box::new(Class { factory: None, object: None, methods: vec![], ..account() }));
    let mut class = account();
    class.methods[1].code = inner;
    let (bytes, strings) = encoded(&class);
    assert_eq!(refused::<Class>(&bytes, &strings).1, "a class definition inside a class definition");
    round_trip(&[account()]);
}

#[test]
fn a_slab_size_that_differs_from_its_image_is_malformed() {
    let storage = Storage { size: 4, image: vec![0; 3], ..Storage::default() };
    let (bytes, strings) = encoded(&storage);
    assert_eq!(refused::<Storage>(&bytes, &strings), (0, "an image of 3 bytes for a slab of 4".into()));
}

#[test]
fn a_collating_sequence_that_is_not_one_is_malformed() {
    let refusal = |sequence: Sequence| {
        let (bytes, strings) = encoded(&sequence);
        refused::<Sequence>(&bytes, &strings).1
    };
    let short = Sequence { characters: letters_first().characters[..254].to_vec(), ..letters_first() };
    assert_eq!(refusal(short), "X'FF' at position 254 of a sequence of 254");
    let mut characters = letters_first().characters;
    characters[1] = 0x81;
    assert_eq!(refusal(Sequence { characters, ..letters_first() }), "character 1 of the sequence, X'81', is at position 0");
    assert_eq!(refusal(Sequence { high_value: 0xC1, ..letters_first() }), "HIGH-VALUE X'C1' or LOW-VALUE X'C1' is not at the sequence's end");
    let also_first = Sequence { characters: [&[0x81][..], &letters_first().characters[1..]].concat(), low_value: 0x81, ..letters_first() };
    round_trip(&[also_first]);
}

#[test]
fn places_round_trip_with_every_base() {
    let bases =
        [Base::Program, Base::Local, Base::Linkage(2), Base::ReturnCode, Base::Eib, Base::SelfRef, Base::JniEnv];
    every_variant(&bases, 7);
    let subscript = Subscript { stride: 12, value: IntExpr::Item(1), check: Some(50) };
    round_trip(&[subscript.clone(), Subscript { stride: 4, value: IntExpr::Const(-1), check: None }]);
    round_trip(&[ODO, Odo { check: false, ..ODO }]);
    round_trip(&[REFMOD, RefMod { length: None, ..REFMOD }]);
    let place = Place {
        base: Base::Linkage(0),
        offset: 16,
        len: 12,
        kind: Kind::Alnum { justified: false },
        subscripts: vec![subscript.clone(), subscript],
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
    every_variant(&[IntExpr::Const(i64::MIN), IntExpr::Item(0), IntExpr::Fixed { expr: 2, dmax: 3, prepass: vec![1, 4] }], 3);
    let exprs = [Expr::Operand(Operand::Load(0)), Expr::Neg(0), Expr::Bin(0, BinOp::Div, 1), Expr::Pow(1, IntExpr::Const(2))];
    every_variant(&exprs, 4);
    let conds = [
        Cond::Rel { a: Comparand::Operand(Operand::Load(0)), op: RelOp::Ge, b: Comparand::Expr { expr: 3, dmax: 2, mode: Mode::Fixed, prepass: vec![0, 5] }, how: Compare::Fixed },
        Cond::Class { place: 1, test: ByteClass::Packed { signed: true } },
        Cond::Sign { value: Comparand::Operand(Operand::Load(2)), test: SignTest::Negative },
        Cond::Sign { value: FLOAT_EXPR, test: SignTest::Zero },
        Cond::Name { subject: 0, values: vec![(1, None), (2, Some(3))], how: Compare::Alphanumeric },
        Cond::Not(0),
        Cond::And(0, 1),
        Cond::Or(1, 0),
        Cond::Counter(0),
        Cond::InTable { index: 4, count: Count::Odo(ODO) },
        Cond::Sql(SqlTest::NotFound),
    ];
    every_variant(&conds, 10);
    every_variant(&[Comparand::Operand(Operand::Const(0)), FLOAT_EXPR], 2);
    let compares = [
        Compare::PackedPfd,
        Compare::Address,
        Compare::Float,
        Compare::Fixed,
        Compare::National,
        Compare::Alphanumeric,
        Compare::Refused(0),
        Compare::References,
    ];
    every_variant(&compares, 8);
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
        Op::SetAddress { records: vec![1, 0], address: Operand::AddressOf(2) },
        Op::SetUpDown { by: IntExpr::Const(-2), down: true, targets: vec![(1, UpDown::Number(StepPlan { dmax: 0, store: StorePlan::Index })), (2, UpDown::Pointer)] },
        Op::Step { var: 1, by: 0, plan: StepPlan { dmax: 0, store: PACKED }, prepass: vec![2, 3] },
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
        Op::Alter { para: 2, to: 5 },
        Op::EnterSegment(50),
        Op::SetSegment(0),
    ];
    every_variant(&ops, 30);
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
        Terminator::AlteredGoTo { para: 3, otherwise: 9 },
    ];
    every_variant(&terminators, 11);
    round_trip(&[Range { first: 1, last: 3, kind: RangeKind::Perform }, Range { first: 4, last: 2, kind: RangeKind::SortProcedure }]);
    every_variant(&[RangeKind::Perform, RangeKind::SortProcedure, RangeKind::UseBeforeReporting], 3);
    let kinds = [FrameKind::Main, FrameKind::Perform, FrameKind::SortProcedure, FrameKind::UseBeforeReporting { at: 4 }];
    every_variant(&kinds, 4);
    let main = Frame { first: 0, last: 9, kind: FrameKind::Main, ret: 0, depth: 0, temps: vec![] };
    round_trip(&[main, Frame { first: 2, last: 3, kind: kinds[3], ret: 5, depth: 2, temps: vec![3, 0, -1] }]);
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
    every_variant(Func::ALL, 21);
    for &func in Func::ALL {
        assert_eq!(Func::named(func.name()), Some(func));
    }
    assert_eq!((Func::named("NUMVAL-C"), Func::named("NUMVALC")), (Some(Func::NumvalC), None));
    assert_eq!((Func::Random.arity(), Func::Max.arity().contains(&40)), (0..=1, true));
    every_variant(&[TrimSide::Leading, TrimSide::Trailing], 2);
    let args = vec![Comparand::Operand(Operand::Load(0)), FLOAT_EXPR, Comparand::Operand(Operand::Function(1))];
    round_trip(&[
        FunctionPlan { func: Func::Max, args, integer: None, side: None, refmod: Some(REFMOD), arity: None, at: 5 },
        FunctionPlan { func: Func::Char, args: vec![], integer: Some(IntExpr::Item(2)), side: None, refmod: None, arity: Some(3), at: 6 },
    ]);
    every_variant(&[UpDown::Pointer, UpDown::Number(TALLY), UpDown::Refused(2)], 3);
    every_variant(&[Receiver::SelfRef, Receiver::Super, Receiver::Class { name: 1, external: 7 }, Receiver::Object(2)], 4);
    every_variant(&[MethodName::New, MethodName::Named(3), MethodName::Dynamic(4)], 3);
    let invoke = InvokePlan {
        receiver: Receiver::Class { name: 1, external: 1 },
        method: MethodName::Named(2),
        args: vec![(Operand::Load(0), 3), (Operand::Const(1), 4)],
        returning: Some((5, 6)),
        on_exception: true,
        not_on_exception: false,
    };
    round_trip(&[invoke]);
    round_trip(&[SortPlan::Placeholder]);
    round_trip(&[ReleasePlan::Placeholder]);
    round_trip(&[ReturnPlan::Placeholder]);
    round_trip(&[ReportOp::Placeholder]);
    round_trip(&[CicsCommand::Placeholder]);
}

/// An indexed file with FILE STATUS, an alternate key, LINAGE and a print file's carriage.
fn master() -> FileDesc {
    let span = RecordSpan { offset: 0, len: 6 };
    FileDesc {
        name: 1,
        assign: 2,
        organization: Organization::Indexed,
        access: Access::Dynamic,
        optional: true,
        format: Format::Variable,
        status: Some((3, ALNUM)),
        keys: Some(IndexKeys { prime: span, alternates: vec![(RecordSpan { offset: 6, len: 20 }, true)] }),
        relative: None,
        linage: Some(Linage { lines: IntExpr::Const(60), footing: Some(IntExpr::Item(4)), top: None, bottom: Some(IntExpr::Const(3)), counter: Some((5, PACKED)) }),
        carriage: Some(Carriage { machine: true, reserved: false }),
        sort: false,
    }
}

#[test]
fn file_declarations_and_statements_round_trip_with_every_tag() {
    let relative = RelativeKey { place: 7, value: IntExpr::Item(7), store: PACKED, digits: Some(5) };
    let numbered = FileDesc { organization: Organization::Relative, keys: None, relative: Some(relative), linage: None, carriage: None, ..master() };
    round_trip(&[master(), numbered]);
    every_variant(&[Organization::Sequential, Organization::LineSequential, Organization::Indexed, Organization::Relative], 4);
    every_variant(&[Access::Sequential, Access::Random, Access::Dynamic], 3);
    every_variant(&[Format::Fixed, Format::Variable, Format::Text], 3);
    every_variant(&[OpenMode::Input, OpenMode::Output, OpenMode::Extend, OpenMode::InputOutput], 4);
    let from = FromMove { from: Operand::Const(0), to: 8, plan: ALNUM };
    let verbs = [
        FileVerb::Open(OpenMode::Extend),
        FileVerb::Close,
        FileVerb::Read { sequential: false, previous: true, into: Some((9, ALNUM)), key: 1 },
        FileVerb::Write { record: 8, from: Some(from), advancing: Some(Advance::Lines { before: true, count: IntExpr::Item(4) }) },
        FileVerb::Rewrite { record: 8, from: None },
        FileVerb::Delete,
        FileVerb::Start { rel: StartRel::NotLess, key: StartKey::Named { key: 1, span: RecordSpan { offset: 6, len: 4 } } },
    ];
    every_variant(&verbs, 7);
    let advances = [
        Advance::Lines { before: false, count: IntExpr::Const(2) },
        Advance::Page { before: true },
        Advance::Mnemonic { before: false, space: Spacing::Channel(12) },
    ];
    every_variant(&advances, 3);
    every_variant(&[Spacing::Lines(0), Spacing::Channel(1), Spacing::PageMode], 3);
    every_variant(&[StartRel::Equal, StartRel::Greater, StartRel::NotLess], 3);
    let keys = [StartKey::Prime, StartKey::Named { key: 0, span: RecordSpan { offset: 0, len: 6 } }, StartKey::Relative(IntExpr::Item(7)), StartKey::RelativeKey];
    every_variant(&keys, 4);
    let phrase = Phrase { on: true, not_on: true };
    let ops = [
        FileOp { file: 0, verb: FileVerb::Close, phrase: None, end_of_page: None },
        FileOp { file: 1, verb: verbs[2].clone(), phrase: Some(phrase), end_of_page: None },
        FileOp { file: 0, verb: verbs[3].clone(), phrase: None, end_of_page: Some(Phrase { on: false, not_on: true }) },
    ];
    round_trip(&ops);
    assert_eq!(ops.iter().map(FileOp::arms).collect::<Vec<_>>(), [0, 3, 5]);
}

#[test]
fn keys_on_a_file_that_is_not_indexed_are_malformed() {
    let reason = "keys on a file that is not indexed, or an indexed file without them".to_owned();
    for file in [FileDesc { organization: Organization::Sequential, ..master() }, FileDesc { keys: None, ..master() }] {
        let (bytes, strings) = encoded(&file);
        assert_eq!(refused::<FileDesc>(&bytes, &strings), (0, reason.clone()));
    }
}

#[test]
fn initialize_display_and_search_all_round_trip_with_every_tag() {
    let space = InitField { offset: 0, len: 10, value: Figurative::Space, store: FILL };
    let zero = InitField { offset: 10, len: 5, value: Figurative::Zero, store: MovePlan::Numeric { from: NumericFrom::Zero, store: PACKED } };
    let null = InitField { offset: 15, len: 4, value: Figurative::Null, store: MovePlan::Address };
    round_trip(&[InitPlan { fields: vec![] }, InitPlan { fields: vec![space, zero, null] }]);
    let items = [
        DisplayItem::Bytes(0),
        DisplayItem::National(1),
        DisplayItem::Digits { place: 2, digits: 10, signed: true },
        DisplayItem::Refused { place: 3, abend: 0 },
        DisplayItem::Text(4),
        DisplayItem::Value(Operand::Function(0)),
    ];
    every_variant(&items, 6);
    round_trip(&[DisplayPlan { items: items.to_vec(), no_advancing: true }, DisplayPlan { items: vec![], no_advancing: false }]);
    let key = SearchKey { ascending: true, key: Comparand::Operand(Operand::Load(5)), value: FLOAT_EXPR, how: Compare::Alphanumeric };
    let descending = SearchKey { ascending: false, how: Compare::Refused(1), ..key.clone() };
    let plan = SearchAllPlan { index: 6, store: StorePlan::Index, count: Count::Odo(ODO), keys: vec![key, descending] };
    let fixed = SearchAllPlan { count: Count::Fixed(20), keys: vec![], ..plan.clone() };
    round_trip(&[plan, fixed]);
}

#[test]
fn inspect_string_and_unstring_round_trip_with_every_tag() {
    every_variant(&[InspectMode::Characters, InspectMode::All, InspectMode::Leading, InspectMode::First], 4);
    every_variant(&[Chars::Literal(vec![0x6B]), Chars::Place(3), Chars::Value(Operand::Function(1))], 3);
    every_variant(&[Replacement::Chars(Chars::Place(4)), Replacement::Fill(0x40)], 2);
    let built = ConvertTable::Built(vec![(0x81, 0xC1), (0x82, 0xC2)]);
    every_variant(&[built.clone(), ConvertTable::Operands { from: Chars::Place(1), to: Chars::Literal(vec![0xC1]) }], 2);
    let tally = InspectPhrase {
        mode: InspectMode::Leading,
        pattern: Some(Chars::Literal(vec![0x40])),
        by: None,
        counter: Some((7, TALLY)),
        bounds: vec![Bound { after: true, value: Chars::Literal(vec![0x5C]) }],
    };
    let replace = InspectPhrase {
        mode: InspectMode::Characters,
        pattern: None,
        by: Some(Replacement::Fill(0xF0)),
        counter: None,
        bounds: vec![Bound { after: false, value: Chars::Place(2) }, Bound { after: false, value: Chars::Place(8) }],
    };
    let converting = Converting { table: built, bounds: vec![] };
    let inspect = InspectPlan { target: 0, tallying: vec![tally], replacing: vec![replace], converting: Some(converting) };
    round_trip(&[inspect, InspectPlan { target: 1, tallying: vec![], replacing: vec![], converting: None }]);
    let sources = vec![
        StringSource { chars: Chars::Place(2), delimiter: Some(Chars::Literal(vec![0x40])) },
        StringSource { chars: Chars::Literal(vec![0xC1, 0xC2]), delimiter: None },
    ];
    let string = StringPlan { into: 0, pointer: Some((1, PACKED)), sources };
    round_trip(&[string, StringPlan { into: 3, pointer: None, sources: vec![] }]);
    let field = UnstringInto { target: 4, plan: ALNUM, delimiter: Some(DelimiterIn { target: 5, found: ALNUM, none: FILL }), count: Some((6, PACKED)) };
    round_trip(&[field, UnstringInto { delimiter: None, count: None, ..field }]);
    let unstring = UnstringPlan {
        source: 0,
        pointer: Some((1, PACKED)),
        delimiters: vec![(true, Chars::Literal(vec![0x40])), (false, Chars::Place(2))],
        into: vec![field],
        tallying: Some((3, TALLY)),
    };
    round_trip(&[unstring, UnstringPlan { source: 0, pointer: None, delimiters: vec![], into: vec![], tallying: None }]);
}

#[test]
fn call_plans_round_trip_with_every_tag() {
    let services = [
        LeService::Cee3abd,
        LeService::Cee3dmp,
        LeService::Ceedate,
        LeService::Ceedatm,
        LeService::Ceedays,
        LeService::Ceedywk,
        LeService::Ceefrst,
        LeService::Ceegmt,
        LeService::Ceegmto,
        LeService::Ceegtst,
        LeService::Ceeloct,
        LeService::Ceemout,
        LeService::Ceesecs,
        LeService::Ceeutc,
    ];
    every_variant(&services, 14);
    let targets = [CallTarget::Named { name: 1, le: Some(LeService::Ceedate) }, CallTarget::Dynamic(Operand::Load(2)), CallTarget::Pointer(3)];
    every_variant(&targets, 3);
    let args = [CallArg::Reference(0), CallArg::Content(Chars::Literal(vec![0xF1])), CallArg::Value(Operand::LengthOf(1)), CallArg::Omitted];
    every_variant(&args, 4);
    let call = CallPlan { target: targets[0], args: args.to_vec(), returning: Some(4), on_exception: true, not_on_exception: false };
    let plain = CallPlan { target: CallTarget::Named { name: 2, le: None }, args: vec![], returning: None, on_exception: false, not_on_exception: true };
    round_trip(&[call, plain]);
}

#[test]
fn the_sql_table_round_trips_with_every_tag() {
    let types = [
        HostType::SmallInt { signed: true },
        HostType::Integer { signed: false },
        HostType::BigInt { signed: true },
        HostType::Decimal { digits: 7, scale: 2, signed: true },
        HostType::Zoned { digits: 5, scale: 0, signed: true, sign: Some(SIGN) },
        HostType::Real,
        HostType::Double,
        HostType::Char(10),
        HostType::VarChar(30),
        HostType::Structure(vec![(4, INTEGER), (5, HostType::Char(20))]),
    ];
    every_variant(&types, 10);
    let id = HostPlace { var: 0, member: None, ty: Ok(INTEGER), indicator: None };
    let member = HostPlace { var: 1, member: Some((4, 20)), ty: Ok(HostType::Char(20)), indicator: Some((2, 2)) };
    let untyped = HostPlace { var: 3, member: None, ty: Err(1), indicator: None };
    round_trip(&[id.clone(), member.clone(), untyped.clone()]);
    let statements = [
        SqlStatement::Query { inputs: vec![id.clone()], into: vec![member.clone(), untyped] },
        SqlStatement::Change { delete: true, inputs: vec![], current_of: Some(2) },
        SqlStatement::Open { cursor: 2, inputs: vec![id] },
        SqlStatement::Fetch { cursor: 2, into: vec![member] },
        SqlStatement::Close { cursor: 2 },
        SqlStatement::Commit,
        SqlStatement::Rollback,
        SqlStatement::Declaration,
        SqlStatement::Unsupported(3),
    ];
    every_variant(&statements, 9);
    let fields = [
        SqlcaField::CaId,
        SqlcaField::CaBc,
        SqlcaField::Code,
        SqlcaField::ErrMl,
        SqlcaField::ErrMc,
        SqlcaField::ErrP,
        SqlcaField::State,
        SqlcaField::ErrD(3),
        SqlcaField::Warn(10),
    ];
    every_variant(&fields, 9);
    let sqlca = Sqlca { fields: vec![(SqlcaField::Code, 7, INTEGER), (SqlcaField::ErrD(6), 8, INTEGER), (SqlcaField::State, 9, HostType::Char(5))] };
    round_trip(&[Sqlca::default(), sqlca]);
    let text = "SELECT NAME FROM EMP WHERE ID = ?";
    let query = SqlEntry { ordinal: 1, verb: 0, statement: statements[0].clone(), text: 1, fingerprint: fingerprint(text), with_hold: false };
    round_trip(&[query]);
}

#[test]
fn a_host_structure_or_a_field_the_sqlca_lacks_is_malformed() {
    let structure = HostType::Structure(vec![(1, HostType::Char(3))]);
    let place = HostPlace { var: 0, member: None, ty: Ok(structure.clone()), indicator: None };
    let (bytes, strings) = encoded(&place);
    assert_eq!(refused::<HostPlace>(&bytes, &strings), (0, "a host structure where lowering gives its members".into()));
    let cases = [
        ((SqlcaField::Code, structure), "a host structure where lowering gives its members"),
        ((SqlcaField::ErrD(0), INTEGER), "SQLERRD(0) is not an SQLCA field"),
        ((SqlcaField::ErrD(7), INTEGER), "SQLERRD(7) is not an SQLCA field"),
        ((SqlcaField::Warn(11), HostType::Char(1)), "SQLWARN11 is not an SQLCA field"),
    ];
    for ((field, ty), reason) in cases {
        let (bytes, strings) = encoded(&Sqlca { fields: vec![(field, 0, ty)] });
        assert_eq!(refused::<Sqlca>(&bytes, &strings), (0, reason.into()));
    }
}

#[test]
fn a_program_s_sql_table_runs_from_ordinal_1_with_each_text_s_fingerprint_and_with_hold() {
    let mut program = payroll();
    let first = program.symbols.len() as u32;
    let declare = "DECLARE C1 CURSOR WITH HOLD FOR SELECT NAME FROM EMP";
    program.symbols.extend(["OPEN", "C1", declare, "COMMIT"].map(String::from));
    let statement = SqlStatement::Open { cursor: first + 1, inputs: vec![] };
    let open = SqlEntry { ordinal: 1, verb: first, statement, text: first + 2, fingerprint: fingerprint(declare), with_hold: true };
    let commit = SqlEntry { ordinal: 2, verb: first + 3, statement: SqlStatement::Commit, text: first + 3, fingerprint: fingerprint("COMMIT"), with_hold: false };
    program.sql = vec![open.clone(), commit.clone()];
    round_trip(std::slice::from_ref(&program));
    let refusal = |sql: Vec<SqlEntry>| {
        let (bytes, strings) = encoded(&Program { sql, ..program.clone() });
        refused::<Program>(&bytes, &strings)
    };
    assert_eq!(refusal(vec![commit.clone()]), (0, "SQL entry 1 has ordinal 2".into()));
    let reason = format!("SQL entry 1 has fingerprint 00000000, not its text's {:08X}", fingerprint(declare));
    assert_eq!(refusal(vec![SqlEntry { fingerprint: 0, ..open.clone() }]), (0, reason));
    assert_eq!(refusal(vec![SqlEntry { with_hold: false, ..open.clone() }]), (0, "SQL entry 1 has WITH HOLD clear for its text".into()));
    assert_eq!(refusal(vec![SqlEntry { ordinal: 1, with_hold: true, ..commit }]), (0, "SQL entry 1 has WITH HOLD set for its text".into()));
    assert_eq!(refusal(vec![SqlEntry { text: 99, ..open }]), (0, format!("symbol 99 of a table of {}", first + 4)));
}

/// The walker builds the same payloads over its own references (semantics-library.md §9, C6).
#[test]
fn payloads_take_the_walker_s_own_handles() {
    let first = StringSource { chars: Chars::Place("WS-FIRST"), delimiter: None };
    let string: StringPlan<&str, &str> = StringPlan { into: "WS-OUT", pointer: None, sources: vec![first] };
    let call: CallPlan<&str, &str> = CallPlan {
        target: CallTarget::Dynamic("WS-PROGRAM"),
        args: vec![CallArg::Reference("WS-RECORD"), CallArg::Content(Chars::Value("LENGTH OF WS-RECORD"))],
        returning: None,
        on_exception: false,
        not_on_exception: false,
    };
    let host: HostPlace<&str> = HostPlace { var: "WS-ID", member: None, ty: Ok(INTEGER), indicator: Some(("WS-IND", 0)) };
    let search: SearchAllPlan<&str, &str, u32> = SearchAllPlan { index: "IX", store: StorePlan::Index, count: 20, keys: vec![] };
    assert_eq!((string.into, call.args.len(), host.indicator, search.count), ("WS-OUT", 2, Some(("WS-IND", 0)), 20));
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
