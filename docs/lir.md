# The LIR

The lowered program the VM runs and a Rust emitter would read: its types, how each construct the
interpreter runs lowers into it, and how lowering is checked.

**Status:** draft, 2026-09-30, for the operator's review. Nothing here is built. It details §7 of
[codegen-runtime.md](codegen-runtime.md), and step 2 of its §14 builds it.
[semantics-library.md](semantics-library.md) gives the library the LIR calls, and
[load-module.md](load-module.md) the file that holds it.

**Citations** are `crate/path.rs:line` on main at 79a199e (the ironwork-codegen checkout), or,
marked **(int)**, on the integration branch `feat/le-under-cics` at 0e73f5d (the ironwork-le-cics
checkout), which has SORT and MERGE, LE callable services, Report Writer and OO COBOL, or, marked
**(f2)**, on main at f201664, where the second lowering slice (CALL, INVOKE, ALTER and the rest of
§8.9) starts, or, marked **(7af)**, on main at 7af8643, whose walker runs PERFORM by return points
(§8.4).

---

## 1. What lowering changes

- **The walker resolves as it runs.** On every execution `Machine::locate`
  (exec/src/machine.rs:538-590) looks a `Ref` up by name and works out its base, subscripts, OCCURS
  DEPENDING ON and reference modification; `arithmetic` (machine.rs:1491-1539) works out dmax, fixed
  or float, and each receiver's store; `assign` (machine.rs:1657-1753) chooses a MOVE by the two
  categories.
- **The LIR holds the answers.** Every name is resolved, every category decided, and every transfer
  of control is an explicit edge. Run time is left with data: values, subscripts, lengths, LINKAGE
  addresses, and the state of files, CICS, the database and objects.
- **Both executors read the same plans.** Plans are compile-time data in `rt`. The VM executes them,
  and step 2 changes the walker to read them too, so the two cannot choose differently.
- **Lowering never changes a result.** Where the walker's order of locates, reads, stores and
  abends is observable, the LIR keeps it. A change of behaviour is an open question (§13).

## 2. Ubiquitous language

| Term | Meaning |
|---|---|
| Base | What a place's offset counts from: the program's slab, LOCAL-STORAGE, a LINKAGE record, or a run-unit cell |
| Plan | A decision the walker makes on each execution, made once at lowering |
| Block | A straight run of ops ending in one terminator |
| Paragraph end | Where control passes the end of a paragraph, and the return point armed there is taken |
| Range | Paragraphs run as a unit: an out-of-line PERFORM, a SORT or MERGE procedure, a USE BEFORE REPORTING, USE AFTER EXCEPTION/ERROR or USE FOR DEBUGGING section |
| Return point | What a running range arms at the end of its last paragraph, where control passing returns to it (C99) |
| Frame | The VM's record of a running range: its number, where it returns, the return point it displaced, the segment register and the nesting depth |
| Abend op | An abend the walker gives only when it reaches a construct, kept as an op so it still happens only then |
| Debug id | An index into the debug table: the file, line and column an op or place names when it abends |

**Two reference types.** `Place` is the LIR's static description of a data reference: a base, a
constant offset, subscript and reference-modification expressions, OCCURS DEPENDING ON, and the
SSRANGE checks. `Loc` is the evaluated run-time location: a concrete offset, length and kind, which
is what `Machine::locate` returns today (machine.rs:69-75). An executor evaluates a `Place` to a
`Loc`. The semantics library takes `Loc`, never a `Place` and never an AST type.

## 3. Design decisions

1. **No compile-side `Layout` and no AST.** The LIR holds places, plans, its own small enums, and an
   item table for `dump` that no executor reads (§4). `Kind`, `SignClause`, `Pos`, `Sym`,
   `Figurative`, `RelOp`, `BinOp`, `OpenMode` and `InspectMode` live in `rt`
   (semantics-library.md §4.3).
2. **A run-time failure stays a run-time failure.** Where the walker abends only on reaching a
   construct (a condition-name used as data, a national value moved to a numeric item, FUNCTION
   CHAR with two arguments, a host variable with no SQL type), lowering keeps its code and message
   as an `Abend` op or plan entry, so every program the walker runs lowers.
3. **Plans are shared, not copied.** Lowering builds each plan once; the VM and, from step 2, the
   walker execute it.
4. **Control transfer is explicit, and the walker's rules are the baseline.** Paragraph ends, GO TO,
   PERFORM entry and every service that transfers control are terminators. The VM's frames and
   return points reproduce the walker's (§8.4), assumption C99 (§8.6).
5. **Services stay library code,** as `rt` functions over `Loc`s and values. A payload names places
   and expressions by id, and the library asks the executor for each `Loc` or value where the walker
   would locate or evaluate it (semantics-library.md §9, C6), so abends and side effects keep the
   walker's order. A service that runs COBOL in its middle calls back through one trait (§9.6).
6. **Everything is an index.** Ids are `u32` indices into per-program tables, and names sit in a
   per-program symbol table, so the load module is a flat encoding of these types.

## 4. Program shape

```rust
pub type BlockId = u32; pub type ParaId = u32; pub type RangeId = u32; pub type PlaceId = u32;
pub type ExprId = u32; pub type CondId = u32; pub type ConstId = u32; pub type SymId = u32;
pub type DebugId = u32; pub type AbendId = u32; pub type TempId = u16;
/// An `SqlEntry`'s ordinal (§9.7).
pub type SqlId = u32;
// And a `u32` id per plan or service table: ArithId, InitId, DisplayId, InspectId, StringId,
// UnstringId, SearchAllId, FunctionId, UserFunctionId, FileOpId, CallId, SortId, ReleaseId,
// ReturnId, InvokeId, CicsId, MarkupId.

/// One program, lowered. Methods and FACTORY or OBJECT data lower as programs too (§9.8).
pub struct Program {
    pub id: SymId, pub options: ProgramOptions, pub initial: bool, pub recursive: bool,
    pub storage: Storage, pub items: Vec<Item>, pub paragraphs: Vec<Paragraph>,
    /// The first paragraph after DECLARATIVES, where a run starts (machine.rs:267 (int)).
    pub procedure_start: ParaId,
    pub ranges: Vec<Range>, pub blocks: Vec<Block>, pub places: Vec<Place>,
    pub exprs: Vec<Expr>, pub conds: Vec<Cond>, pub consts: Vec<Const>,
    /// The ArithPlan, MovePlan, InitPlan, DisplayPlan, InspectPlan, StringPlan, UnstringPlan,
    /// SearchAllPlan and FunctionPlan tables.
    pub plans: Plans,
    /// The FileOp, FileDesc, CallPlan, SortPlan, ReleasePlan, ReturnPlan, InvokePlan and
    /// CicsCommand tables, the Sqlca, the ENTRY points (§9.3), for a class definition its class
    /// (§9.8), the UserFunctionPlan table and for a function definition its FunctionDefinition
    /// (§9.15), the declaratives' `Declaratives` (§9.10), the JSON and XML statements (§9.13), the
    /// report model `Op::Report` names (§9.6), and the EXTERNAL and GLOBAL `Scope` (§9.16).
    pub services: Services,
    pub sql: Vec<SqlEntry>, pub abends: Vec<AbendText>, pub edits: Vec<Edit>,
    pub symbols: Vec<String>, pub debug: Debug,
}

/// An edited PICTURE (`Layout.edits`) and the currency sign value its currency symbol stands for
/// (`Layout.currencies`), empty when it has none (§9.11).
pub struct Edit { pub syms: Vec<Sym>, pub currency: String }

/// Fixed at compile time (codegen-runtime.md §10, invariant 7). `ssrange` is `Compiled.ssrange`
/// (exec/src/lib.rs:29). `options.dynam` makes a literal CALL resolve at run time, as the walker does
/// every CALL (load-module.md §8.3). `cards` are the CBL and PROCESS cards as written (ast.rs:7).
/// `collating` is `Compiled.collating` (exec/src/lib.rs, after 79a199e). `decimal_point_comma` is
/// SPECIAL-NAMES DECIMAL-POINT IS COMMA and `numval_currency` the cs NUMVAL-C and TEST-NUMVAL-C
/// take without argument-2, both as the walker reads them (§9.11). `when_compiled` is
/// `Compiled.when_compiled`, which FUNCTION WHEN-COMPILED gives (§9.9), in a program that uses
/// WHEN-COMPILED, and None in any other, so its module does not depend on when it was compiled.
pub struct ProgramOptions {
    pub options: numeric::Options, pub ssrange: bool, pub cards: Vec<String>,
    pub collating: Collating, pub decimal_point_comma: bool, pub numval_currency: String,
    pub when_compiled: Option<CompileTime>,
}

/// Seconds since 1970-01-01T00:00:00Z, from 0 to 9999-12-31T23:59:59Z (253402300799), and
/// hundredths below 100, zero when `source` is `SourceDateEpoch`; a decoder refuses anything else.
pub struct CompileTime { pub seconds: i64, pub hundredths: u32, pub source: TimeSource }
pub enum TimeSource { SourceDateEpoch, Clock }

/// The sequence PROGRAM COLLATING SEQUENCE names, from an ALPHABET clause of SPECIAL-NAMES, as
/// `collating::Sequence` builds it (exec/src/collating.rs, after 79a199e): alphanumeric comparisons,
/// HIGH-VALUE and LOW-VALUE, FUNCTION CHAR and ORD, and a file SORT without its own COLLATING
/// SEQUENCE phrase follow it.
pub enum Collating {
    /// EBCDIC, and an ALPHABET of EBCDIC or NATIVE: each byte its own position, HIGH-VALUE X'FF',
    /// LOW-VALUE X'00'.
    Native,
    Sequence(Sequence),
}

/// `positions` gives each byte's position from 0, characters that collate equal (ALSO) sharing one;
/// `characters` the first character given each position, which CHAR returns (ORD is a byte's
/// position + 1); HIGH-VALUE is the last character of the highest position, LOW-VALUE the first of
/// the lowest.
pub struct Sequence { pub positions: Box<[u8; 256]>, pub characters: Vec<u8>, pub high_value: u8, pub low_value: u8 }

pub struct Storage {
    /// The slab's size (`Layout.size`), then the slab and LOCAL-STORAGE as VALUE clauses leave them.
    pub size: u32, pub image: Vec<u8>, pub local_image: Vec<u8>,
    /// What VALUE initialization prints (TRUNC(OPT) reports), and its abend, if it has one.
    pub init_reports: Vec<SymId>, pub init_abend: Option<AbendId>,
    /// Each LINKAGE record's size; USING and RETURNING as ordinals (machine.rs:1054-1069).
    pub linkage: Vec<u32>, pub using: Vec<u16>, pub returning: Option<u16>,
    /// Offset and size of each file's record area in the slab (layout.rs:98-99).
    pub file_areas: Vec<(u32, u32)>,
    /// PARMCHECK's buffer (`Layout.parmcheck`): its offset in the slab and its size, present only
    /// under PARMCHECK (§9.14).
    pub parmcheck: Option<(u32, u32)>,
}

/// A data item as `dump` prints it and a debugger will read it; no executor reads it. Compile's
/// `Item` (layout.rs:47-78) with its AST fields resolved: `depending_on` is the OCCURS DEPENDING ON
/// object's item index, and `keys` each ASCENDING (true) or DESCENDING key's, where compile's hold
/// `ast::Ref`s.
pub struct Item {
    pub name: Option<SymId>, pub level: u8, pub parent: Option<u32>,
    pub offset: u32, pub size: u32, pub occurs: u32, pub dims: Vec<(u32, u32)>, pub kind: Kind,
    pub local: bool, pub linkage: Option<u16>, pub redefines: Option<SymId>,
    pub depending_on: Option<u32>, pub keys: Vec<(bool, u32)>, pub at: DebugId,
}

/// `code` is the typed abend code of semantics-library.md §7, DRY-3. `at` is None where the op or
/// terminator that raises the abend gives its position (§10), and the failing data entry's
/// position for `Storage.init_abend`, which no op raises.
pub struct AbendText { pub code: AbendCode, pub message: SymId, pub at: Option<DebugId> }

/// `section_end` is the last paragraph of its section (exec/src/lib.rs:99-103). `priority` is its
/// section's priority-number, 0 for none; 50 to 99 is an independent segment (§8.9). `abandoned`,
/// on a paragraph that ends a range, is the abend of passing its end when the frame armed there
/// cannot resume (§8.4).
pub struct Paragraph { pub name: SymId, pub is_section: bool, pub entry: BlockId, pub section_end: ParaId, pub priority: u8, pub at: DebugId, pub abandoned: Option<AbendId> }
pub struct Block { pub ops: Vec<Op>, pub end: Terminator }
```

- **The initial image is exact.** VALUE initialization (machine.rs:230-244) depends only on
  literals, kinds, options and the collating sequence, so lowering runs it once and keeps the
  bytes, the TRUNC(OPT) reports it printed (numeric/src/binary.rs:57-69) and any abend, with the
  bytes as they stand at it and the position of the data entry it names. The VM replays them at
  each fresh activation (machine.rs:198-227).
- **The collating sequence is data.** Lowering keeps `Compiled.collating` whole, so an executor
  compares, fills HIGH-VALUE and LOW-VALUE and answers CHAR and ORD without the ALPHABET clause.
  A load module's OPTIONS section holds it after the cards (load-module.md §5.1).
- **The compile time is data.** `compile` takes it from SOURCE_DATE_EPOCH when the build sets it
  (whole seconds, the reproducible-builds convention; a value that is not digits, or is past
  253402300799, refuses the compile), and from the clock otherwise; `TimeSource` says which.
  `compile_at` takes it from the caller. A class definition's methods and data take the class's.
  The walker and the LIR read the one `Compiled.when_compiled`, so they give the same
  WHEN-COMPILED; a program the walker compiles when a CALL first loads it is compiled at that
  moment. Lowering keeps it only in a program with a WHEN-COMPILED plan, so no other program's LIR
  depends on when it was compiled.
- **No literal text reaches run time.** Lowering parses every numeric literal (`Const::Number`,
  §6); VALUE clauses reach the runtime only as the image, and 88-level values as constants of
  `Cond::Name`. The item table records DEPENDING ON objects and keys; places evaluate them (§5.4).

## 5. Places

### 5.1 The type

```rust
pub struct Place {
    pub base: Base,
    /// `Item.offset` from the base (layout.rs:52).
    pub offset: u32,
    /// One occurrence, `Item.size`, before OCCURS DEPENDING ON and reference modification.
    pub len: u32,
    /// The item's kind, or alphanumeric under reference modification (machine.rs:584).
    pub kind: Kind,
    /// `Item.scaling`: PICTURE P positions right of the digits, which `ProgramFacts::scaling`
    /// gives for the place's `Loc`, so a read and a store scale the value as the walker's do.
    pub scaling: u32,
    /// One per entry of `Item.dims` (layout.rs:69-70), outermost first.
    pub subscripts: Vec<Subscript>,
    /// On a group that ends with an OCCURS DEPENDING ON table (layout.rs:289-300).
    pub odo: Option<Odo>,
    pub refmod: Option<RefMod>,
    pub name: SymId,
    /// The Ref's position, which the walker names in this place's abends.
    pub at: DebugId,
    /// What the compiler fixed of NUMCHECK's test where this reference reads its item (§9.14).
    pub numcheck: PlaceNumcheck,
}

pub enum Base {
    /// The activation's slab: WORKING-STORAGE, FD and SD record areas (SAME RECORD AREA shares
    /// one, layout.rs:486-513 (int)), report control areas, the sort special registers
    /// (exec/src/sort.rs:22-42 (int)); in a method, its own WORKING-STORAGE.
    Program,
    Local,
    /// LINKAGE record n; S0C4 while it has no address.
    Linkage(u16),
    /// RETURN-CODE at run-unit offset 0 (unit.rs:18), when the program declares none.
    ReturnCode,
    /// The task's EXEC interface block, for services and ADDRESS EIB.
    Eib,
    /// SELF's cell, pushed at method entry (machine/oo.rs:409 (int)): four bytes, an object
    /// reference, whatever reference modification the Ref has (`oo_register`, machine/oo.rs:94-109
    /// (f2)). Evaluating it outside a method abends IRONWORK "SELF outside a method".
    SelfRef,
    /// JNIENVPTR's cell, made on first use (machine/oo.rs:101-117 (int)): four bytes, a pointer.
    JniEnv,
    /// An XML PARSE fragment register (§9.13): `offset` and `len` are 0 and the current event's
    /// fragment gives both, empty outside a processing procedure. Its reference modification is
    /// checked whatever `check` says (`xml_register`, machine/xml.rs).
    Xml(XmlRegister),
}

/// Each `check` is present only under SSRANGE: 1 to `count`; 0 to `max`; start and length at
/// least 1 and inside the item.
pub struct Subscript { pub stride: u32, pub value: IntExpr, pub check: Option<u32> }
pub struct Odo { pub object: IntExpr, pub max: u32, pub element: u32, pub check: bool }
pub struct RefMod { pub start: IntExpr, pub length: Option<IntExpr>, pub check: bool }
/// What `compile::numcheck` fixed (§9.14): under ZON(LAX), what the item may hold because of the
/// item its record redefines (C280); and that the compiler removed the test here, having found it
/// always fails (C281).
pub struct PlaceNumcheck { pub lax: Option<rt::store::LaxRedefinition>, pub removed: bool }
```

### 5.2 Bases

- **Program** is bound at activation (machine.rs:557); file record areas (file_io.rs:66-69) and the
  SD's area (machine/sort.rs:180, 215 (int)) are offsets in it. **Local** is pushed at activation
  (machine.rs:217-220).
- **Linkage(n)** is bound by USING (machine.rs:1054-1060), RETURNING (1064-1069), SET ADDRESS OF
  (1151-1167), CICS for DFHEIBLK and DFHCOMMAREA (machine/cics.rs:386, 413-428), method entry
  for FACTORY and OBJECT data (machine/oo.rs:413-419 (int)), and activation for an EXTERNAL record
  or a containing program's GLOBAL one, which layout places as LINKAGE records (§9.16).
- **Pointer-based storage is LINKAGE.** The walker has no BASED items: a pointer reaches data only
  through SET ADDRESS OF a LINKAGE record.
- **DFHEIBLK stays `Linkage(0)`,** first in USING (syntax/src/parser.rs:495-510) and bound to the
  EIB by CICS, because a program may SET ADDRESS OF it and the walker honours that.
- **Instance data stays LINKAGE.** A method has its FACTORY or OBJECT data as LINKAGE records after
  its own (exec/src/oo.rs:188-208 (int)), and Check refuses SET ADDRESS OF them (oo.rs:227-240
  (int)), so their binding changes only at method entry.

### 5.3 How each form lowers

| Form | LIR | Replaces in `locate` (machine.rs) |
|---|---|---|
| Undeclared RETURN-CODE | `Base::ReturnCode`, offset 0, length 2, BINARY S9(4) | 539-541 |
| SELF, JNIENVPTR | `Base::SelfRef`, `Base::JniEnv` | 553-555 (int), calling oo.rs:71-81 (int) |
| XML-TEXT, XML-NTEXT and the namespace registers, undeclared | `Base::Xml(register)` | `xml_register` (machine/xml.rs) |
| Name lookup and its memo | Resolved at lowering; a condition-name used as data becomes an `Abend` op | 528-536, 542-544 |
| Subscript count | Check refuses a wrong count (lib.rs:565-568); lowering asserts it | 547-549 |
| WORKING-STORAGE, record areas | `Base::Program` | 557 |
| LOCAL-STORAGE | `Base::Local` | 556 |
| LINKAGE, pointer-based | `Base::Linkage(n)`; S0C4 checked at run time | 550-555 |
| Constant offset | `offset` | 559 |
| Subscripts | `Subscript { stride, value, check }`: offset += (s − 1) × stride | 560-566 |
| OCCURS DEPENDING ON | `Odo`: length −= (max − current) × element | 567-572, and `occurrences` 594-603 |
| Reference modification | `RefMod`; kind alphanumeric | 573-585 |
| Run-unit bounds | Checked at run time for places that are not static | 586-588 |

### 5.4 Evaluation at run time

An executor evaluates a place to a `Loc` in the walker's order and with its messages: the base (S0C4
for an unbound LINKAGE record); each subscript, outermost first, then its check; the OCCURS
DEPENDING ON object, its check, and the clamp to 0 to `max` (machine.rs:602); the
reference-modification start, then length (default: to the end), then its check; last the run-unit
bound. The checks are library functions over the evaluated integers, which `locate` and the VM both
call (semantics-library.md §8, E10b). The DEPENDING ON object is an `IntExpr` because the walker
evaluates it as an expression (machine.rs:598), so an object written with subscripts lowers as the
walker runs it. Each `IntExpr::Fixed` among them locates its `prepass` before it is evaluated
(§7.5), so a place nested in a subscript is evaluated in the walker's order too.

A place is **static** when its base is Program, Local or ReturnCode and it has no subscripts, no
OCCURS DEPENDING ON and no reference modification. It cannot abend, and its address is the
activation's base plus a constant. Most places in batch code are static.

### 5.5 SSRANGE

`check` is present only when the program was compiled with SSRANGE (exec/src/lib.rs:43-48). The
three messages are the walker's: subscript (machine.rs:562-564), reference modification (579-581)
and OCCURS DEPENDING ON (599-601). Without SSRANGE the only checks are the LINKAGE binding and the
run-unit bound, so a reference reaches anywhere in run-unit memory (codegen-runtime.md §3).

## 6. Values and expressions

```rust
pub enum Operand {
    /// The value by the place's kind (machine.rs:636-652).
    Load(PlaceId),
    Const(ConstId),
    /// After OCCURS DEPENDING ON and reference modification (machine.rs:675-678).
    LengthOf(PlaceId),
    /// NULL for a LINKAGE record with no address (machine.rs:946-955).
    AddressOf(PlaceId),
    Function(FunctionId),
    /// An invocation of a user-defined function (§9.15).
    UserFunction(UserFunctionId),
}

/// A literal, converted once, where `literal_value` converts it on every use (machine.rs:621-634).
/// Bytes are alphanumeric in the program's code page, or hexadecimal. `Refused` is an alphanumeric
/// literal, or ALL one, the code page cannot encode, whose reading abends.
pub enum Const { Bytes(Vec<u8>), National(Vec<u8>), Number(Fixed), Figurative(Figurative), All(Vec<u8>), Refused(AbendId) }

/// A subscript, bound, TIMES count or exponent, as `integer()` gives it (machine.rs:613-619).
/// `Fixed` locates each place of `prepass`, then evaluates `expr` with `dmax` (§7.5).
/// `Walk(k)` is subscript k of the JSON walk in progress (§9.13); only a markup payload's places
/// and conditions hold it.
pub enum IntExpr { Const(i64), Item(PlaceId), Fixed { expr: ExprId, dmax: u32, prepass: Vec<PlaceId> }, Walk(u8) }

pub enum Expr {
    Operand(Operand), Neg(ExprId), Bin(ExprId, BinOp, ExprId),
    /// An exponent from 0 to 31, else abend IRONWORK (machine.rs:1439-1449).
    Pow(ExprId, IntExpr),
}

pub enum Cond {
    Rel { a: Comparand, op: RelOp, b: Comparand, how: Compare },
    /// NUMERIC or ALPHABETIC of a data item: a byte test chosen by its kind (machine.rs:1825-1839).
    Class { place: PlaceId, test: ByteClass },
    /// POSITIVE, NEGATIVE, ZERO, and a class test of anything but a data item (machine.rs:1840-1849):
    /// an operand's value as it reads, or an expression's as `Comparand::Expr` evaluates it.
    Sign { value: Comparand, test: SignTest },
    /// A level-88 name: equal to any value, or within any THRU pair (machine.rs:1791-1820).
    Name { subject: PlaceId, values: Vec<(ConstId, Option<ConstId>)>, how: Compare },
    Not(CondId),
    /// Left to right, the right side only when needed (machine.rs:1784-1785).
    And(CondId, CondId), Or(CondId, CondId),
    /// A TIMES counter above zero.
    Counter(TempId),
    /// SEARCH: the index from 1 to the table's current count (machine.rs:887-893), the count
    /// evaluated first.
    InTable { index: PlaceId, count: Count },
    /// After EXEC SQL: SQLCODE < 0, = 100, or a warning (§9.7).
    Sql(SqlTest),
}

/// An operand keeps its place for the comparison; an expression does not (machine.rs:1906-1911).
/// `Expr` is `expr_value`'s evaluation, fixed at lowering: `prepass` holds the places its float
/// test and, in `Mode::Fixed`, its dmax pass locate, and `dmax` is 0 in `Mode::Float` (§7.5).
pub enum Comparand {
    Operand(Operand),
    Expr { expr: ExprId, dmax: u32, mode: Mode, prepass: Vec<PlaceId> },
}

/// The branch of `compare` (machine.rs:1852-1894) the two sides take, fixed by their kinds:
/// packed bytes under NUMPROC(PFD); addresses; extended float when either side is COMP-1 or
/// COMP-2; fixed; national; alphanumeric images at the longer length; or a pair the walker refuses.
/// `References` is two addresses of which one is an object reference or SELF
/// (`compare_references`, machine/oo.rs:145-158 (f2)): each is looked up in the run unit's objects,
/// the first first, abending IRONWORK for one freed or never given (the message names the Refs of
/// both sides as written); equal when both identify the same object, or are both NULL, else less.
/// NULL written as a figurative constant is not an address, so `A = NULL` is `Address`.
pub enum Compare { PackedPfd, Address, Float, Fixed, National, Alphanumeric, Refused(AbendId), References, ZonedBytes { zoned_first: bool } }

pub enum ByteClass { Packed { signed: bool }, Zoned { signed: bool }, Digits, Alphabetic }
pub enum SignTest { Positive, Negative, Zero }
/// `Temp` is the count `SetCount` held in the top frame earlier in the statement (§9.12).
pub enum Count { Fixed(u32), Odo(Odo), Temp(TempId) }
```

- **Subscripts and bounds** are `IntExpr`: a literal is `Const`, a plain integer item `Item`, and
  anything else `Fixed`, its dmax and the places its dmax pass locates found at lowering rather
  than on every call (machine.rs:614).
- **Sign conditions.** The walker reads an operand directly and evaluates anything else with
  `expr_value`, so an operand stays `Comparand::Operand`, read as its kind: ZERO or an
  alphanumeric item keeps the walker's sign-condition abend rather than arithmetic's.
- **A zoned integer against a nonnumeric operand** is `ZonedBytes`: the item's bytes, its sign
  removed under ZWB and kept under NOZWB, never its value, compared as alphanumeric
  (`rt::store::compared_zoned_bytes`, assumption C221), so invalid data compares rather than abends.
  Under INVDATA(NOFORCENUMCMP) an unsigned zoned integer against ZERO, or against an unsigned zoned
  integer of its own length, is `ZonedBytes` too, the other item taken by its bytes the same way
  (`rt::store::compare_zoned_bytes`, assumption C223). INVDATA(CLEANSIGN) reads a sign half-byte of
  0 to 9 as F wherever a zoned or packed item is read as a number (`rt::store::read_stored`, C222).
- **Condition-names.** `Name` holds the conditional variable's place, with the 88-level reference's
  subscripts, and each VALUE as a constant, a THRU pair as `(low, Some(high))`. The walker reads the
  subject once per value (machine.rs:1806-1818) and the VM once, with the same result, since nothing
  is stored between. SET TO TRUE moves the first value's low end (machine.rs:1132-1137).
- **Condition-names of mixed categories.** `Name` has one `Compare` for all its values. When the
  values take different branches of `compare` (a numeric subject with the values ZERO and SPACE,
  say), lowering writes the test out as the walker runs it: an `Or` of one alternative
  per value, in the order of the VALUE clause, each `Rel { Load(subject) = value }` or, for a THRU
  pair, `And(Rel ≥ low, Rel ≤ high)`, each with its own `Compare`. `Or` and `And` evaluate left to
  right and stop at the first alternative that holds, as the walker's loop returns at its first hit,
  so the reads, the abends (a `Compare::Refused` value abends only when reached) and the result are
  the walker's.
- **A literal the code page cannot encode** (Japanese text, a check mark) compiles, and the walker
  abends IRONWORK "U+hhhh has no byte in CCSID n" each time `literal_value` converts it, at the
  position its caller gives. Lowering makes it `Const::Refused` with that abend and position, where
  its `Side` is an alphanumeric literal's, so every plan is the one an encodable literal gets; an
  executor abends where it reads the constant, which is where the walker converts the literal: a
  MOVE after locating the receiver, a comparison after the locates of its zoned-bytes test, a
  condition-name at the value reached. Where lowering converts a literal itself (DISPLAY's text,
  `Chars::Literal` of STRING, UNSTRING, INSPECT and BY CONTENT), such a literal stays a value,
  `DisplayItem::Value` or `Chars::Value`, read in the walker's order by the same library code. In
  a report, whose literals lower without a position, and in JSON PARSE's USING phrase, such a
  literal is refused by name.
- **Abbreviated relations.** `Cond::NameOrRel` (syntax/src/ast.rs:283-285), decided at run time by
  what the name resolves to (machine.rs:1787-1790), lowers to `Name` or `Rel`.
- **Arithmetic expressions** are `Expr` trees for `eval_fixed` and `eval_float`
  (machine.rs:1419-1487), under a plan (§7).

## 7. Arithmetic plans

### 7.1 What the walker works out on each execution

| Quantity | Walker | Fixed at lowering as |
|---|---|---|
| dmax | Largest scale among the receivers and the expressions, divisors and exponents aside (machine.rs:1493-1501, 1409-1417) | `ArithPlan.dmax` |
| ARITH | `options.arith` (machine.rs:1420) | `ArithPlan.arith` |
| Fixed or float | Float for every expression when a receiver is COMP-1 or COMP-2 (Programming Guide SC27-8714-03, p. 800), else `uses_float` on each expression (machine.rs:1506, 1400-1406), which is then not run | `ArithStep.mode`, with an empty `probe` when a receiver decides it |
| Float intermediate | `arith.float_intermediate()` (machine.rs:1507) | inside `Mode::Float` |
| Receiver's store | `locate(t).kind`, then `store_value` and `store_fixed_checked` (machine.rs:1542-1633) | `StorePlan` |
| ROUNDED | `t.rounded` (machine.rs:1521) | `ArithStep.rounded` |
| SIZE ERROR | `handler.is_some()` (machine.rs:1513, 1521) | `ArithPlan.handled` |
| Remainder's quotient scale | The first receiver's scale (machine.rs:1528-1529) | `RemainderPlan.quotient_scale` |
| Literals | `literal_fixed` (machine.rs:1411) | `Const::Number` |

ADD, SUBTRACT, MULTIPLY and DIVIDE reach the machine already reduced to (receiver, expression)
pairs (syntax/src/ast.rs:368-378), so one plan type serves all five verbs.

### 7.2 The plan

```rust
pub struct ArithPlan {
    pub dmax: u32,
    pub arith: Arith,
    /// The places the walker's dmax pre-pass locates, in its order, static ones left out (§7.4).
    pub prepass: Vec<PlaceId>,
    /// One per receiver, in the walker's order.
    pub steps: Vec<ArithStep>,
    pub remainder: Option<RemainderPlan>,
    /// ON or NOT ON SIZE ERROR is written: a size error keeps the receiver, division by zero is a
    /// size error rather than S0CB, and the op returns Arm(0) or Arm(1) (machine.rs:1512-1537).
    pub handled: bool,
    /// ADD, SUBTRACT, MULTIPLY or DIVIDE, the walker's `per_receiver`: a step whose expression is
    /// a binary operation with its receiver as an operand evaluates only the other operand with
    /// the rest, and reads the receiver when it stores (§7.4).
    pub per_receiver: bool,
}

/// `probe` holds the places the walker's float test locates before the step is evaluated (§7.4).
pub struct ArithStep { pub target: PlaceId, pub expr: ExprId, pub mode: Mode, pub store: StorePlan, pub rounded: bool, pub probe: Vec<PlaceId> }
pub enum Mode { Fixed, Float(Precision) }

/// How a numeric value reaches a receiver, by the receiver's kind.
pub enum StorePlan {
    Zoned { digits: u32, scale: u32, signed: bool, sign: Option<SignClause> },
    Packed { digits: u32, scale: u32, signed: bool },
    /// `name` labels a TRUNC(OPT) report (machine.rs:1596-1608).
    Binary { digits: u32, scale: u32, signed: bool, native: bool, name: SymId },
    NumericEdited { edit: u32, digits: u32, scale: u32, blank_when_zero: bool },
    Float(Precision),
    Index,
    /// Not a numeric receiver: abend IRONWORK when reached (machine.rs:1626).
    Refused(AbendId),
}

pub struct RemainderPlan { pub target: PlaceId, pub dividend: ExprId, pub divisor: ExprId, pub quotient_scale: u32, pub store: StorePlan }

/// PERFORM VARYING's step, SET UP and DOWN BY, and the TALLYING adds: an add, then a store with no
/// size error (machine.rs:517-521, 799-800, 1179-1180).
pub struct StepPlan { pub dmax: u32, pub store: StorePlan }

/// SET UP or DOWN BY on one receiver, by what reading it gives: an address moved by the step, a
/// number the step is added to, or a value the walker refuses once it has read it.
pub enum UpDown { Pointer, Number(StepPlan), Refused(AbendId) }
```

The same `StorePlan` serves MOVE's numeric receivers, with ROUNDED off and no size check
(machine.rs:1566-1568). A floating-point value reaching a fixed-point receiver is rounded whatever
`rounded` says, and one reaching a narrower COMP-1 is rounded too, as `store_value` and `assign`
do (rt/src/store.rs, `float::to_receiver` and `float::narrow_rounded`), so no plan carries a
rounding of its own for them.

### 7.3 What stays dynamic

- **Values,** with their S0C7 checks, and an exponent held in a data item, checked to be 0 to 31.
- **The places of intermediate values.** `precision::Fixed` carries them at run time; NUMVAL and
  NUMVAL-C give places that depend on the text (machine.rs:2067-2072), MIN and MAX on which argument
  wins (machine.rs:1314-1330).
- **Division by zero:** S0CB with no handler, a size error with one (machine.rs:1512-1518).
- **Overflow:** the receiver's size error (machine.rs:1628-1632), and IRONWORK past 256 bits.
- **The text of TRUNC(OPT) reports,** which names the value stored.

### 7.4 The walker's locate passes

A locate can abend (an unbound LINKAGE record, an SSRANGE subscript, invalid data in a subscript),
and the walker locates places before it uses them, so these passes decide which abend a statement
gives and what storage holds at it. The plans keep them:

- **The dmax pre-pass.** Before evaluating or storing anything, the walker locates each receiver
  and each operand of its expression outside divisors and exponents, then the REMAINDER receiver and
  dividend, only to find dmax (machine.rs:1494-1501). Plans fix dmax at compile time, and executors
  still evaluate every place of `ArithPlan.prepass` to a `Loc` before the first store, so a locate
  abend on a later receiver leaves earlier receivers unchanged.
- **The float test.** Each step locates its receiver, then its expression's operands left to right
  until the first floating-point one (machine.rs:1400-1406, 1504-1506), before it evaluates the
  expression, which reads the left operand before locating the right. `ArithStep.probe` holds those
  operands, evaluated after the receiver and before the expression.
- **Every step is evaluated before any is stored.** The steps' float tests and expressions run in
  order first, an abend in an expression held until its step stores; then the REMAINDER's dividend
  and divisor; then each step stores. Under `per_receiver` a step whose expression is a binary
  operation with its receiver as an operand evaluates the other operand in the first pass, and
  reads its receiver and combines the two when it stores (machine.rs `arithmetic`).
- **Stores locate again.** Each step evaluates its receiver again when it stores, and the remainder
  after the quotient (machine.rs:1504, 1531), so a subscript an earlier receiver changed takes
  effect. The `Loc`s of the two passes are discarded.

Static places cannot abend, and are left out of both lists.

### 7.5 The same passes outside a statement's plan

The walker makes the same passes wherever it evaluates an expression, not only in the arithmetic
verbs, and the types that hold those expressions keep them the same way: a `prepass` list, in the
walker's order, static places left out, which an executor evaluates to `Loc`s and discards before it
evaluates the expression. Without it, an operand whose locate abends (an SSRANGE subscript, an
unbound LINKAGE record, invalid data in a subscript) would abend after an earlier operand was read,
rather than before.

| Where | The walker | `prepass` | dmax and mode |
|---|---|---|---|
| `IntExpr::Fixed`: a subscript, reference-modification bound, DEPENDING ON object, TIMES count or exponent | `integer`: the dmax pass, then `eval_fixed` (machine.rs:613-619) | The dmax pass's places | `dmax`; always fixed |
| `Comparand::Expr`: an expression compared | `expr_value`: the float test, then for a fixed-point expression the dmax pass (machine.rs:1382-1391, 1400-1417) | The float test's places, then in `Mode::Fixed` the dmax pass's, so a place both reach is listed twice | `dmax`, 0 in `Mode::Float`; `mode` as `ArithStep.mode` |
| `Cond::Sign` of an expression | `class`, through `expr_value` (machine.rs:1840-1849) | As `Comparand::Expr`, which it holds | As `Comparand::Expr` |
| `Op::Step`: PERFORM VARYING's increment | Locates the variable, then the dmax pass over variable + BY, then `eval_fixed` (machine.rs:517-521) | The places of that dmax pass, the variable's first, located after the variable | `StepPlan.dmax`; always fixed |
| `Argument::Value(Comparand::Expr)`: a FUNCTION's argument expression | `function_arguments`: in a fixed-point expression the argument's dmax pass, then `eval_fixed`; in a floating-point one `eval_float`; outside any arithmetic expression `expr_value` | The dmax pass's places; none in float; outside, as `Comparand::Expr` | The larger of the holding expression's dmax and the argument's, or the holding expression's `Mode::Float`; outside, as `Comparand::Expr` |

- **The dmax pass** locates every operand of the expression except divisors and exponents, left to
  right (`dmax_refs`); **the float test** every operand, left to right, up to and including the
  first floating-point one.
- **A function's argument expressions** take part in the arithmetic of the expression that holds
  the function: its dmax, which counts the receivers (Programming Guide SC27-8714-03, p. 794), or
  its floating point, which covers every operation in it (p. 800). A function that is an operand of
  no arithmetic expression, moved or displayed or compared on its own, gives them their own.
- **An exponent in float mode.** `eval_float` evaluates an exponent as a float and then abends
  (machine.rs:1483), so it never makes the exponent's own dmax pass: in `Mode::Float` an executor
  evaluates `Pow`'s `IntExpr::Fixed` exponent as a float expression and does not locate its
  `prepass`.

## 8. Control flow

### 8.1 Ops, terminators and ranges

```rust
pub enum Op {
    /// `to` located, then `from`, which takes NUMCHECK's `check` (§9.2) before it is read.
    Move { from: Operand, to: PlaceId, plan: MovePlan, check: SenderCheck },
    /// SET TO, and PERFORM VARYING's FROM: as `Move`, but a data item sender is read as a number,
    /// its digits checked, where MOVE carries a zoned or packed sender's invalid digits (C260).
    Set { from: Operand, to: PlaceId, plan: MovePlan },
    Initialize { target: PlaceId, plan: InitId },
    Arith(ArithId),
    /// SET ADDRESS OF: `address` evaluated once, then each LINKAGE record bound to it in turn.
    SetAddress { records: Vec<u16>, address: Operand },
    /// SET UP BY or DOWN BY: `by` evaluated once, then each receiver read and moved in turn.
    SetUpDown { by: IntExpr, down: bool, targets: Vec<(PlaceId, UpDown)> },
    /// PERFORM VARYING's increment: `var` located, then each place of `prepass` (§7.5), then
    /// `var + by` computed with `plan.dmax` and stored.
    Step { var: PlaceId, by: ExprId, plan: StepPlan, prepass: Vec<PlaceId> },
    /// SEARCH's index steps, SORT-RETURN.
    SetInt { target: PlaceId, value: IntExpr },
    Inspect(InspectId), String(StringId), Unstring(UnstringId), SearchAll(SearchAllId),
    /// The PERFORM and CALL depth (§8.7), and TIMES counters.
    Nest, Unnest(u8), SetTemp(TempId, IntExpr), DecTemp(TempId),
    /// SEARCH's table count, `occurrences` of `Odo` once, held in a counter (§9.12).
    SetCount(TempId, Odo),
    Display(DisplayId),
    Accept { target: PlaceId, from: AcceptFrom, plan: MovePlan },
    File(FileOpId), Call(CallId), Cancel(Operand),
    Sort(SortId), Release(ReleaseId), Return(ReturnId), Report(ReportOp),
    Invoke(InvokeId), Cics(CicsId), Sql(SqlId),
    /// ALTER and independent segments (§8.9).
    Alter { para: ParaId, to: ParaId }, EnterSegment(u8),
    /// Under the DEBUG option (§9.10): the line register, and a debugging section after an ALTER.
    DebugLine(u32), DebugAlter { range: RangeId, name: SymId, contents: SymId },
    /// JSON GENERATE, JSON PARSE, XML GENERATE or XML PARSE (§9.13).
    Markup(MarkupId),
}

/// What an op tells the VM, as the walker's `Flow` (machine.rs:46-59 (7af)) does. The library's
/// services return it too (semantics-library.md §4.1).
pub enum Step {
    Next,
    /// The handler a service selected; only a block's last op returns it, for `Select`.
    Arm(u8),
    /// A transfer the service chose at run time (HANDLE CONDITION), or one a
    /// procedure it ran left by (§9.6).
    GoTo(ParaId),
    End(Ending),
    /// A procedure the op ran passed the return point of active frame `frame` (§8.4).
    Return(u64),
    /// A procedure the op ran passed the return point of a PERFORM control had left (§8.4).
    Resume(Resume),
}

pub enum Terminator {
    Jump(BlockId),
    Branch { cond: CondId, then: BlockId, otherwise: BlockId },
    /// On the Arm the block's last op returned.
    Select(Vec<BlockId>),
    /// Control passes the end of paragraph `next` − 1 for `next`, which may be one past the last:
    /// the return point armed there is taken (§8.4).
    ParagraphEnd { next: ParaId },
    /// GO TO, with the transfer rules of §8.4.
    GoTo(ParaId),
    /// GO TO … DEPENDING ON: `targets[k − 1]` for a value k in range, as a `GoTo`, depth reset
    /// included; else `otherwise`, the next statement.
    Switch { value: IntExpr, targets: Vec<ParaId>, otherwise: BlockId },
    /// An out-of-line PERFORM: push a frame, arm its return point, enter the range (§8.4).
    PerformEnter { range: RangeId, ret: BlockId, resume: Option<Resume> },
    /// EXIT PROGRAM: nothing in the run unit's first program, GOBACK in any other (machine.rs:377-378).
    ExitProgram { next: BlockId },
    End(Ending),
    Abend(AbendId),
    /// The entry of a paragraph an ALTER names (§8.9): a `GoTo` of the target the alter table
    /// holds for `para`, or `Jump(otherwise)` while it holds none.
    AlteredGoTo { para: ParaId, otherwise: BlockId },
    /// Under the DEBUG option, the entry of a paragraph a debugging section serves (§9.10).
    Debug { range: RangeId, name: SymId, next: BlockId },
}

/// The statement after an out-of-line PERFORM that runs once and is a statement of paragraph
/// `para` (§8.4). `block` also keys what that PERFORM displaced.
pub struct Resume { pub para: ParaId, pub block: BlockId }

pub struct Range { pub first: ParaId, pub last: ParaId, pub kind: RangeKind }
pub enum RangeKind { Perform, SortProcedure, UseBeforeReporting, UseProcedure, Debugging, Processing }
```

- **Tags** (load-module.md §4.3): `PerformEnter` is tag 12 and `Debug` 11 of `Terminator`, and tag 6
  is retired; `DebugLine` and `DebugAlter` are tags 30 and 31 of `Op`, `Markup` 32, `Set` 33, and tag 29
  (`SetSegment`) is retired. `Processing` is tag 5 of `RangeKind`, `Xml` tag 7 of `Base` and
  `Walk` tag 3 of `IntExpr`.
- **A range's region** (`Range::region`) is the paragraphs a GO TO stays in it for: `first` to
  `last`; `first` to the program's last paragraph when `last` comes before `first`; and every
  paragraph for a SORT or MERGE procedure (§8.6). `UseProcedure` is a USE AFTER EXCEPTION/ERROR
  procedure, `Debugging` a USE FOR DEBUGGING section (§9.10), and `Processing` an XML PARSE
  processing procedure (§9.13).

The spec's `PerformEnter(range, loop)` is split: a PERFORM's loop is ordinary blocks around
`PerformEnter`, because inline PERFORM needs the same loops without a range.

### 8.2 Paragraphs, sections and fall-through

- **One entry block per paragraph,** section headers included: a section header is a paragraph
  holding the statements before the section's first paragraph (syntax/src/ast.rs:160-169).
- **A section is a range** from its header to `section_end`, as `procedure` resolves a section name
  (exec/src/lib.rs:106-120). END DECLARATIVES ends a section as a header does (lib.rs:114-120 (int)).
- **Fall-through** goes from paragraph p to p + 1, across section boundaries, as `run_region`
  does (machine/perform.rs:100-175 (7af)).
- **The paragraph's last block** ends in `ParagraphEnd { next: p + 1 }`. It is a plain `Jump`
  instead when no range's last paragraph lies in p to `next` − 1, since then no return point can be
  armed at `next` − 1 and no region ends before `next`, and no debugging section serves `next`
  (§9.10). The same holds for every `ParagraphEnd` EXIT and NEXT SENTENCE produce.

### 8.3 PERFORM

Out of line, 3 TIMES, and inline VARYING:

```text
PERFORM A THRU C 3 TIMES                  PERFORM VARYING I FROM 1 BY 1 UNTIL I > 9, inline
b0: Nest; SetTemp t0 = 3; Jump b1         b0: Nest; Set I <- 1; Jump b1
b1: Branch Counter(t0) b2 else b3         b1: Branch (I > 9) b4 else b2
b2: DecTemp t0; PerformEnter r0 -> b1     b2: body …; Jump b3         EXIT PERFORM CYCLE: Jump b3
b3: Unnest(1); Jump next                  b3: Step I by 1; Jump b1
                                          b4: Unnest(1); Jump next    EXIT PERFORM: Jump b4
```

- **Once:** `Nest; PerformEnter r -> b1`, `b1: Unnest(1)`. When the PERFORM is a statement of its
  paragraph, not inside another statement, it can resume (§8.4): `PerformEnter r -> b1, resume
  (p, b2)`, `b1: Unnest(1); Jump b2`, and `b2` holds the statements after it, as `after`
  (machine/perform.rs:88-94 (7af)) finds them.
- **TIMES:** the count is evaluated once and below zero counts as zero (machine.rs:480). Each
  TIMES statement has its own `TempId`, and its counter lives in the frame the statement runs under
  (`Frame.temps`, §8.4), not in the program: a paragraph can PERFORM itself, directly or through
  others, and the walker's `for` loop in `repeat_nested` gives each activation of the statement a
  count of its own (machine.rs:480). A counter in the program would let the inner PERFORM reset
  the outer one's.
- **UNTIL:** VARYING without the Set and the Step. **TEST AFTER** moves the Branch after the body,
  and for VARYING before the Step (machine.rs:497, 514-521).
- **VARYING:** FROM is stored with MOVE rules, as `Set` (machine.rs:502-504); each step
  re-evaluates the variable's place, locates the places of BY in `Op::Step.prepass` (§7.5), and
  stores with `StepPlan`, no ROUNDED and no size error (machine.rs:517-521).
- **VARYING … AFTER** lowers as `vary` runs it (machine.rs:598-636 (f2)), which follows the
  Language Reference's figures (SC27-8713-03, pp. 425-428): one loop per variable, the last
  varying fastest. An inner loop's test coming true augments the variable outside it, then sets
  the inner variable to its FROM value again, then tests the outer one. TEST BEFORE sets every
  variable to its FROM value before the first test; TEST AFTER sets only the first, and each inner
  one as its loop is entered. EXIT PERFORM leaves every level at once.

```text
PERFORM P VARYING I FROM 1 BY 1 UNTIL CI AFTER J FROM I BY 1 UNTIL CJ    (TEST BEFORE)
b0: Nest; Set I <- 1; Set J <- I; Jump h0
h0: Branch CI exit else h1                s0: Step I by 1; Set J <- I; Jump h0
h1: Branch CJ s0 else run                 s1: Step J by 1; Jump h1
run: PerformEnter r -> s1                 exit: Unnest(1); Jump next
```

### 8.4 Return points and frames

Assumption C99 (§8.6) and the walker's `perform_range` and `run_region`
(machine/perform.rs:60-175 (7af)):

- **Arming.** Each run of a range is a frame with a number of its own. It arms a return point at
  the end of its range's last paragraph, displacing whatever was armed there, and its paragraphs
  run in the range's region.
- **Checking.** Wherever control passes the end of a paragraph (falling through, EXIT PARAGRAPH,
  EXIT SECTION at the section's last paragraph, NEXT SENTENCE with no period after it), the point
  armed there decides: none, and control goes on; the running frame's, and it completes; an active
  outer frame's, and that frame completes, every frame inside it being left; a frame control left,
  and control resumes after its PERFORM, or the run abends.
- **Leaving.** A GO TO to a paragraph outside a frame's region leaves the frame, and each one out to
  the first whose region holds the target. A left frame's point stays armed.

The VM's state, per activation (a CALL starts with nothing armed, C99), in `rt::lir`:

```rust
/// A return point: the frame that armed it, and its PERFORM's resume.
pub struct ReturnPoint { pub frame: u64, pub resume: Option<Resume> }
/// An active range. `displaced` is the point it replaced; `segment` the segment register when it
/// was pushed (§8.9); its paragraphs run at `depth` (§8.7); `temps` are its TIMES counters (§8.3).
pub struct Frame { pub id: u64, pub kind: FrameKind, pub displaced: Option<ReturnPoint>, pub segment: u8, pub depth: u32, pub temps: Vec<i64> }
/// Main holds every paragraph and arms nothing. A Procedure frame is a range a service or a
/// `Debug` runs in a dispatch loop of its own (§9.6).
pub enum FrameKind { Main, Perform { range: RangeId, ret: BlockId, resume: Option<Resume> }, Procedure { range: RangeId } }
/// The point armed at each paragraph's end; what each resumable PERFORM control left displaced,
/// by its `Resume.block`; the frames, Main first; the next frame's number.
pub struct Returns { pub armed: Vec<Option<ReturnPoint>>, pub saved: BTreeMap<BlockId, Option<ReturnPoint>>, pub frames: Vec<Frame>, pub next_frame: u64 }
```

- **PerformEnter { range, ret, resume }** pushes a Perform frame numbered `next_frame`, which then
  counts up, with `displaced` taken from `armed[last]`, the segment register, and the depth; arms
  `armed[last]` with its number and `resume`; and goes to the entry of `first`. The arrival
  register (§9.10) is PERFORM.
- **Completing** the top frame puts `displaced` back at `armed[last]`, pops the frame and restores
  the segment register from it. A Perform frame goes to `ret` at its depth; a Procedure frame's
  dispatch loop ends, `Completed`.
- **Leaving** a frame pops it and leaves `armed` as it is. A Perform frame with a resume records
  `saved[resume.block] = displaced`. Leaving a Procedure frame ends its dispatch loop with the
  transfer that left it (§9.6).
- **Transfers.** `GoTo(t)`, taken by `GoTo`, a `Switch` or `AlteredGoTo` target and `Step::GoTo`:
  leave frames while the top one's region does not hold t, reset the depth to the top frame's, set
  the arrival register to GO TO, and go to t's entry. `Resume(r)`: the same for `r.para`, then the
  segment register takes `r.para`'s priority, clearing nothing, and control goes to `r.block`.
  `Return(f)`: leave frames until frame f is on top, and complete it. `End(e)`: the run ends. Main
  holds every paragraph, so only a Procedure frame's loop sees a transfer it cannot follow.
- **ParagraphEnd { next }**, with e = `next` − 1 and F the top frame, by `armed[e]`:
  1. none: past the last paragraph, `End(EndOfProgram)`; in F's region, `next` at F's depth, the
     arrival register FALL THROUGH; otherwise `GoTo(next)`;
  2. F's point: F completes;
  3. the point of a frame on the stack: `Return` to it;
  4. the point of a frame control left, with a resume r: `armed[e]` takes `saved[r.block]`, or
     nothing, and then `Resume(r)`;
  5. any other: `Abend(paragraphs[e].abandoned)`, IRONWORK "control passed the end of P, which is
     armed to return to a PERFORM that control left by GO TO; ironwork returns there only to a
     PERFORM that runs once and is not inside another statement", at paragraph e. Lowering sets
     `Paragraph.abandoned` on every paragraph that ends a range.
- **TIMES counters** are the top frame's: `SetTemp` and `SetCount` set one there, and `DecTemp`,
  `Counter` and `Count::Temp` read it there. A new frame starts with none set, and a popped frame's
  go with it (§8.3). A SEARCH sets and reads its count within the statement, under one frame.
- **Elision.** Control in paragraph p always lies in the top frame's region, so a GO TO from p to t
  is a plain `Jump` when every range whose region holds p also holds t and no debugging section
  serves t (§9.10).

### 8.5 The walker's baseline

The walker keeps the same state in `perform::Returns` (machine/perform.rs:8-33 (7af)): `armed`,
`saved` by the PERFORM statement's address, the active frames' numbers, and a counter. A frame is a
Rust call of `perform_range`, and `run_region` returns `Flow::Return(frame)` and
`Flow::Resume(paragraph, statement)` up through the calls, where the VM pops frames.

- **Overlapping ranges.** Take `PERFORM A THRU C` from MAIN, and in A `PERFORM B THRU D`, where B, C
  and D each DISPLAY their names. The inner PERFORM runs B and C; at the end of C the outer
  PERFORM's point is armed and it is active, so the outer PERFORM completes and the inner one is
  left, its point at D still armed. The output is B C.
- **A GO TO out and back.** In codegen-runtime.md's B3, B does `GO TO D` with D after C: the
  PERFORM is left and D runs as the main line. If control later passes the end of C, it resumes
  after the PERFORM when that PERFORM runs once and is a statement of its paragraph, and abends
  otherwise.
- **EXIT SECTION** in a performed paragraph goes to the end of the section's last paragraph, past
  the performed paragraph's point, as `Flow::ExitSection` does (perform.rs:133-134 (7af)); from
  there, leaving the PERFORM's region is a GO TO to the next paragraph, which leaves its frame.
- **A THRU range that ends before it starts,** `PERFORM B THRU A`, has a region from B to the last
  paragraph: control falls on from B to the end of the program, and reaches A's end only by a GO TO
  that leaves the frame first, so passing it resumes after the PERFORM or abends (rules 4 and 5).
- **EXIT PERFORM outside an inline PERFORM** is an S-level error in Check, as the Language
  Reference does not allow it (SC27-8713-03, p. 344). Under a card's COMPILE the program runs
  anyway, and the statement moves to the next paragraph, as EXIT PARAGRAPH does (machine.rs:278).
- **A procedure a statement runs,** an EXCEPTION/ERROR procedure or a debugging section, runs by
  `run_paragraphs`, a `perform_range` with no resume, so it arms, completes and is left by the same
  rules; §9.6 and §9.10 give what its statement does when it is left.

### 8.6 Assumption C99

V1 is superseded by C99 `PERFORM_RETURN_POINTS` in `numeric::assumptions::ASSUMPTIONS`
(numeric/src/assumptions.rs), basis `Chosen`, oracle Enterprise COBOL, which the walker and the VM
share:

> An out-of-line PERFORM arms a return point at the end of its range's last paragraph (Language
> Reference SC27-8713-03, p. 419), one per activation, since a CALL resets return points
> (Programming Guide SC27-8714-03, p. 547). Control that passes that end by any path, falling
> through or by GO TO, returns to the PERFORM, so PERFORM B THRU A with A before B returns when
> control reaches the end of A, and a range that passes the end of another active PERFORM's range
> returns there to that PERFORM. Neither manual says what a PERFORM that control leaves by GO TO
> leaves behind: its return point stays armed, as the Programming Guide's warning against ranges
> that keep control from the end implies (p. 772), until control passes it and returns after that
> PERFORM, which then puts back the point it displaced; ironwork refuses at run time to return so
> into a PERFORM that repeats or is inside another statement. EXIT SECTION goes to the end of the
> section, past the return point of a performed paragraph in it (LR p. 345).

**V2 is folded into C99.** The walker runs a SORT or MERGE input or output procedure with the same
return point: `procedure_range` is `perform_range(start, end, Some((0, last)), None)`
(machine/sort.rs:677-680 (7af)). What V2 described is that frame's region, the whole program: a
GO TO never leaves the procedure, which ends when control passes its last paragraph's end, at the
end of the program, which the SORT passes on, or at an active PERFORM's return point, which the
SORT takes as the procedure's end (sort.rs:707-708, 734-735 (7af)). C99's text does not name SORT
procedures, so the whole-program region is recorded here as the walker's baseline and wants an
oracle case with C99's. The V series keeps V1 and V2 retired.

B3 of codegen-runtime.md then reads: the VM's result is C99's, which is the walker's.

### 8.7 Nesting depth

The walker counts every PERFORM, inline ones included, every CALL, LINK, XCTL and INVOKE, and each
SORT procedure, USE procedure and debugging section run, against `MAX_DEPTH` = 100 (unit.rs:55;
machine.rs:453-459), and abends IRONWORK "PERFORM and CALL nest deeper than 100" at the statement.
The check comes once per statement, before the first iteration, so `PERFORM P 0 TIMES` can still
abend.

- **`Nest`** at the head of each PERFORM checks and raises the depth; **`Unnest(n)`** lowers it on
  each lexical exit: normal completion, EXIT PERFORM, and NEXT SENTENCE out of `n` inline PERFORMs.
- **Frames record the depth** their paragraphs run at. A paragraph end, a transfer and a resume
  reset the depth to the top frame's, which releases any inline PERFORMs left by EXIT PARAGRAPH,
  EXIT SECTION or GO TO, and any PERFORM statements whose frames were left, as the walker's
  unwinding does.
- **A procedure run** by a file op or a `Debug` checks and raises the depth at its statement or
  paragraph, runs at the raised depth, and lowers it when its dispatch loop ends. XML PARSE runs
  its processing procedure at the statement's depth and raises nothing, as `xml_event` calls
  `perform_range` (machine/xml.rs).

### 8.8 Other transfers

| Statement | Terminator | Walker |
|---|---|---|
| GO TO | `GoTo`, or `Jump` when elided | machine.rs:375 |
| GO TO … DEPENDING ON | `Switch`; out of range, the next statement | machine.rs:459-464 (f2) |
| GO TO with no target | Nothing: unaltered it falls through; altered, its paragraph's entry transfers (§8.9) | machine.rs:458 (f2) |
| ALTER | `Alter` per pair, in order, then any `DebugAlter` (§9.10) | machine.rs:465-473 (f2) |
| ENTRY | Nothing; the statement after it starts a block a CALL enters (§9.3) | machine.rs:458 (f2) |
| EXIT PARAGRAPH | `ParagraphEnd { next: p + 1 }` | perform.rs:133 (7af) |
| EXIT SECTION | `ParagraphEnd { next: section_end + 1 }`: the check is at the section's last paragraph | perform.rs:134 (7af) |
| EXIT PERFORM, EXIT PERFORM CYCLE | `Jump` to the loop's exit or continuation, with `Unnest` | machine.rs:469-470 |
| NEXT SENTENCE | `Jump` past the next separator period of the paragraph, or `ParagraphEnd`, with `Unnest` | machine.rs:291, 392 |
| STOP RUN | `End(StopRun)` | machine.rs:410 |
| GOBACK, EXIT METHOD | `End(Goback)` | machine.rs:376; 422 (int) |
| EXIT PROGRAM | `ExitProgram`: whether this is the run unit's first program is known only at run time | machine.rs:377-378 |
| Falling off the last paragraph | `ParagraphEnd`, `End(EndOfProgram)` | perform.rs:106-107 (7af) |

### 8.9 ALTER and independent segments

ALTER changes where a paragraph's GO TO goes. The walker keeps the change in `Loaded.altered`, a
target per paragraph of the loaded program (unit.rs:33-34 (f2)), and checks it whenever control
reaches a paragraph, before its statements (`run_region`, machine/perform.rs:125 (7af)).

```rust
Op::Alter { para: ParaId, to: ParaId }                       // ALTER para TO PROCEED TO to
Terminator::AlteredGoTo { para: ParaId, otherwise: BlockId } // at the entry of each altered paragraph
Op::EnterSegment(u8)                                         // at the entry of every paragraph
```

- **The alter table** is mutable state of the loaded program: one optional target per paragraph,
  empty until an ALTER runs. It lives as long as the program's WORKING-STORAGE: it is cleared
  whenever that storage is initialized afresh (the first CALL, the first after a CANCEL, and every
  CALL of an INITIAL program, machine.rs:227-232 (f2)) and kept from one CALL to the next
  otherwise. A dynamic CALL's copy for an ENTRY name has its own. Check refuses ALTER in a
  RECURSIVE program, under THREAD and in a method (exec/src/lib.rs:221-227 (f2)), so no two
  activations share one.
- **`Alter`** sets `para`'s entry to `to`, the first paragraph of the procedure ALTER names.
- **`AlteredGoTo`** begins the entry block of each paragraph some ALTER names, after
  `EnterSegment` and any `Debug`: while the table holds a target t for `para`, control goes to t
  exactly as by `GoTo(t)` (§8.4), leaving each frame whose region does not hold t; otherwise
  `Jump(otherwise)` runs the paragraph as written. The target is known only at run time, so it is
  never elided to a `Jump`.
- **A GO TO with no target** that nothing has altered does nothing: control falls through to the
  next paragraph (machine.rs:458 (f2)). It lowers to no op; Check makes it its paragraph's only
  sentence.
- **Independent segments.** Control reaching a paragraph whose section has a priority-number of 50
  or more from a paragraph of another segment finds the segment in its initial state: the walker
  clears the alter entries of that segment's paragraphs (`enter_segment`, machine.rs:256-269 (7af);
  assumption C52 `ALTERED_GO_TO_RESET`).
- **The segment register** is per activation. It starts at the priority of the paragraph the
  activation starts in (`procedure_start`, or an ENTRY's paragraph, perform.rs:42 (7af)).
  `EnterSegment(p)` at the head of every paragraph's entry block does what `enter_segment` does:
  when p differs from the register and is 50 or more, it clears the alter entries of the
  paragraphs whose `priority` is p; then the register holds p.
- **Frames restore it.** `run_region` saves the register when a range starts and restores it when
  the range ends by completing or by a return to its own or an outer PERFORM, clearing nothing,
  and not when control leaves it (perform.rs:102, 170-172 (7af)); a resume, inside the region or
  above it, sets the register to the resumed paragraph's priority (perform.rs:140-143, 153-157
  (7af)). So `Frame.segment` holds the register at the push, and completing a frame restores it:
  a PERFORM's return, a return to a PERFORM through nested ones, and a procedure a statement or a
  `Debug` runs, which no op after the statement could restore. Leaving a frame restores nothing,
  the target's `EnterSegment` setting the register, and a GO TO out of a range that comes back as
  a `Resume` sets it from `r.para`, so the resumed statement runs with its own paragraph's
  priority.
- **Only where it shows.** Clearing matters only to a paragraph an ALTER names, so lowering emits
  `EnterSegment` only when an ALTER names a paragraph of an independent segment; otherwise
  priorities have no effect the walker shows, and independent segments lower as other paragraphs
  do. The register's save and restore are the VM's, with no op.

## 9. Statements as LIR ops

### 9.1 Every statement

"One call" means the op is one semantics-library call. "Lowered" means lowering does work the
walker does on each execution; the last column names that work.

| Statement | LIR | Maps | The walker re-decides at run time |
|---|---|---|---|
| MOVE | `Move` per receiver (§9.2) | Lowered | Category dispatch in `assign` and `alnum_image` (machine.rs:1657-1768) |
| COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE | `Arith`, then `Select` if handled | Lowered | §7.1 |
| INITIALIZE | `Initialize` with a flat plan of (offset, length, value, store) | Lowered | The walk over the item's children (machine.rs:1968-1993) |
| SET TO TRUE, TO FALSE | `Move` of the first VALUE's low end, or of WHEN SET TO FALSE's value, into the conditional variable by item index; nothing when there is none | Lowered | The conditional variable by item index (machine.rs `set`) |
| SET TO | `Set` per receiver; a `POINTER` receiver takes only an address or NULL, else `Refused` | One call | The kind test |
| SET TO ENTRY | None: `Unsupported` | Not lowered | The entry loaded and named in the run unit's list |
| SET ADDRESS OF | One `SetAddress` for all the records; a target that is not an 01 or 77 of LINKAGE ends the block in `Abend` after the records before it | One call | Resolve and linkage test |
| SET UP BY, DOWN BY | One `SetUpDown`: each receiver `Pointer`, `Number` with a `StepPlan` of dmax 0, or `Refused` | One call | Read, then match on the value |
| INSPECT | `Inspect` over constant patterns and a prebuilt CONVERTING table when both operands are literals of one length and the item is not national; each TALLYING counter with its `StepPlan` | One call | Literal images and the CONVERTING table (machine.rs:822-834, 849-869) |
| STRING | `String`, then `Select` of two arms whether or not a phrase is written | One call | `natural_bytes` of literals (machine.rs:686-697) |
| UNSTRING | `Unstring` with each receiver's MOVE plan, DELIMITER IN's two, and COUNT IN's and POINTER's stores, then `Select` as for STRING | One call | `assign` dispatch per field (machine.rs:773) |
| SEARCH | Blocks: `SetCount` of an OCCURS DEPENDING ON table, `InTable` branch, one branch per WHEN, a `SetInt` of index + 1, and of the VARYING item + 1 when it is not the index | Lowered | The index by name (machine.rs:880); table and count |
| SEARCH ALL | `SearchAll`, with each key matched to a WHEN term by item, then `Select`, then the whole condition | One call | `flatten_and` and `key_term` by name on each execution (machine.rs:908-917) |
| IF | `Branch` | Lowered | - |
| EVALUATE | A chain of `Branch`, one per object; each comparison evaluates its subject, as the walker does | Lowered | - |
| DISPLAY | `Display` with a format per item | One call | Kind dispatch (machine.rs:1913-1959) |
| ACCEPT | `Accept` with the MOVE plan of what its source gives: SYSIN's line as bytes, a date, day, weekday or time as an integer of its digits. `rt::accept` stores SYSIN data itself, card images filling the receiver unconverted (C261); the plan only names the receiver's store | One call | - |
| CALL, CANCEL | `Call`, then `Select`; `Cancel` (§9.3) | One call | Literal names decoded (machine.rs:966-971) |
| OPEN … START | `File` per file named, then `Select` when a phrase is written (§9.4) | One call | File by name, keys, FILE STATUS, which phrase applies |
| SORT, MERGE, RELEASE, RETURN | `Sort`, which runs its procedures as ranges; `Release`; `Return`, then `Select` of two arms (§9.6) | One call | SD, files, keys, collating sequence, special registers and procedures by name |
| INITIATE, GENERATE, TERMINATE, SUPPRESS | `Report` per report named, over `Services.report` (§9.6) | One call | Report and group by name; SOURCE, SUM, VALUE and CONTROL as the AST (machine/report.rs) |
| INVOKE | `Invoke`, then `Select` (§9.8) | One call | Receiver kind, Java types |
| EXEC CICS | `Cics`, which returns `GoTo` for a HANDLE label (§9.5) | One call | The command string, option scans and HANDLE labels |
| EXEC SQL | `Sql`, then a `Branch` on `Cond::Sql` per WHENEVER GO TO (§9.7) | One call | Host variables, SQLCA fields and WHENEVER labels by name |
| EXEC DLI, other EXEC | An `Abend` terminator with the walker's EXEC message, at the block | - | - |
| Declarative EXEC SQL | Nothing (machine.rs:393); its `SqlEntry` still exists | - | - |
| FUNCTION | `Operand::Function` (§9.9) | One call | Name and arity (machine.rs `function`) |
| DECLARATIVES | Their paragraphs, and a range for each procedure that can run: the file ops run USE AFTER EXCEPTION/ERROR ones; under DEBUG, `Debug`, `DebugAlter` and `DebugLine` (§9.10) | Lowered | The procedure by file and mode; the triggers |
| PERFORM, GO TO, EXIT, STOP RUN, GOBACK, NEXT SENTENCE | Terminators (§8) | Lowered | Procedure names (machine.rs:308-310) |
| GO TO … DEPENDING ON, ALTER, ENTRY | `Switch`; `Alter` and `AlteredGoTo` (§8.9); an entry block (§9.3) | Lowered | Procedure names (machine.rs:459-473 (f2)); where an ENTRY begins |
| JSON GENERATE, JSON PARSE, XML GENERATE | `Markup`, then `Select` when a phrase is written (§9.13) | One call | The phrases' items by name, the tree walked by name and kind |
| XML PARSE | `Markup`, which runs the processing procedure per event, then `Select` (§9.13) | One call | The procedure by name, the registers by name |

### 9.2 MOVE

```rust
pub enum MovePlan {
    /// Group or alphanumeric: the image, left- or right-justified, space-padded or cut (1659-1672).
    Alnum { image: Image, justified: bool },
    /// The image over the PICTURE's data positions, then edited (1695-1701).
    AlnumEdited { image: Image, edit: u32, positions: u32 },
    /// UTF-16 units, padded with U+0020 (1673-1685).
    National(NationalFrom),
    Numeric { from: NumericFrom, store: StorePlan },
    Float { from: FloatFrom, precision: Precision },
    /// POINTER, OBJECT REFERENCE, FUNCTION-POINTER, PROCEDURE-POINTER: an address or NULL.
    Address,
    Index,
    /// A pair the walker refuses when it moves it.
    Refused(AbendId),
}

/// `Stored`: a numeric, floating-point or pointer sender's own bytes as stored, once it has been read.
pub enum Image { Bytes, All, Figurative, Digits { digits: u32 }, Stored }
pub enum NationalFrom { Units, Decoded, Figurative }
pub enum NumericFrom {
    /// The sender as a number; a zoned or packed sender through `store::move_sender`, which gives
    /// digits that are not decimal as bytes to carry unchecked (C260).
    Value,
    /// NUMPROC(PFD), packed to packed of the same kind and scale: the bytes (1704-1711).
    PackedCopy,
    Float,
    Zero,
    /// Another figurative constant or an ALL literal: bytes filled, not converted (1720-1724).
    Fill,
    /// Alphanumeric bytes read as an unsigned zoned integer of their length (1731-1734); to a zoned or
    /// packed integer without P scaling, the low halves of their last bytes, stored unchecked (C240).
    Zoned,
    /// A numeric-edited sender, de-edited (1727-1730).
    DeEdit { edit: u32, digits: u32, scale: u32 },
}
pub enum FloatFrom { Float, Fixed, Zero }
/// NUMCHECK's test of a MOVE's sending item once it is located, before it is read (§9.14).
pub enum SenderCheck {
    /// No NUMCHECK, a sender that is not a data item, or a zoned sender ZON(LAX) exempts.
    None,
    /// As every operand read is tested.
    Item,
    /// An alphanumeric or group sender to a numeric receiver: an unsigned integer's digits.
    Integer,
}
```

Every category pair, by the value the walker reads from the sender (line numbers in machine.rs):

| Sender | Group, alphanumeric | Alnum-edited | National | Numeric, numeric-edited | Float | Pointer kinds | Index |
|---|---|---|---|---|---|---|---|
| Group | Copied | Copied, not edited | Decoded to UTF-16 | Copied, not converted | Copied | Refused | Refused |
| Alphanumeric, either edited | Copied | Edited | Decoded to UTF-16 | Unsigned zoned integer, S0C7 unless digits; to a zoned or packed integer, its digits' low halves unchecked (C240); numeric-edited is de-edited | Refused | Refused | Refused |
| National | Refused | Refused | Units | Refused | Refused | Refused | Refused |
| Integer numeric | Its digits, unsigned; zoned or packed digits that are not decimal, unchecked (C260) | Digits, edited; unchecked as to alphanumeric (C260) | Refused | Stored; PFD packed copy; zoned or packed digits that are not decimal, unchecked to zoned, zoned to packed, and in the PFD copy (C260) | Converted | Refused | Stored |
| Numeric with decimals | Refused | Refused | Refused | Stored; unchecked as for integers (C260) | Converted | Refused | Stored |
| COMP-1, COMP-2 | Refused | Refused | Refused | Rounded in the receiver's low-order position, at most 9 significant digits from short and 18 from long (`float::to_receiver`), then stored | Narrowed rounding (`float::narrow_rounded`) or lengthened | Refused | Refused |
| ZERO | Zeros | Zeros, edited | U+0030 units | Zero | Zero | Refused | Refused |
| SPACE, QUOTE, HIGH-, LOW-VALUE | Filled | Filled, edited | Its unit | Bytes filled | Refused | Refused | Refused |
| NULL | Filled with X'00' | Filled, edited | U+0000 | Bytes filled | Refused | NULL | Refused |
| ALL literal | Repeated | Repeated, edited | Refused | Bytes filled cyclically | Refused | Refused | Refused |
| Pointer kinds | Refused | Refused | Refused | Refused | Refused | Copied | Refused |

- **Group moves** convert nothing, as `assign` (machine.rs:1953-1970 (7af)) and the Language
  Reference (SC27-8713-03, p. 410) have it. A group sender to a numeric, floating-point,
  numeric-edited or alphanumeric-edited receiver is `Alnum { image: Bytes, justified: false }`: its
  bytes, space-padded or cut. A numeric, floating-point or pointer item moved to a group receiver is
  `Alnum { image: Stored, justified: false }`: its bytes as stored, and a zoned or packed sender's
  unchecked (C260). A literal moved to a group receiver moves as to an alphanumeric one. SET TO and
  WRITE and REWRITE FROM take the same plans.
- **A zoned or packed sender** of a MOVE, or of WRITE, REWRITE or RELEASE FROM, is read by
  `rt::store::move_sender`, not `read`. Where the pair's code checks nothing (`moved_unchecked`:
  to zoned, zoned to packed, the PFD packed copy, to alphanumeric, alphanumeric-edited or group) and
  the digits or sign are not decimal, it gives the bytes as stored, and `assign` carries them
  unchecked; the data exception comes where the receiver is next read as a number (C260). The plans
  are unchanged: the VM must read these senders through `move_sender` too, or it abends at the MOVE
  where the walker does not.
- **NUMCHECK's test of the sender** is `Move.check`, and `FromMove.check` for WRITE, REWRITE and
  RELEASE FROM. Lowering decides it with `rt::store::move_check` from the sender's and receiver's
  kinds, the function the walker's `move_source` calls on each execution: `Integer` for an
  alphanumeric or group sender to a zoned, packed, binary, floating-point or numeric-edited
  receiver; `None` for a zoned sender to a zoned, alphanumeric or group receiver under ZON(LAX)
  (Programming Guide SC27-8714-03, pp. 388-391); `Item` for any other data item; `None` for any
  other sender, and in a program without NUMCHECK. The executor locates the receiver, then the
  sender, runs `rt::store::numcheck_sender` with `check`, then reads the sender with `move_sender`.
  `Set` carries no test of its own: its sender is an operand read, which §9.14 tests.
- **Several receivers** lower to one `Move` each. Each locates its receiver, then reads the sender
  again (machine.rs:315-318), so a receiver stored earlier can change what a later one gets (§11).
- **MOVE, ADD and SUBTRACT CORRESPONDING** reach lowering already expanded: the compiler turns
  each into a MOVE per pair of corresponding items, or one ADD or SUBTRACT with a computation per
  pair (compile's `corresponding`; C130, C131), so lowering sees only `Move` and `Arith`.
- **Figuratives and ALL literals** are constants; the plan says how each fills the receiver.

### 9.3 CALL, CANCEL and LE services

```rust
/// `returning` receives the callee's RETURNING item, whose kind is the callee's, so that MOVE is
/// chosen when the callee returns (machine.rs:1043-1046).
pub struct CallPlan {
    pub target: CallTarget, pub args: Vec<CallArg>, pub returning: Option<PlaceId>,
    pub on_exception: bool, pub not_on_exception: bool,
}

pub enum CallTarget {
    /// A literal: the name is fixed; finding the program is not (unit.rs:136-156). `le` names the
    /// callable service to run when no program has the name (machine.rs:1034 (int)).
    Named { name: SymId, le: Option<LeService> },
    /// An identifier, decoded, trimmed and upper-cased at run time (machine.rs:966-971).
    Dynamic(Operand),
    /// A FUNCTION-POINTER or PROCEDURE-POINTER: a JNI service (machine/oo.rs:446-496 (int)).
    Pointer(PlaceId),
}

pub enum CallArg {
    Reference(PlaceId),
    /// Copied to a temporary as `content_argument` does (machine.rs:1275-1292 (f2)).
    Content(Chars),
    /// A fullword, an address or bytes, as `value_argument` does (machine.rs:1295-1306 (f2)).
    Value(Operand),
    Omitted,
}

/// An ENTRY statement (`Compiled.entries`, exec/src/lib.rs:60-82 (f2)): a CALL of `name` starts at
/// `block`, the block that begins with the statement after the ENTRY in paragraph `paragraph`, in
/// the Main frame, with the segment register at that paragraph's priority, and binds `using`, LINKAGE
/// record ordinals, in place of `Storage.using`.
pub struct EntryPoint { pub name: SymId, pub paragraph: ParaId, pub block: BlockId, pub using: Vec<u16> }
```

`Services.entries` holds them in source order, the order `Loaded.entry` numbers them. A dynamic
CALL of an entry name gets a copy of the program of its own (assumption C51 `ENTRY_CALLS`,
unit.rs:150-171 (f2)).

- **The op returns** `Arm(0)` after a normal return, `Arm(1)` when the program is not found and ON
  EXCEPTION is written, or `End(StopRun)`; `Next` in place of `Arm(0)` when neither ON EXCEPTION
  nor NOT ON EXCEPTION is written, so that only an op with a phrase is followed by a `Select`.
  Without ON EXCEPTION a missing program abends S806.
- **The target.** A literal's name is `literal_value` then `program_name` done at lowering
  (machine.rs:1150-1155 (f2)): the text in the program's code page, decoded, trimmed and
  upper-cased. A literal that is not alphanumeric or hexadecimal, or that the code page cannot
  encode, lowers to the walker's IRONWORK abend, an `Abend` terminator at the statement, since the
  walker gives it before looking for a program. A data item holding a FUNCTION-POINTER or
  PROCEDURE-POINTER is `Pointer`, as `call_through_pointer` decides by the item's declared kind
  (machine/oo.rs:563-568 (f2)), when the item is a field of JNINATIVEINTERFACE; through any other
  pointer, which SET TO ENTRY can make name a program, the CALL is `Unsupported`, as SET TO ENTRY
  is. Any other identifier, and LENGTH OF or ADDRESS OF, is `Dynamic`,
  whose operand is read as its kind reads it (a numeric item's invalid data abends S0C7) before a
  value that is not alphanumeric bytes abends IRONWORK "a program name must be alphanumeric".
- **Arguments** keep `call_nested`'s order and forms (machine.rs:1191-1211 (f2)): OMITTED;
  BY REFERENCE of a data item, its address; BY VALUE, `value_argument` of the operand's value; and
  anything else, BY CONTENT or BY REFERENCE of a literal, LENGTH OF or ADDRESS OF, a copy. A
  `Content` literal's bytes are made at lowering as `content_argument` makes them: alphanumeric
  and hexadecimal bytes, national UTF-16 units, an ALL literal's bytes once, a figurative constant
  as the collating sequence's one byte, and a number as unsigned zoned digits, as many as the
  literal has, the last with a minus zone when it is negative. `Content(Value(o))` is LENGTH OF as
  a binary fullword or ADDRESS OF as its four bytes.
- **Through a pointer** (machine/oo.rs:563-672 (f2)) every argument is `Value` or `Omitted`: the JNI
  service takes each operand's value as it reads, OMITTED as NULL, whatever BY phrase is written.
  No depth is counted, ON EXCEPTION never runs, and `returning` receives the service's result.
- **RETURNING** is located after the callee returns and receives its RETURNING item's value by
  MOVE rules chosen from the value's kind, or the service's result; not after STOP RUN.
- **LE services** run only after the program search fails, as assumption L1
  `LE_SERVICE_AFTER_PROGRAMS` (int) records. `LeService` is an enum of the services
  `rt::le::call` runs, over an `LeHost` that gives it the run unit, the code page and a loaded
  method's name for CEE3DMP; arguments are addresses, as for a program (`rt::callee::addresses`).
  ON EXCEPTION never runs for a service. A `Dynamic` target's name is matched with a service when
  the CALL runs.
- **CANCEL** is one `Cancel` per name, in order, each read as `program_name` reads a `Dynamic`
  target (machine.rs:478-483 (f2)).
- **Dynamic at run time:** loading and compiling on first CALL, RECURSIVE and INITIAL handling, the
  recursion check, the depth check (after the load, before the arguments), CANCEL's effect, and
  temporaries (machine.rs:973-1122). How a static CALL binds is load-module.md §8.3.
- **PARMCHECK and NUMCHECK** run inside the op: the buffer is set after the arguments and tested
  after the callee returns, and a BY CONTENT or BY VALUE data item is tested as it is copied (§9.14).
- **One sequence in `rt::callee`** for both executors, and for LINK, XCTL, INVOKE and a user-defined
  function's invocation (§9.15): `addresses` builds the arguments, `Bindings` gives the callee's
  LINKAGE records their addresses, and `run` wraps the executor's activation of the callee
  (inactive after, an INITIAL program a CALL entered cancelled, temporaries released, an abend
  named by a library program's own files). CANCEL is `rt::callee::cancel`. Each host trait reaches
  the run unit through `rt::unit::UnitHost`.

### 9.4 Files

A file statement stays one `rt::files` call per file it names, as `machine/file_io.rs` makes it. The
payload names the file by index and gives the places, plans and phrases the walker finds by name on
each execution; the status, the record, the DD and the in-memory file are run-time state.

```rust
/// SELECT and FD. `format` is how records are held when the DD does not say (`described_format`);
/// `read_lengths` the shortest and longest variable-length record a READ takes without status 04,
/// as VLR measures them (`compile::read_lengths`); `fixed` no RECORDING MODE V and its smallest
/// record as long as its largest, which a SORT's records follow (`fixed_length`, machine/sort.rs);
/// `record_min` the RECORD clause's smallest record, which a variable-length report record is cut
/// to no shorter than (`ReportFile`); `status` FILE STATUS with the MOVE its two characters take
/// (`set_status`), None also when it names no data item and no statement names the file.
pub struct FileDesc {
    pub name: SymId, pub assign: SymId, pub organization: Organization, pub access: Access,
    pub optional: bool, pub format: rt::files::Format, pub read_lengths: (u32, u32),
    pub fixed: bool, pub record_min: Option<u32>, pub depending: Option<RecordDepending>,
    pub status: Option<(PlaceId, MovePlan)>,
    /// RECORD KEY, then each ALTERNATE RECORD KEY with WITH DUPLICATES, as spans of the record area.
    pub keys: Option<IndexKeys>,
    pub relative: Option<RelativeKey>, pub linage: Option<Linage>, pub carriage: Option<Carriage>,
    /// `error` is the file's own USE AFTER EXCEPTION/ERROR procedure (§9.10).
    pub sort: bool, pub error: Option<RangeId>,
}
pub struct IndexKeys { pub prime: RecordSpan, pub alternates: Vec<(RecordSpan, bool)> }
pub struct RecordSpan { pub offset: u32, pub len: u32 }
/// RECORD IS VARYING DEPENDING ON (`fileio::Depending`): the item, read as an integer and stored as
/// `set_integer` stores, and the shortest and longest record the clause allows (`compile::varying_lengths`).
pub struct RecordDepending { pub item: PlaceId, pub lengths: (u32, u32) }
/// Read as `value`, stored by `store` when a sequential READ or WRITE sets it; `digits` bounds the
/// record numbers it holds (`relative_fits`).
pub struct RelativeKey { pub place: PlaceId, pub value: IntExpr, pub store: StorePlan, pub digits: Option<u32> }
/// Each value evaluated in this order whenever the page's geometry is taken, and LINAGE-COUNTER.
pub struct Linage { pub lines: IntExpr, pub footing: Option<IntExpr>, pub top: Option<IntExpr>, pub bottom: Option<IntExpr>, pub counter: Option<(PlaceId, StorePlan)> }
pub struct Carriage { pub machine: bool, pub reserved: bool }

pub struct FileOp { pub file: u16, pub verb: FileVerb, pub phrase: Option<Phrase>, pub end_of_page: Option<Phrase> }
/// The ON and NOT ON phrases written.
pub struct Phrase { pub on: bool, pub not_on: bool }

pub enum FileVerb {
    Open(OpenMode),
    Close,
    /// CLOSE REEL or UNIT, which leaves the file open, NO REWIND or LOCK (rt::fileio::close).
    CloseWith(Closing),
    /// `sequential`: the phrase is AT END and a held file reads in sequence; else INVALID KEY, and
    /// `key` is the key of reference of an indexed file.
    Read { sequential: bool, previous: bool, into: Option<(PlaceId, MovePlan)>, key: u8 },
    /// `record` is located after FROM has moved into `FromMove.to`, the record as a receiving item.
    Write { record: PlaceId, from: Option<FromMove>, advancing: Option<Advance> },
    Rewrite { record: PlaceId, from: Option<FromMove> },
    Delete,
    Start { rel: StartRel, key: StartKey },
}
/// `check` is NUMCHECK's test of `from` (§9.2).
pub struct FromMove { pub from: Operand, pub to: PlaceId, pub plan: MovePlan, pub check: SenderCheck }
pub enum Advance { Lines { before: bool, count: IntExpr }, Page { before: bool }, Mnemonic { before: bool, space: Spacing } }
pub enum Spacing { Lines(u64), Channel(u8), PageMode }
pub enum StartRel { Equal, Greater, NotLess }
pub enum StartKey { Prime, Named { key: u8, span: RecordSpan }, Relative(IntExpr), RelativeKey }
```

- **The op and its phrases.** With no phrase written the op returns `Next`. Otherwise it returns
  `Arm(1)` for the ON phrase (AT END or INVALID KEY), `Arm(2)` for NOT ON, `Arm(3)` for
  END-OF-PAGE, `Arm(4)` for NOT END-OF-PAGE, and `Arm(0)` when no phrase written runs, and a
  `Select` of `FileOp::arms()` blocks follows: 3, or 5 with END-OF-PAGE. An arm whose phrase is not
  written goes where `Arm(0)` goes. `conclude` sets FILE STATUS before the phrase runs, and a
  failing status with no phrase to run takes the file's error path: a USE AFTER EXCEPTION/ERROR
  procedure the op runs through `Procedures::run`, whose leaving the op returns as its `Step`
  (§9.10), else `IO-xx` when the file has no FILE STATUS.
- **One op per file.** OPEN and CLOSE name several files; the walker opens each in turn and stops at
  the first abend, as a block of ops does.
- **Which phrase READ takes** is fixed by the file: AT END for a sequential file, sequential access,
  READ NEXT or PREVIOUS under dynamic access, and a line-sequential file, which is never held in
  memory and so always reads through `read_stream`; INVALID KEY otherwise. The other phrase never
  runs, and is not lowered. A random READ of a keyed file that is not open goes through
  `read_stream` and takes AT END with status 47, which neither phrase covers, so the choice gives the
  walker's result either way.
- **WRITE's phrases.** A WRITE to a file open with a page (LINAGE, OUTPUT or EXTEND) runs END-OF-PAGE
  or NOT END-OF-PAGE; to a held file, INVALID KEY or NOT INVALID KEY through `conclude`; to a
  streamed file neither. Which applies is known only at run time, so the op carries both.
- **RECORD IS VARYING DEPENDING ON.** `depending` gives `rt::fileio` the item a successful READ or
  RETURN stores the record's length in (`deliver`, `return_record`) and WRITE, REWRITE and RELEASE
  take it from (`record_length`): the record area's first n bytes go out, or status 44 when n lies
  outside `lengths` or past the area, and RELEASE stops the sort. The item is located where the
  walker locates it, each time, and is never a receiving item.
- **A FILE STATUS that names no data item.** The walker looks the name up only when `set_status`
  runs, on a statement that names the file, a SORT or MERGE that uses or gives it, or a report
  written to it, and abends there; and a file with a FILE STATUS clause takes no `IO-xx` abend
  however it is named. Lowering leaves `status` None when no lowered statement names the file,
  which gives the walker's result, and refuses the program otherwise.
- **Fixed at lowering:** the file by name; each key's span, from its place, which must be a static
  item of the file's record area; which key READ KEY or START KEY names, a leading part for START;
  START's relation, one other than =, > or NOT < being the walker's abend at the statement; the
  mnemonic-name's movement (`printer::mnemonic_space`); the described format; the FILE STATUS and
  INTO moves, which take alphanumeric bytes; the RELATIVE KEY's read and store; LINAGE's values
  and LINAGE-COUNTER's place, a static item of the slab. Check refuses what the walker would abend on
  here (no such file, a key outside the record, a mnemonic-name that is no channel, one on a LINAGE
  file), and lowering refuses it too, by name.
- **Run-time state:** the open file, its mode and position, the DD and its format, the page, the
  status each call gives, which WRITE path applies, and the last file whose statement failed, which
  SORT reads.

### 9.5 EXEC CICS

`rt::cics::CicsCommand` (rt/src/cics/command.rs), generic like §9.3's payloads: `P`, `O` and `S`
are the LIR's `PlaceId`, `Operand` and `SymId`, or the walker's `&Ref`, `&Operand` and `&str`.

```rust
pub struct CicsCommand<P, O, S> { pub name: S, pub command: Cics<P, O, S>, pub resp: Resp<P, O, S> }
/// RESP, RESP2 and NOHANDLE decide what `raise` does.
pub struct Resp<P, O, S> { pub resp: Opt<P, O, S>, pub resp2: Opt<P, O, S>, pub nohandle: bool }
/// An option's argument; an absent option is None.
pub enum Datum<P, O, S> { Place(P), Value(O), Text(S), Bare }
pub type Opt<P, O, S> = Option<Datum<P, O, S>>;

/// 34 variants: File { verb: FileControl, file, options: FileOptions } for the ten file-control
/// commands; Return; Link and Xctl (Transfer); Abend; HandleCondition(Vec<(Condition,
/// Option<ParaId>)>); IgnoreCondition(Vec<Condition>); PushHandle; PopHandle; HandleAbend
/// { program: Opt, label: Option<ParaId>, reset }, tag 34 with 9 retired; HandleAid; SendMap; ReceiveMap; SendControl;
/// Receive(Record); Asktime; Formattime; Assign; Getmain; Freemain; Enq; Deq; Delay;
/// Syncpoint { rollback }; Address; SendText; WriteOperator; WriteqTs; ReadqTs; DeleteqTs;
/// WriteqTd; ReadqTd; DeleteqTd; and Unsupported, "EXEC CICS … is not supported yet".
pub enum Cics<P, O, S> { /* … */ }
```

`name` is the command as written, which messages give (SEND for SEND MAP written as SEND
MAP(name)). `cics::Condition` is the one RESP and default-abend table (semantics-library.md §7,
DRY-4). An option is evaluated when the service reads it, in the order the walker read it, so
binding evaluates nothing. `rt::cics::run` returns `Next`, `GoTo(ParaId)` for a handled condition,
or `End` for RETURN, XCTL and a LINKed program's STOP RUN. A HANDLE ABEND exit is not a transfer
`run` returns: an abend that reaches a program's activation, from the program, a CALL or a lower
logical level, goes to `rt::cics::abend_exit` with the activation's number, and the executor runs
a LABEL that activation set as a GO TO, or enters the PROGRAM with `enter_exit_program` where the
program runs the logical level (the walker's `run_level`; a CALLed program's `run_called` takes
only a LABEL). Handler tables, the task,
the EIB and the terminal stay run-time state. What `run` asks of its executor is `CicsHost`: the
run unit and the program's handlers, operands that are not data items, DFHCOMMAREA's address, a
mapset from the copy libraries, the symbolic map's `mapI` and `mapO` by name, and running a program
for LINK and XCTL. The walker binds a block in machine/cics_bind.rs, matching the command words and
options the translator gives and resolving HANDLE labels there. SYNCPOINT is a service
(cics/services.rs) that settles the SQL session through `Session::settle`.

- **Lowering binds with the walker's `bind`.** Each EXEC CICS block is one `Op::Cics` naming its
  command in `Services.cics`. Lowering calls machine/cics_bind.rs's `bind` and maps the command it
  gives through `cics::Handles` (`CicsCommand::map`): a data item becomes a place, located as the
  walker locates it, not as a receiving item; another operand an `Operand`; text a symbol. So the
  command, its options and RESP, RESP2 and NOHANDLE are the walker's, and every refusal `run` gives
  (outside a task, a command ironwork does not carry out, an option it needs) comes from the same
  code at the same point. A command ironwork does not carry out lowers to `Cics::Unsupported`.
- **Labels are paragraphs, and handlers are run-time state.** HANDLE CONDITION holds the `ParaId`
  its labels resolve to, as `crate::procedure_from` resolves them for the walker. HANDLE, IGNORE,
  PUSH and POP change the program level's `Handlers` when the op runs, and a condition `raise`
  sends to a label makes the op return
  `Step::GoTo(para)`, which the VM takes as a GO TO by the transfer rules of §8.4: it leaves every
  frame whose region does not hold the paragraph and resets the depth, as the walker's
  `Flow::GoTo` does. The table is per activation, as the walker's `Machine.cics_handlers` is, so a
  LINKed program starts with none; the walker moves the HANDLE ABEND exit into a statically CALLed
  program and back (C237). RETURN and XCTL return `Step::End`. No new terminator is needed.
- **Refused:** a HANDLE label that names no procedure, which the walker abends on only after the
  task check (IRONWORK at the block, or the outside-a-task abend first), so no one terminator
  gives both. HANDLE ABEND, whose exit an abend takes when it reaches the program's activation,
  in the walker's `run_level` and the VM's alike (below); lowering still refuses it, so no VM
  activation sets an exit, and the VM has none of the walker's CALL and XCTL rules for one
  (C237-C239).
- **Not lowered:** the observer's sinks (`cics_sinks`), which tell an observer a command's operands
  and change no result, as with CALL's and DISPLAY's. The VM tells them from the options the
  command keeps, which leave out SYSID on any command but ASSIGN, WRITE's FROM under JOURNALNAME or
  JOURNALNUM, and QNAME written beside QUEUE; an observed command kept as `Unsupported` stops the VM
  as `Halt::Unimplemented`.
- **On the VM.** A program level's handlers live in its activation. An abend that reaches a level
  whose HANDLE ABEND exit is active goes to the exit as `run_level` sends it: a LABEL restarts the
  activation's dispatch at the label, its frames gone as the walker's Rust calls are, the points
  they armed still armed and the depth the activation's. LINK and XCTL run the program as a new
  activation through `CicsHost::run_program`. The mapset comes from `Loader::mapset`; the LIR has
  no place for the symbolic map SEND MAP without FROM and RECEIVE MAP without INTO or SET find by
  name, so those stop the VM as `Halt::Unimplemented`.

### 9.6 SORT, MERGE and Report Writer

Both run COBOL procedures from inside a service, through one trait, and so does a file op for a
USE AFTER EXCEPTION/ERROR procedure (§9.10):

```rust
/// How a service runs a COBOL procedure. The walker implements it with `run_paragraphs` and
/// `procedure_range`; the VM runs a dispatch loop of its own over the range under a Procedure
/// frame (§8.4), with the arrival register (§9.10) at `arrival`: USE PROCEDURE, or SORT INPUT,
/// SORT OUTPUT or MERGE OUTPUT.
pub trait Procedures {
    fn run(&mut self, range: RangeId, arrival: Arrival) -> Result<RangeEnd, Abend>;
}

pub enum RangeEnd {
    Completed,
    /// Control left the procedure's frame by this transfer: `GoTo`, `End`, `Return` or `Resume`.
    Left(Step),
}

/// rt/src/lir/sort.rs. `FileSort` is generic over the handles the executor resolves as the
/// statement runs (`SortHost`): the walker's references and procedure names, or the LIR's ids.
pub enum SortPlan { File(FileSort), Table(TableSort) }
pub struct FileSort<R = PlaceId, Q = RangeId, K = SortKeys, F = u16> {
    pub sd: u16, pub merge: bool, pub keys: K,
    pub input: Option<SortIo<Q, F>>, pub output: Option<SortIo<Q, F>>,
    pub sort_return: R, pub sort_control: R,
}
pub enum SortIo<Q = RangeId, F = u16> { Files(Vec<F>), Procedure(Q) }
/// Offsets in the record with kind and direction; `collating` is the sequence of the keys marked
/// `collated` when it is not EBCDIC.
pub struct SortKeys { pub keys: Vec<SortKey>, pub collating: Option<Box<[u8; 256]>> }
pub struct SortKey { pub ascending: bool, pub offset: u32, pub len: u32, pub kind: Kind, pub item: u32, pub collated: bool }
pub struct TableSort { pub first: PlaceId, pub count: Count, pub stride: u32, pub keys: SortKeys, pub name: SymId }
/// `record` is located after FROM has moved into `FromMove.to`, the record as a receiving item.
pub struct ReleasePlan { pub record: PlaceId, pub file: Option<u16>, pub from: Option<FromMove>, pub sort_return: PlaceId, pub name: SymId }
pub struct ReturnPlan { pub file: Option<u16>, pub into: Option<(PlaceId, MovePlan)>, pub sort_return: PlaceId, pub name: SymId }
```

- **SORT and MERGE** run as `rt::sort::sort`: gather, a stable sort, scatter, SORT-RETURN, and the
  FASTSRT plan, which it works out from the files' `FileDesc`s on each run. A procedure runs
  through `SortHost::run_procedure`, which checks and raises the depth at the statement (§8.7) and
  sets the arrival register to SORT INPUT, SORT OUTPUT or MERGE OUTPUT.
- **What lowering resolves** (lower/sort.rs). A SORT whose subject is a file's name, unqualified
  and unsubscripted, is a `FileSort`, any other a `TableSort`, as `sorting` tells them apart. The
  SD and the USING and GIVING files are file indices; SORT-RETURN and SORT-CONTROL are places of
  the special registers Check declares, named at the statement; each key is its item's offset in
  the SD's record area (`file_keys`) or in the table's element (`table_keys`), with its length,
  kind and item. `collating` is the COLLATING SEQUENCE phrase's alphabet, else for a file SORT or
  MERGE the PROGRAM COLLATING SEQUENCE, and None when that is EBCDIC or NATIVE; a key is
  `collated` when it is alphanumeric, a group or edited (`compile::sort::collates`).
- **A procedure is a range.** Each INPUT or OUTPUT PROCEDURE is a `SortProcedure` range from its
  first paragraph to the last of THRU's, found as `crate::procedure` finds it. Its region is every
  paragraph (§8.6), so a GO TO in it never leaves its frame, and its last paragraph ends in
  `ParagraphEnd` and carries `abandoned`. `Left(End(e))` makes the op return
  `Step::End(e)`; any other `Left`, a return to an active PERFORM, the SORT takes as the procedure's
  end and goes on (machine/sort.rs:707-708, 734-735 (7af)).
- **RELEASE and RETURN** find the active sort in run-time state (`rt::sort::Active`, which the
  executor keeps). RELEASE locates `FromMove.to`, or `record` without FROM; then `release_ready`,
  with `file` the record's file (`Item.file`); then FROM's MOVE into the `Loc` it located; then
  `record` located again and `release`. A record holding the object of its own OCCURS DEPENDING
  ON is at its maximum length as FROM's receiver and at its current length as it is released.
  RETURN is `return_record`, with INTO located as a receiving item and given the record's bytes by
  its `MovePlan`; it returns `Arm(0)` at end and `Arm(1)` for a record, and a `Select` of two
  blocks, AT END's and NOT AT END's, follows whether or not either is written.
- **A table SORT** is one op with the element's stride and key offsets fixed. It evaluates `count`
  (the OCCURS DEPENDING ON object, as `occurrences` does), then locates `first`, the subject with
  a subscript of 1 added and no reference modification, then checks the elements lie in run-unit
  storage, as `sort_table` does.
- **The key order** is `rt::sort::Keys`, `order` and `KeyValue`, which take keys described as plain
  data (position, length, DFSORT format, direction) as well as a program's items, so a sort utility
  with no program behind it orders records the same way.
- **Refused:** a key, file or procedure the walker cannot resolve and a COLLATING SEQUENCE it
  cannot build, which it abends on only part way through the statement (after SORT-RETURN is set,
  the input phase has run, or a table's count is taken); a table SORT of a name that is no data
  item; and a sort statement in a program without the sort special registers. Check refuses each,
  so no program that compiles cleanly meets them.
- **Report Writer.** `rt::report::Writer<X, C, V, U>` is the report model `compile::report`
  resolves, generic over a SOURCE or SUM operand `X`, a CONTROL item `C`, a VALUE or CODE literal
  `V` and a USE BEFORE REPORTING procedure `U`: the interpreter's `Expr`, `Ref`, `Literal` and
  paragraph span, evaluated on every GENERATE, or once lowered (`lir::ReportWriter`, lower/report.rs)
  a `Comparand`, `PlaceId`, `ConstId` and `RangeId`. It is `Services.report`; a program with no
  REPORT SECTION holds the empty writer, two bytes encoded. `rt::report::run` carries out a
  `ReportOp`, which names a report and DETAIL group by index:

  ```rust
  pub enum ReportOp { Initiate(u32), Generate { report: u32, detail: Option<u32> }, Terminate(u32), Suppress }
  ```

- **What the report model holds once lowered.** A SOURCE, a SUM operand outside the REPORT
  SECTION and a summed SOURCE are each a `Comparand`: an operand the writer reads with its storage
  (`ReportHost::operand`, `operand_with_loc`), or an expression it evaluates as `expr_value` does,
  with its float test, dmax pass and mode (§7.5). A CONTROL is a place, located as the walker
  locates it at each GENERATE. A VALUE and the CODE are constants. Each data item the model names
  by `usize` (a field, a SUM total, PAGE-COUNTER, LINE-COUNTER, the state item, PRINT-SWITCH) is the
  id of a static place of the slab at the item's offset, whole, as `ReportHost::item` takes it;
  lowering refuses one in LOCAL-STORAGE or LINKAGE. Report and group names stay text, for the
  writer's messages, and a field's `pos` the position it names in them.
- **The statements.** INITIATE and TERMINATE are an `Op::Report` per report they name, in order;
  GENERATE one `Generate`, its target found as `generate_target` finds it; SUPPRESS PRINTING
  `Suppress`. A name the walker cannot find becomes an `Abend` terminator with its IRONWORK message
  after the ops before it.
- **USE BEFORE REPORTING** is a `UseBeforeReporting` range of its section, which the group's
  `declarative` names and `ReportHost::use_before_reporting` runs, with the arrival USE PROCEDURE,
  after checking and raising the depth at the statement. `GoTo` abends IRONWORK at the report
  statement; `End(_)` ends the run as STOP RUN or GOBACK; `Left`, a return to an active PERFORM or
  a resumed statement, abandons the report statement, and the executor carries out the transfer.

### 9.7 EXEC SQL

The SQL statement table. The load module stores it as its `SQL` section (load-module.md §7), and
the SQL runtime ([sql-runtime.md](sql-runtime.md)) reads it.

```rust
/// One entry per EXEC SQL block, declarative ones included. Ordinals run from 1 in listing order,
/// with INCLUDE expanded as COPY is and not counted (syntax/src/sql.rs:133), and are dense:
/// `Program.sql[k − 1]` has ordinal k. Ordinal 0 is never an entry; it names a commit or rollback
/// no EXEC SQL statement asks for (sql-runtime.md §7, assumption SQ1).
pub struct SqlEntry {
    pub ordinal: u32,
    /// The block's command word, which the call record and the EXEC messages name.
    pub verb: SymId,
    pub statement: SqlStatement,
    /// The canonical text a `Database` call receives and a recording writes: a query's or change's
    /// own, with `?` for each input; `DECLARE C CURSOR [WITH HOLD] FOR …` for OPEN; `FETCH C`,
    /// `CLOSE C`, `COMMIT` and `ROLLBACK`; empty for a declaration. The walker formats it on every
    /// call (machine/sql.rs:69-70, 84, 99).
    pub text: SymId,
    /// `fingerprint(text)`: 32-bit FNV-1a (syntax/src/sql.rs:140), which Replay and Recorder now
    /// hash per call (sql/replay.rs:33, 128, 215).
    pub fingerprint: u32,
    /// True on the OPEN of a cursor declared WITH HOLD, false on every other entry.
    pub with_hold: bool,
}

/// `syntax::sql::Statement` with its host variables resolved and each OPEN given its DECLARE.
pub enum SqlStatement {
    Query { inputs: Vec<HostPlace>, into: Vec<HostPlace> },
    Change { delete: bool, inputs: Vec<HostPlace>, current_of: Option<SymId> },
    Open { cursor: SymId, inputs: Vec<HostPlace> },
    Fetch { cursor: SymId, into: Vec<HostPlace> },
    Close { cursor: SymId },
    Commit, Rollback,
    /// WHENEVER, DECLARE CURSOR, INCLUDE, DECLARE SECTION and the other declarations: no op.
    Declaration,
    /// Dynamic SQL, a statement on a cursor declared for it, and the like: abend EXEC naming it,
    /// when reached (machine/sql.rs:123).
    Unsupported(SymId),
}

/// A host variable, or one member of a host structure, resolved (machine/sql.rs:176-198).
pub struct HostPlace {
    pub var: PlaceId,
    /// A structure member: its offset from the structure's start, and its length.
    pub member: Option<(u32, u32)>,
    /// Never `HostType::Structure`, which lowering expands into members. An item with no SQL type
    /// keeps the walker's EXEC abend, raised when the statement reaches it (decision 2).
    pub ty: Result<HostType, AbendId>,
    /// The indicator and, for a structure member, its element's offset: 2 × the member's index.
    pub indicator: Option<(PlaceId, u32)>,
}

/// The SQLCA fields the program declares, or its own SQLCODE and SQLSTATE (machine/sql.rs:248-274).
pub struct Sqlca { pub fields: Vec<(SqlcaField, PlaceId, HostType)> }
```

- **Identity.** A recording names a call `PROGRAM:ORDINAL:HASH`: the program id as written,
  `ordinal` and `fingerprint` (sql/replay.rs:40). Nothing specific to an executor or a module enters
  it, so a recording made under either executor replays under the other, and the differential test
  can use recordings.
- **Host variables are places.** Only subscripted ones evaluate anything at run time, where
  `sql_targets` now locates every host variable and works out its type on every statement. The
  library asks for each `Loc` where the walker locates it: the inputs before the database call, the
  INTO list after it.
- **The op** calls the library with the entry and leaves SQLCODE and the warning flag in a VM
  register. The library keeps `sql_inputs`, `sql_assign` and `sqlca` over `Loc`s, and `Session`,
  `convert`, the `Database` trait, `Replay`, `Recorder` and the PostgreSQL backend unchanged.
- **WHENEVER** is not in the entry. Lowering emits it as branches after the op, in the walker's
  order (machine/sql.rs:277-286). `SqlTest` classifies as the walker does (assumption SQ2): SQLERROR
  is SQLCODE < 0, NOT FOUND is 100, and SQLWARNING is neither, and warned or > 0. The classes
  exclude each other, so only a condition whose action is GO TO gets a test, and a CONTINUE still
  stops the later ones from applying. The walker returns the branch as `Flow::GoTo`
  (machine/sql.rs:289), as a GO TO statement does, so each target block lowers as GO TO does
  (§8.4, §8.8): `Unnest` for the inline PERFORMs around the statement, then `GoTo(para)`, or a
  plain `Jump` when every range whose region holds the statement's paragraph also holds the
  target and no debugging section serves it. The walker resolves the label by name on every
  statement; lowering resolves it once, and a label that names no procedure makes the target block
  end in `Abend` with the walker's IRONWORK message, raised only when that branch is taken. A
  statement whose entry is `Declaration` (`run` gives None) gets no branch.
- **`Op::Sql(k)` names ordinal k,** `Program.sql[k − 1]`. Lowering builds the whole table first,
  from the DATA DIVISION's EXEC SQL blocks and every one among the procedure's statements, and
  refuses a program whose ordinals do not run from 1 without a gap. A declarative block
  (`ExecBlock::declarative`: INCLUDE, DECLARE SECTION, WHENEVER, DECLARE CURSOR, TABLE and
  STATEMENT) has its entry and no op, as the walker does nothing for it.
- **The SQLCA is resolved once.** The walker resolves the SQLCA fields by name on every statement
  and writes each it can locate, leaving one it cannot as it was (`host::sqlca`). Lowering keeps the
  fields that resolve to an item of an SQL type, as places of `Services.sqlca`, and leaves out one
  written with the wrong number of subscripts, which no statement can locate. A field that is a host
  structure is refused.
- **Host variables** are lowered as `host_places` builds them: a structure's members, each indicator
  element at 2 × its index, and an item with no SQL type as `Err(AbendId)`, the walker's message
  with the host variable's position in `AbendText.at`. A host variable that names no data item is
  refused with the place.
- **No database attached** abends EXEC at run time, as now (machine/sql.rs:28-30).
- **In `rt`.** `SqlEntry` and `SqlStatement` are generic like §9.5's `CicsCommand`: `P` and `S` are
  `PlaceId` and `SymId`, or the walker's `&Ref` and `String`. `rt::sql::run` runs an entry and fills
  the `Sqlca`, and returns SQLCODE and SQLWARN0 for the WHENEVER tests, or None for a declaration.
  What it asks of its executor is `SqlHost`: the session, whether a CICS task is running, the
  program id, text, a host variable's position for a program check, and the abend of a host
  variable with no SQL type. The walker builds the entry, its `HostPlace`s and the `Sqlca` in
  machine/sql.rs on every statement, and takes the WHENEVER branch there.

### 9.8 OO COBOL

```rust
/// Java types come from `operand_type` and `item_type`, which read only declarations
/// (exec/src/oo.rs:298-341 (int)); the walker works them out on every INVOKE. Generic like §9.3's
/// payloads, with `PlaceId`, `Operand` and `SymId` the defaults.
pub struct InvokePlan<P, O, S> {
    pub receiver: Receiver<P, S>, pub method: MethodName<P, S>,
    pub args: Vec<(O, S)>, pub returning: Option<(P, S)>,
    pub on_exception: bool, pub not_on_exception: bool,
}
/// The walker decides the receiver by name on every INVOKE (machine/oo.rs:192-215 (int)).
/// `Class` is a REPOSITORY class-name: `name` as written, which the walker's messages give, and
/// `external`, which finds the class.
pub enum Receiver<P, S> { SelfRef, Super, Class { name: S, external: S }, Object(P) }
pub enum MethodName<P, S> { New, Named(S), Dynamic(P) }

/// A class definition, in `Services.class` of the program lowered from its source; every name is
/// that program's symbol. `parent` is the external name of the class it inherits.
pub struct Class {
    pub external: SymId, pub parent: SymId,
    pub factory: Option<ClassPart>, pub object: Option<ClassPart>, pub methods: Vec<Method>,
}
/// FACTORY or OBJECT data, lowered as a program: its `Storage` image is the data as VALUE clauses
/// leave it, which each object's or the factory's storage starts from, and `records` the offset of
/// each 01 or 77 record in it (exec/src/oo.rs:222-226 (f2)).
pub struct ClassPart { pub data: Program, pub records: Vec<u32> }
/// `params` and `returns` are Java signatures. The method's LINKAGE records from `own_records` on
/// are its part's records, bound at each invocation to the receiving object's or the factory's data.
pub struct Method {
    pub name: SymId, pub factory: bool, pub params: Vec<SymId>, pub returns: Option<SymId>,
    pub own_records: u16, pub code: Program,
}
```

- **A class definition** lowers to the program its shell compiles to, with `Services.class` set.
  Its FACTORY and OBJECT data and its methods are compiled as `load_class` compiles them the first
  time a run reaches the class (`class_code`, exec/src/oo.rs:356-491 (f2)), and each is lowered as a
  program of its own; a method's instance data are LINKAGE records after its own, as the walker
  binds them (machine/oo.rs:515-521 (f2)). Lowering compiles them with the compiler flags that give
  the class's options (flags only set trunc-check, sort-keys, FASTSRT ADV printing, warnings and
  debug, after the cards), and refuses a method or part whose options differ from the class's. A
  method or part never holds a class: the decoder refuses one before reading it, which keeps a
  damaged module from nesting programs without end.
- **INVOKE** evaluates as `invoke` does (machine/oo.rs:377-437 (f2)): the method name, the
  receiver, then each argument, then the method is looked up by name, factory or instance, the
  arguments' Java types and the RETURNING item's (both fixed at lowering by `operand_type` and
  `item_type`, which Check has already required). SELF and SUPER outside a method abend IRONWORK;
  Check refuses them. `NEW` sent to anything but a class abends IRONWORK. `rt::oo::invoke` runs
  the plan over an `OoHost`: the run unit, the activation's method, names and Java signatures read
  when the walker read them, an argument's bytes, a part's VALUE clauses, and running a method's
  code. A class is found and compiled by `Loader::class`; `rt::oo::ClassCode<H>` holds it.
- **Arguments** pass as `argument` makes them (machine/oo.rs:309-332 (f2)): a data item as its
  bytes, a one-character reference modification of Java type `C` as its first UTF-16 unit, LENGTH
  OF and an integer literal as a binary fullword, ZERO as four zero bytes, another figurative
  constant as its EBCDIC byte (not the collating sequence's), an alphanumeric or national literal
  as its bytes. An argument of a reference type passes the object its four bytes identify.
- **The op returns** `Arm(1)` when no method matches and ON EXCEPTION is written, else `Arm(0)`, or
  `Next` when neither phrase is written; without ON EXCEPTION the walker abends U4038. RETURNING is
  located after the method returns and receives its value by MOVE rules chosen from the value's
  kind: a new object reference for NEW or a returned object, the RETURNING item's value otherwise.
- **Object references** are `Kind::ObjectReference` places, moved as addresses (machine.rs:1743
  (int)) and compared by `Compare::References` (§6).
- **Dynamic:** class loading, an object's class, method lookup along the inheritance chain
  (machine/oo.rs:259-282 (int)), part and method storage, local and global references, the depth
  check before a COBOL method runs, and RETURN-CODE kept across it.

### 9.9 Intrinsic functions

```rust
/// Each argument evaluated as a comparison evaluates it; then `arity`'s abend, if set, when the
/// values the arguments give number outside `func.arity()`; then the function, which reads
/// `integer` again: CHAR, INTEGER-OF-DATE, DATE-OF-INTEGER and RANDOM their first argument,
/// NATIONAL-OF its second; last, on an alphanumeric result, `refmod`. HEX-OF, BIT-OF and
/// BYTE-LENGTH read a `Load` argument's bytes as stored, and any other argument's value as DISPLAY
/// would hold it; WHEN-COMPILED reads `ProgramOptions.when_compiled`.
pub struct FunctionPlan {
    pub func: Func, pub args: Vec<Argument>, pub integer: Option<IntExpr>,
    pub side: Option<TrimSide>, pub refmod: Option<RefMod>, pub arity: Option<AbendId>, pub at: DebugId,
}

/// `All`: a table written with ALL subscripts, expanded when the function runs. `element` is the
/// table as written, each ALL subscript 1; each `(position, count)` an ALL subscript's position
/// among its subscripts and its occurrences.
pub enum Argument { Value(Comparand), All { element: PlaceId, all: Vec<(u32, Count)> } }

/// One row per function: variant, tag, name and argument counts. Adding a function is adding a row.
functions! {
    Char = 0, "CHAR", 1..=1;  Ord = 1, "ORD", 1..=1;  NationalOf = 2, "NATIONAL-OF", 1..=2;  …
    Min = 16, "MIN", 1..=usize::MAX;  …  Random = 20, "RANDOM", 0..=1;
    Acos = 21, "ACOS", 1..=1;  …  YearToYyyy = 72, "YEAR-TO-YYYY", 1..=2;
    WhenCompiled = 73, "WHEN-COMPILED", 0..=0;
    Ulength = 74, "ULENGTH", 1..=1;  …  ContentOf = 81, "CONTENT-OF", 1..=1;
}
```

Tags 0 to 20 are the walker's first twenty-one functions (`rt::intrinsic::function::evaluate`),
21 to 72 the alphabetical first part of `rt::intrinsic::FUNCTIONS`, 73 WHEN-COMPILED, and 74 to 81
the eight that follow in it: ULENGTH, UPOS, USUBSTR, USUPPLEMENTARY, UVALID, UWIDTH,
COMBINED-DATETIME and CONTENT-OF. Every function the walker runs has a row.

- **The walker's order** (machine.rs `function`): every argument by `expr_value`, which is
  `Comparand` (an operand read as its kind; an expression with its float test, dmax pass and mode);
  then the argument count; then the function. CHAR, NATIONAL-OF's CCSID, INTEGER-OF-DATE,
  DATE-OF-INTEGER and RANDOM's seed evaluate their argument a second time with `integer`, which is
  observable (a subscript's locate, FUNCTION RANDOM advancing), so the plan keeps it. Reference
  modification of the result is evaluated last, with `integer`, and checked against the result
  whatever SSRANGE says, so `refmod.check` is false.
- **The argument count** is checked on the values the arguments give, after ALL subscripts expand
  them. `arity` holds the walker's message: "FUNCTION X takes 1..=1 arguments" for the first
  twenty-one but MIN and MAX, "FUNCTION X takes 1..=1 arguments, not 2" for the rest, "FUNCTION X needs arguments"
  for the functions of any number of arguments (MIN, MAX, ORD-MIN, ORD-MAX, RANGE, SUM, MEAN,
  MEDIAN, MIDRANGE, VARIANCE, STANDARD-DEVIATION), and PRESENT-VALUE's "needs a rate and at least
  one amount". `arity` is None where no count the arguments can give is wrong. Where an OCCURS
  DEPENDING ON count leaves the number to run time, the plan carries the abend and the executor
  tests it; a message that names the count (a function of fixed arguments past the first
  twenty-one) is refused.
- **HEX-OF, BIT-OF and BYTE-LENGTH** (`storage_function`) count their arguments as written before
  evaluating any, "FUNCTION X takes one argument", and read an item's storage rather than its value,
  so invalid data is shown rather than ending the run. A wrong count lowers with no arguments and
  `arity` set. ALL subscripts are not expanded for them: the walker reads the table as written,
  each ALL subscript 1.
- **ALL subscripts** (`function_arguments`, `all_elements`): the walker reads every ALL dimension's
  occurrence count first, left to right, an OCCURS DEPENDING ON dimension as its object's current
  value (checked under SSRANGE), then each element, the rightmost ALL varying fastest, evaluating
  the other subscripts as written for each. A table whose ALL dimensions have no OCCURS DEPENDING
  ON and at most 256 elements lowers as one `Argument::Value` per element, its ALL subscripts
  constants. Any other is `Argument::All`, with `Count::Fixed` or `Count::Odo` per ALL subscript,
  which the executor expands in the walker's order.
- **A name Check admits but `Func` lacks** is refused, "a FUNCTION the LIR does not name". None is
  left.
- **The result's category** decides the MOVE and comparison plans around the operand, as
  `Machine::function`'s value reads:

  | Result | Functions |
  |---|---|
  | The argument's own value | CONTENT-OF, a number of unknown scale for an item with PICTURE scaling positions |
  | Alphanumeric or national as the first argument is | USUBSTR |
  | Alphanumeric bytes | CHAR, TRIM, UPPER-CASE, LOWER-CASE, REVERSE, CURRENT-DATE, WHEN-COMPILED, HEX-OF, BIT-OF, HEX-TO-CHAR, BIT-TO-CHAR, DISPLAY-OF, UUID4 |
  | National | NATIONAL-OF; FORMATTED-CURRENT-DATE, FORMATTED-DATE, FORMATTED-TIME and FORMATTED-DATETIME of a national format, alphanumeric otherwise |
  | Floating point, long under ARITH(COMPAT) and extended under ARITH(EXTEND) | NUMVAL, NUMVAL-C, COMBINED-DATETIME (long rounded, then lengthened), RANDOM, ACOS, ANNUITY, ASIN, ATAN, COS, E, EXP, EXP10, LOG, LOG10, MEAN, MEDIAN, MIDRANGE, NUMVAL-F, PI, PRESENT-VALUE, SECONDS-FROM-FORMATTED-TIME, SECONDS-PAST-MIDNIGHT, SIN, SQRT, STANDARD-DEVIATION, TAN, VARIANCE; ABS, REM, MIN, MAX and SUM of a floating-point argument; RANGE when neither its greatest nor its least argument is fixed-point |
  | An integer of 1 digit | SIGN, TEST-DATE-YYYYMMDD, TEST-DAY-YYYYDDD |
  | 3 digits | ORD |
  | 4 digits | YEAR-TO-YYYY |
  | 7 digits | INTEGER-OF-DATE, DAY-OF-INTEGER, INTEGER-OF-DAY, DAY-TO-YYYYDDD, INTEGER-OF-FORMATTED-DATE |
  | 8 digits | DATE-OF-INTEGER, DATE-TO-YYYYMMDD |
  | 9 digits | LENGTH, BYTE-LENGTH, ORD-MIN, ORD-MAX, TEST-NUMVAL, TEST-NUMVAL-C, TEST-NUMVAL-F, TEST-FORMATTED-DATETIME, ULENGTH, UPOS, USUPPLEMENTARY, UVALID, UWIDTH |
  | 30 digits, 31 under ARITH(EXTEND) | FACTORIAL; INTEGER and INTEGER-PART of a floating-point argument |
  | 31 digits, the arguments' most decimal places | MOD, REM, INTEGER, INTEGER-PART, ABS |
  | `Fixed::add`'s places, from a one-digit zero through each argument | SUM of fixed-point arguments |
  | `Fixed::sub`'s places for the greatest less the least | RANGE of fixed-point arguments |
  | The winning argument's own value | MIN, MAX |

  Where the decimal places or digits depend on which argument wins
  (MIN, MAX and RANGE of arguments of different sizes), on an argument that is an expression or an
  item with PICTURE scaling positions, or on how many elements an OCCURS DEPENDING ON table gives
  (SUM), the result is a number of unknown scale, and moving or comparing it as alphanumeric is
  refused, since the walker decides that by the value. MIN or MAX of arguments of different
  categories, RANGE of fixed-point arguments with floating-point or ZERO ones, and an OCCURS
  DEPENDING ON table among arguments of another category, whose count would decide the result's
  category, are refused.
- **The float test** (`uses_float`, `is_floating_point`) counts the functions of
  `rt::intrinsic::FLOATING_POINT` as floating-point without locating anything, and those of
  `rt::intrinsic::MIXED` (ABS, MAX, MIN, RANGE, REM, SUM) as floating-point when an argument is,
  testing their arguments as written in order and stopping at the first floating-point one, as it
  stops at the first floating-point operand elsewhere. The `prepass` and `probe` lists keep those
  locates.

### 9.10 DECLARATIVES

Declaratives lower as the paragraphs before `procedure_start`, and each procedure that can run is a
range its trigger runs as a procedure (§9.6): in its own dispatch loop, under a Procedure frame that
arms its last paragraph's end as a PERFORM does (§8.4), since the walker runs it by
`run_paragraphs`. What happens when control leaves it is the trigger's.

```rust
/// In `Services`: the open modes' EXCEPTION/ERROR procedures, INPUT, OUTPUT, I-O and EXTEND in
/// that order, as `mode_index` orders them; DEBUG-ITEM's offset and length in the slab.
pub struct Declaratives { pub modes: [Option<RangeId>; 4], pub debug_item: Option<(u32, u32)> }
// In `FileDesc`: `error: Option<RangeId>`, the file's own procedure.
```

**USE AFTER EXCEPTION/ERROR** (`io_failure` and `run_error_declarative`, machine/file_io.rs:34-48,
machine/declaratives.rs:109-121 (7af)). Each procedure is a `UseProcedure` range of its section.
The file op sets FILE STATUS; a failing status that no phrase written takes runs the file's own
procedure, else the one for the mode the file is open in or being opened in, through
`Procedures::run` with arrival USE PROCEDURE, after checking and raising the depth at the
statement (§8.7). `Completed` goes back into the statement, which carries on as the walker's does
(`conclude` returns, OPEN stops, WRITE skips the page update). `Left(step)` abandons the statement
and the op returns `step`: a GO TO out, STOP RUN or GOBACK, a return to an active PERFORM, or a
resume after one control left, which the VM then carries out as it would the statement's own
(§8.4). With neither procedure, the file's error path is as before: `IO-xx` when it has no FILE
STATUS. Which procedure applies is known only at run time, from the file's open mode, so lowering
gives the op both and refuses no file statement.

**USE FOR DEBUGGING** under the DEBUG runtime option, when `Table.triggers` is not empty (`exec`,
`debug_before`, `debug_alter` and `run_debugging`, machine.rs:299-309 and
machine/declaratives.rs:132-186 (7af)). Each debugging section that serves a paragraph is a
`Debugging` range, and `Declaratives.debug_item` is set. Without the option the sections are only
paragraphs, and nothing below is lowered.

- **Registers.** The VM keeps three per activation: the line register, which DEBUG-LINE shows; the
  arrival register, which DEBUG-CONTENTS shows, set by how control reaches a paragraph's entry
  (START PROGRAM at the start of the run, PERFORM LOOP by `PerformEnter`, FALL THROUGH by a
  paragraph end, blank after a transfer, USE PROCEDURE or the SORT procedure's name by
  `Procedures::run`); and a flag that a debugging section is running.
- **`DebugLine(line)`** sets the line register. Lowering puts one before each statement that has a
  position, including an ENTRY and a GO TO with no target, as `exec` does; before each
  `PerformEnter`, since every out-of-line iteration sets it; and at the entry of each section
  header, after any `Debug`.
- **`Debug { range, name, next }`** begins the entry block of a paragraph a section serves, after
  `EnterSegment` and before `AlteredGoTo`. A CALL through ENTRY and a resume enter past it. Unless
  a section is running, it checks and raises the depth at the paragraph, fills DEBUG-ITEM with
  spaces and then DEBUG-LINE (six digits: the paragraph's own line when the arrival is START
  PROGRAM, else the line register), DEBUG-NAME `name` and DEBUG-CONTENTS from the arrival register,
  each encoded in the program's code page and cut to its field (offsets 0, 7 and 56; lengths 6, 30
  and 30), saves the line register, sets the flag, runs `range` through `Procedures::run`, then
  clears the flag, restores the line register and lowers the depth. `Completed` goes to `next`.
  `Left(step)` leaves the top frame with `step`, as `run_region` breaks with it: `Return` to that
  frame completes it; otherwise the frame is left and `step` carries on from the frame below, and
  when the top frame is Main the run ends, with `End`'s ending or else END OF PROGRAM (`run_from`,
  machine/perform.rs:37-48 (7af)).
- **`DebugAlter { range, name, contents }`** follows an ALTER's `Alter` ops, one for each pair
  whose altered paragraph a section serves, in order, except for an ALTER in the declaratives under
  ALL PROCEDURES (`Table.declarative_alters`). It runs as `Debug` does, with DEBUG-LINE the
  ALTER's line and DEBUG-CONTENTS `contents`, the TO PROCEED TO name as written, and returns
  `Next`, or the `Left` step, which ends the ALTER as the statement's own transfer.
- **No elision into a served paragraph.** A GO TO to it stays `GoTo` and a paragraph end before it
  stays `ParagraphEnd`, so the arrival register is set on every way in.

### 9.11 Editing parameters

SPECIAL-NAMES DECIMAL-POINT IS COMMA and CURRENCY SIGN IS literal [WITH PICTURE SYMBOL literal]
reach run time through `ProgramFacts` (rt/src/store.rs), which the walker answers from the program's
`Environment` and `Layout`. The LIR carries each answer, so an executor answers the same:

| Walker reads | Where it is used | In the LIR |
|---|---|---|
| `ProgramFacts::decimal_point` (`Machine::decimal_point`) | A store into a numeric-edited item, by MOVE, arithmetic, INITIALIZE, SET, ACCEPT, POINTER or COUNT IN (rt/src/store.rs `store_fixed_checked`); NUMVAL, NUMVAL-C, NUMVAL-F, TEST-NUMVAL, TEST-NUMVAL-C and TEST-NUMVAL-F (rt/src/intrinsic/function.rs) | `ProgramOptions.decimal_point_comma` |
| `ProgramFacts::edit(edit)`'s currency, from `Layout.currencies` | The same numeric-edited stores (`edit::numeric`), and a de-editing MOVE from a numeric-edited sender (`edit::de_edit`, `NumericFrom::DeEdit`) | `Edit.currency` of `Program.edits[edit]` |
| `Machine::default_currency`: the only CURRENCY SIGN clause's value, else $ (C102) | NUMVAL-C and TEST-NUMVAL-C without argument-2 | `ProgramOptions.numval_currency` |
| `display::number`: a numeric literal's point as the program's | DISPLAY of a numeric literal | `DisplayItem::Text`, whose text lowering writes with the comma |

A numeric literal written with a decimal comma reaches the parser with a period (the lexer reads
the program's point), so `Const::Number` needs nothing more; the PICTURE symbols and sizes are
decided when the layout is built. A hexadecimal CURRENCY SIGN literal is refused by the parser.

### 9.12 STRING, UNSTRING, INSPECT and SEARCH

`rt::text` runs STRING, UNSTRING and INSPECT for both executors, over the walker's references in
the interpreter and the LIR's ids in the VM. The walker leaves each receiver's store to its `Loc`'s
kind; the plans fix it:

- **POINTER and COUNT IN** are `(PlaceId, StorePlan)`: `set_integer`'s store, `Refused` with
  "a numeric value stored into a non-numeric item" for a receiver that is not numeric.
- **TALLYING counters** (INSPECT's and UNSTRING's TALLYING IN) are `(PlaceId, StepPlan)` with dmax
  0. A counter that does not read as a fixed-point number (alphanumeric, edited, national,
  floating-point or pointer) has a `Refused` store with the walker's message, "a TALLYING counter
  must be numeric" or "TALLYING IN needs a numeric item", which an executor raises once it has
  located the counter, where the walker reads it first; reading such an item cannot abend.
- **UNSTRING's receivers** take the field as an alphanumeric sender with no storage (`MovePlan` of
  `Value::Bytes`); DELIMITER IN takes the delimiter that way (`found`) or SPACE (`none`).
- **Literals** are `Chars::Literal`, `natural_bytes` of the value `literal_value` gives. INSPECT's
  REPLACING BY a figurative constant is `Replacement::Fill` of its character. INSPECT of a national
  item or of a function's value keeps its literals, a figurative BY value among them, as
  `Chars::Value` of their constants, and its CONVERTING operands as `ConvertTable::Operands`:
  `rt::text` reads them as national characters where what is inspected is national, taking the
  character's two bytes from the item's `Loc` kind or the value's type (assumptions C191, C230).
- **INSPECT of a function result** (TALLYING only, assumption C190): `InspectPlan`'s target is
  `Inspected::Value`, and the executor runs it through `rt::text::tally`, which evaluates the value
  once before the phrases' operands. Its plan carries no REPLACING or CONVERTING phrases, since the
  walker ignores them there.
- **STRING, UNSTRING and SEARCH ALL** always return `Arm`, so a `Select` of two arms follows each,
  to the next statement where no phrase is written.

SEARCH evaluates the table's count (`Count`, as `occurrences`) before it reads the index, as the
walker does, and once, before its loop. A serial SEARCH of an OCCURS DEPENDING ON table starts with
`SetCount`, which evaluates the count there, abending as `occurrences` abends under SSRANGE, and
holds it in a counter of its own; its `InTable` reads `Count::Temp` of that counter, so a VARYING
item that is the DEPENDING ON object, or shares its storage, steps without changing the count, as
in the walker. SEARCH ALL evaluates its plan's `Count` once inside `SearchAll`. A table with neither
INDEXED BY nor VARYING, serial or ALL, ends the block in the walker's `Abend`, after `SetCount` when
the table has a DEPENDING ON object, whose evaluation comes first. `SetInt` steps the index and a
VARYING item that is not the index by one; lowering refuses one that is not an index or an integer
item, where `integer` + 1 and the item + 1 truncate differently.

### 9.13 JSON and XML

Each statement is one `Op::Markup` naming its payload in `Services.markup`. The payload holds the
statement's places, operands and phrases, and the tree of items the walker's walk reaches, with
every choice it makes by name or kind made at lowering: which items it ignores (FILLER, REDEFINES,
RENAMES, the null indicators), which SUPPRESS leaves out, how an unnamed group's members join their
parent, each name as the document writes or matches it, each table's count (`Count::Fixed`, or the
OCCURS DEPENDING ON object), and how each value converts. Values, the document and its encoding stay
run-time data, read where the walker reads them.

```rust
pub enum Markup { JsonGenerate(JsonGenerate), XmlGenerate(XmlGenerate), XmlParse(XmlParse), JsonParse(JsonParse) }
/// None written, the program's CODEPAGE (ENCODING FROM CODEPAGE), or an operand.
pub enum Ccsid { Unnamed, CodePage, Operand(Operand) }
/// How GENERATE writes an elementary value (`converted`, machine/json.rs).
pub enum Convert { Chars { justified: bool }, National, Float(Precision), Fixed { integers: u32 }, Refused(AbendId) }
/// A USING value of JSON GENERATE: a literal's first byte, a condition-name, or the walker's abend.
pub enum Marker { Byte(Option<u8>), Condition(CondId), Refused(AbendId) }

pub struct JsonGenerate {
    pub from: PlaceId, pub subscripts: Vec<IntExpr>, pub nodes: Vec<JsonNode>, pub name: Option<SymId>,
    pub receiver: PlaceId, pub encoding: Ccsid, pub count: Option<(PlaceId, StorePlan)>,
    pub code: (PlaceId, StorePlan), pub on_exception: bool, pub not_on_exception: bool,
}
pub struct JsonNode {
    pub offset: u32, pub len: u32, pub kind: Kind, pub name: SymId, pub occurs: Option<Count>,
    pub indicator: Option<(Result<PlaceId, AbendId>, Marker)>, pub null: Option<Figurative>, pub value: JsonValue,
}
pub enum JsonValue { Object { members: Vec<u32>, eligible: bool }, Leaf(JsonLeaf) }
pub struct JsonLeaf { pub suppress: Vec<Figurative>, pub boolean: Option<Marker>, pub convert: Convert }

pub struct XmlGenerate {
    pub receiver: PlaceId, pub encoding: Ccsid, pub namespace: Option<Operand>, pub prefix: Option<Operand>,
    pub declaration: bool, pub from: PlaceId, pub subscripts: Vec<IntExpr>, pub nodes: Vec<XmlNode>,
    pub suppressing: bool, pub count: Option<(PlaceId, StorePlan)>, pub code: (PlaceId, StorePlan),
    pub on_exception: bool, pub not_on_exception: bool,
}
pub struct XmlNode { pub offset: u32, pub len: u32, pub kind: Kind, pub name: SymId, pub occurs: Option<Count>, pub value: XmlValue }
pub enum XmlValue { Element { members: Vec<u32> }, Members { members: Vec<u32> }, Leaf { form: XmlForm, suppress: Vec<Figurative>, convert: Convert } }

pub struct XmlParse {
    pub document: PlaceId, pub encoding: Option<Operand>, pub national: bool, pub procedure: RangeId,
    pub event: PlaceId, pub code: (PlaceId, StorePlan), pub information: (PlaceId, StorePlan),
    pub code_value: IntExpr, pub on_exception: bool, pub not_on_exception: bool,
}
pub enum XmlRegister { Text, NText, Namespace, NNamespace, Prefix, NPrefix }

pub struct JsonParse {
    pub source: PlaceId, pub encoding: Ccsid, pub into: PlaceId, pub subscripts: Vec<IntExpr>,
    pub nodes: Vec<ParseNode>, pub ignore_all: bool, pub code: (PlaceId, StorePlan), pub status: (PlaceId, StorePlan),
    pub on_exception: bool, pub not_on_exception: bool,
}
pub struct ParseNode {
    pub offset: u32, pub len: u32, pub kind: Kind, pub name: Named, pub occurs: Option<Count>, pub ignored: bool,
    pub indicator: Option<Indicator>, pub null: Option<(Figurative, MovePlan)>, pub value: ParseValue,
}
pub enum Named { Exactly(SymId), Folded(SymId), Omitted }
pub enum ParseValue { Object { members: Vec<u32> }, Leaf(ParseLeaf), Suppressed }
pub struct ParseLeaf { pub boolean: Option<Flag>, pub text: Option<MovePlan>, pub number: NumberInto }
pub enum NumberInto { Float(MovePlan), Store(StorePlan), Edited(MovePlan), Digits, Incompatible }
pub struct Indicator { pub place: Option<Result<PlaceId, AbendId>>, pub flag: Flag }
pub enum Flag { Set { on: SetTo, off: SetTo }, Literals { on: (ConstId, MovePlan), off: (ConstId, MovePlan) } }
pub enum SetTo { Nothing, Move { place: PlaceId, value: ConstId, plan: MovePlan }, Refused(AbendId) }
```

- **The op and its phrases.** With ON EXCEPTION or NOT ON EXCEPTION written the op returns `Arm(1)`
  when the code it stores is not 0 and `Arm(0)` when it is, for a `Select` of two blocks, as CALL's;
  with neither, `Next`.
- **Trees nest by index.** `nodes[0]` is the statement's own item; every member comes after the node
  that holds it, which the decoder and the verifier check, so no walk loops. A node's `offset` counts
  from the start of the occurrence of the node that holds it (`nodes[0]` from its place's `Loc`), and
  a table's elements are `len` apart, `occurs` of them, counted when the walk reaches the table.
- **The walk's subscripts.** FROM's or INTO's subscripts are evaluated once, after its locate, and
  each table entered adds its occurrence number. A JSON phrase's indicator or condition-name is
  located with the first of them (`IntExpr::Walk(k)` in its place's subscripts), as `subscripted`
  and `locate_item` do; too few is the walker's IRONWORK abend, kept with the phrase's position.
- **Phrases that name no data item.** A JSON phrase naming a condition-name, a NAME literal that is
  not alphanumeric or national, and an INDICATING literal without IN abend at the statement before
  anything is read, so the statement lowers to an `Abend` terminator. XML GENERATE resolves its
  phrases only after the encoding and namespace, and such a phrase is refused by name.
- **JSON GENERATE** (`json_generate`, machine/json.rs): FROM located (its first element when it
  names a whole table, which makes `nodes[0]` a table), its subscripts, the tree, then the receiver
  located (as a receiving item), the CCSID read, the document written, COUNT IN and JSON-CODE stored.
  Each occurrence of a node, a group or a leaf alike, is null when its indicator's marker holds or
  it equals `null`, tested in that order as `json_null` tests them, before its members or value.
  An `Object` with no member left is left out when `eligible`, else `{}`; a table whose elements are
  all left out is left out; the root left out is `{}`, or `[]` for a whole table. A leaf then tests
  `suppress` (left out) and `boolean`, then converts. A group whose members the walk all ignores is
  ignored with them and has no node.
- **XML GENERATE** (`xml_generated`, machine/xml/generate.rs): the receiver, the CCSID (`Unnamed` is
  UTF-16 for a national receiver, else CODEPAGE), XML-CODE 415, 411 or 414 ending it there; the
  namespace (416), the prefix read only for a namespace that is not empty (419); FROM, its
  subscripts and the tree; 420 for a national value in a single-byte document; then the document,
  COUNT IN, and XML-CODE 400, 417, 418 or 0. A `Members` node is an unnamed group whose members join,
  occurrence by occurrence, the element that holds it; an `Element` left with nothing is left out
  when `suppressing`, but never `nodes[0]`, which takes the namespace declaration. An elementary
  FROM is converted as its place locates it, whatever its form.
- **XML PARSE** (`xml_parse`, machine/xml.rs): the document located, the encoding read (a CCSID
  ironwork has no page for abends IRONWORK), the document located again and decoded. The op then
  runs the event loop: `rt::xml::Scanner` reports each event, which sets XML-EVENT (30 characters),
  XML-CODE (0, or the exception's code), XML-INFORMATION and the `Base::Xml` registers, and the op
  runs `procedure` through `Procedures::run` (§9.6) under a Procedure frame with the arrival PERFORM
  LOOP. `Left(step)` ends the statement with `step`, its phrases not run. After `Completed` the op
  reads `code_value`: a warning `Scanner::advance` reports as an EXCEPTION event (an undeclared
  prefix) goes on when it is 0, and that or any other EXCEPTION otherwise ends the parse with the
  scanner's code, whatever the procedure set; END-OF-DOCUMENT ends it with 0; at END-OF-INPUT, 1
  locates and decodes the document again as the next segment (`Scanner::feed`) and any other value
  ends the input (`Scanner::finish`); after any other event, -1 ends it with -1. XML-CODE takes the
  result. The procedure is a `Processing` range, so a GO TO out of it stays a `GoTo` and its last
  paragraph keeps `abandoned` (§8.4).
- **JSON PARSE** (`json_parse`, machine/json/parse.rs): the source located, the CCSID read
  (FROM CODEPAGE of a national source is 109 before it is), the text decoded and parsed; for a
  document that parses, INTO located with its subscripts, and each value moved into the node its
  name reaches (`Named`), the first that matches. `nodes[0]` named `Omitted` takes the document
  itself; otherwise the document is an object whose members INTO's name matches. A node takes a
  value as `parse_value` does: a duplicate at the same node and offset, the indicator (located,
  then its flag set on for null), a null by `null`'s MOVE or as ignored or a status, an object into
  a group's members, any other value into a leaf: a boolean by its flag, a string by `text` or as the
  number it spells with the program's decimal point (`ProgramOptions.decimal_point_comma`), a number
  by `number`. JSON-CODE and then JSON-STATUS are stored.
- **The special registers are places.** JSON-CODE, JSON-STATUS, XML-CODE, XML-EVENT and
  XML-INFORMATION are WORKING-STORAGE items Check declares for a program that has the statement
  (compile/src/markup.rs), stored with `set_integer`'s plans. XML-TEXT, XML-NTEXT, XML-NAMESPACE,
  XML-NNAMESPACE, XML-NAMESPACE-PREFIX and XML-NNAMESPACE-PREFIX, where the program declares none,
  are `Base::Xml` places, alphanumeric or national, whose reference modification abends IRONWORK
  "reference modification (s:l) of NAME is outside its N bytes".
- **Refused:** an item with PICTURE scaling positions in a GENERATE tree or JSON PARSE leaf (an
  executor reads values by `Kind`, which does not carry them), a JSON phrase naming a
  reference-modified item, an XML GENERATE phrase or JSON PARSE INTO that names no data item, and a
  PROCESSING PROCEDURE the walker cannot find.

### 9.14 NUMCHECK and PARMCHECK

The walker runs NUMCHECK's test (`rt::store::numcheck`; Programming Guide SC27-8714-03,
pp. 388-392) inside the reads it makes, between an item's locate and its read, and PARMCHECK's set
and test inside a CALL, between its arguments and its callee (p. 397). An op of their own would
move them past a locate, a read or a store of the same statement, so the LIR carries NUMCHECK in
its reads and in `Move.check` (§9.2), and PARMCHECK in `Storage.parmcheck` and the CALL op. Both
executors call the same `rt` functions: `store::numcheck`, `store::move_check`,
`store::numcheck_sender`, `store::noalphnum` and `store::nonnumeric`, and `parmcheck::set` and
`parmcheck::test`.

**NUMCHECK.** Under `ProgramOptions.options.numcheck` an executor runs `store::numcheck`, with
`as_integer` false and the place's `at` as its position, after the place is evaluated to a `Loc`
and before the item is read, at each read of this table and at no other. Under MSG it writes its
warning and the read goes on; under ABD it ends the run with U4038 before the read.
`store::numcheck` reads two facts of the `Loc` through `ProgramFacts`, which an executor gives from
the place's `numcheck`: `lax_redefinition`, the tolerance ZON(LAX) gives an item redefining a
signed or numeric-edited one, and `numcheck_removed`, set where the compiler found the test always
fails, reported it and removed it. The walker keys the removal by the reference's position and its
item, as a place is keyed. The warning names the `Loc`'s item through `item_name`: for a
condition-name's subject that is its conditional variable, not the condition-name its place bears,
so the VM finds it among `Program.items` of the place's storage and shape, and stops as
`Halt::Unimplemented` where items of other names share them and the test fails; an XML register's
`Loc` has no item in the walker, which names it RETURN-CODE.

| The walker's read | In the LIR | Tested |
|---|---|---|
| `operand` and `operand_with_loc` of a data item | `Operand::Load` read for its value: in an `Expr`; as a `Comparand` of `Cond::Sign`, of a FUNCTION's `Argument::Value` other than HEX-OF's, BIT-OF's and BYTE-LENGTH's, or of a report's SOURCE, SUM or CONTROL; `Op::Set`'s `from`; `SetAddress`, `Cancel`, `CallTarget::Dynamic`, `CallArg::Value`, `UserArgument::Value`; `Chars::Value`, `Inspected::Value`; a markup statement's ENCODING, NAMESPACE and NAMESPACE-PREFIX; what the library reads through `Values::value` | At each read |
| `operand` of each element of a table written with ALL subscripts | `Argument::All`'s elements | At each element |
| `integer`, through `eval_fixed` or `eval_float` | `IntExpr::Item` and the `Load`s of an `IntExpr::Fixed`, wherever they stand: a place's subscripts, OCCURS DEPENDING ON object and reference modification, so at each evaluation of the place, as each locate of the walker's tests them; a `Pow` exponent, `SetTemp`, `Switch`, `SetInt`, `SetUpDown`'s `by`, `Cond::InTable`'s index, `Count::Odo`, a FUNCTION's `integer` and `refmod`, LINAGE, RELATIVE KEY, START's KEY, ADVANCING, XML-CODE, a markup walk's subscripts; what the library reads through `Host::integer` | At each read |
| `eval_fixed` of an ADD or SUBTRACT receiver's own value, and of PERFORM VARYING's variable | The receiver's read under `ArithPlan.per_receiver`; `Op::Step`'s `var` | At each read |
| `condition` of a condition-name | `Cond::Name`'s `subject` | Once, before its one read |
| `content_argument` of a data item | `CallArg::Content(Chars::Place)` of a CALL of a program or an LE service; an EXEC CICS option's content (`CicsHost::content`) | Before its bytes are copied |
| `move_source` | `Op::Move`'s and `FromMove`'s `from` | As `check` names, with `store::numcheck_sender` |
| `compare` | `Cond::Rel` and `SearchKey` | As below |

The walker reads these without the test, and so does an executor: DISPLAY's items; a class test's
item; the argument of HEX-OF, BIT-OF and BYTE-LENGTH; INVOKE's arguments, which it takes as bytes;
BY REFERENCE arguments and RETURNING; the arguments `arguments_text` shows an observer of a CALL of
an operating-system routine; what the library reads after `Host::locate`, such as the
items and counters of STRING, UNSTRING and INSPECT and the receivers of SET UP and DOWN BY; SQL host
variables; file and sort keys; and what JSON and XML GENERATE write out and JSON and XML PARSE read.

**Comparisons.** `Cond::Rel` and `SearchKey` carry nothing more. The walker decides at run time
whether it tests a side, from the options and the kinds of what it compares, and locates places on
the way (`Machine::compare`, `comparand_against`, `checks_against`); an executor does the same from
`ProgramOptions`, the places' kinds, the constants and the `Compare` lowering chose:

- **A side is tested against the other operand** (`checks_against`) always, but under
  ZON(NOALPHNUM) (`store::noalphnum`) not when the other operand is nonnumeric: an alphanumeric,
  hexadecimal or ALL literal, a figurative constant other than ZERO and NULL, or a data item of a
  kind `store::nonnumeric` names, which is located to find its kind. Without NOALPHNUM nothing is
  located for this.
- **Neither side compared by its bytes:** after the zoned-bytes test's locates (§6), `a` and then
  `b` are each read as an operand, but a data item side first asks `checks_against` of the other
  operand, and is read untested when it says no.
- **`ZonedBytes`:** once the zoned item's bytes are taken, it is located and tested when
  `checks_against` of the other operand says so. Then the other operand: an unsigned, unscaled zoned
  integer item is located twice and tested, and compared by its bytes; anything else is read as a
  side of the previous item is, the zoned item its other operand, which is never nonnumeric, so a
  data item is tested.

**Each locate counts.** A place's evaluation reads its subscripts, OCCURS DEPENDING ON object and
reference modification, and NUMCHECK tests each of those reads, so under MSG an executor evaluates
each place as often as the walker locates it, not only in its order (§7.4, §7.5). Beside the
passes the plans keep, an executor repeats these locates of the walker's: `compare`'s of the
zoned-bytes test and `checks_against`, above; and on a zero divisor `binary_division`'s, which
locate the division's item operands again, left to right until one is not an integer binary item,
before the S0C9 or size error.

**Refused.** Where the LIR reads an item more often than the walker, and NUMCHECK may test the item
or what locating it reads, lowering refuses the program: a condition-name whose values compare
differently (§6), whose `Or` of relations reads the subject once per value where the walker tests
and reads it once; and a serial SEARCH, whose loop reads the index twice at each step, where the
walker reads it once. The count `SetCount` holds is read once, as the walker reads it. Lowering
refuses too ALL ZERO or ALL NULL compared with a data item NUMCHECK may test or locate, which
`Machine::nonnumeric` takes for nonnumeric and the LIR keeps as ZERO or NULL.

**PARMCHECK.** `Storage.parmcheck` is the buffer after the WORKING-STORAGE the program declares
(assumption PARMCHECK_BUFFER). Inside the CALL op an executor runs `parmcheck::set` over the calling
program's slab, and later `parmcheck::test` with the op's position, `Program.id`,
`ProgramOptions.options.parmcheck`'s ABD, and as `arguments` the address and place name of each
argument that is a data item: `Reference`, `Content(Chars::Place)` and `Value(Load)`.

| CALL of | Set | Test, naming |
|---|---|---|
| A program | After the arguments, before the callee is entered | After it returns and its temporaries are released, unless it ended the run with STOP RUN; before RETURNING and the phrases. The program's name in the run unit |
| An LE service | After the arguments | After the service returns and its temporaries are released, before the phrases. The service's name |
| A service through a pointer | Before the arguments | After the service returns, with no arguments. The pointer as written |

A CALL that abends or finds no program is not tested, and INVOKE has no PARMCHECK.

### 9.15 User-defined functions

```rust
/// In `Services.user_functions`, which `Operand::UserFunction` names. `name` is the function-name
/// as written, which messages give; `external` finds the definition.
pub struct UserFunctionPlan {
    pub name: SymId, pub external: SymId, pub args: Vec<UserArgument>,
    pub refmod: Option<RefMod>, pub at: DebugId,
}
pub enum UserArgument { Reference(PlaceId), Value(Comparand) }
/// `Services.function` of a function definition (FUNCTION-ID without IS PROTOTYPE): a place for
/// each formal parameter's LINKAGE record, in order, and one for the RETURNING record.
pub struct FunctionDefinition { pub params: Vec<PlaceId>, pub returning: PlaceId }
```

`Machine::invoke_function` (exec/src/machine/function.rs) and `rt::vm`'s `user_function` run the
same sequence when the operand is evaluated (assumption C274):

1. **The definition** is loaded by `external` as a static CALL loads a program
   (`RunUnit::load_entry`). None has the name: S806 "FUNCTION F: its definition, X, is in neither
   the source nor the program libraries". The program found is not a definition, or is the run
   unit's first program: IRONWORK "FUNCTION F: X is a program, not a user-defined function". A
   definition that does not lower stops the VM.
2. **Each argument**, in order. A data item whose formal parameter is not BY VALUE is `Reference`:
   located, its address passed, a reference-modified item at its first byte (C272). Any other, a
   BY VALUE item, a literal, an expression, LENGTH OF, ADDRESS OF or a function, is `Value`: the
   `Comparand` `expr_value` evaluates, a nested intrinsic function's arguments in their own
   arithmetic whatever arithmetic holds the invocation.
3. **The depth** is raised (§8.7), then `rt::callee::run` with `By::Function` wraps the
   activation, taint's pending read taken before it and resumed after it
   (`RunUnit::resume_statement`), since the function's statements start inside the invoking one.
4. **The activation**: the function's storage as a CALL gives it; each formal parameter given its
   argument's address, or a temporary of zeros the size of its record (`rt::callee::Bound`,
   `bound_addresses`); the USING records bound; each `Value` moved into its parameter by MOVE
   rules, located through `FunctionDefinition.params`, with the function's facts and the
   invocation's position; the RETURNING record given a temporary; the procedure run; and the
   RETURNING item located through `FunctionDefinition.returning` and read, after STOP RUN too.
5. **After it** the program's `active` flag is what it was before, since functions are recursive;
   the temporaries pushed since the arguments are released; an abend is named by the function's own
   files; the depth is lowered.
6. **STOP RUN** in the function abends `Signal::StopRun` at the invocation, and the run ends at the
   statement holding it: the walker's `exec` takes the signal, and the VM's dispatch loop takes it
   from an op or a terminator's condition as `End(StopRun)`.
7. **The value** is reference-modified last, by `refmod`, checked whatever SSRANGE says.

- **The value reads as** the RETURNING item's description (`compile::function::Formal`) reads: a
  number with its decimal places and its digits, PICTURE scaling positions counted; a float;
  alphanumeric or national bytes; an address. The description gives the dmax pass and the float
  test the invocation's kind (`Machine::operand_kind`, `is_floating_point`) without running it.
- **The definition's places** are the records the PROCEDURE DIVISION header names, with no
  subscripts, as `Machine::parameter` and `returned` locate them by name. Lowering refuses a record
  holding an OCCURS DEPENDING ON table: locating it reads the count and may abend at the
  invocation's position, which the definition does not know.

### 9.16 EXTERNAL and GLOBAL

Layout places an EXTERNAL record, a record redefining one, the records of an EXTERNAL file and a
containing program's GLOBAL records as LINKAGE records after the program's own (`Layout.bindings`,
compile/src/scope.rs), so their places are `Base::Linkage(n)` and nothing else in the LIR changes.
The walker gives each its address when the program is activated, and each EXTERNAL or GLOBAL file
its connector (`bind_shared`, machine/scope.rs; assumptions C180 and C181). `Services.scope` holds
what that needs, found by name once at lowering:

```rust
pub struct Scope {
    /// The PROGRAM-IDs of the programs containing this one, innermost first.
    pub containers: Vec<SymId>,
    /// Each LINKAGE record whose storage the run unit or a containing program holds, in ordinal order.
    pub records: Vec<(u16, Binding)>,
    /// Each file whose connector is not its own; each file whose record area is a bound record's.
    pub files: Vec<SharedFile>, pub areas: Vec<(u16, u16)>,
    /// A program that contains others: its GLOBAL records and files, and its GLOBAL EXCEPTION/ERROR
    /// procedures for files and for the open modes.
    pub globals: Vec<Global>, pub global_files: Vec<(u16, RangeId)>, pub global_modes: [Option<RangeId>; 4],
}
pub enum Binding { External { name: SymId, size: u32 }, ExternalFile(u16), Global { program: SymId, section: Section, name: SymId } }
pub enum Section { WorkingStorage, LocalStorage, Linkage, File }
pub struct SharedFile { pub file: u16, pub external: bool, pub declared_in: Option<SymId> }
pub struct Global { pub section: Section, pub name: SymId, pub at: GlobalAt }
pub enum GlobalAt { Program(u32), Local(u32), Linkage(u16) }
```

- **Activation.** After the program is marked active and before LOCAL-STORAGE is pushed, as
  `activation_within` orders them, the executor binds each of `records` in order: `External` to
  `RunUnit::external` of the name and size, which allocates the record the first time a program
  describes it and refuses another size; `ExternalFile(k)` to the run unit's record area of file
  k's name and area size; `Global` to what the containing program of that PROGRAM-ID, as it is
  running, gives as its GLOBAL `name` of `section` (for `File`, the file's name). Then each of
  `files`: an EXTERNAL one is connected to `RunUnit::external_file` of its name; a GLOBAL one to
  the containing program's own file of its name, after its print-file carriage is found to be the
  same. Each refusal is the walker's IRONWORK abend, with no position: a containing program not
  running, a record or file it does not have, a size or carriage that differs.
- **The running containers.** A CALL gives the callee the running programs among its
  `containers`: the caller and the caller's own containers, matched by PROGRAM-ID, each as it was
  when control left it (its slab, its LOCAL-STORAGE and a copy of its LINKAGE addresses), as
  `containers_of` does. LINK, XCTL and the run's first program have none.
- **What a containing program gives.** Each GLOBAL record and file it declares is in `globals`
  where `global_address` finds it: the first root item of its name in WORKING-STORAGE or
  LOCAL-STORAGE (an offset), the first LINKAGE record of its name (the address the container's
  activation holds), the first file of its name (its record area in the slab, or the record bound
  to it). A record the walker would not find there has no entry, and binding it gives the walker's
  abend. Matching a binding's names to these entries is the one lookup by data name at run time,
  made once per activation, as CALL finds a program by name.
- **A file's record area** is the slab's, or for a file in `areas` the address of the record bound
  to it. A key of such a file is the span of its item within that record, which lowering takes
  from a place on any record bound to the file's area, since every one is bound to the same
  storage.
- **USE GLOBAL.** A failing status with no phrase to run and no procedure of the program's own
  runs the first GLOBAL procedure of its running containers, innermost out: for the file, matched
  by name and the PROGRAM-ID that declares it, then for the mode it is open or being opened in
  (`global_declarative`). It runs as a procedure of the containing program (§9.6, §9.10): an
  activation of that program over its storage as it was left, with nothing armed, nothing in
  progress, and the containers outside it; one PERFORM deeper than the statement. `Completed` goes
  back into the statement; STOP RUN leaves it as the program's own procedure's would; any other
  leaving abends IRONWORK "the GLOBAL EXCEPTION/ERROR procedure of P, run for Q, left by GO TO,
  GOBACK or EXIT PROGRAM, which is not supported yet" at the statement, as the walker does.
- **Refused at compile time, not by lowering:** what assumption C181 lists as not supported yet,
  and EXTERNAL in FACTORY or OBJECT WORKING-STORAGE.

## 10. The debug table

```rust
/// `sources` is the program's file table: the source, then each COPY member (ast.rs:21-22).
/// `positions` maps a DebugId to its position; `ops` holds, per block, one DebugId per op and
/// one for the terminator; `statements` holds, per block, each statement that starts there.
pub struct Debug {
    pub sources: Vec<SymId>,
    pub positions: Vec<Pos>,
    pub ops: Vec<Vec<DebugId>>,
    pub statements: Vec<Vec<(u32, DebugId)>>,
}
```

`Pos` is `rt::Pos`: the file index, line and column (today syntax/src/lib.rs:16-22). An abend's
position comes from where the walker takes it:

| Abend | The walker names | The VM takes |
|---|---|---|
| Locating a place: S0C4, SSRANGE, bounds | The reference (machine.rs:538-590) | The place's `at` |
| Invalid data in an operand: S0C7 | The reference (machine.rs:672) | The place's `at` |
| Divide by zero without SIZE ERROR (S0CB), stores, IO-xx, S806, nesting depth | The statement | The op's entry |
| A FUNCTION's own failure | The function (machine.rs:1220) | The plan's `at` |
| EXEC CICS, EXEC SQL, other EXEC | The EXEC block | The op's entry |
| Invalid data in any SQL input | The first host variable (machine/sql.rs:212) | The op's entry, which holds that position |
| VALUE initialization | The data entry (machine.rs:239-240) | `Storage.init_abend`'s `AbendText.at` |
| GO TO out of USE BEFORE REPORTING | The report statement (report.rs:382 (int)) | The op's entry |
| Passing an armed end whose PERFORM cannot resume | The paragraph (perform.rs:159-166 (7af)) | `Paragraph.abandoned`'s `AbendText.at` |
| Settling SQL, closing files at the end | No position (exec/src/lib.rs:165-166) | No position |

- **Each op that can abend** has one entry, and a place carries its own. Lowering splits a walker
  call that names two positions into ops that each name one.
- **Positions index the program's own file table.** Today the CLI prints every abend against the
  first program's table (cli/src/main.rs:238-240), so an abend in a CALLed program from another
  source names the first program's file. The table holds each program's own; printing stays as
  today until question 4 is settled.
- **Statement starts.** An entry `(k, at)` in a block's `statements` is a statement with a
  position starting before op `k`, or before the terminator when `k` is the block's op count;
  `at` is its position. A block's entries are in order, and several may share one `k`. Lowering
  records one wherever the walker's `exec` meets a statement that `statement_pos` gives a position
  (every statement but NEXT SENTENCE, a separator period, CONTINUE and EXIT), including an ENTRY,
  under any option: it is not an op, so a run that does not trace statements pays nothing. A CALL
  through ENTRY begins after the ENTRY's own entry, as the walker begins at the statement after
  it. Statements after a transfer that nothing reaches keep their entries in their own block.
- **Statement events.** When `RunUnit.statements` is set (`rt::unit::StatementFilter`: all
  statements, or those on given lines), the walker raises `Event::Statement { file, line }` in
  `exec` before the statement runs, and the VM raises one for each entry of a block as it reaches
  op `k`, after the block's `Paragraph` event, and before the terminator for `k` equal to the op
  count. `file` is resolved as a sink's is. `run --trace-statements` journals them (evidence.md
  §1.2).
- **The load module keeps the table** (load-module.md §9), so a module whose source is gone still
  prints `PAYROLL.cbl:LINE:COL: ABEND …` (codegen-runtime.md B1).

## 11. Walker behaviour the lowering keeps

Each is today's baseline, which the LIR reproduces. Changing any is question 3, made in both
executors and recorded.

| # | Behaviour | Where | IBM | LIR |
|---|---|---|---|---|
| 1 | Changed in both executors: a condition-name test and SET TO TRUE or FALSE take the conditional variable by item index (machine.rs `locate_item`, lower/set.rs `conditional_variable`), so a FILLER variable or one whose name repeats works | - | As IBM | The place by item index |
| 2 | MOVE to several receivers reads the sender again for each, after earlier stores | machine.rs:315-318 | The sender's subscripts are evaluated once, before the first receiver | One `Move` per receiver |
| 3 | COMPUTE with several receivers evaluates the expression once for each, all before the first store; ADD, SUBTRACT, MULTIPLY and DIVIDE read the shared operands before the first store and each receiver as it is stored | machine.rs `arithmetic` | Computed once, then stored into each | One `ArithStep` per receiver, and `ArithPlan.per_receiver` (§7.4) |
| 4 | INITIALIZE gives an alphanumeric-edited item ZERO | machine.rs:1976-1980 | SPACE | The walker's values in the plan |
| 5 | The dmax pre-pass and the float test locate receivers and operands before any store or evaluation, and `integer`, `expr_value` and the VARYING step locate operands before they evaluate them | machine.rs:1494-1506, 613-619, 517-521 | - | `ArithPlan.prepass` and `ArithStep.probe` (§7.4); the `prepass` of `IntExpr::Fixed`, `Comparand::Expr` and `Op::Step` (§7.5) |
| 6 | EVALUATE evaluates a subject again at each comparison | machine.rs:420-442 | Once | As the walker; a subject may be cached only where evaluating it cannot abend and calls no FUNCTION |

## 12. Invariants and verification

### 12.1 Invariants

1. **Lowering never changes a result** (§1).
2. **Every program the walker runs lowers.** Lowering is total over programs that pass Check.
3. **No data name is looked up at run time.** What is still found by name then is a program (CALL,
   CANCEL, LINK, XCTL, and an LE service behind them), a class or method, a CICS resource or
   map named by data, and at activation an EXTERNAL record or file in the run unit and a GLOBAL
   record or file in a running containing program (§9.16).
4. **Places match the layout.** A place has one subscript per `Item.dims` entry, and `check` fields
   exactly when the program has SSRANGE.
5. **The control-flow graph is closed.** Every block ends in one terminator; every `Jump`, `Branch`
   and `Select` target is a block; every `GoTo` and `Switch` target is a paragraph entry; every
   `Select` follows an op that returns `Arm`, with an arm for each value it can return.
6. **Every op and place has a debug entry,** and its position is the one §10 gives.
7. **Lowering is deterministic.** The same `Compiled` gives the same LIR, byte for byte once encoded
   (codegen-runtime.md §10, invariant 6).

### 12.2 What a lowering test asserts

- **Every test program lowers.** The test `Harness` (exec/src/testing.rs), which `run_with` in
  exec/src/tests.rs and the helpers of tests/report.rs, tests/printer.rs, tests/oo.rs and
  le/tests.rs use, lowers each program after it compiles it and before it runs it: the program it
  runs, and each program of the run unit's library, compiled as a CALL would compile it. A lowered
  program must pass `verify`, come back equal from the load-module codec and encode again to the
  same bytes, and lower again the same; any other `LowerError`, or a panic, fails the test and
  names the program. While step 2 is under way `Unsupported` is accepted, and the check never
  changes a test's outcome otherwise. A test in lower/tests.rs puts bench/*.cbl through the same
  check. So do the helpers of machine/sql.rs's tests, `run_flagged` in tests/sort.rs, and the test
  of machine/cics_bind.rs that writes every command in IBM's table. Tests that compile without
  running do not use the Harness, so their programs are not lowered yet.
- **Coverage.** With `IRONWORK_LOWER_REPORT=<file>` set, the check appends one line per program:
  the test (or bench file), PROGRAM-ID, a fingerprint of the source and flags, and `ok`,
  `unsupported` with the construct, or `error`. `tools/lower-coverage.sh` runs exec's tests one at
  a time with it set and prints how many programs lower and the constructs the rest lack. Step 2 is
  done when it reports every program lowered.
- **A verifier passes** on every lowered program, checking invariants 3 to 6. It runs inside `lower`
  in debug builds, and on every program the module reader decodes, since a module is untrusted input
  (load-module.md §4.8).
- **Golden LIR.** A text printer for `Program`, which `ironwork dump` also uses, and snapshot tests
  of small programs: PERFORM A THRU C with a GO TO out; overlapping ranges; EXIT SECTION past a
  range's end; a subscripted, reference-modified item under SSRANGE and without it; COMPUTE ROUNDED
  with ON SIZE ERROR; an ADD to two subscripted receivers whose second subscript is out of range; an
  88 with THRU and several values; each MOVE row of §9.2; WHENEVER with two GO TOs.
- **Determinism.** Lowering each test program twice gives equal LIR.

### 12.3 The differential test

Once the VM exists (step 3), each test program and oracle case runs under both executors, with
`Clock::Fixed` (unit.rs:47-52), and CI compares: run-unit memory at the end or at the abend, every
byte; DISPLAY output and standard error, which carries TRUNC(OPT), FASTSRT and SORT reports;
RETURN-CODE and the ending; the abend's code, message and position; every file written under a DD;
the SQL call log (verb, ordinal, text, inputs), from the scripted database of machine/sql.rs:305-336
or a recording; and for CICS the returned task: next TRANSID, COMMAREA, TS and TD queues, and
terminal screens. The run unit's events are compared too, with `RunUnit.statements` set to all
statements in both, so each run checks the statement starts of §10 against the walker's `exec`.
Both run with `RunUnit.taint` on, and the taint of every byte and each sink's `input` are compared
(evidence.md §1.3). So the VM follows three rules. It writes data to memory through
`RunUnit::write` or `rt::host::write`, or marks what it wrote with `RunUnit::mark`. It locates a
receiver it only writes with `loc_written`, where the walker uses `locate_written`. And an op that
runs something taint does not follow calls `RunUnit::unfollowed`, as the walker does.
The fuzz target of B2 runs both with a step limit, and passes when they agree or both stop at it.
The golden programs of §12.2 run in both, which exercises C99.

**What runs now.** `rt::vm` runs the core: storage, every data op, conditions, control flow, and
CALL and user-defined functions within the run unit, with NUMCHECK, ZONECHECK and PARMCHECK; LE
callable services and the virtual printer; OO COBOL, INVOKE and CALL through a function-pointer; the
file statements with LINAGE and their USE AFTER EXCEPTION/ERROR procedures, SORT, MERGE, RELEASE and
RETURN with their procedures, and the Report Writer with its USE BEFORE REPORTING procedures, each a
host of the `rt` service the walker calls; EXTERNAL and GLOBAL records and files and USE GLOBAL
procedures; JSON and XML GENERATE and PARSE; EXEC SQL; and EXEC CICS in a task (`rt::vm::run_task`),
LINK and XCTL included. A run stops as `Halt::Unimplemented`, naming what was reached, at a CALL,
LINK or XCTL of a program, or a function, method or class data, that does not lower; at FUNCTION
UUID4, whose value differs on every run, and FUNCTION RANDOM in a subscript, reference modification
or OCCURS DEPENDING ON; at the CICS cases of §9.5, SEND MAP with no FROM and RECEIVE MAP with no
INTO or SET among them; and at the few places where the LIR does not keep what decides the
interpreter's result. The test `Harness` runs every program that lowers on both executors, a CICS
task included, the system clock read once for both, and fails the test when they differ in DISPLAY
output, standard error, the ending or abend (code, message, position and file), RETURN-CODE, the
events an observer is told (Load, Open, Close, Paragraph, Sink, Statement), run-unit memory and each
program's place in it, a DD's file, a task's data sets and transient-data files, the task the run
returns (RETURN's TRANSID and COMMAREA, the queues, held records, browses and abend code), or, with a
database, the recording of the EXEC SQL calls and answers; a run the VM stops is counted, not
failed. A task with a terminal runs on the interpreter alone, since a run consumes its terminal.
With `IRONWORK_VM_REPORT` set it appends a line per run, and `tools/vm-coverage.sh` totals them.

### 12.4 What lowering refuses

`LowerError { pos: Pos, message: String }` converts to `syntax::Error` and prints as a compile
error, `FILE:LINE:COL: message` (syntax/src/lib.rs:44-56). Lowering refuses only a construct not
lowered yet (`lowering: CONSTRUCT is not lowered yet`; step 2 is done when no test program meets
this) and a program past an encoding limit (`lowering: WHAT exceeds N`, such as more than 2³² − 1
ops; layout already refuses storage over 128 MiB, layout.rs:108-109). Everything the walker refuses
only on reaching it lowers to an `Abend` op (decision 2).

## 13. Open questions

1. **C99.** Keep the walker's return-point rules as the VM's until an Enterprise COBOL run settles
   C99, with the whole-program region of a SORT procedure (§8.6)? No
   Enterprise COBOL oracle is available now: which should settle it, and must C99 be settled before
   step 5 makes the VM the default?
2. **Nested dispatch.** The VM runs CALL, INVOKE, SORT procedures and USE BEFORE REPORTING as Rust
   recursion, bounded by `MAX_DEPTH`, as the walker does. Accept that, or require the VM to keep its
   own activation stack?
3. **The walker's divergences from IBM** in §11 (items 2 to 4; item 1 is fixed). Fix them in both executors during
   step 2, each recorded as a change of semantics, or keep them until an oracle rules? Items 2 and 3
   follow rules the Language Reference states, so they need no oracle.
4. **Abends in called programs.** Should `Abend` carry its program, so the CLI names the right file
   for an abend in a CALLed program? It changes what `ironwork run` prints today.
5. **Constructs the parser refused:** answered. MOVE, ADD and SUBTRACT CORRESPONDING are expanded by
   the compiler before lowering (§9.2); PERFORM VARYING … AFTER and GO TO … DEPENDING ON are parsed
   and run by the walker, and lower as it runs them (§8.3, §8.8).
