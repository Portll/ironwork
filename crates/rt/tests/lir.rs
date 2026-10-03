//! Every LIR type through the load-module codec, and a small lowered program encoded whole.

use std::collections::BTreeSet;
use std::fmt;

mod common;

use common::payroll;
use ironwork_rt::abend::{AbendCode, Ending, FileStatus, Signal};
use ironwork_rt::cics::{Assign, Cics, Condition, Control, Datum, FileControl, FileOptions, Record, Resp, Sink, Transfer};
use ironwork_rt::files::Format;
use ironwork_rt::lir::*;
use ironwork_rt::module::codec::{Decode, Encode, Writer, decode_all};
use ironwork_rt::module::{ModuleError, StringTable};
use ironwork_rt::picture::Sym;
use ironwork_rt::sql::{HostType, fingerprint};
use ironwork_rt::store::LaxRedefinition;
use ironwork_rt::storage::Kind;
use ironwork_rt::vocab::{AcceptFrom, BinOp, Figurative, InspectMode, OpenMode, Pos, RelOp, SignClause, SignPosition};
use numeric::precision::{Fixed, Places};
use numeric::options::{Compile, FastsrtAdvPrint, Invdata, Stop, Warnings};
use numeric::{Arith, BinCheck, CicsReturnWarning, Currency, DispSign, Initcheck, IntDate, Nsymbol, Numcheck, Numproc, Options, Parmcheck, Qualify, Quote, SortKeys, Trunc, TruncCheck, Vlr, VsamOpenFs, ZonCheck};
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
    every_variant_but(values, count, &[]);
}

/// `values` round-trip and between them carry every tag below `count` but the `retired` ones.
fn every_variant_but<T: Encode + Decode + PartialEq + fmt::Debug>(values: &[T], count: u8, retired: &[u8]) {
    round_trip(values);
    let tags: BTreeSet<u8> = values.iter().map(|v| encoded(v).0[0]).collect();
    assert_eq!(tags, (0..count).filter(|t| !retired.contains(t)).collect(), "{}", std::any::type_name::<T>());
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
        cics_return_warning: CicsReturnWarning::Never,
        invdata: Some(Invdata { forcenumcmp: true, cleansign: false }),
        zwb: false,
        quote: Quote::Apost,
        currency: Some(Currency::Char('£')),
        nsymbol: Nsymbol::Dbcs,
        dispsign: DispSign::Sep,
        intdate: IntDate::Lilian,
        qualify: Qualify::Extend,
        initial: true,
        vlr: Vlr::Compat,
        vsamopenfs: VsamOpenFs::Succ,
        numcheck: Some(Numcheck { zon: Some(ZonCheck { alphnum: false, lax: true }), pac: false, bin: Some(BinCheck { truncbin: false }), abd: true }),
        parmcheck: Some(Parmcheck { abd: true, bytes: 9999 }),
        initcheck: Some(Initcheck::Strict),
        optimize: 2,
    };
    round_trip(&[every, Options { currency: Some(Currency::Hex(0x5B)), ..every }]);
    let each = [
        Options { fastsrt_adv_print: FastsrtAdvPrint::Include, ..Options::default() },
        Options { warnings: Warnings::Block, ..Options::default() },
        Options { compile: Some(Compile::SyntaxOnly), ..Options::default() },
        Options { dynam: true, ..Options::default() },
        Options { debug: true, ..Options::default() },
        Options { cics_return_warning: CicsReturnWarning::Always, ..Options::default() },
        Options { invdata: Some(Invdata::default()), ..Options::default() },
        Options { zwb: false, ..Options::default() },
        Options { quote: Quote::Apost, ..Options::default() },
        Options { currency: Some(Currency::Hex(0x4A)), ..Options::default() },
        Options { nsymbol: Nsymbol::Dbcs, ..Options::default() },
        Options { dispsign: DispSign::Sep, ..Options::default() },
        Options { intdate: IntDate::Lilian, ..Options::default() },
        Options { qualify: Qualify::Extend, ..Options::default() },
        Options { initial: true, ..Options::default() },
        Options { vlr: Vlr::Compat, ..Options::default() },
        Options { vsamopenfs: VsamOpenFs::Succ, ..Options::default() },
        Options { numcheck: Some(Numcheck::default()), ..Options::default() },
        Options { parmcheck: Some(Parmcheck { abd: false, bytes: 100 }), ..Options::default() },
        Options { initcheck: Some(Initcheck::Lax), ..Options::default() },
        Options { optimize: 1, ..Options::default() },
    ];
    round_trip(&each);
    for options in each {
        assert_ne!(encoded(&options).0, encoded(&Options::default()).0);
    }
    every_variant(&[FastsrtAdvPrint::Exclude, FastsrtAdvPrint::Include], 2);
    every_variant(&[Warnings::Proceed, Warnings::Block], 2);
    every_variant(&[Compile::Full, Compile::Until(Stop::W), Compile::SyntaxOnly], 3);
    every_variant(&[Stop::W, Stop::E, Stop::S], 3);
    every_variant(&[CicsReturnWarning::Once, CicsReturnWarning::Always, CicsReturnWarning::Never], 3);
    every_variant(&[Quote::Quote, Quote::Apost], 2);
    every_variant(&[Currency::Char('$'), Currency::Hex(0x5B)], 2);
    every_variant(&[Nsymbol::National, Nsymbol::Dbcs], 2);
    every_variant(&[DispSign::Compat, DispSign::Sep], 2);
    every_variant(&[IntDate::Ansi, IntDate::Lilian], 2);
    every_variant(&[Qualify::Compat, Qualify::Extend], 2);
    every_variant(&[Vlr::Standard, Vlr::Compat], 2);
    every_variant(&[VsamOpenFs::Compat, VsamOpenFs::Succ], 2);
    every_variant(&[Initcheck::Lax, Initcheck::Strict], 2);
}

#[test]
fn kinds_and_options_have_load_module_s_bytes() {
    assert_eq!(encoded(&Kind::Zoned { digits: 5, scale: 2, signed: true, sign: Some(SIGN) }).0, [3, 5, 2, 1, 1, 1, 1]);
    assert_eq!(encoded(&Kind::Float(Precision::Extended)).0, [6, 2]);
    let options = Options { arith: Arith::Extend, trunc: Trunc::Opt, ..Options::default() };
    assert_eq!(
        encoded(&options).0,
        [
            0x01, 0x01, 0x00, 0xF4, 0x08, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
    let (bytes, strings) = encoded(&(7u8, Options { codepage: 999, ..options }));
    let reason = "CODEPAGE(999) is not a page the tables carry".to_owned();
    assert_eq!(refused::<(u8, Options)>(&bytes, &strings), (1, reason));
    let (bytes, strings) = encoded(&(7u8, Options { optimize: 3, ..options }));
    assert_eq!(refused::<(u8, Options)>(&bytes, &strings), (1, "OPTIMIZE(3) is not a level".to_owned()));
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
    let options = ProgramOptions { options: Options::default(), ssrange: true, cards, collating: Collating::Native, decimal_point_comma: false, numval_currency: "$".into(), when_compiled: None };
    let sequenced = ProgramOptions { collating: Collating::Sequence(letters_first()), ..options.clone() };
    let comma = ProgramOptions { decimal_point_comma: true, numval_currency: "EUR ".into(), ..options.clone() };
    let when_compiled = Some(CompileTime { seconds: 1_790_510_400, hundredths: 42, source: TimeSource::Clock });
    let stamped = ProgramOptions { when_compiled, ..options.clone() };
    round_trip(&[options, sequenced, comma, stamped]);
    every_variant(&[TimeSource::SourceDateEpoch, TimeSource::Clock], 2);
    round_trip(&[Edit { syms: vec![Sym::Currency, Sym::Nine, Sym::Point, Sym::Nine], currency: "CHF ".into() }, Edit { syms: vec![Sym::Z], currency: String::new() }]);
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
        parmcheck: Some((1, 2)),
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
    round_trip(&[Paragraph { name: 1, is_section: true, entry: 0, section_end: 3, priority: 50, at: 2, abandoned: Some(4) }]);
    round_trip(&[Block { ops: vec![], end: Terminator::Jump(1) }, Block { ops: vec![Op::Nest, Op::Arith(0)], end: Terminator::Abend(0) }]);
    let plans = Plans {
        arith: vec![ArithPlan { dmax: 0, arith: Arith::Extend, prepass: vec![], steps: vec![], remainder: None, handled: false, per_receiver: false }],
        init: vec![InitPlan { fields: vec![InitField { offset: 0, len: 2, value: InitValue::Default(Figurative::Null), store: FILL, scaling: 0 }] }],
        display: vec![DisplayPlan { items: vec![DisplayItem::Text(0)], no_advancing: false }],
        inspect: vec![InspectPlan { target: Inspected::Item(0), tallying: vec![], replacing: vec![], converting: None }],
        string: vec![StringPlan { into: 0, pointer: None, sources: vec![] }],
        unstring: vec![UnstringPlan { source: 0, pointer: None, delimiters: vec![], into: vec![], tallying: None }],
        search_all: vec![SearchAllPlan { index: 0, store: StorePlan::Index, count: Count::Fixed(5), keys: vec![] }],
        function: vec![FunctionPlan {
            func: Func::Trim,
            args: vec![Argument::Value(Comparand::Operand(Operand::Load(0)))],
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
        sorts: vec![SortPlan::File(file_sort())],
        releases: vec![ReleasePlan { record: 4, file: Some(1), from: None, sort_return: 9, name: 3 }],
        returns: vec![ReturnPlan { file: Some(1), into: None, sort_return: 9, name: 2 }],
        invokes: vec![InvokePlan {
            receiver: Receiver::SelfRef,
            method: MethodName::New,
            args: vec![],
            returning: None,
            on_exception: false,
            not_on_exception: false,
        }],
        cics: vec![CicsCommand { name: 3, command: Cics::Syncpoint { rollback: false }, resp: Resp { resp: None, resp2: None, nohandle: true }, sinks: vec![(2, Sink::Sysid)] }],
        sqlca: Sqlca { fields: vec![(SqlcaField::Code, 0, INTEGER)] },
        entries: vec![EntryPoint { name: 2, paragraph: 1, block: 4, using: vec![0, 1] }],
        class: Some(Box::new(account())),
        user_functions: vec![UserFunctionPlan { name: 4, external: 5, args: vec![], refmod: None, at: 3 }],
        function: Some(FunctionDefinition { params: vec![0, 2], returning: 1 }),
        declaratives: Declaratives { modes: [Some(0), None, None, Some(2)], debug_item: Some((120, 86)) },
        markup: vec![Markup::XmlParse(xml_parse())],
        report: ReportWriter { reports: vec![report()], print_switch: Some(16) },
        scope: scope(),
    };
    round_trip(&[Services::default(), services]);
}

/// A program contained in another, with an EXTERNAL record and file and the GLOBAL names of the
/// program containing it, which contains others itself.
fn scope() -> Scope {
    Scope {
        containers: vec![4, 5],
        records: vec![(1, Binding::External { name: 2, size: 40 }), (2, Binding::ExternalFile(0)), (3, Binding::Global { program: 4, section: Section::File, name: 6 })],
        files: vec![SharedFile { file: 0, external: true, declared_in: None }, SharedFile { file: 1, external: false, declared_in: Some(4) }],
        areas: vec![(0, 2), (1, 3)],
        globals: vec![Global { section: Section::WorkingStorage, name: 7, at: GlobalAt::Program(16) }, Global { section: Section::Linkage, name: 8, at: GlobalAt::Linkage(0) }],
        global_files: vec![(2, 1)],
        global_modes: [None, Some(1), None, None],
    }
}

#[test]
fn external_and_global_scopes_round_trip() {
    round_trip(&[Scope::default(), scope()]);
    let global = |section| Binding::Global { program: 0, section, name: 1 };
    every_variant(&[Binding::External { name: 0, size: 1 }, Binding::ExternalFile(3), global(Section::Linkage)], 3);
    every_variant(&[Section::WorkingStorage, Section::LocalStorage, Section::Linkage, Section::File], 4);
    every_variant(&[GlobalAt::Program(0), GlobalAt::Local(8), GlobalAt::Linkage(2)], 3);
}

/// A class with FACTORY data, OBJECT data and a method of each, its programs the sample program.
fn account() -> Class {
    let part = ClassPart { data: payroll(), records: vec![0, 40] };
    let method = Method { name: 3, factory: false, params: vec![4], returns: Some(5), own_records: 1, code: payroll() };
    let open = Method { factory: true, params: vec![], returns: None, own_records: 0, ..method.clone() };
    Class { external: 1, parent: 2, factory: Some(part.clone()), object: Some(part), methods: vec![method, open] }
}

#[test]
fn user_defined_functions_round_trip() {
    every_variant(&[UserArgument::Reference(2), UserArgument::Value(FLOAT_EXPR)], 2);
    let plan = UserFunctionPlan {
        name: 1,
        external: 2,
        args: vec![UserArgument::Reference(0), UserArgument::Value(Comparand::Operand(Operand::UserFunction(1)))],
        refmod: Some(RefMod { length: None, ..REFMOD }),
        at: 7,
    };
    round_trip(&[plan, UserFunctionPlan { name: 0, external: 0, args: vec![], refmod: None, at: 0 }]);
    round_trip(&[FunctionDefinition { params: vec![], returning: 0 }, FunctionDefinition { params: vec![3, 1], returning: 2 }]);
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
fn a_parmcheck_buffer_past_the_slab_is_malformed() {
    let storage = Storage { size: 4, image: vec![0; 4], parmcheck: Some((2, 3)), ..Storage::default() };
    let (bytes, strings) = encoded(&storage);
    assert_eq!(refused::<Storage>(&bytes, &strings), (0, "a PARMCHECK buffer at 2 for 3 in a slab of 4".into()));
    round_trip(&[Storage { parmcheck: Some((2, 2)), ..storage }]);
}

#[test]
fn a_compile_time_outside_what_when_compiled_shows_is_malformed() {
    let refusal = |seconds: i64, hundredths: u32, source: TimeSource| {
        let (bytes, strings) = encoded(&CompileTime { seconds, hundredths, source });
        refused::<CompileTime>(&bytes, &strings).1
    };
    assert_eq!(refusal(-1, 0, TimeSource::Clock), "a compile time of -1 seconds and 0 hundredths from Clock");
    assert_eq!(refusal(CompileTime::LATEST + 1, 0, TimeSource::SourceDateEpoch), "a compile time of 253402300800 seconds and 0 hundredths from SourceDateEpoch");
    assert_eq!(refusal(0, 100, TimeSource::Clock), "a compile time of 0 seconds and 100 hundredths from Clock");
    assert_eq!(refusal(0, 5, TimeSource::SourceDateEpoch), "a compile time of 0 seconds and 5 hundredths from SourceDateEpoch");
    round_trip(&[CompileTime { seconds: CompileTime::LATEST, hundredths: 99, source: TimeSource::Clock }]);
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
        [Base::Program, Base::Local, Base::Linkage(2), Base::ReturnCode, Base::Eib, Base::SelfRef, Base::JniEnv, Base::Xml(XmlRegister::NText)];
    every_variant(&bases, 8);
    let subscript = Subscript { stride: 12, value: IntExpr::Item(1), check: Some(50) };
    round_trip(&[subscript.clone(), Subscript { stride: 4, value: IntExpr::Const(-1), check: None }]);
    round_trip(&[ODO, Odo { check: false, ..ODO }]);
    round_trip(&[REFMOD, RefMod { length: None, ..REFMOD }]);
    let place = Place {
        base: Base::Linkage(0),
        offset: 16,
        len: 12,
        kind: Kind::Alnum { justified: false },
        scaling: 2,
        moved: vec![Odo { element: 3, ..ODO }],
        subscripts: vec![subscript.clone(), subscript],
        odo: vec![ODO, Odo { max: 2, ..ODO }],
        refmod: Some(REFMOD),
        name: 3,
        at: 9,
        numcheck: PlaceNumcheck { lax: Some(LaxRedefinition::LeadingSpaces(3)), removed: true },
    };
    let bare = Place { base: Base::ReturnCode, moved: vec![], subscripts: vec![], odo: vec![], refmod: None, numcheck: PlaceNumcheck::default(), ..place.clone() };
    round_trip(&[place, bare]);
    every_variant(&[LaxRedefinition::Signed, LaxRedefinition::LeadingSpaces(1)], 2);
}

#[test]
fn values_and_conditions_round_trip_with_every_tag() {
    let operands = [Operand::Load(1), Operand::Const(2), Operand::LengthOf(3), Operand::AddressOf(4), Operand::Function(5), Operand::UserFunction(6)];
    every_variant(&operands, 6);
    let consts = [
        Const::Bytes(vec![0xC1, 0x40]),
        Const::National(vec![0x00, 0x41]),
        Const::Number(fixed(-125, 1, 2)),
        Const::Figurative(Figurative::HighValue),
        Const::All(vec![]),
        Const::Refused(3),
    ];
    every_variant(&consts, 6);
    every_variant(&[IntExpr::Const(i64::MIN), IntExpr::Item(0), IntExpr::Fixed { expr: 2, dmax: 3, prepass: vec![1, 4] }, IntExpr::Walk(2)], 4);
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
    let classes = [ByteClass::Packed { signed: false }, ByteClass::Zoned { signed: true }, ByteClass::Digits, ByteClass::Alphabetic, ByteClass::AlphabeticLower, ByteClass::AlphabeticUpper];
    every_variant(&classes, 6);
    every_variant(&[SignTest::Positive, SignTest::Negative, SignTest::Zero], 3);
    every_variant(&[Count::Fixed(10), Count::Odo(ODO), Count::Temp(2)], 3);
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
        per_receiver: true,
    };
    round_trip(&[plan]);
}

#[test]
fn control_flow_round_trips_with_every_tag() {
    let move_plan = MovePlan::Numeric { from: NumericFrom::Value, store: PACKED };
    let ops = [
        Op::Move { from: Operand::Load(0), to: 1, plan: move_plan, check: SenderCheck::Item },
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
        Op::Report(ReportOp::Generate { report: 1, detail: Some(2) }),
        Op::Invoke(0),
        Op::Cics(0),
        Op::Sql(1),
        Op::Alter { para: 2, to: 5 },
        Op::EnterSegment(50),
        Op::DebugLine(42),
        Op::DebugAlter { range: 1, name: 2, contents: 3 },
        Op::Markup(0),
        Op::Set { from: Operand::Load(2), to: 1, plan: MovePlan::Index },
        Op::SetCount(3, ODO),
        Op::SetEntry { entry: Operand::Const(4), targets: vec![1, 2] },
    ];
    // Tag 29 is retired (load-module.md §4.3).
    every_variant_but(&ops, 36, &[29]);
    let resume = Resume { para: 2, block: 11 };
    every_variant(&[Step::Next, Step::Arm(2), Step::GoTo(3), Step::End(Ending::Goback), Step::Return(u64::MAX), Step::Resume(resume)], 6);
    let terminators = [
        Terminator::Jump(1),
        Terminator::Branch { cond: 0, then: 1, otherwise: 2 },
        Terminator::Select(vec![3, 4, 5]),
        Terminator::ParagraphEnd { next: 2 },
        Terminator::GoTo(0),
        Terminator::Switch { value: IntExpr::Item(1), targets: vec![0, 2], otherwise: 6 },
        Terminator::PerformEnter { range: 0, ret: 7, resume: Some(resume) },
        Terminator::PerformEnter { range: 1, ret: 7, resume: None },
        Terminator::ExitProgram { next: 8 },
        Terminator::End(Ending::StopRun),
        Terminator::Abend(0),
        Terminator::AlteredGoTo { para: 3, otherwise: 9 },
        Terminator::Debug { range: 2, name: 4, next: 10 },
    ];
    // Tag 6 is retired.
    every_variant_but(&terminators, 13, &[6]);
    round_trip(&[Range { first: 1, last: 3, kind: RangeKind::Perform }, Range { first: 4, last: 2, kind: RangeKind::SortProcedure }]);
    let kinds = [RangeKind::Perform, RangeKind::SortProcedure, RangeKind::UseBeforeReporting, RangeKind::UseProcedure, RangeKind::Debugging, RangeKind::Processing];
    every_variant(&kinds, 6);
    let point = ReturnPoint { frame: 7, resume: Some(resume) };
    round_trip(&[point, ReturnPoint { frame: 0, resume: None }]);
    let frames = [FrameKind::Main, FrameKind::Perform { range: 0, ret: 5, resume: Some(resume) }, FrameKind::Procedure { range: 1 }];
    every_variant(&frames, 3);
    let main = Frame { id: 0, kind: FrameKind::Main, displaced: None, segment: 0, depth: 0, temps: vec![] };
    let performed = Frame { id: 9, kind: frames[1], displaced: Some(point), segment: 51, depth: 2, temps: vec![3, 0, -1] };
    round_trip(&[main.clone(), performed.clone()]);
    let saved = [(11, Some(point)), (12, None)].into_iter().collect();
    round_trip(&[Returns::default(), Returns { armed: vec![None, Some(point)], saved, frames: vec![main, performed], next_frame: 10 }]);
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
fn a_range_s_region_runs_to_the_program_s_end_when_it_ends_before_it_starts_and_a_sort_procedure_s_holds_every_paragraph() {
    let range = |first, last, kind| Range { first, last, kind };
    assert_eq!(range(2, 4, RangeKind::Perform).region(9), (2, 4));
    assert_eq!(range(4, 2, RangeKind::Perform).region(9), (4, 8));
    assert_eq!(range(3, 3, RangeKind::UseProcedure).region(9), (3, 3));
    assert_eq!(range(2, 4, RangeKind::SortProcedure).region(9), (0, 8));
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
    every_variant(&[Image::Bytes, Image::All, Image::Figurative, Image::Digits { digits: 3 }, Image::Stored], 5);
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
    every_variant(&[SenderCheck::None, SenderCheck::Item, SenderCheck::Integer], 3);
    every_variant(Func::ALL, 82);
    for &func in Func::ALL {
        assert_eq!(Func::named(func.name()), Some(func));
    }
    assert_eq!((Func::named("NUMVAL-C"), Func::named("NUMVALC")), (Some(Func::NumvalC), None));
    assert_eq!((Func::Random.arity(), Func::Max.arity().contains(&40)), (0..=1, true));
    assert_eq!((Func::named("WHEN-COMPILED"), Func::PresentValue.arity().contains(&1)), (Some(Func::WhenCompiled), false));
    every_variant(&[TrimSide::Leading, TrimSide::Trailing], 2);
    let all = Argument::All { element: 4, all: vec![(0, Count::Fixed(3)), (1, Count::Odo(ODO))] };
    every_variant(&[Argument::Value(FLOAT_EXPR), all.clone()], 2);
    let args = vec![Argument::Value(Comparand::Operand(Operand::Load(0))), Argument::Value(FLOAT_EXPR), all, Argument::Value(Comparand::Operand(Operand::Function(1)))];
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
}

fn report() -> ironwork_rt::report::Report {
    use ironwork_rt::report::*;
    let field = |content| Field { item: 9, column: 4, content, group_indicate: true, blank_when_zero: false, rounded: true, pos: Pos { file: 1, line: 20, col: 12 } };
    let detail = Group {
        name: Some("DETAIL-LINE".into()),
        kind: GroupKind::Detail,
        level: 0,
        next_group: Some(NextGroup::Plus(1)),
        lines: vec![Line { number: LineNumber::Plus(1), fields: vec![field(FieldContent::Source(Comparand::Operand(Operand::Load(3)))), field(FieldContent::Value(1))] }],
        unprinted: vec![field(FieldContent::Program)],
        cross: vec![(0, Origin::Source(Comparand::Expr { expr: 4, dmax: 2, mode: Mode::Fixed, prepass: vec![5] })), (1, Origin::Total(0))],
        rolls: vec![(1, Origin::Value(2))],
        totals: vec![0],
        indicate: Some(0),
        declarative: Some(2),
    };
    let footing = Group {
        name: None,
        kind: GroupKind::ControlFooting,
        level: 1,
        next_group: Some(NextGroup::NextPage),
        lines: vec![Line { number: LineNumber::NextPage(Some(5)), fields: vec![field(FieldContent::Sum(1))] }, Line { number: LineNumber::Line(9), fields: vec![] }],
        indicate: None,
        declarative: None,
        ..detail.clone()
    };
    Report {
        name: "SALES".into(),
        file: 2,
        code: Some(5),
        width: 132,
        page: Some(Page { limit: 60, heading: 1, first_detail: 5, last_detail: 50, footing: 55 }),
        controls: vec![Control { reference: 7, saved: 18, len: 4 }],
        groups: vec![detail, footing],
        sums: vec![Sum { total: 11, reset: None }, Sum { total: 12, reset: Some(1) }],
        subtotals: vec![Subtotal { sum: 0, operand: Comparand::Operand(Operand::Load(6)), adding: Adding::Upon(vec![0]) }],
        page_counter: 13,
        line_counter: 14,
        state: 15,
        report_heading: None,
        page_heading: Some(1),
        page_footing: None,
        report_footing: None,
        control_headings: vec![None, None],
        control_footings: vec![None, Some(1)],
        first_detail_written: Some(5),
    }
}

#[test]
fn report_writers_and_ops_round_trip_with_every_tag() {
    use ironwork_rt::report::*;
    let unpaged = Report { page: None, code: None, first_detail_written: None, ..report() };
    round_trip(&[ReportWriter::default(), ReportWriter { reports: vec![report(), unpaged], print_switch: Some(16) }]);
    every_variant(&[ReportOp::Initiate(0), ReportOp::Generate { report: 1, detail: None }, ReportOp::Terminate(2), ReportOp::Suppress], 4);
    every_variant(&[LineNumber::Line(1), LineNumber::Plus(2), LineNumber::NextPage(None)], 3);
    every_variant(&[NextGroup::Line(1), NextGroup::Plus(2), NextGroup::NextPage], 3);
    let kinds = [GroupKind::ReportHeading, GroupKind::PageHeading, GroupKind::ControlHeading, GroupKind::Detail, GroupKind::ControlFooting, GroupKind::PageFooting, GroupKind::ReportFooting];
    every_variant(&kinds, 7);
    every_variant(&[FieldContent::Source(Comparand::Operand(Operand::Const(1))), FieldContent::Value(2), FieldContent::Sum(3), FieldContent::Program], 4);
    every_variant(&[Origin::Source(Comparand::Operand(Operand::Load(1))), Origin::Value(2), Origin::Total(3)], 3);
    every_variant(&[Adding::EveryGenerate, Adding::Upon(vec![1]), Adding::Correlated(vec![])], 3);
}

fn file_sort() -> FileSort {
    let zoned = Kind::Zoned { digits: 5, scale: 0, signed: true, sign: Some(SIGN) };
    let keys = vec![
        SortKey { ascending: true, offset: 0, len: 5, kind: zoned, item: 7, collated: false },
        SortKey { ascending: false, offset: 5, len: 10, kind: Kind::Alnum { justified: false }, item: 8, collated: true },
    ];
    let mut positions = Box::new([0u8; 256]);
    positions.iter_mut().enumerate().for_each(|(i, p)| *p = 255 - i as u8);
    FileSort {
        sd: 1,
        merge: false,
        keys: ironwork_rt::lir::SortKeys { keys, collating: Some(positions) },
        input: Some(SortIo::Files(vec![2, 3])),
        output: Some(SortIo::Procedure(4)),
        sort_return: 9,
        sort_control: 10,
    }
}

#[test]
fn sort_plans_round_trip_with_every_tag() {
    let merge = FileSort {
        merge: true,
        keys: ironwork_rt::lir::SortKeys { keys: vec![], collating: None },
        input: Some(SortIo::Procedure(1)),
        output: None,
        ..file_sort()
    };
    let unbuilt = FileSort { input: None, output: Some(SortIo::Files(vec![])), ..file_sort() };
    round_trip(&[file_sort(), merge, unbuilt]);
    every_variant(&[SortIo::Files(vec![1]), SortIo::Procedure(2)], 2);
    let table = TableSort { first: 3, count: Count::Odo(ODO), stride: 12, keys: file_sort().keys, name: 5 };
    let fixed = TableSort { count: Count::Fixed(10), ..table.clone() };
    every_variant(&[SortPlan::File(file_sort()), SortPlan::Table(table), SortPlan::Table(fixed)], 2);
    round_trip(&[
        ReleasePlan { record: 4, file: Some(1), from: Some(FromMove { from: Operand::Load(2), to: 5, plan: ALNUM, check: SenderCheck::Integer }), sort_return: 9, name: 3 },
        ReleasePlan { record: 4, file: None, from: None, sort_return: 9, name: 3 },
    ]);
    round_trip(&[ReturnPlan { file: Some(1), into: Some((6, ALNUM)), sort_return: 9, name: 2 }, ReturnPlan { file: None, into: None, sort_return: 9, name: 2 }]);
}

#[test]
fn cics_commands_round_trip_with_every_tag() {
    let (place, value, text) = (Some(Datum::Place(1)), Some(Datum::Value(Operand::Const(2))), Some(Datum::Text(3)));
    let record = Record { into: place, set: None, length: value };
    let transfer = Transfer { program: text, commarea: place, length: Some(Datum::Bare) };
    let control = Control { erase: true, freekb: false, alarm: true, frset: false };
    let options = FileOptions {
        ridfld: place,
        keylength: value,
        reqid: None,
        from: None,
        numrec: place,
        record: record.clone(),
        generic: true,
        rrn: false,
        gteq: true,
        equal: false,
        update: true,
    };
    let assign = Assign {
        applid: place,
        sysid: None,
        userid: None,
        netname: text,
        facility: None,
        startcode: None,
        abcode: None,
        program: value,
        cwaleng: None,
        twaleng: place,
    };
    let commands = vec![
        Cics::File { verb: FileControl::Readprev, file: text, options },
        Cics::Return { transid: text, commarea: None, length: None, channel: value, immediate: true },
        Cics::Link(transfer.clone()),
        Cics::Xctl(transfer),
        Cics::Abend { abcode: value, cancel: true },
        Cics::HandleCondition(vec![(Condition::NOTFND, Some(4)), (Condition::ERROR, None)]),
        Cics::IgnoreCondition(vec![Condition::LENGERR, Condition::BUSY]),
        Cics::PushHandle,
        Cics::PopHandle,
        Cics::HandleAbend { program: None, label: Some(2), reset: true },
        Cics::HandleAid,
        Cics::SendMap { map: text, mapset: None, from: place, maponly: false, dataonly: true, cursor: Some(Datum::Bare), control },
        Cics::ReceiveMap { map: text, mapset: text, into: place, set: None },
        Cics::SendControl { cursor: value, control: Control::default() },
        Cics::Receive(record.clone()),
        Cics::Asktime { abstime: place },
        Cics::Formattime { abstime: place, datesep: Some(Datum::Bare), timesep: None, outputs: vec![(5, Datum::Place(6)), (7, Datum::Bare)] },
        Cics::Assign(assign),
        Cics::Getmain { flength: value, length: None, initimg: value, set: place },
        Cics::Freemain,
        Cics::Enq,
        Cics::Deq,
        Cics::Delay,
        Cics::Syncpoint { rollback: true },
        Cics::Address { eib: place, commarea: place, cwa: None, twa: None },
        Cics::SendText { from: place, length: value },
        Cics::WriteOperator { text: value, textlength: None },
        Cics::WriteqTs { queue: text, from: place, length: None, rewrite: true, item: place, numitems: None },
        Cics::ReadqTs { queue: place, next: true, item: None, numitems: place, record: record.clone() },
        Cics::DeleteqTs { queue: text },
        Cics::WriteqTd { queue: text, from: place, length: value },
        Cics::ReadqTd { queue: text, record },
        Cics::DeleteqTd { queue: text },
        Cics::Unsupported,
        Cics::Refused(10),
    ];
    // Tags 1 and 9 are retired (load-module.md §4.3).
    every_variant_but(&commands, 37, &[1, 9]);
    let resp = Resp { resp: place, resp2: Some(Datum::Place(8)), nohandle: false };
    let sinks = vec![(1, Sink::QueueName), (11, Sink::Sysid)];
    round_trip(&commands.into_iter().map(|command| CicsCommand { name: 9, command, resp: resp.clone(), sinks: sinks.clone() }).collect::<Vec<_>>());
    let kinds = [
        Sink::DynamicTransfer,
        Sink::RecordKey,
        Sink::RecordUpdate,
        Sink::Log,
        Sink::Screen,
        Sink::WebResponse,
        Sink::HttpHeader,
        Sink::OutboundHost,
        Sink::OutboundHttp,
        Sink::QueueName,
        Sink::Sysid,
    ];
    every_variant(&kinds, 11);
    let verbs = [
        FileControl::Read,
        FileControl::Write,
        FileControl::Rewrite,
        FileControl::Delete,
        FileControl::Unlock,
        FileControl::Startbr,
        FileControl::Resetbr,
        FileControl::Readnext,
        FileControl::Readprev,
        FileControl::Endbr,
    ];
    every_variant(&verbs, 10);
    every_variant(&[Datum::Place(0), Datum::Value(Operand::Load(1)), Datum::Text(2), Datum::Bare], 4);
    every_variant(Condition::ALL, 121);
    assert_eq!(refused::<Condition>(&[121], &StringTable::default()), (0, "Condition has no tag 121".into()));
}

fn xml_parse() -> XmlParse {
    XmlParse {
        document: 0,
        encoding: Some(Operand::Const(1)),
        national: true,
        procedure: 2,
        event: 3,
        code: (4, PACKED),
        information: (5, StorePlan::Index),
        code_value: IntExpr::Item(4),
        on_exception: true,
        not_on_exception: false,
    }
}

fn json_generate() -> JsonGenerate {
    let leaf = JsonLeaf { suppress: vec![Figurative::Space, Figurative::Zero], boolean: Some(Marker::Condition(2)), convert: Convert::Fixed { integers: 5 } };
    let group = JsonNode {
        offset: 0,
        moved: vec![],
        len: 12,
        kind: Kind::Group,
        name: 1,
        occurs: Some(Count::Odo(ODO)),
        indicator: Some((Err(3), Marker::Refused(4))),
        null: Some(Figurative::Space),
        value: JsonValue::Object { members: vec![1], eligible: true },
    };
    let field = JsonNode {
        offset: 2,
        moved: vec![ODO],
        len: 5,
        kind: Kind::Zoned { digits: 5, scale: 0, signed: false, sign: None },
        name: 2,
        occurs: None,
        indicator: Some((Ok(6), Marker::Byte(Some(0xE8)))),
        null: Some(Figurative::Zero),
        value: JsonValue::Leaf(leaf),
    };
    JsonGenerate {
        from: 0,
        subscripts: vec![IntExpr::Item(1)],
        nodes: vec![group, field],
        name: None,
        receiver: 2,
        encoding: Ccsid::CodePage,
        count: Some((3, PACKED)),
        code: (4, StorePlan::Binary { digits: 9, scale: 0, signed: true, native: false, name: 0 }),
        on_exception: false,
        not_on_exception: true,
    }
}

fn xml_generate() -> XmlGenerate {
    let leaf = XmlValue::Leaf { form: XmlForm::Attribute, suppress: vec![Figurative::Space], convert: Convert::Chars { justified: true } };
    let nodes = vec![
        XmlNode { offset: 0, moved: vec![], len: 20, kind: Kind::Group, name: 1, occurs: None, value: XmlValue::Element { members: vec![1] } },
        XmlNode { offset: 0, moved: vec![ODO], len: 10, kind: Kind::Group, name: 2, occurs: Some(Count::Fixed(2)), value: XmlValue::Members { members: vec![2] } },
        XmlNode { offset: 0, moved: vec![], len: 10, kind: Kind::Alnum { justified: true }, name: 3, occurs: None, value: leaf },
    ];
    XmlGenerate {
        receiver: 0,
        encoding: Ccsid::Operand(Operand::Const(0)),
        namespace: Some(Operand::Load(1)),
        prefix: None,
        declaration: true,
        from: 2,
        subscripts: vec![],
        nodes,
        suppressing: true,
        count: None,
        code: (3, PACKED),
        on_exception: true,
        not_on_exception: true,
    }
}

#[test]
fn json_and_xml_statements_round_trip_with_every_tag() {
    every_variant(&[Markup::JsonGenerate(json_generate()), Markup::XmlGenerate(xml_generate()), Markup::XmlParse(xml_parse())], 3);
    every_variant(&[Ccsid::Unnamed, Ccsid::CodePage, Ccsid::Operand(Operand::Load(0))], 3);
    let converts = [Convert::Chars { justified: false }, Convert::National, Convert::Float(Precision::Short), Convert::Fixed { integers: 10 }, Convert::Refused(1)];
    every_variant(&converts, 5);
    every_variant(&[Marker::Byte(None), Marker::Condition(0), Marker::Refused(2)], 3);
    every_variant(&[JsonValue::Object { members: vec![], eligible: false }, json_generate().nodes[1].value.clone()], 2);
    every_variant(&xml_generate().nodes.into_iter().map(|n| n.value).collect::<Vec<_>>(), 3);
    every_variant(&[XmlForm::Attribute, XmlForm::Element, XmlForm::Content], 3);
    let registers = [XmlRegister::Text, XmlRegister::NText, XmlRegister::Namespace, XmlRegister::NNamespace, XmlRegister::Prefix, XmlRegister::NPrefix];
    every_variant(&registers, 6);
    assert_eq!(registers.map(XmlRegister::national), [false, true, false, true, false, true]);
    assert_eq!(Markup::XmlParse(xml_parse()).phrases(), (true, false));
}

#[test]
fn a_markup_tree_whose_member_comes_before_its_holder_is_malformed() {
    let mut g = json_generate();
    g.nodes[0].value = JsonValue::Object { members: vec![0], eligible: false };
    let (bytes, strings) = encoded(&g);
    assert_eq!(refused::<JsonGenerate>(&bytes, &strings), (0, "markup node 0 holds node 0 of 2".into()));
    let mut x = xml_generate();
    x.nodes.clear();
    let (bytes, strings) = encoded(&x);
    assert_eq!(refused::<XmlGenerate>(&bytes, &strings), (0, "a markup tree with no root".into()));
}

/// An indexed file of variable-length records with FILE STATUS, an alternate key, LINAGE and a
/// print file's carriage.
fn master() -> FileDesc {
    let span = RecordSpan { offset: 0, len: 6 };
    FileDesc {
        name: 1,
        assign: 2,
        organization: Organization::Indexed,
        access: Access::Dynamic,
        optional: true,
        format: Format::Variable,
        read_lengths: (26, 300),
        fixed: false,
        record_min: Some(26),
        depending: Some(RecordDepending { item: 6, lengths: (26, 280) }),
        status: Some((3, ALNUM)),
        keys: Some(IndexKeys { prime: span, alternates: vec![(RecordSpan { offset: 6, len: 20 }, true)] }),
        relative: None,
        linage: Some(Linage { lines: IntExpr::Const(60), footing: Some(IntExpr::Item(4)), top: None, bottom: Some(IntExpr::Const(3)), counter: Some((5, PACKED)) }),
        carriage: Some(Carriage { machine: true, reserved: false }),
        sort: false,
        error: Some(1),
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
    let from = FromMove { from: Operand::Const(0), to: 8, plan: ALNUM, check: SenderCheck::None };
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
    let space = InitField { offset: 0, len: 10, value: InitValue::Default(Figurative::Space), store: FILL, scaling: 0 };
    let zero = InitField { offset: 10, len: 5, value: InitValue::Default(Figurative::Zero), store: MovePlan::Numeric { from: NumericFrom::Zero, store: PACKED }, scaling: 2 };
    let null = InitField { offset: 15, len: 4, value: InitValue::Default(Figurative::Null), store: MovePlan::Address, scaling: 0 };
    let valued = InitField { offset: 19, len: 3, value: InitValue::Value(4), store: FILL, scaling: 0 };
    let replaced = InitField { offset: 22, len: 5, value: InitValue::Replacing(Operand::Load(6)), store: MovePlan::Refused(1), scaling: 0 };
    round_trip(&[InitPlan { fields: vec![] }, InitPlan { fields: vec![space, zero, null, valued, replaced] }]);
    every_variant(&[InitValue::Default(Figurative::Zero), InitValue::Value(4), InitValue::Replacing(Operand::Const(2))], 3);
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
    let inspect = InspectPlan { target: Inspected::Item(0), tallying: vec![tally], replacing: vec![replace], converting: Some(converting) };
    round_trip(&[inspect, InspectPlan { target: Inspected::Value(Operand::Function(0)), tallying: vec![], replacing: vec![], converting: None }]);
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
    let targets = [CallTarget::Named { name: 1, le: Some(LeService::Ceedate) }, CallTarget::Dynamic(Operand::Load(2)), CallTarget::Pointer(3), CallTarget::Entry(4)];
    every_variant(&targets, 4);
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
        statements: vec![vec![(0, 0), (1, 1)]],
    };
    assert_eq!(encoded(&debug).0, [1, 1, 2, 0, 10, 16, 2, 5, 8, 1, 2, 0, 1, 1, 2, 0, 0, 1, 1]);
    let far = Pos { file: u16::MAX, line: u32::MAX, col: 0 };
    let unreached = Debug { sources: vec![], positions: vec![far, Pos::default(), far], ops: vec![vec![2], vec![]], statements: vec![vec![], vec![(0, 0), (0, 2)]] };
    round_trip(&[Debug::default(), debug, unreached]);
    let none = StringTable::default();
    assert_eq!(refused::<Debug>(&[0, 1, 0, 1, 0, 0], &none), (3, "a line of -1".into()));
    assert_eq!(refused::<Debug>(&[0, 1, 0x80, 0x80, 0x08, 0, 0, 0], &none), (2, "a file of 65536".into()));
}

#[test]
fn a_small_program_round_trips_byte_identically() {
    let program = payroll();
    let (bytes, strings) = encoded(&program);
    assert_eq!(strings.iter().collect::<Vec<_>>()[..4], ["SSRANGE", "$", "PAYROLL", "PAYROLL.cbl"]);
    let decoded = decode_all::<Program>("LIR", &bytes, &strings).unwrap();
    assert_eq!(decoded, program);
    assert_eq!(encoded(&decoded), (bytes.clone(), strings.clone()));
    for len in 0..bytes.len() {
        assert!(decode_all::<Program>("LIR", &bytes[..len], &strings).is_err(), "{len}");
    }
}
