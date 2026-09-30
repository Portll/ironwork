use crate::Pos;

mod oo;
pub use oo::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub id: String,
    /// Options from CBL and PROCESS cards, in the order written.
    pub options: Vec<String>,
    /// PROGRAM-ID ... IS INITIAL: WORKING-STORAGE starts afresh on every CALL.
    pub initial: bool,
    pub recursive: bool,
    pub working_storage: Vec<DataEntry>,
    /// LOCAL-STORAGE: fresh for every activation of the program.
    pub local_storage: Vec<DataEntry>,
    pub linkage: Vec<DataEntry>,
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
    pub organization: Organization,
    pub access: Access,
    pub record_key: Option<Ref>,
    /// ALTERNATE RECORD KEY items, and whether each allows duplicates.
    pub alternate_keys: Vec<(Ref, bool)>,
    pub relative_key: Option<Ref>,
    pub optional: bool,
    pub status: Option<Ref>,
    /// RECORDING MODE: F, V, U or S.
    pub recording: Option<char>,
    pub record_min: Option<u32>,
    pub record_max: Option<u32>,
    pub records: Vec<DataEntry>,
    /// FD ... REPORT IS: the reports written to the file.
    pub reports: Vec<String>,
    /// FD ... LINAGE: the logical page. An SD's is read and dropped, as IBM ignores it.
    pub linage: Option<Linage>,
    /// Described by SD: a sort or merge file, which needs no data set.
    pub sort: bool,
    pub pos: Pos,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Input,
    Output,
    Extend,
    InputOutput,
}

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
    Packed,
    Float1,
    Float2,
    National,
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
    pub picture: Option<String>,
    /// None when no USAGE is written; the item then inherits its group's.
    pub usage: Option<Usage>,
    pub value: Option<Literal>,
    pub redefines: Option<String>,
    /// OCCURS: the number of occurrences, or the most of them for OCCURS DEPENDING ON.
    pub occurs: Option<u32>,
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
    /// USAGE OBJECT REFERENCE class-name: the class; None for a universal reference.
    pub object_class: Option<String>,
    pub pos: Pos,
}

pub use rt::vocab::Figurative;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    Alnum(String),
    Hex(Vec<u8>),
    National(String),
    /// As written: optional sign, digits, optional decimal point.
    Number(String),
    Figurative(Figurative),
    All(Box<Literal>),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Numeric,
    Alphabetic,
    Positive,
    Negative,
    Zero,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Rel(Expr, RelOp, Expr),
    Class(Expr, Class),
    Name(Ref),
    /// After AND or OR, a bare name that is either a condition-name or the object of an
    /// abbreviated relation; which one depends on what the name resolves to.
    NameOrRel { subject: Expr, op: RelOp, name: Ref },
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
    /// VARYING, and each AFTER phrase, outermost first.
    Varying { varying: Box<Varying>, after: Vec<Varying>, test_after: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Move { from: Operand, to: Vec<Ref>, pos: Pos },
    Compute { targets: Vec<Target>, expr: Expr, size_error: Option<SizeError>, pos: Pos },
    /// ADD, SUBTRACT, MULTIPLY and DIVIDE, reduced to their arithmetic.
    Arith(Box<Arith>),
    If { cond: Cond, then: Vec<Stmt>, otherwise: Vec<Stmt>, pos: Pos },
    PerformInline { body: Vec<Stmt>, repeat: Loop, pos: Pos },
    PerformProc { from: ProcName, thru: Option<ProcName>, repeat: Loop, pos: Pos },
    Evaluate { subjects: Vec<Subject>, whens: Vec<When>, other: Vec<Stmt>, pos: Pos },
    Display { items: Vec<Operand>, no_advancing: bool, pos: Pos },
    Open { files: Vec<(OpenMode, String)>, pos: Pos },
    Close { files: Vec<String>, pos: Pos },
    Read(Box<ReadStmt>),
    Write { record: Ref, from: Option<Operand>, advancing: Option<Advancing>, invalid: Handlers, end_of_page: Handlers, pos: Pos },
    Rewrite { record: Ref, from: Option<Operand>, invalid: Handlers, pos: Pos },
    Delete { file: String, invalid: Handlers, pos: Pos },
    Start { file: String, key: Option<(RelOp, Ref)>, invalid: Handlers, pos: Pos },
    Initialize { targets: Vec<Ref>, pos: Pos },
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
    Accept { target: Ref, from: AcceptFrom, pos: Pos },
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
    ExitMethod { pos: Pos },
    Sorting(Box<Sorting>),
    StopRun { pos: Pos },
    Continue,
    Exit(ExitKind),
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
    /// SET targets TO value: an index or integer to a number, a pointer to ADDRESS OF, NULL or another pointer.
    To { targets: Vec<Ref>, value: Operand },
    /// SET ADDRESS OF targets TO pointer.
    AddressOf { targets: Vec<Ref>, value: Operand },
    UpDown { targets: Vec<Ref>, down: bool, by: Expr },
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unstring {
    pub source: Ref,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspect {
    pub target: Ref,
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
    /// SPECIAL-NAMES entries naming a printer channel, space suppression, a punch pocket or AFP:
    /// each mnemonic-name and its environment-name.
    pub mnemonics: Vec<(String, String)>,
    /// SOURCE-COMPUTER ... WITH DEBUGGING MODE: debugging lines and USE FOR DEBUGGING sections are
    /// compiled rather than read as comments.
    pub debugging_mode: bool,
}

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

