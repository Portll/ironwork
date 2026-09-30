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
§8.9) starts.

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
| Paragraph end | The terminator after a paragraph's last statement, where a PERFORM range can complete |
| Range | Paragraphs run as a unit: an out-of-line PERFORM, a SORT or MERGE procedure, a USE BEFORE REPORTING section |
| Frame | The VM's record of an active range: its bounds, where it returns, and the nesting depth |
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
   PERFORM entry and every service that transfers control are terminators. The VM's frame stack
   reproduces the walker's Rust recursion (§8.4), recorded as V1 (§8.6).
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
// UnstringId, SearchAllId, FunctionId, FileOpId, CallId, SortId, ReleaseId, ReturnId, InvokeId,
// CicsId.

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
    /// The FileOp, FileDesc, CallPlan, SortPlan, ReportOp, InvokePlan and CicsCommand tables, the
    /// Sqlca, the ENTRY points (§9.3) and, for a class definition, its class (§9.8).
    pub services: Services,
    pub sql: Vec<SqlEntry>, pub abends: Vec<AbendText>, pub edits: Vec<Vec<Sym>>,
    pub symbols: Vec<String>, pub debug: Debug,
}

/// Fixed at compile time (codegen-runtime.md §10, invariant 7). `ssrange` is `Compiled.ssrange`
/// (exec/src/lib.rs:29). `options.dynam` makes a literal CALL resolve at run time, as the walker does
/// every CALL (load-module.md §8.3). `cards` are the CBL and PROCESS cards as written (ast.rs:7).
/// `collating` is `Compiled.collating` (exec/src/lib.rs, after 79a199e).
pub struct ProgramOptions {
    pub options: numeric::Options, pub ssrange: bool, pub cards: Vec<String>,
    pub collating: Collating,
}

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
/// section's priority-number, 0 for none; 50 to 99 is an independent segment (§8.9).
pub struct Paragraph { pub name: SymId, pub is_section: bool, pub entry: BlockId, pub section_end: ParaId, pub priority: u8, pub at: DebugId }
pub struct Block { pub ops: Vec<Op>, pub end: Terminator }
```

- **The initial image is exact.** VALUE initialization (machine.rs:230-244) depends only on
  literals, kinds, options and the collating sequence, so lowering runs it once and keeps the
  bytes, the TRUNC(OPT) reports it printed (numeric/src/binary.rs:57-69) and any abend, with the
  bytes as they stand at it and the position of the data entry it names. The VM replays them at
  each fresh activation (machine.rs:198-227).
- **The collating sequence is data.** Lowering keeps `Compiled.collating` whole, so an executor
  compares, fills HIGH-VALUE and LOW-VALUE and answers CHAR and ORD without the ALPHABET clause.
  A load module's OPTIONS section holds it after the cards; load-module.md §5.1 does not list it
  yet.
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
    /// One per entry of `Item.dims` (layout.rs:69-70), outermost first.
    pub subscripts: Vec<Subscript>,
    /// On a group that ends with an OCCURS DEPENDING ON table (layout.rs:289-300).
    pub odo: Option<Odo>,
    pub refmod: Option<RefMod>,
    pub name: SymId,
    /// The Ref's position, which the walker names in this place's abends.
    pub at: DebugId,
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
}

/// Each `check` is present only under SSRANGE: 1 to `count`; 0 to `max`; start and length at
/// least 1 and inside the item.
pub struct Subscript { pub stride: u32, pub value: IntExpr, pub check: Option<u32> }
pub struct Odo { pub object: IntExpr, pub max: u32, pub element: u32, pub check: bool }
pub struct RefMod { pub start: IntExpr, pub length: Option<IntExpr>, pub check: bool }
```

### 5.2 Bases

- **Program** is bound at activation (machine.rs:557); file record areas (file_io.rs:66-69) and the
  SD's area (machine/sort.rs:180, 215 (int)) are offsets in it. **Local** is pushed at activation
  (machine.rs:217-220).
- **Linkage(n)** is bound by USING (machine.rs:1054-1060), RETURNING (1064-1069), SET ADDRESS OF
  (1151-1167), CICS for DFHEIBLK and DFHCOMMAREA (machine/cics.rs:386, 413-428), and method entry
  for FACTORY and OBJECT data (machine/oo.rs:413-419 (int)).
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
}

/// A literal, converted once, where `literal_value` converts it on every use (machine.rs:621-634).
/// Bytes are alphanumeric in the program's code page, or hexadecimal.
pub enum Const { Bytes(Vec<u8>), National(Vec<u8>), Number(Fixed), Figurative(Figurative), All(Vec<u8>) }

/// A subscript, bound, TIMES count or exponent, as `integer()` gives it (machine.rs:613-619).
/// `Fixed` locates each place of `prepass`, then evaluates `expr` with `dmax` (§7.5).
pub enum IntExpr { Const(i64), Item(PlaceId), Fixed { expr: ExprId, dmax: u32, prepass: Vec<PlaceId> } }

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
    /// SEARCH: the index from 1 to the table's current count (machine.rs:887-893).
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
pub enum Compare { PackedPfd, Address, Float, Fixed, National, Alphanumeric, Refused(AbendId), References }

pub enum ByteClass { Packed { signed: bool }, Zoned { signed: bool }, Digits, Alphabetic }
pub enum SignTest { Positive, Negative, Zero }
pub enum Count { Fixed(u32), Odo(Odo) }
```

- **Subscripts and bounds** are `IntExpr`: a literal is `Const`, a plain integer item `Item`, and
  anything else `Fixed`, its dmax and the places its dmax pass locates found at lowering rather
  than on every call (machine.rs:614).
- **Sign conditions.** The walker reads an operand directly and evaluates anything else with
  `expr_value`, so an operand stays `Comparand::Operand`, read as its kind: ZERO or an
  alphanumeric item keeps the walker's sign-condition abend rather than arithmetic's.
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
| Fixed or float | `uses_float` on each expression (machine.rs:1506, 1400-1406) | `ArithStep.mode` |
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
```

The same `StorePlan` serves MOVE's numeric receivers, with ROUNDED off and no size check
(machine.rs:1566-1568).

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
| `Op::Step`: PERFORM VARYING's increment | Locates the variable, then the dmax pass over variable + BY, then `eval_fixed` (machine.rs:517-521) | The places of BY's dmax pass, located after the variable | `StepPlan.dmax`; always fixed |

- **The dmax pass** locates every operand of the expression except divisors and exponents, left to
  right (`dmax_refs`); **the float test** every operand, left to right, up to and including the
  first floating-point one.
- **An exponent in float mode.** `eval_float` evaluates an exponent as a float and then abends
  (machine.rs:1483), so it never makes the exponent's own dmax pass: in `Mode::Float` an executor
  evaluates `Pow`'s `IntExpr::Fixed` exponent as a float expression and does not locate its
  `prepass`.

## 8. Control flow

### 8.1 Ops, terminators and ranges

```rust
pub enum Op {
    Move { from: Operand, to: PlaceId, plan: MovePlan },
    Initialize { target: PlaceId, plan: InitId },
    Arith(ArithId),
    SetAddress { record: u16, address: Operand },
    SetUpDown { target: PlaceId, by: IntExpr, down: bool, plan: StepPlan },
    /// PERFORM VARYING's increment: `var` located, then each place of `prepass` (§7.5), then
    /// `var + by` computed with `plan.dmax` and stored.
    Step { var: PlaceId, by: ExprId, plan: StepPlan, prepass: Vec<PlaceId> },
    /// SEARCH's index steps, SORT-RETURN.
    SetInt { target: PlaceId, value: IntExpr },
    Inspect(InspectId), String(StringId), Unstring(UnstringId), SearchAll(SearchAllId),
    /// The PERFORM and CALL depth (§8.7), and TIMES counters.
    Nest, Unnest(u8), SetTemp(TempId, IntExpr), DecTemp(TempId),
    Display(DisplayId),
    Accept { target: PlaceId, from: AcceptFrom, plan: MovePlan },
    File(FileOpId), Call(CallId), Cancel(Operand),
    Sort(SortId), Release(ReleaseId), Return(ReturnId), Report(ReportOp),
    Invoke(InvokeId), Cics(CicsId), Sql(SqlId),
    /// ALTER, independent segments and the segment register (§8.9).
    Alter { para: ParaId, to: ParaId }, EnterSegment(u8), SetSegment(u8),
}

/// What an op tells the VM, as the walker's `Flow` (machine.rs:58-67) does. The library's services
/// return it too (semantics-library.md §4.1).
pub enum Step {
    Next,
    /// The handler a service selected; only a block's last op returns it, for `Select`.
    Arm(u8),
    /// A transfer the service chose at run time: HANDLE CONDITION, HANDLE ABEND.
    GoTo(ParaId),
    End(Ending),
}

pub enum Terminator {
    Jump(BlockId),
    Branch { cond: CondId, then: BlockId, otherwise: BlockId },
    /// On the Arm the block's last op returned.
    Select(Vec<BlockId>),
    /// Control leaves a paragraph for paragraph `next`, which may be one past the last (§8.4).
    ParagraphEnd { next: ParaId },
    /// GO TO, with the range rules of §8.4.
    GoTo(ParaId),
    /// GO TO … DEPENDING ON: `targets[k − 1]` for a value k in range, as a `GoTo`, depth reset
    /// included; else `otherwise`, the next statement.
    Switch { value: IntExpr, targets: Vec<ParaId>, otherwise: BlockId },
    /// An out-of-line PERFORM: push a frame, enter the range; `ret` runs when it completes.
    PerformEnter { range: RangeId, ret: BlockId },
    /// EXIT PROGRAM: nothing in the run unit's first program, GOBACK in any other (machine.rs:377-378).
    ExitProgram { next: BlockId },
    End(Ending),
    Abend(AbendId),
    /// The entry of a paragraph an ALTER names (§8.9): a `GoTo` of the target the alter table
    /// holds for `para`, or `Jump(otherwise)` while it holds none.
    AlteredGoTo { para: ParaId, otherwise: BlockId },
}

pub struct Range { pub first: ParaId, pub last: ParaId, pub kind: RangeKind }
pub enum RangeKind { Perform, SortProcedure, UseBeforeReporting }
```

The spec's `PerformEnter(range, loop)` is split: a PERFORM's loop is ordinary blocks around
`PerformEnter`, because inline PERFORM needs the same loops without a range.

### 8.2 Paragraphs, sections and fall-through

- **One entry block per paragraph,** section headers included: a section header is a paragraph
  holding the statements before the section's first paragraph (syntax/src/ast.rs:160-169).
- **A section is a range** from its header to `section_end`, as `procedure` resolves a section name
  (exec/src/lib.rs:106-120). END DECLARATIVES ends a section as a header does (lib.rs:114-120 (int)).
- **Fall-through** goes from paragraph p to p + 1, across section boundaries, as `run_paragraphs`
  does (machine.rs:275).
- **The paragraph's last block** ends in `ParagraphEnd { next: p + 1 }`, or in a plain `Jump` when
  no range's last paragraph lies in p to `next` − 1, since no frame can complete there. Only
  paragraphs that end a PERFORM, a SORT procedure or a USE BEFORE REPORTING section, and the
  program's last, keep the check.

### 8.3 PERFORM

Out of line, 3 TIMES, and inline VARYING:

```text
PERFORM A THRU C 3 TIMES                  PERFORM VARYING I FROM 1 BY 1 UNTIL I > 9, inline
b0: Nest; SetTemp t0 = 3; Jump b1         b0: Nest; Move I <- 1; Jump b1
b1: Branch Counter(t0) b2 else b3         b1: Branch (I > 9) b4 else b2
b2: DecTemp t0; PerformEnter r0 -> b1     b2: body …; Jump b3         EXIT PERFORM CYCLE: Jump b3
b3: Unnest(1); Jump next                  b3: Step I by 1; Jump b1
                                          b4: Unnest(1); Jump next    EXIT PERFORM: Jump b4
```

- **Once:** `Nest; PerformEnter r -> b1`, `b1: Unnest(1)`.
- **TIMES:** the count is evaluated once and below zero counts as zero (machine.rs:480). Each
  TIMES statement has its own `TempId`, and its counter lives in the frame the statement runs under
  (`Frame.temps`, §8.4), not in the program: a paragraph can PERFORM itself, directly or through
  others, and the walker's `for` loop in `repeat_nested` gives each activation of the statement a
  count of its own (machine.rs:480). A counter in the program would let the inner PERFORM reset
  the outer one's.
- **UNTIL:** VARYING without the Move and the Step. **TEST AFTER** moves the Branch after the body,
  and for VARYING before the Step (machine.rs:497, 514-521).
- **VARYING:** FROM is stored with MOVE rules (machine.rs:502-504); each step re-evaluates the
  variable's place, locates the places of BY in `Op::Step.prepass` (§7.5), and stores with
  `StepPlan`, no ROUNDED and no size error (machine.rs:517-521).
- **VARYING … AFTER** lowers as `vary` runs it (machine.rs:598-636 (f2)), which follows the
  Language Reference's figures (SC27-8713-03, pp. 425-428): one loop per variable, the last
  varying fastest. An inner loop's test coming true augments the variable outside it, then sets
  the inner variable to its FROM value again, then tests the outer one. TEST BEFORE sets every
  variable to its FROM value before the first test; TEST AFTER sets only the first, and each inner
  one as its loop is entered. EXIT PERFORM leaves every level at once.

```text
PERFORM P VARYING I FROM 1 BY 1 UNTIL CI AFTER J FROM I BY 1 UNTIL CJ    (TEST BEFORE)
b0: Nest; Move I <- 1; Move J <- I; Jump h0
h0: Branch CI exit else h1                s0: Step I by 1; Move J <- I; Jump h0
h1: Branch CJ s0 else run                 s1: Step J by 1; Jump h1
run: PerformEnter r -> s1                 exit: Unnest(1); Jump next
```

### 8.4 Frames

```rust
/// The VM's record of an active range: `ret` runs when it completes, and its paragraphs run at
/// `depth` (§8.7). Main is the program's own run and holds every paragraph, since the walker
/// restarts its run at any GO TO target.
/// `temps` holds the TIMES counters of the statements running under the frame, by `TempId`.
struct Frame { first: ParaId, last: ParaId, kind: FrameKind, ret: BlockId, depth: u32, temps: Vec<i64> }
enum FrameKind { Main, Perform, SortProcedure, UseBeforeReporting { at: DebugId } }
```

- **PerformEnter { range, ret }** pushes a Perform frame and goes to the entry of `first`. A range
  whose `last` precedes `first` runs nothing and goes straight to `ret`.
- **TIMES counters** are the top frame's: `SetTemp` sets one there, and `DecTemp` and `Counter`
  read it there. A new frame starts with none set, and a popped frame's go with it (§8.3).
- **ParagraphEnd { next }:** if `next` ≤ the top frame's `last`, go to `next`. Otherwise pop the
  frame and complete it: a Perform frame goes to its `ret`; Main ends the run with EndOfProgram; a
  callback frame returns to its service (§9.6).
- **GoTo(t):** while the top frame's range does not hold t, a Perform frame is popped and its `ret`
  never runs; a SortProcedure frame is re-bounded (§9.6); a UseBeforeReporting frame abends. Then
  go to t. Main holds every paragraph.
- **Transfers from services** (`Step::GoTo`: HANDLE CONDITION, HANDLE ABEND) and WHENEVER's GO TO
  follow the GoTo rule, because the walker returns them as `Flow::GoTo` (machine/cics.rs:276-278,
  machine/sql.rs:289). So do a `Switch` target and an `AlteredGoTo`'s target (§8.9), which the
  walker returns the same way (machine.rs:315, 462 (f2)).
- **Elision.** Control in paragraph p always lies inside the top frame's range, so a GO TO from p to
  t is a plain `Jump` when every range holding p also holds t.

### 8.5 The walker's baseline

The walker has no frame stack. Each out-of-line PERFORM is a Rust call of `run_paragraphs(from, to)`
inside `repeat` (machine.rs:340-347, 446-526), and control flow is the `Flow` value returned up
through those calls.

- **A range completes** when the index passes its own `to`: `while i <= to` (machine.rs:273).
- **A GO TO inside the range** is followed (machine.rs:277). **One outside it** is returned upward
  (machine.rs:279): `repeat_nested` passes it out as `Step::Out` and stops the loop, with no further
  iterations and no VARYING step (machine.rs:471, 476, 484, 495, 512); the PERFORM returns it to its
  paragraph's `run_sentences` (machine.rs:292); the enclosing `run_paragraphs` follows it if its
  range holds the target, or returns it further; and at the top `run_procedure` restarts the run at
  the target (machine.rs:264). The abandoned PERFORMs never return, and their depth is released as
  the Rust calls unwind (machine.rs:449).
- **Overlapping ranges.** Take `PERFORM A THRU C` from MAIN, and in A `PERFORM B THRU D`, where B, C
  and D each DISPLAY their names. The inner call runs B, C and D, passing the end of C without
  returning, since its `to` is D; it returns to A, and the outer call runs B and C again. The output
  is B C D B C. One return point per paragraph end would instead return from the outer PERFORM at
  the first end of C.
- **A GO TO out and back.** In codegen-runtime.md's B3, B does `GO TO D` with D after C: the walker
  runs D and what follows as the main line, and the PERFORM never returns. A return-point
  implementation would return to the old PERFORM's successor when control next passes the end of C.
- **EXIT SECTION** jumps to the paragraph after the section (machine.rs:276); past the range's `to`,
  the range completes, though control never passed the end of `to`.
- **EXIT PERFORM outside an inline PERFORM** is an S-level error in Check, as the Language
  Reference does not allow it (SC27-8713-03, p. 344). Under a card's COMPILE the program runs
  anyway, and the statement moves to the next paragraph, as EXIT PARAGRAPH does (machine.rs:278).

The frame rules of §8.4 reproduce every one of these. If an oracle settles V1 the other way, the
VM's rules change and the LIR does not: `ParagraphEnd` already marks every paragraph end where a
range can complete, which a return-point model needs.

### 8.6 Assumptions V1 and V2

Lowering and VM assumptions form a new **V** series in `numeric::assumptions::ASSUMPTIONS`. Both
entries are provisional (basis `Chosen`, settled only by an Enterprise COBOL run), and the series
is append-only: an id is never reused or renumbered.

```rust
pub const PERFORM_RANGE_EXITS: &str = "V1";
pub const SORT_PROCEDURE_EXITS: &str = "V2";

Assumption {
    id: PERFORM_RANGE_EXITS,
    claim: "A PERFORM returns only when control leaves the end of its own last paragraph while it \
            is the innermost active PERFORM; passing the end of an outer PERFORM's last paragraph \
            inside an inner one does not return from the outer. A GO TO to a paragraph outside the \
            innermost PERFORM's range abandons it, and every PERFORM out to the first whose range \
            holds the target; they never return and their loops stop. EXIT SECTION past a range's \
            last paragraph completes it. A THRU range that ends before it starts runs nothing.",
    basis: Basis::Chosen,
    oracle: Oracle::EnterpriseCobol,
}

Assumption {
    id: SORT_PROCEDURE_EXITS,
    claim: "A GO TO out of a SORT or MERGE input or output procedure does not leave it: a target \
            at or before the procedure's last paragraph continues there with the procedure's end \
            unchanged, and a target past it runs to the end of the program, which ends the \
            program, and the SORT passes that on.",
    basis: Basis::Chosen,
    oracle: Oracle::EnterpriseCobol,
}
```

B3 of codegen-runtime.md then reads: the VM's result is V1's; the walker's is recorded beside it;
a difference is reported as a change of semantics.

### 8.7 Nesting depth

The walker counts every PERFORM, inline ones included, every CALL, LINK, XCTL and INVOKE, and each
SORT procedure and USE BEFORE REPORTING run, against `MAX_DEPTH` = 100 (unit.rs:55;
machine.rs:453-459), and abends IRONWORK "PERFORM and CALL nest deeper than 100" at the statement.
The check comes once per statement, before the first iteration, so `PERFORM P 0 TIMES` can still
abend.

- **`Nest`** at the head of each PERFORM checks and raises the depth; **`Unnest(n)`** lowers it on
  each lexical exit: normal completion, EXIT PERFORM, and NEXT SENTENCE out of `n` inline PERFORMs.
- **Frames record the depth** their paragraphs run at. `ParagraphEnd` and `GoTo` reset the depth to
  the top frame's, which releases any inline PERFORMs left by EXIT PARAGRAPH, EXIT SECTION or GO TO,
  and any PERFORM statements whose frames a GO TO popped, as the walker's unwinding does.

### 8.8 Other transfers

| Statement | Terminator | Walker |
|---|---|---|
| GO TO | `GoTo`, or `Jump` when elided | machine.rs:375 |
| GO TO … DEPENDING ON | `Switch`; out of range, the next statement | machine.rs:459-464 (f2) |
| GO TO with no target | Nothing: unaltered it falls through; altered, its paragraph's entry transfers (§8.9) | machine.rs:458 (f2) |
| ALTER | `Alter` per pair, in order | machine.rs:465-473 (f2) |
| ENTRY | Nothing; the statement after it starts a block a CALL enters (§9.3) | machine.rs:458 (f2) |
| EXIT PARAGRAPH | `ParagraphEnd { next: p + 1 }` | machine.rs:275, 411 |
| EXIT SECTION | `ParagraphEnd { next: section_end + 1 }` | machine.rs:276, 412 |
| EXIT PERFORM, EXIT PERFORM CYCLE | `Jump` to the loop's exit or continuation, with `Unnest` | machine.rs:469-470 |
| NEXT SENTENCE | `Jump` past the next separator period of the paragraph, or `ParagraphEnd`, with `Unnest` | machine.rs:291, 392 |
| STOP RUN | `End(StopRun)` | machine.rs:410 |
| GOBACK, EXIT METHOD | `End(Goback)` | machine.rs:376; 422 (int) |
| EXIT PROGRAM | `ExitProgram`: whether this is the run unit's first program is known only at run time | machine.rs:377-378 |
| Falling off the last paragraph | `ParagraphEnd`, completing Main | machine.rs:265 |

### 8.9 ALTER and independent segments

ALTER changes where a paragraph's GO TO goes. The walker keeps the change in `Loaded.altered`, a
target per paragraph of the loaded program (unit.rs:33-34 (f2)), and checks it whenever control
reaches a paragraph, before its statements (`run_paragraphs_from`, machine.rs:312-317 (f2)).

```rust
Op::Alter { para: ParaId, to: ParaId }                       // ALTER para TO PROCEED TO to
Terminator::AlteredGoTo { para: ParaId, otherwise: BlockId } // at the entry of each altered paragraph
Op::EnterSegment(u8)                                         // at the entry of every paragraph
Op::SetSegment(u8)                                           // where a PERFORM range returns
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
  `EnterSegment`: while the table holds a target t for `para`, control goes to t exactly as by
  `GoTo(t)` (§8.4), so under **V1** an altered GO TO out of a PERFORM range abandons the range and
  every PERFORM out to the first whose range holds t, and the depth is reset to that frame's;
  otherwise `Jump(otherwise)` runs the paragraph as written. The target is known only at run time,
  so it is never elided to a `Jump`.
- **A GO TO with no target** that nothing has altered does nothing: control falls through to the
  next paragraph (machine.rs:458 (f2)). It lowers to no op; Check makes it its paragraph's only
  sentence.
- **Independent segments.** Control reaching a paragraph whose section has a priority-number of 50
  or more from a paragraph of another segment finds the segment in its initial state: the walker
  clears the alter entries of that segment's paragraphs (`enter_segment`, machine.rs:336-349 (f2);
  assumption C52 `ALTERED_GO_TO_RESET`). The walker keeps the segment in a register that
  `run_paragraphs_from` sets as each paragraph is entered and restores when a range completes
  normally, but not when a GO TO leaves it (machine.rs:296, 330 (f2)).
- **The segment register** is per activation. It starts at the priority of the paragraph the
  activation starts in (`procedure_start`, or an ENTRY's paragraph, machine.rs:272 (f2)).
  `EnterSegment(p)` at the head of every paragraph's entry block does what `enter_segment` does:
  when p differs from the register and is 50 or more, it clears the alter entries of the
  paragraphs whose `priority` is p; then the register holds p. `SetSegment(q)` begins the block
  each `PerformEnter` returns to: the register goes back to q, the priority of the paragraph the
  PERFORM is written in, clearing nothing. That is the value the walker restores, because while a
  paragraph's statements run the register holds its priority: every range a statement performs
  restores it, and a GO TO out of one never returns to the statement. A range abandoned by a GO TO
  never reaches its return block, so, as in the walker, the register keeps the last paragraph's
  segment until the target's `EnterSegment`.
- **Only where it shows.** Clearing matters only to a paragraph an ALTER names, so lowering emits
  `EnterSegment` and `SetSegment` only when an ALTER names a paragraph of an independent segment;
  otherwise priorities have no effect the walker shows, and independent segments lower as other
  paragraphs do.

## 9. Statements as LIR ops

### 9.1 Every statement

"One call" means the op is one semantics-library call. "Lowered" means lowering does work the
walker does on each execution; the last column names that work.

| Statement | LIR | Maps | The walker re-decides at run time |
|---|---|---|---|
| MOVE | `Move` per receiver (§9.2) | Lowered | Category dispatch in `assign` and `alnum_image` (machine.rs:1657-1768) |
| COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE | `Arith`, then `Select` if handled | Lowered | §7.1 |
| INITIALIZE | `Initialize` with a flat plan of (offset, length, value, store) | Lowered | The walk over the item's children (machine.rs:1968-1993) |
| SET TO TRUE | `Move` of the first VALUE's low end | Lowered | The conditional variable by name (machine.rs:1134) |
| SET TO | `Move`; plan `Address` for pointer receivers | One call | The kind test (machine.rs:1144-1148) |
| SET ADDRESS OF | `SetAddress`; a record that is not an 01 or 77 of LINKAGE lowers to `Abend` | One call | Resolve and linkage test (machine.rs:1159-1165) |
| SET UP BY, DOWN BY | `SetUpDown`: pointer arithmetic or a `StepPlan` | One call | Read, then match on the value (machine.rs:1173-1183) |
| INSPECT | `Inspect` over constant patterns and a prebuilt CONVERTING table when both operands are literals | One call | Literal images and the CONVERTING table (machine.rs:822-834, 849-869) |
| STRING | `String`, then `Select` on overflow | One call | `natural_bytes` of literals (machine.rs:686-697) |
| UNSTRING | `Unstring` with each receiver's MOVE plan, then `Select` | One call | `assign` dispatch per field (machine.rs:773) |
| SEARCH | Blocks: `InTable` branch, one branch per WHEN, `SetInt` steps | Lowered | The index by name (machine.rs:880); table and count |
| SEARCH ALL | `SearchAll`, with each key matched to a WHEN term by item, then `Select`, then the whole condition | One call | `flatten_and` and `key_term` by name on each execution (machine.rs:908-917) |
| IF | `Branch` | Lowered | - |
| EVALUATE | A chain of `Branch`, one per object; each comparison evaluates its subject, as the walker does | Lowered | - |
| DISPLAY | `Display` with a format per item | One call | Kind dispatch (machine.rs:1913-1959) |
| ACCEPT | `Accept` with a MOVE plan | One call | - |
| CALL, CANCEL | `Call`, then `Select`; `Cancel` (§9.3) | One call | Literal names decoded (machine.rs:966-971) |
| OPEN … START | `File`, then `Select` (§9.4) | One call | File by name, keys, FILE STATUS |
| SORT, MERGE, RELEASE, RETURN | `Sort`; `Release`; `Return`, then `Select` (§9.6) | One call | SD by name, key places, the FASTSRT plan |
| INITIATE, GENERATE, TERMINATE, SUPPRESS | `Report` (§9.6) | One call | Report and group by name (machine/report.rs:50-52 (int)) |
| INVOKE | `Invoke`, then `Select` (§9.8) | One call | Receiver kind, Java types |
| EXEC CICS | `Cics` (§9.5) | One call | The command string and option scans |
| EXEC SQL | `Sql`, then `Branch` on `Cond::Sql` (§9.7) | One call | Host variables, SQLCA fields and WHENEVER labels by name |
| EXEC DLI, other EXEC | `Abend` with the walker's EXEC message (machine.rs:396-408) | - | - |
| Declarative EXEC SQL | Nothing (machine.rs:393); its `SqlEntry` still exists | - | - |
| FUNCTION | `Operand::Function` (§9.9) | One call | Name and arity (machine.rs:1225-1233) |
| PERFORM, GO TO, EXIT, STOP RUN, GOBACK, NEXT SENTENCE | Terminators (§8) | Lowered | Procedure names (machine.rs:308-310) |
| GO TO … DEPENDING ON, ALTER, ENTRY | `Switch`; `Alter` and `AlteredGoTo` (§8.9); an entry block (§9.3) | Lowered | Procedure names (machine.rs:459-473 (f2)); where an ENTRY begins |

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

pub enum Image { Bytes, All, Figurative, Digits { digits: u32 } }
pub enum NationalFrom { Units, Decoded, Figurative }
pub enum NumericFrom {
    Value,
    /// NUMPROC(PFD), packed to packed of the same kind and scale: the bytes (1704-1711).
    PackedCopy,
    Float,
    Zero,
    /// Another figurative constant or an ALL literal: bytes filled, not converted (1720-1724).
    Fill,
    /// Alphanumeric bytes read as an unsigned zoned integer of their length (1731-1734).
    Zoned,
    /// A numeric-edited sender, de-edited (1727-1730).
    DeEdit { edit: u32, digits: u32, scale: u32 },
}
pub enum FloatFrom { Float, Fixed, Zero }
```

Every category pair, by the value the walker reads from the sender (line numbers in machine.rs):

| Sender | Group, alphanumeric | Alnum-edited | National | Numeric, numeric-edited | Float | Pointer kinds | Index |
|---|---|---|---|---|---|---|---|
| Group, alphanumeric, either edited | Copied | Edited | Decoded to UTF-16 | Unsigned zoned integer, S0C7 unless digits; numeric-edited is de-edited | Refused | Refused | Refused |
| National | Refused | Refused | Units | Refused | Refused | Refused | Refused |
| Integer numeric | Its digits, unsigned | Digits, edited | Refused | Stored; PFD packed copy | Converted | Refused | Stored |
| Numeric with decimals | Refused | Refused | Refused | Stored | Converted | Refused | Stored |
| COMP-1, COMP-2 | Refused | Refused | Refused | Converted, then stored | Narrowed or lengthened | Refused | Refused |
| ZERO | Zeros | Zeros, edited | U+0030 units | Zero | Zero | Refused | Refused |
| SPACE, QUOTE, HIGH-, LOW-VALUE | Filled | Filled, edited | Its unit | Bytes filled | Refused | Refused | Refused |
| NULL | Filled with X'00' | Filled, edited | U+0000 | Bytes filled | Refused | NULL | Refused |
| ALL literal | Repeated | Repeated, edited | Refused | Bytes filled cyclically | Refused | Refused | Refused |
| Pointer kinds | Refused | Refused | Refused | Refused | Refused | Copied | Refused |

- **Group moves** are these elementary moves with the group as alphanumeric, which differs from IBM
  (§11, item 2).
- **Several receivers** lower to one `Move` each. Each locates its receiver, then reads the sender
  again (machine.rs:315-318), so a receiver stored earlier can change what a later one gets (§11).
- **MOVE CORRESPONDING** expands at lowering into one `Move` per pair of corresponding elementary
  items. The parser refuses it today (parser.rs:826-827).
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
  (machine/oo.rs:563-568 (f2)); any other identifier, and LENGTH OF or ADDRESS OF, is `Dynamic`,
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
  `LE_SERVICE_AFTER_PROGRAMS` (int) records. `LeService` is an enum of the services `le_service`
  dispatches (machine/le_services.rs:59-74 (int)); arguments are addresses, as for a program
  (le_services.rs:31-49 (int)). ON EXCEPTION never runs for a service. A `Dynamic` target's name is
  matched with a service when the CALL runs.
- **CANCEL** is one `Cancel` per name, in order, each read as `program_name` reads a `Dynamic`
  target (machine.rs:478-483 (f2)).
- **Dynamic at run time:** loading and compiling on first CALL, RECURSIVE and INITIAL handling, the
  recursion check, the depth check (after the load, before the arguments), CANCEL's effect, and
  temporaries (machine.rs:973-1122). How a static CALL binds is load-module.md §8.3.

### 9.4 Files

```rust
/// `on` and `not_on` are the phrases written; `covers` is the status class ON covers: b'1' for AT
/// END, b'2' for INVALID KEY (file_io.rs:51-64).
pub struct FileOp { pub file: u16, pub verb: FileVerb, pub on: bool, pub not_on: bool, pub covers: u8 }

pub enum FileVerb {
    Open(OpenMode),
    Close,
    /// `key` is 0 for the prime key, then each alternate (file_io.rs:111-118).
    Read { next: bool, previous: bool, into: Option<(PlaceId, MovePlan)>, key: Option<u8> },
    Write { record: PlaceId, from: Option<(Operand, MovePlan)>, advancing: Option<Advance> },
    Rewrite { record: PlaceId, from: Option<(Operand, MovePlan)> },
    Delete,
    Start { key: Option<(StartRel, u8, PlaceId)> },
}

/// WRITE's ADVANCING phrase; the library takes it with the count evaluated.
pub enum Advance { Lines { before: bool, count: IntExpr }, Page { before: bool } }
```

A file's declaration is a `FileDesc`: its ASSIGN name, organization, access, OPTIONAL, record
format, FILE STATUS and RELATIVE KEY places, and its keys as spans of the record area. `File`
returns `Arm(0)` for no phrase, `Arm(1)` for ON and `Arm(2)` for NOT ON. `conclude` sets the status
and chooses; a failing status with no phrase and no FILE STATUS abends `IO-xx` (file_io.rs:41-47).
The walker finds the file by name (file_io.rs:27-29), works out key spans at OPEN
(file_io.rs:91-107), finds which key a READ or START names (file_io.rs:111-118), and locates FILE
STATUS on every status (file_io.rs:31-38); all are fixed at lowering.

### 9.5 EXEC CICS

```rust
/// RESP, RESP2 and NOHANDLE decide what `raise` does (machine/cics.rs:261-286).
pub struct CicsCommand { pub command: Cics, pub resp: Option<PlaceId>, pub resp2: Option<PlaceId>, pub nohandle: bool }

/// One variant per arm of `Machine::cics` (machine/cics.rs:104-137), `cics_service`
/// (cics_services.rs:17-32) and `cics_file` (cics_files.rs:38-50).
pub enum Cics {
    File(CicsFileVerb, CicsFileOptions),
    Return { transid: Option<Datum>, commarea: Option<Datum>, length: Option<IntExpr> },
    Link { program: Datum, commarea: Option<Datum>, length: Option<IntExpr>, xctl: bool },
    Abend { abcode: Option<Datum>, cancel: bool },
    /// Each condition with its label, or None to remove the entry.
    HandleCondition(Vec<(cics::Condition, Option<ParaId>)>),
    IgnoreCondition(Vec<cics::Condition>),
    PushHandle, PopHandle, HandleAbend(AbendHandler), HandleAid,
    SendMap(MapRef, SendMapOptions), ReceiveMap(MapRef, ReceiveMapOptions),
    SendControl(SendControlOptions), Receive(ReceiveOptions),
    Service(CicsService),
    /// Settles the SQL unit of work through `Session::settle` (machine/sql.rs:135-145).
    Syncpoint { rollback: bool },
    /// "EXEC CICS … is not supported yet", when reached (cics_services.rs:32).
    Unsupported(SymId),
}

/// An argument: a data item, a literal, or a name written as text.
pub enum Datum { Place(PlaceId), Const(ConstId), Text(SymId) }
/// A map named by literals is found at lowering; one named by data is looked up when it runs.
pub enum MapRef { Found(MapId), Named { map: Datum, mapset: Option<Datum> } }
/// A mapset's position in the module's BMS section (load-module.md §5.3), and a map's within it.
pub struct MapId { pub mapset: u32, pub map: u32 }
```

`cics::Condition` is the one RESP and default-abend table (semantics-library.md §7, DRY-4). Each
options struct has one field per option its handler reads, as a `Datum`, `PlaceId` or `IntExpr`.
The op returns `Next`, `GoTo(ParaId)` for a handled condition or HANDLE ABEND, or `End` for RETURN,
XCTL and a LINKed program's STOP RUN. Handler tables, the task, the EIB and the terminal stay
run-time state. The walker matches the command string and scans the option list on every command
(machine/cics.rs:39-52, 98-137), resolves HANDLE labels by name (cics.rs:288-291), reads a mapset
from the copy libraries on first use in a task (cics_bms.rs:37-57), and finds DFHCOMMAREA by name
(cics_services.rs:128).

### 9.6 SORT, MERGE and Report Writer

Both run COBOL procedures from inside a service, through one trait:

```rust
/// How a service runs a COBOL procedure. The walker implements it with `run_paragraphs`; the VM
/// runs a nested dispatch loop over the range, under a frame of the range's kind.
pub trait Procedures {
    fn run(&mut self, range: RangeId) -> Result<RangeEnd, Abend>;
}

pub enum RangeEnd {
    Completed,
    /// STOP RUN, GOBACK or the end of the program was reached inside the procedure.
    Ended(Ending),
}

/// `keys` are offsets in the record with kind and direction (machine/sort.rs:179-191 (int));
/// `fastsrt` says which USING and GIVING files DFSORT would do the I/O of (sort.rs:412-448 (int)).
pub struct SortPlan {
    pub sd: u16, pub merge: bool, pub keys: Vec<SortKey>, pub input: SortIo, pub output: SortIo,
    pub fastsrt: Vec<Fastsrt>, pub sort_return: PlaceId, pub sort_control: PlaceId,
}
pub enum SortIo { Files(Vec<u16>), Procedure(RangeId) }
```

- **SORT and MERGE** run as `rt` code: gather, a stable sort, scatter, SORT-RETURN and FASTSRT,
  unchanged (sort.rs:526-593 (int)). A procedure runs through `Procedures::run`.
- **RELEASE and RETURN** find the active sort in run-time state, as `Machine.sort` holds it now
  (sort.rs:595-669 (int)). RETURN returns `Arm` for AT END.
- **A table SORT** is one op with the element's stride and key offsets fixed; the count stays
  dynamic under OCCURS DEPENDING ON (sort.rs:673-707 (int)).
- **GO TO out of a SORT procedure** does not unwind past it: `procedure_range`
  (sort.rs:510-524 (int)) behaves as V2 states (§8.6), and SORT passes the end of the program on
  (sort.rs:548, 574 (int)). The SortProcedure frame is re-bounded the same way.
- **Report Writer.** `report::Writer` (exec/src/report.rs (int)) resolves the report model at
  compile time, but keeps a `Ref` for each CONTROL item and an AST `Expr` for each SOURCE and SUM
  operand, resolved on every GENERATE (machine/report.rs:200, 288, 297, 664 (int)). Lowering
  replaces them with places and expressions, and `Report` ops name a report or group by index.
- **USE BEFORE REPORTING** runs through `Procedures::run` under a UseBeforeReporting frame. A GO TO
  out of it abends IRONWORK at the report statement (machine/report.rs:382 (int)); STOP RUN or
  GOBACK inside it ends the run (report.rs:380-381, 44-45 (int)).

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
  stops the later ones from applying. Each target block ends in `GoTo(para)`, not a plain jump,
  because the walker returns the branch as `Flow::GoTo` (machine/sql.rs:289), which leaves PERFORM
  ranges by V1. The walker resolves the label by name on every statement.
- **No database attached** abends EXEC at run time, as now (machine/sql.rs:28-30).

### 9.8 OO COBOL

```rust
/// Java types come from `operand_type` and `item_type`, which read only declarations
/// (exec/src/oo.rs:298-341 (int)); the walker works them out on every INVOKE.
pub struct InvokePlan {
    pub receiver: Receiver, pub method: MethodName,
    pub args: Vec<(Operand, SymId)>, pub returning: Option<(PlaceId, SymId)>,
    pub on_exception: bool, pub not_on_exception: bool,
}
/// The walker decides the receiver by name on every INVOKE (machine/oo.rs:192-215 (int)).
/// `Class` is a REPOSITORY class-name: `name` as written, which the walker's messages give, and
/// `external`, which finds the class.
pub enum Receiver { SelfRef, Super, Class { name: SymId, external: SymId }, Object(PlaceId) }
pub enum MethodName { New, Named(SymId), Dynamic(PlaceId) }

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
  Check refuses them. `NEW` sent to anything but a class abends IRONWORK.
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
/// `side` is TRIM's LEADING or TRAILING.
pub struct FunctionPlan { pub func: Func, pub args: Vec<ExprId>, pub side: Option<TrimSide>, pub refmod: Option<RefMod>, pub at: DebugId }

/// The functions Check admits (exec/src/lib.rs:32-35).
pub enum Func {
    Char, Ord, NationalOf, Length, UpperCase, LowerCase, Reverse, CurrentDate, Numval, NumvalC,
    Trim, Mod, Rem, Integer, IntegerPart, Abs, Min, Max, IntegerOfDate, DateOfInteger,
}
```

The walker matches the name as a string and checks the argument count on each call
(machine.rs:1225-1233). A wrong count lowers to an `Abend` op; everything else is one call.

## 10. The debug table

```rust
/// `sources` is the program's file table: the source, then each COPY member (ast.rs:21-22).
/// `positions` maps a DebugId to its position; `ops` holds, per block, one DebugId per op and
/// one for the terminator.
pub struct Debug { pub sources: Vec<SymId>, pub positions: Vec<Pos>, pub ops: Vec<Vec<DebugId>> }
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
| GO TO out of USE BEFORE REPORTING | The report statement (report.rs:382 (int)) | The frame's `at` |
| Settling SQL, closing files at the end | No position (exec/src/lib.rs:165-166) | No position |

- **Each op that can abend** has one entry, and a place carries its own. Lowering splits a walker
  call that names two positions into ops that each name one.
- **Positions index the program's own file table.** Today the CLI prints every abend against the
  first program's table (cli/src/main.rs:238-240), so an abend in a CALLed program from another
  source names the first program's file. The table holds each program's own; printing stays as
  today until question 4 is settled.
- **The load module keeps the table** (load-module.md §9), so a module whose source is gone still
  prints `PAYROLL.cbl:LINE:COL: ABEND …` (codegen-runtime.md B1).

## 11. Walker behaviour the lowering keeps

Each is today's baseline, which the LIR reproduces. Changing any is question 3, made in both
executors and recorded.

| # | Behaviour | Where | IBM | LIR |
|---|---|---|---|---|
| 1 | A condition-name test and SET TO TRUE find the conditional variable again by its unqualified name, so one whose name is ambiguous, or FILLER, abends IRONWORK | machine.rs:1134, 1797-1803 | No such failure | The place by item index, plus an `Abend` op where the walker would fail |
| 2 | A group sender to a numeric receiver is converted as an unsigned zoned integer; an integer sender to a group receiver becomes its digits | machine.rs:1659-1672, 1725-1736, 1761-1764 | A group move copies bytes, unconverted | `MovePlan` as §9.2 |
| 3 | MOVE to several receivers reads the sender again for each, after earlier stores | machine.rs:315-318 | The sender's subscripts are evaluated once, before the first receiver | One `Move` per receiver |
| 4 | COMPUTE with several receivers evaluates the expression again for each (machine.rs:321-323); ADD and SUBTRACT with several receivers read shared operands again after earlier stores | machine.rs:1503-1522 | Computed once, then stored into each | One `ArithStep` per receiver |
| 5 | INITIALIZE gives an alphanumeric-edited item ZERO | machine.rs:1976-1980 | SPACE | The walker's values in the plan |
| 6 | The dmax pre-pass and the float test locate receivers and operands before any store or evaluation, and `integer`, `expr_value` and the VARYING step locate operands before they evaluate them | machine.rs:1494-1506, 613-619, 517-521 | - | `ArithPlan.prepass` and `ArithStep.probe` (§7.4); the `prepass` of `IntExpr::Fixed`, `Comparand::Expr` and `Op::Step` (§7.5) |
| 7 | EVALUATE evaluates a subject again at each comparison | machine.rs:420-442 | Once | As the walker; a subject may be cached only where evaluating it cannot abend and calls no FUNCTION |

## 12. Invariants and verification

### 12.1 Invariants

1. **Lowering never changes a result** (§1).
2. **Every program the walker runs lowers.** Lowering is total over programs that pass Check.
3. **No data name is looked up at run time.** What is still found by name then is a program (CALL,
   CANCEL, LINK, XCTL, and an LE service behind them), a class or method, and a CICS resource or
   map named by data.
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
  check. `run` and `run_task` in machine/sql.rs, `run_flagged` in tests/sort.rs and tests that
  compile without running do not use the Harness, so their programs are not lowered yet.
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
terminal screens. The fuzz target of B2 runs both with a step limit, and passes when they agree or
both stop at it. The golden programs of §12.2 run in both, which exercises V1.

### 12.4 What lowering refuses

`LowerError { pos: Pos, message: String }` converts to `syntax::Error` and prints as a compile
error, `FILE:LINE:COL: message` (syntax/src/lib.rs:44-56). Lowering refuses only a construct not
lowered yet (`lowering: CONSTRUCT is not lowered yet`; step 2 is done when no test program meets
this) and a program past an encoding limit (`lowering: WHAT exceeds N`, such as more than 2³² − 1
ops; layout already refuses storage over 128 MiB, layout.rs:108-109). Everything the walker refuses
only on reaching it lowers to an `Abend` op (decision 2).

## 13. Open questions

1. **V1.** Keep the walker's PERFORM rules as the VM's until an Enterprise COBOL run settles V1? No
   Enterprise COBOL oracle is available now: which should settle it, and must V1 be settled before
   step 5 makes the VM the default?
2. **Nested dispatch.** The VM runs CALL, INVOKE, SORT procedures and USE BEFORE REPORTING as Rust
   recursion, bounded by `MAX_DEPTH`, as the walker does. Accept that, or require the VM to keep its
   own activation stack?
3. **The walker's divergences from IBM** in §11 (items 1 to 5). Fix them in both executors during
   step 2, each recorded as a change of semantics, or keep them until an oracle rules? Items 2 to 4
   follow rules the Language Reference states, so they need no oracle.
4. **Abends in called programs.** Should `Abend` carry its program, so the CLI names the right file
   for an abend in a CALLed program? It changes what `ironwork run` prints today.
5. **Constructs the parser refuses:** MOVE CORRESPONDING. The LIR defines it. Add it to the front
   end and the walker in step 2, or later? PERFORM VARYING … AFTER and GO TO … DEPENDING ON are
   now parsed and run by the walker, and lower as it runs them (§8.3, §8.8).
