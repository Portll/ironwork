use crate::Pos;

mod oo;
pub use oo::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub id: String,
    /// Where its PROGRAM-ID names it.
    pub pos: Pos,
    /// Options from CBL and PROCESS cards, in the order written.
    pub options: Vec<String>,
    /// PROGRAM-ID ... IS INITIAL, or once compiled the INITIAL option: WORKING-STORAGE starts
    /// afresh on every CALL.
    pub initial: bool,
    pub recursive: bool,
    /// PROGRAM-ID ... IS COMMON, for a program another contains.
    pub common: bool,
    pub working_storage: Vec<DataEntry>,
    /// LOCAL-STORAGE: fresh for every activation of the program.
    pub local_storage: Vec<DataEntry>,
    pub linkage: Vec<DataEntry>,
    /// The SCREEN SECTION's entries, as written.
    pub screens: Vec<ScreenEntry>,
    /// PROCEDURE DIVISION USING: the LINKAGE items the caller's arguments address.
    pub using: Vec<Param>,
    pub returning: Option<String>,
    pub paragraphs: Vec<Paragraph>,
    /// Files declared by SELECT and described by FD, with their record descriptions.
    pub files: Vec<FileDecl>,
    /// The program's source file, then each COPY member, as positions index them.
    pub sources: Vec<String>,
    /// EXEC blocks in the DATA DIVISION: SQL declarations, cursors and DECLARE SECTION markers.
    pub exec_declarations: Vec<ExecBlock>,
    /// The REPORT SECTION, and the DECLARATIVES that serve it.
    pub report_writer: crate::report::ReportWriter,
    /// The DECLARATIVES' USE AFTER EXCEPTION/ERROR and USE FOR DEBUGGING procedures.
    pub declaratives: Declaratives,
    /// The REPOSITORY's classes, and for a class definition or a method what it is.
    pub oo: Option<Box<Oo>>,
    pub environment: Environment,
    /// The PROGRAM-IDs of the programs it directly contains.
    pub nested: Vec<String>,
    /// Special registers the PROCEDURE DIVISION names and no entry declares, which the compiler
    /// declares: WHEN-COMPILED.
    pub registers: Vec<String>,
    /// The contained programs of its compilation a CALL from it reaches (Language Reference,
    /// Conventions for program-names): those it directly contains, and each COMMON one that a
    /// program containing it directly contains, but itself and those that contain it.
    pub callable: Vec<String>,
    /// The other contained programs of its compilation, and those containing it, which a CALL from
    /// it does not reach.
    pub hidden: Vec<String>,
    /// For a separately compiled program, each name two of its programs share, where the second's
    /// PROGRAM-ID is.
    pub duplicates: Vec<(String, Pos)>,
    /// The names a static CALL from it finds in its compilation: its source's separately compiled
    /// programs and their ENTRY names, and in a bundle the other sources' programs.
    pub linked: Vec<String>,
    /// The programs that contain it, innermost first, with the names each declares GLOBAL.
    pub containers: Vec<Container>,
    /// The messages reading its source gave that did not stop the parse, in the order found.
    pub messages: Vec<crate::Error>,
    /// FUNCTION-ID in place of PROGRAM-ID: a user-defined function, or a prototype of one.
    pub function: Option<Function>,
    /// The user-defined functions it may invoke: those defined or prototyped before it in its
    /// source, and a function itself.
    pub prototypes: Vec<Prototype>,
    /// Under `--compliance extended`, the user-defined functions its source defines or prototypes
    /// after it that its REPOSITORY paragraph names, which GnuCOBOL lets it invoke.
    pub later_functions: Vec<Prototype>,
    /// Under `--compliance extended`, the user-defined functions its REPOSITORY paragraph names that
    /// its source does not define, as a program source of the compile's libraries defines them, with
    /// that source's file name.
    pub elsewhere_functions: Vec<(Prototype, String)>,
    /// The user-defined functions its REPOSITORY paragraph, or its outermost program's, names. One
    /// with an intrinsic function's name is invoked by that name only where it is named here.
    pub repository_functions: Vec<String>,
}

impl Program {
    /// Whether FUNCTION `name` invokes an intrinsic function rather than a user-defined one.
    pub fn intrinsic(&self, name: &str) -> bool {
        is_intrinsic_name(name) && !self.repository_functions.iter().any(|f| f == name)
    }
}

/// Whether `name` is an intrinsic function's name.
pub fn is_intrinsic_name(name: &str) -> bool {
    rt::intrinsic::FIRST.contains(&name) || rt::intrinsic::FUNCTIONS.contains(&name)
}

impl Program {
    /// The name a CALL or a function invocation loads it by: a function's external name.
    pub fn load_name(&self) -> &str {
        self.function.as_ref().map_or(&self.id, |f| &f.external)
    }

    pub fn is_prototype(&self) -> bool {
        self.function.as_ref().is_some_and(|f| f.prototype)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// AS literal-1, else the function-name.
    pub external: String,
    /// IS PROTOTYPE: a description for invocations to be checked against, with no code.
    pub prototype: bool,
    pub pos: Pos,
}

/// A user-defined function as its definition or prototype describes it to an invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prototype {
    pub name: String,
    pub external: String,
    pub using: Vec<Param>,
    pub returning: Option<String>,
    pub linkage: Vec<DataEntry>,
    /// The PICTURE notation of its definition: DECIMAL-POINT IS COMMA and the currency signs.
    pub environment: Environment,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub by_value: bool,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Organization {
    Sequential,
    LineSequential,
    Indexed,
    Relative,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Sequential,
    Random,
    Dynamic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDecl {
    pub name: String,
    /// The DD name ASSIGN gives, with any `UT-S-` style prefix removed.
    pub assign: String,
    /// ASSIGN's target where it can be a data item holding the file's name at each OPEN. The
    /// compile keeps it only where it is one, under `--compliance extended`.
    pub assign_item: Option<AssignItem>,
    pub organization: Organization,
    pub access: Access,
    pub record_key: Option<Ref>,
    /// ALTERNATE RECORD KEY items, and whether each allows duplicates.
    pub alternate_keys: Vec<(Ref, bool)>,
    /// Micro Focus's split keys under `--compliance extended`, KEY IS name = item ...: the name a
    /// record or alternate key above gives, and the items it joins, in order.
    pub split_keys: Vec<(String, Vec<Ref>)>,
    pub relative_key: Option<Ref>,
    pub optional: bool,
    pub status: Option<Ref>,
    /// FILE STATUS's second data-name, for a VSAM file: the return, function and feedback codes.
    pub vsam_status: Option<Ref>,
    /// PASSWORD IS items, checked and of no effect, as files here have no passwords.
    pub passwords: Vec<Ref>,
    /// RECORDING MODE: F, V, U or S.
    pub recording: Option<char>,
    pub record_min: Option<u32>,
    pub record_max: Option<u32>,
    /// RECORD IS VARYING: `record_min` and `record_max` are its FROM and TO, each None when not
    /// written (Language Reference SC27-8713-03, p. 187).
    pub record_varying: bool,
    /// RECORD IS VARYING ... DEPENDING ON: the item holding each record's length.
    pub record_depending: Option<Ref>,
    /// LABEL RECORD IS data-name: the names, which Enterprise COBOL resolves though it reads the
    /// clause as comments.
    pub label_records: Vec<Ref>,
    pub records: Vec<DataEntry>,
    /// FD ... REPORT IS: the reports written to the file.
    pub reports: Vec<String>,
    /// FD ... LINAGE: the logical page. An SD's is read and dropped, as IBM ignores it.
    pub linage: Option<Linage>,
    /// Described by SD: a sort or merge file, which needs no data set.
    pub sort: bool,
    pub external: bool,
    pub global: bool,
    /// For a GLOBAL file of a program containing this one, that program's PROGRAM-ID.
    pub declared_in: Option<String>,
    pub pos: Pos,
}

impl FileDecl {
    /// The items split key `name` joins, where `name` is one of the file's split keys.
    pub fn split_key(&self, name: &str) -> Option<&[Ref]> {
        self.split_keys.iter().find(|(key, _)| key == name).map(|(_, pieces)| pieces.as_slice())
    }
}

/// ASSIGN TO a name, or ASSIGN TO DYNAMIC or USING a data-name: Micro Focus and GnuCOBOL take a
/// name that is a data item's, and the other two forms always, as the item holding the file's
/// name (Enterprise COBOL's assignment-name is never a data item, Language Reference
/// SC27-8713-03, ASSIGN clause).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignItem {
    pub reference: Ref,
    /// DYNAMIC or USING was written.
    pub explicit: bool,
    /// Under `--compliance extended`, an ASSIGN TO DISK name, or an ASSIGN TO name the PROCEDURE
    /// DIVISION uses: the item is declared for the program where it declares none.
    pub declared_if_missing: bool,
    /// The name as the source spells it, the value of an item declared for it.
    pub spelled: String,
}

/// A program containing another, as the contained program sees it: its PROGRAM-ID, and the 01
/// records and files it declares GLOBAL, each with what is subordinate to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub working_storage: Vec<DataEntry>,
    pub local_storage: Vec<DataEntry>,
    pub linkage: Vec<DataEntry>,
    pub files: Vec<FileDecl>,
}

/// LINAGE IS lines [WITH FOOTING AT footing] [LINES AT TOP top] [LINES AT BOTTOM bottom].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linage {
    pub lines: LinageValue,
    pub footing: Option<LinageValue>,
    pub top: Option<LinageValue>,
    pub bottom: Option<LinageValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinageValue {
    /// The digits as written, whose count sizes LINAGE-COUNTER.
    Integer(String),
    Data(Ref),
}

pub use rt::vocab::{Closing, OpenMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Advancing {
    Lines { before: bool, count: Expr },
    Page { before: bool },
    /// A mnemonic-name of SPECIAL-NAMES, with the environment-name it stands for: C01 to C12,
    /// CSP, S01 to S05 or AFP-5A.
    Mnemonic { before: bool, name: String, environment: String },
}

impl Advancing {
    pub fn before(&self) -> bool {
        match self {
            Self::Lines { before, .. } | Self::Page { before } | Self::Mnemonic { before, .. } => *before,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Usage {
    #[default]
    Display,
    Binary,
    /// COMP-5: binary, never truncated to the PICTURE.
    NativeBinary,
    /// GnuCOBOL's and Micro Focus's BINARY-CHAR [SIGNED|UNSIGNED]: one byte of binary, no PICTURE.
    BinaryChar { signed: bool },
    /// Micro Focus's COMP-X: binary in the fewest bytes that hold the PICTURE's digits, or n bytes
    /// for PIC X(n).
    CompX,
    Packed,
    Float1,
    Float2,
    National,
    /// DISPLAY-1: DBCS characters, two bytes each.
    Dbcs,
    Pointer,
    Index,
    ObjectReference,
    /// FUNCTION-POINTER or PROCEDURE-POINTER.
    ProgramPointer,
}

pub use rt::vocab::{SignClause, SignPosition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataEntry {
    pub level: u8,
    /// None for FILLER or an unnamed entry.
    pub name: Option<String>,
    /// The name as the source spells it, where that is not all capitals: JSON and XML GENERATE
    /// keep it.
    pub spelled: Option<String>,
    pub picture: Option<String>,
    /// None when no USAGE is written; the item then inherits its group's.
    pub usage: Option<Usage>,
    pub value: Option<Literal>,
    pub redefines: Option<String>,
    /// OCCURS: the number of occurrences, or the most of them for OCCURS DEPENDING ON.
    /// The OCCURS maximum, [`UNBOUNDED`] for OCCURS ... TO UNBOUNDED.
    pub occurs: Option<u32>,
    /// OCCURS ... DEPENDING ON: the fewest occurrences, 1 when no integer-1 TO is written
    /// (Language Reference SC27-8713-03, p. 204).
    pub occurs_min: Option<u32>,
    /// OCCURS ... DEPENDING ON: the item that holds how many occurrences there are.
    pub depending_on: Option<Ref>,
    pub sign: Option<SignClause>,
    pub justified: bool,
    pub sync: bool,
    pub blank_when_zero: bool,
    /// OCCURS ... INDEXED BY: the index names the table declares.
    pub indexed_by: Vec<String>,
    /// OCCURS ... ASCENDING/DESCENDING KEY: each key and whether it ascends, for SEARCH ALL.
    pub keys: Vec<(bool, Ref)>,
    /// Level 88: the values that make the condition true.
    /// Each value, or the low and high ends of a THRU range.
    pub condition_values: Vec<(Literal, Option<Literal>)>,
    /// Level 88 WHEN SET TO FALSE: the value SET ... TO FALSE stores.
    pub false_value: Option<Literal>,
    /// Level 66 RENAMES: the item renamed, or the first and last of a THRU range.
    pub renames: Option<(Ref, Option<Ref>)>,
    /// USAGE OBJECT REFERENCE class-name: the class; None for a universal reference.
    pub object_class: Option<String>,
    /// EXTERNAL, written on the entry or attained from its FD.
    pub external: bool,
    /// GLOBAL, written on the entry or attained from its FD.
    pub global: bool,
    /// ANY LENGTH, GnuCOBOL's and Micro Focus's parameter as long as its argument, read under
    /// `--compliance extended`.
    pub any_length: bool,
    /// BASED, GnuCOBOL's and Micro Focus's item with no storage until SET ADDRESS OF gives it some,
    /// read under `--compliance extended`.
    pub based: bool,
    pub pos: Pos,
}

pub use rt::vocab::Figurative;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    Alnum(String),
    Hex(Vec<u8>),
    National(String),
    /// G'...', or N'...' under NSYMBOL(DBCS): characters the code page's DBCS part encodes.
    Dbcs(String),
    /// As written: optional sign, digits, optional decimal point.
    Number(String),
    Figurative(Figurative),
    All(Box<Literal>),
    /// A floating-point literal as written: mantissa, E, exponent.
    Float(String),
}

/// A paragraph, or a section header holding the statements before the section's first paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    pub name: String,
    pub statements: Vec<Stmt>,
    /// The section the paragraph belongs to, or the section's own name for its header.
    pub section: Option<String>,
    pub is_section: bool,
    /// The section's priority-number, 0 when it has none: 50 to 99 is an independent segment.
    pub priority: u8,
    pub pos: Pos,
}

/// A procedure name as written, with the section that qualifies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcName {
    pub name: String,
    pub section: Option<String>,
}

/// USE AFTER EXCEPTION/ERROR and USE FOR DEBUGGING sections; USE BEFORE REPORTING ones are the
/// Report Writer's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declaratives {
    pub errors: Vec<UseAfterError>,
    pub debugging: Vec<UseForDebugging>,
}

/// A USE AFTER STANDARD EXCEPTION/ERROR PROCEDURE section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseAfterError {
    /// The section's header among the program's paragraphs.
    pub section: usize,
    pub global: bool,
    pub on: ErrorUse,
    pub pos: Pos,
}

/// The files an EXCEPTION/ERROR procedure serves: those it names, or those open in one mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorUse {
    Files(Vec<String>),
    Mode(OpenMode),
}

/// A USE FOR DEBUGGING section, which runs before each procedure it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseForDebugging {
    pub section: usize,
    /// Empty for ALL PROCEDURES.
    pub procedures: Vec<ProcName>,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitKind {
    Plain,
    Paragraph,
    Section,
    Perform,
    PerformCycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    Bool(bool),
    Expr(Expr),
    Cond(Cond),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Object {
    Any,
    Bool(bool),
    Cond(Cond),
    Value { not: bool, from: Expr, thru: Option<Expr> },
}

/// One WHEN group: any of its alternatives, each with an object per subject, selects the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct When {
    pub alternatives: Vec<Vec<Object>>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub name: String,
    pub qualifiers: Vec<String>,
    pub subscripts: Vec<Expr>,
    pub refmod: Option<RefMod>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefMod {
    pub start: Box<Expr>,
    pub length: Option<Box<Expr>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<Expr>,
    /// A keyword argument, as in FUNCTION TRIM(X LEADING).
    pub modifier: Option<String>,
    pub refmod: Option<RefMod>,
    /// Each argument written as a table with ALL subscripts, as in FUNCTION SUM(T(ALL)): its index
    /// and the positions of those subscripts, where its reference holds 1.
    pub all_subscripts: Vec<(usize, Vec<usize>)>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Ref(Ref),
    Literal(Literal),
    Function(FunctionCall),
    LengthOf(Ref),
    AddressOf(Ref),
}

pub use rt::vocab::{AcceptFrom, BinOp, RelOp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Operand(Operand),
    Neg(Box<Expr>),
    Bin(Box<Expr>, BinOp, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    Numeric,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
    /// Every two bytes a DBCS character, X'41' to X'FE' each, or the DBCS space.
    Dbcs,
    /// Every two bytes a DBCS character with a first byte X'41' to X'7E', or the DBCS space.
    Kanji,
    Positive,
    Negative,
    Zero,
    /// A class-name of the SPECIAL-NAMES CLASS clause.
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Rel(Expr, RelOp, Expr),
    Class(Expr, Class),
    Name(Ref),
    /// After AND or OR, a bare name that is either a condition-name or the object of an
    /// abbreviated relation; which one depends on what the name resolves to. `negated` is a NOT
    /// the relational operator carries, as in NOT =.
    NameOrRel { subject: Expr, op: RelOp, negated: bool, name: Ref },
    Not(Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub r: Ref,
    pub rounded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Varying {
    pub var: Ref,
    pub from: Expr,
    pub by: Expr,
    pub until: Cond,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loop {
    Once,
    Times(Expr),
    Until { cond: Cond, test_after: bool },
    /// Micro Focus's and GnuCOBOL's FOREVER under `--compliance extended`: until EXIT PERFORM, GO
    /// TO, GOBACK or STOP RUN leaves it.
    Forever,
    /// VARYING, and each AFTER phrase, outermost first.
    Varying { varying: Box<Varying>, after: Vec<Varying>, test_after: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Move { from: Operand, to: Vec<Ref>, pos: Pos },
    Compute { targets: Vec<Target>, expr: Expr, size_error: Option<SizeError>, pos: Pos },
    /// ADD, SUBTRACT, MULTIPLY and DIVIDE, reduced to their arithmetic.
    Arith(Box<Arith>),
    /// MOVE, ADD or SUBTRACT CORRESPONDING, which the compiler expands into a MOVE per pair of
    /// corresponding items, or one ADD or SUBTRACT over them all.
    Corresponding(Box<Corresponding>),
    If { cond: Cond, then: Vec<Stmt>, otherwise: Vec<Stmt>, pos: Pos },
    PerformInline { body: Vec<Stmt>, repeat: Loop, pos: Pos },
    PerformProc { from: ProcName, thru: Option<ProcName>, repeat: Loop, pos: Pos },
    Evaluate { subjects: Vec<Subject>, whens: Vec<When>, other: Vec<Stmt>, pos: Pos },
    /// DISPLAY; `screen` holds Micro Focus's and GnuCOBOL's screen phrases, or UPON CRT, which
    /// write the items on the screen instead of a device.
    Display { items: Vec<Operand>, upon: Option<Upon>, no_advancing: bool, screen: Option<Box<ScreenPhrases>>, pos: Pos },
    Open { files: Vec<(OpenMode, String)>, pos: Pos },
    Close { files: Vec<(String, Option<Closing>)>, pos: Pos },
    Read(Box<ReadStmt>),
    Write { record: Ref, from: Option<Operand>, advancing: Option<Advancing>, invalid: Handlers, end_of_page: Handlers, pos: Pos },
    Rewrite { record: Ref, from: Option<Operand>, invalid: Handlers, pos: Pos },
    Delete { file: String, invalid: Handlers, pos: Pos },
    /// Micro Focus's and GnuCOBOL's DELETE FILE under `--compliance extended`: each closed file's
    /// data set removed.
    DeleteFile { files: Vec<String>, pos: Pos },
    Start { file: String, key: Option<(RelOp, Ref)>, invalid: Handlers, pos: Pos },
    /// INITIALIZE; `with` holds its FILLER, VALUE, REPLACING and DEFAULT phrases, None without any.
    Initialize { targets: Vec<Ref>, with: Option<Box<InitializeWith>>, pos: Pos },
    /// GO TO; with no target, the altered GO TO that only an ALTER gives one.
    GoTo { target: Option<ProcName>, pos: Pos },
    /// GO TO ... DEPENDING ON: the procedure the item's value numbers, or on when none does.
    GoToDepending { targets: Vec<ProcName>, on: Ref, pos: Pos },
    /// ALTER: each paragraph whose GO TO is to go instead to the procedure paired with it.
    Alter { pairs: Vec<(ProcName, ProcName)>, pos: Pos },
    /// ENTRY: where a CALL of `name` begins, and the LINKAGE items its USING list addresses.
    Entry { name: String, using: Vec<Param>, pos: Pos },
    Goback { pos: Pos },
    /// EXIT PROGRAM: returns from a called program; in the first program it does nothing.
    ExitProgram { pos: Pos },
    Call(Box<Call>),
    Cancel { targets: Vec<Operand>, pos: Pos },
    Set { set: SetStmt, pos: Pos },
    /// ACCEPT; `exception` is the ON EXCEPTION phrases `--compliance extended` reads with
    /// ARGUMENT-VALUE, and `screen` the screen phrases, or FROM CRT, that read the target from a
    /// field of the screen.
    Accept { target: Ref, from: AcceptFrom, exception: Handlers, screen: Option<Box<ScreenPhrases>>, pos: Pos },
    String(Box<StringStmt>),
    Unstring(Box<Unstring>),
    Inspect(Box<Inspect>),
    Search(Box<Search>),
    /// NEXT SENTENCE: control passes to the statement after the next separator period.
    NextSentence,
    /// A separator period in the PROCEDURE DIVISION: where NEXT SENTENCE resumes.
    SentenceEnd,
    Exec(Box<ExecBlock>),
    Report(Box<crate::report::ReportStmt>),
    Invoke(Box<Invoke>),
    JsonGenerate(Box<JsonGenerate>),
    JsonParse(Box<JsonParse>),
    XmlParse(Box<XmlParse>),
    XmlGenerate(Box<XmlGenerate>),
    ExitMethod { pos: Pos },
    Sorting(Box<Sorting>),
    StopRun { pos: Pos },
    Continue { pos: Pos },
    Exit { kind: ExitKind, pos: Pos },
    /// What `--compliance relaxed` compiled in place of a sentence or statement ironwork refuses:
    /// a run that reaches it ends with IWR0078, naming `construct` and `why` it was refused.
    Hole { construct: String, why: String, pos: Pos },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeError {
    pub on: Vec<Stmt>,
    pub not_on: Vec<Stmt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithVerb {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// One arithmetic statement: `targets` each receive `expr`, which is written in terms of the
/// statement's operands and, for the forms without GIVING, the target itself (`Operand::Ref` of
/// the target, placed by the parser).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arith {
    pub verb: ArithVerb,
    pub computations: Vec<(Target, Expr)>,
    pub remainder: Option<(Target, Expr, Expr)>,
    pub size_error: Option<SizeError>,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorrespondingVerb {
    Move,
    Add,
    Subtract,
}

/// `from` is the sending group and `to` the receiving one; `rounded` and `size_error` are ADD's
/// and SUBTRACT's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corresponding {
    pub verb: CorrespondingVerb,
    pub from: Ref,
    pub to: Ref,
    pub rounded: bool,
    pub size_error: Option<SizeError>,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgMode {
    Reference,
    Content,
    Value,
}

/// One CALL argument; `value` is None for OMITTED.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arg {
    pub mode: ArgMode,
    pub value: Option<Operand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub target: Operand,
    pub using: Vec<Arg>,
    pub returning: Option<Ref>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetStmt {
    ConditionTrue(Vec<Ref>),
    /// SET condition-names TO FALSE: each conditional variable gets its WHEN SET TO FALSE value.
    ConditionFalse(Vec<Ref>),
    /// SET targets TO value: an index or integer to a number, a pointer to ADDRESS OF, NULL or another pointer.
    To { targets: Vec<Ref>, value: Operand },
    /// SET procedure-pointers or function-pointers TO ENTRY, the entry named by a literal or identifier.
    Entry { targets: Vec<Ref>, entry: Operand },
    /// SET ADDRESS OF targets TO pointer.
    AddressOf { targets: Vec<Ref>, value: Operand },
    UpDown { targets: Vec<Ref>, down: bool, by: Expr },
    /// SET mnemonic-names TO ON or OFF, each group in order: the UPSI switches the names stand for.
    Switches(Vec<(Vec<Ref>, bool)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Size,
    By(Operand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringStmt {
    pub sources: Vec<(Operand, Delimiter)>,
    pub into: Ref,
    pub pointer: Option<Ref>,
    pub on_overflow: Option<Vec<Stmt>>,
    pub not_on_overflow: Option<Vec<Stmt>>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnstringInto {
    pub target: Ref,
    pub delimiter_in: Option<Ref>,
    pub count_in: Option<Ref>,
}

/// The OCCURS maximum of a table with no upper bound, OCCURS n TO UNBOUNDED DEPENDING ON, which
/// Enterprise COBOL describes in the LINKAGE SECTION.
pub const UNBOUNDED: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unstring {
    /// The sending field: a data item, or under `--compliance extended` a function's value.
    pub source: Operand,
    /// Each delimiter, and whether ALL makes a run of it one delimiter.
    pub delimiters: Vec<(bool, Operand)>,
    pub into: Vec<UnstringInto>,
    pub pointer: Option<Ref>,
    pub tallying: Option<Ref>,
    pub on_overflow: Option<Vec<Stmt>>,
    pub not_on_overflow: Option<Vec<Stmt>>,
    pub pos: Pos,
}

/// BEFORE or AFTER INITIAL value: where in the inspected item a phrase applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub after: bool,
    pub value: Operand,
}

pub use rt::vocab::InspectMode;

/// One TALLYING or REPLACING phrase. `pattern` is None for CHARACTERS; `by` is None for TALLYING.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectPhrase {
    pub mode: InspectMode,
    pub pattern: Option<Operand>,
    pub by: Option<Operand>,
    pub counter: Option<Ref>,
    pub bounds: Vec<Bound>,
}

/// The categories INITIALIZE's VALUE and REPLACING phrases name (Language Reference SC27-8713-03,
/// p. 350); EGCS is DBCS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataCategory {
    Alphabetic,
    Alphanumeric,
    AlphanumericEdited,
    Dbcs,
    National,
    NationalEdited,
    Numeric,
    NumericEdited,
    Utf8,
}

impl DataCategory {
    pub const ALL: [Self; 9] = [
        Self::Alphabetic,
        Self::Alphanumeric,
        Self::AlphanumericEdited,
        Self::Dbcs,
        Self::National,
        Self::NationalEdited,
        Self::Numeric,
        Self::NumericEdited,
        Self::Utf8,
    ];

    pub fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "ALPHABETIC" => Self::Alphabetic,
            "ALPHANUMERIC" => Self::Alphanumeric,
            "ALPHANUMERIC-EDITED" => Self::AlphanumericEdited,
            "DBCS" | "EGCS" => Self::Dbcs,
            "NATIONAL" => Self::National,
            "NATIONAL-EDITED" => Self::NationalEdited,
            "NUMERIC" => Self::Numeric,
            "NUMERIC-EDITED" => Self::NumericEdited,
            "UTF-8" => Self::Utf8,
            _ => return None,
        })
    }

    pub fn word(self) -> &'static str {
        match self {
            Self::Alphabetic => "ALPHABETIC",
            Self::Alphanumeric => "ALPHANUMERIC",
            Self::AlphanumericEdited => "ALPHANUMERIC-EDITED",
            Self::Dbcs => "DBCS",
            Self::National => "NATIONAL",
            Self::NationalEdited => "NATIONAL-EDITED",
            Self::Numeric => "NUMERIC",
            Self::NumericEdited => "NUMERIC-EDITED",
            Self::Utf8 => "UTF-8",
        }
    }
}

/// INITIALIZE's phrases. `value` lists the VALUE phrase's categories, every one for ALL TO VALUE.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitializeWith {
    pub filler: bool,
    pub value: Vec<DataCategory>,
    pub replacing: Vec<(DataCategory, Operand)>,
    pub default: bool,
}

/// What an elementary receiver of INITIALIZE is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialValue<'a> {
    /// The literal of the item's own VALUE clause.
    Value,
    Replacing(&'a Operand),
    /// SPACE, ZERO or NULL, by the item's category.
    Default,
}

impl InitializeWith {
    /// Rules 1c and 2 of INITIALIZE (Language Reference SC27-8713-03, pp. 352-353) for an
    /// elementary item of `category`, which `has_value` when its entry has a VALUE clause; None when
    /// the item is not a receiver.
    pub fn initial_value(&self, category: Option<DataCategory>, has_value: bool) -> Option<InitialValue<'_>> {
        let named = |c: DataCategory| category == Some(c);
        if has_value && self.value.iter().any(|&c| named(c)) {
            return Some(InitialValue::Value);
        }
        if let Some((_, by)) = self.replacing.iter().find(|(c, _)| named(*c)) {
            return Some(InitialValue::Replacing(by));
        }
        (self.default || self.value.is_empty() && self.replacing.is_empty()).then_some(InitialValue::Default)
    }
}

/// `target` is a data item or, for TALLYING alone, a function's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspect {
    pub target: Operand,
    pub tallying: Vec<InspectPhrase>,
    pub replacing: Vec<InspectPhrase>,
    pub converting: Option<(Operand, Operand, Vec<Bound>)>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    pub table: Ref,
    pub all: bool,
    pub varying: Option<Ref>,
    pub at_end: Option<Vec<Stmt>>,
    pub whens: Vec<(Cond, Vec<Stmt>)>,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecKind {
    Sql,
    Cics,
    Dli,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecArg {
    Operand(Operand),
    /// An argument that is not data: a paragraph for HANDLE CONDITION, or text that did not parse.
    Text(String),
}

/// An EXEC ... END-EXEC block, read but not translated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecBlock {
    pub kind: ExecKind,
    /// The command: SELECT, INCLUDE, DECLARE CURSOR, LINK, SEND MAP and so on.
    pub command: String,
    /// CICS options, each with its argument.
    pub options: Vec<(String, Option<ExecArg>)>,
    /// SQL host variables and indicator variables.
    pub host_variables: Vec<Ref>,
    /// The typed SQL statement, for EXEC SQL.
    pub sql: Option<crate::sql::Sql>,
    pub text: String,
    pub pos: Pos,
}

impl ExecBlock {
    /// Whether the block only declares, so a precompiler turns it into data or nothing.
    pub fn declarative(&self) -> bool {
        self.kind == ExecKind::Sql
            && matches!(self.command.as_str(), "INCLUDE" | "BEGIN DECLARE SECTION" | "END DECLARE SECTION" | "WHENEVER" | "DECLARE CURSOR" | "DECLARE TABLE" | "DECLARE STATEMENT")
    }
}

/// The statements of an ON phrase (AT END, INVALID KEY, ...) and of its NOT ON phrase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handlers {
    pub on: Option<Vec<Stmt>>,
    pub not_on: Option<Vec<Stmt>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadStmt {
    pub file: String,
    /// READ NEXT or PREVIOUS: the next record by the key of reference, even under dynamic access.
    pub next: bool,
    pub previous: bool,
    pub into: Option<Ref>,
    /// READ ... KEY IS: the key of reference for a random read.
    pub key: Option<Ref>,
    pub at_end: Handlers,
    pub invalid: Handlers,
    pub pos: Pos,
}

/// What an ALPHABET clause relates its alphabet-name to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alphabet {
    Ebcdic,
    Native,
    Standard1,
    Standard2,
    /// A collating sequence of the program's own, lowest position first.
    Literal(Vec<AlphabetEntry>),
}

impl Alphabet {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ebcdic => "EBCDIC",
            Self::Native => "NATIVE",
            Self::Standard1 => "STANDARD-1",
            Self::Standard2 => "STANDARD-2",
            Self::Literal(_) => "literal",
        }
    }
}

/// One literal of an ALPHABET clause: its characters in successive positions, a THROUGH range of
/// characters in successive positions, or characters that ALSO share one position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlphabetEntry {
    Literal(Literal),
    Through(Literal, Literal),
    Also(Vec<Literal>),
}

/// ENVIRONMENT DIVISION clauses beyond SELECT that the program's meaning depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    /// SPECIAL-NAMES ALPHABET: each alphabet-name and what it is.
    pub alphabets: Vec<(String, Alphabet)>,
    /// OBJECT-COMPUTER PROGRAM COLLATING SEQUENCE.
    pub collating_sequence: Option<String>,
    /// I-O-CONTROL SAME RECORD AREA and SAME AREA clauses, each with the files it names.
    pub same_record_areas: Vec<Vec<String>>,
    pub same_areas: Vec<Vec<String>>,
    /// SPECIAL-NAMES entries naming a printer channel, space suppression, a punch pocket, AFP, or
    /// a device ACCEPT or DISPLAY takes: each mnemonic-name and its environment-name.
    pub mnemonics: Vec<(String, String)>,
    /// SOURCE-COMPUTER ... WITH DEBUGGING MODE: debugging lines and USE FOR DEBUGGING sections are
    /// compiled rather than read as comments.
    pub debugging_mode: bool,
    /// SPECIAL-NAMES DECIMAL-POINT IS COMMA: the comma and the period exchange roles in PICTURE
    /// character-strings, numeric literals and the arguments of NUMVAL and NUMVAL-C.
    pub decimal_point_comma: bool,
    /// SPECIAL-NAMES CURRENCY SIGN clauses in order; none means the symbol and value $.
    pub currency: Vec<CurrencySign>,
    /// SPECIAL-NAMES UPSI-0 to UPSI-7 entries, a contained program's being its container's.
    pub switches: Vec<Switch>,
    /// SPECIAL-NAMES CLASS clauses, a contained program's being its container's.
    pub classes: Vec<ClassClause>,
}

/// SPECIAL-NAMES CLASS class-name IS: each literal, or the two ends of a THROUGH range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassClause {
    pub name: String,
    pub members: Vec<(Literal, Option<Literal>)>,
    pub pos: Pos,
}

/// A SPECIAL-NAMES entry for an UPSI switch: UPSI-`number` [IS mnemonic-name] with the
/// condition-names of its ON and OFF status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switch {
    pub number: u8,
    pub mnemonic: Option<String>,
    pub on: Option<String>,
    pub off: Option<String>,
    pub pos: Pos,
}

/// Micro Focus's and GnuCOBOL's phrases that put a DISPLAY's items or an ACCEPT's field on the
/// screen: where, what is cleared first, and how the field behaves. `attributes` keeps the others
/// by name, as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenPhrases {
    pub at: Option<ScreenAt>,
    pub blank_screen: bool,
    pub blank_line: bool,
    pub erase_eol: bool,
    pub erase_eos: bool,
    pub update: bool,
    pub secure: bool,
    pub attributes: Vec<String>,
    /// An ACCEPT of a SCREEN SECTION's screen: the screen's name, and its TO and USING fields,
    /// which the compiler puts here; ACCEPT OMITTED's name is the parser's ACCEPT_OMITTED, with no
    /// field. A positioned ACCEPT has neither, its one field its target.
    pub screen: Option<String>,
    pub inputs: Vec<ScreenInput>,
    pub pos: Pos,
}

/// A TO or USING field of a screen: the item that shows it, as its PICTURE edits, the item that
/// takes the entry, and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenInput {
    pub field: Ref,
    pub target: Ref,
    pub line: u32,
    pub column: u32,
    pub update: bool,
    pub secure: bool,
}

/// An entry of the SCREEN SECTION, as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenEntry {
    pub level: u8,
    pub name: Option<String>,
    pub line: Option<ScreenPlace>,
    pub column: Option<ScreenPlace>,
    pub value: Option<Literal>,
    pub picture: Option<String>,
    pub from: Option<Operand>,
    pub to: Option<Ref>,
    pub using: Option<Ref>,
    pub blank_screen: bool,
    pub blank_line: bool,
    pub erase_eol: bool,
    pub erase_eos: bool,
    pub secure: bool,
    pub attributes: Vec<String>,
    pub pos: Pos,
}

/// A SCREEN SECTION entry's LINE or COLUMN: a number, or PLUS or MINUS one relative to the entry
/// before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPlace {
    At(u32),
    Plus(u32),
    Minus(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenAt {
    /// AT and four or six digits, or an item holding them: the line, then the column.
    Combined(Operand),
    LineColumn { line: Option<Operand>, column: Option<Operand> },
}

/// DISPLAY's UPON phrase: the name as written, and the environment-name it stands for, a
/// SPECIAL-NAMES mnemonic-name's or the name itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upon {
    pub name: String,
    pub device: String,
}

/// A CURRENCY SIGN clause: the currency sign value, and the PICTURE symbol that stands for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencySign {
    pub value: String,
    pub symbol: char,
    /// CURRENCY SIGN IS X'...': the literal's bytes, which the program's code page makes `value`,
    /// and `symbol` too when there is no PICTURE SYMBOL (`symbol` is [`HEX_SYMBOL`] until then).
    pub hex: Option<Vec<u8>>,
}

/// The symbol of a hexadecimal CURRENCY SIGN without PICTURE SYMBOL before its code page decodes it.
pub const HEX_SYMBOL: char = '\0';

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sorting {
    Sort(SortStmt),
    Release { record: Ref, from: Option<Operand>, pos: Pos },
    Return { file: String, into: Option<Ref>, at_end: Handlers, pos: Pos },
}

/// SORT or MERGE of an SD file, or SORT of a table (no input or output then).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortStmt {
    pub merge: bool,
    /// The SD file, or the table.
    pub subject: Ref,
    /// Each key and whether it ascends, most significant first.
    pub keys: Vec<(bool, Ref)>,
    pub duplicates: bool,
    pub collating: Option<String>,
    pub input: Option<SortIo>,
    pub output: Option<SortIo>,
    pub pos: Pos,
}

/// USING or GIVING files, or an input or output procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortIo {
    Files(Vec<String>),
    Procedure { from: ProcName, thru: Option<ProcName> },
}


/// JSON GENERATE (Language Reference SC27-8713-03, pp. 369-382).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGenerate {
    pub receiver: Ref,
    pub from: Ref,
    pub count: Option<Ref>,
    /// NAME OF item IS literal; `None` for OMITTED.
    pub names: Vec<(Ref, Option<Literal>)>,
    pub suppress: Vec<Suppression>,
    pub converting: Vec<(Ref, JsonConversion)>,
    pub indicating: Vec<NullIndicator>,
    pub encoding: Option<Encoding>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}

/// A SUPPRESS phrase: an item, or EVERY item of a class (`Some(true)` NUMERIC, `Some(false)`
/// NONNUMERIC, `None` both) and, for XML GENERATE, of a form, suppressed always or only WHEN it
/// equals one of the figurative constants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Suppression {
    Item { item: Ref, when: Vec<Figurative> },
    Every { numeric: Option<bool>, form: Option<XmlForm>, when: Vec<Figurative> },
}

/// A value that stands for true (CONVERTING ... TO JSON BOOLEAN) or for null (INDICATING): a
/// condition-name, or a one-character literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marker {
    Condition(Ref),
    Literal(Literal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonConversion {
    Boolean(Marker),
    Null(Figurative),
}

/// INDICATING item IS JSON NULL USING a condition-name of the indicator, or a literal IN it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullIndicator {
    pub item: Ref,
    pub marker: Marker,
    pub indicator: Option<Ref>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Encoding {
    Ccsid(Operand),
    FromCodepage,
}

/// JSON PARSE (Language Reference SC27-8713-03, pp. 382-396).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonParse {
    pub source: Ref,
    pub into: Ref,
    pub detail: bool,
    /// IGNORING JSON NULL FOR an item, or `None` FOR ALL.
    pub ignoring: Vec<Option<Ref>>,
    /// INDICATING item IS JSON NULL USING values, with IN and the indicator for two literals.
    pub indicating: Vec<(Ref, Flag, Option<Ref>)>,
    pub encoding: Option<Encoding>,
    /// NAME OF item IS literal; `None` for OMITTED.
    pub names: Vec<(Ref, Option<Literal>)>,
    pub suppress: Vec<Ref>,
    pub converting: Vec<(Ref, ParseConversion)>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}

/// What a USING phrase of JSON PARSE sets for true, or for null, and for false, or not null: a
/// condition-name set to true or to its WHEN SET TO FALSE value, one of two condition-names set to
/// true, or one of two literals moved in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Flag {
    Condition(Ref),
    Conditions(Ref, Ref),
    Literals(Literal, Literal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseConversion {
    Boolean(Box<Flag>),
    Null(Figurative),
}

/// XML GENERATE (Language Reference SC27-8713-03, pp. 484-494).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlGenerate {
    pub receiver: Ref,
    pub from: Ref,
    pub count: Option<Ref>,
    /// WITH ENCODING: the document's CCSID.
    pub encoding: Option<Operand>,
    pub declaration: bool,
    pub attributes: bool,
    pub namespace: Option<Operand>,
    pub prefix: Option<Operand>,
    pub names: Vec<(Ref, Literal)>,
    pub types: Vec<(Ref, XmlForm)>,
    pub suppress: Vec<Suppression>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}

/// How XML GENERATE expresses an item: as an attribute or an element, or as its parent's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmlForm {
    Attribute,
    Element,
    Content,
}

/// XML PARSE under XMLPARSE(XMLSS) (Language Reference SC27-8713-03, pp. 489-494).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlParse {
    pub document: Ref,
    /// WITH ENCODING: the document's CCSID.
    pub encoding: Option<Operand>,
    pub returning_national: bool,
    pub procedure: ProcName,
    pub thru: Option<ProcName>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}
