# The semantics library and the split of `rt`

Step 1 of [codegen-runtime.md](codegen-runtime.md) §14: extract the semantics library from the
interpreter's `Machine`, and split the runtime crate `rt` out of `crates/exec`.

**Status:** draft, 2026-09-30, for the operator's review. Nothing here is built. The payload types
the library takes are [lir.md](lir.md)'s; this document names them and does not define them.

| Tag | Checkout | State |
|---|---|---|
| `codegen` | `ironwork-codegen`, main at `79a199e` | The interpreter with the EXEC SQL runtime |
| `le-cics` | `ironwork-le-cics`, branch `feat/le-under-cics` at `0e73f5d`, read-only | The fuller interpreter: SORT and MERGE, LE services, Report Writer and OO COBOL, and no SQL |

Step 1 starts on the tree where the two are merged (§8, G0), so the inventory is their union.
Citations are in `codegen` unless tagged `le-cics`.

---

## 1. What step 1 achieves

- **`rt` holds every function that decides a result, and every service.** Storage access, MOVE,
  compare, editing, arithmetic stores, conversions and abends; files, CICS, BMS, the terminal, SQL,
  SORT, LE, Report Writer, the OO runtime, ACCEPT, DISPLAY, CALL and CANCEL.
- **No function in `rt` takes a `syntax` type.** A method that takes a `Ref`, `Expr`, `Stmt` or
  `ExecBlock` today takes a `Loc`, a value or a typed payload instead (§4).
- **`Machine` shrinks to the walker:** the `Stmt` walk, control flow, and the resolution the
  interpreter still does at run time, which lowering (step 2) reuses.
- **Nothing observable changes.** Every test and oracle case passes after every step (§8), and no
  output byte, abend code or abend position moves.

**Two reference types.** `Place` is the LIR's static description of a data reference: a base, a
constant offset, subscript and reference-modification expressions, OCCURS DEPENDING ON, and the
SSRANGE checks. `Loc` is the evaluated run-time location: a concrete offset, length and kind, which
is what `Machine::locate` returns today (machine.rs:69-75). An executor evaluates a `Place` to a
`Loc`. The semantics library takes `Loc`, never a `Place` and never an AST type.

## 2. The four classes

| Class | The test | Ends up |
|---|---|---|
| **walker** | The method walks `Stmt`, `Cond` or `Expr` trees, runs a statement's sub-blocks, or decides control flow | Stays in the interpreter (`exec`). The VM has its own |
| **semantics** | It decides a result from resolved operands: storage access, MOVE, compare, edit, arithmetic stores, conversions, abends | `rt`. Both executors call it |
| **service** | Files, CICS, BMS, terminal, SQL, SORT, LE, Report Writer, OO runtime, ACCEPT and DISPLAY, CALL and CANCEL | `rt`. Both executors call it |
| **resolution** | Work a compiler does once: a `Ref` to an address, a name to an index, a `Literal` to a value | Lowering (`compile`). The interpreter keeps a run-time copy until step 5 |

A method that does two of these is classed by where most of its body goes, and its note says what
splits off.

## 3. The inventory

Every method of every `impl Machine` block, in every file of both checkouts.

| | codegen | le-cics | Union |
|---|---:|---:|---:|
| walker | 20 | 28 | 28 |
| semantics | 35 | 37 | 40 |
| service | 78 | 173 | 178 |
| resolution | 35 | 39 | 41 |
| **all** | **168** | **277** | **287** |

158 methods are in both checkouts, 10 only in `codegen` (`machine/sql.rs`), and 119 only in
`le-cics` (`le_services.rs`, `oo.rs`, `report.rs`, `sort.rs`, and `decode` and `write_record`).

| File | walker | semantics | service | resolution |
|---|---:|---:|---:|---:|
| `machine.rs` | 19 | 27 | 9 | 13 |
| `machine/file_io.rs` | 1 | 6 | 14 | 7 |
| `machine/cics.rs` | 0 | 0 | 17 | 7 |
| `machine/cics_files.rs` | 0 | 0 | 9 | 3 |
| `machine/cics_bms.rs` | 0 | 0 | 8 | 1 |
| `machine/cics_services.rs` | 0 | 0 | 17 | 2 |
| `machine/sql.rs` | 0 | 3 | 5 | 2 |
| `machine/sort.rs` | 2 | 3 | 20 | 3 |
| `machine/le_services.rs` | 0 | 0 | 23 | 0 |
| `machine/oo.rs` | 2 | 0 | 13 | 3 |
| `machine/report.rs` | 4 | 1 | 43 | 0 |

**Reading the tables.** Each method is `name line`: the line of the `fn`, as `codegen/le-cics`
where the two differ, one number where they agree, and `-` for the checkout without it. After it,
what the method takes from `syntax`: `S:` types in its signature, `B:` types only its body uses,
`+program` for a body that reads `self.program` (the AST `Program`), `+layout` for one that reads
`self.layout` (the compile-side `Layout`). `Pos` is left out (§4.3). Every method that takes a
`Val` or a `Loc` also depends on `Figurative` and `SignClause`, which `Val` and `Kind` hold (§4.3).

### 3.1 `crates/exec/src/machine.rs`: storage, MOVE, arithmetic, the walker

| Class | Methods |
|---|---|
| walker | `run_procedure` 255/266 +program; `run_paragraphs` 270/281 +program; `run_sentences` 286/297 S: Stmt; `run_block` 298/309 S: Stmt; `exec` 312/323 S: Stmt, B: ExecKind, ExitKind, Expr, Target, +program; `alternative_matches` 420/434 S: Object, Subject; `repeat` 446/460 S: Loop; `repeat_nested` 461/475 S: Loop, B: BinOp, Expr, Operand; `integer` 613/630 S: Expr; `operand_with_loc` 660/713 S: Operand; `operand` 668/721 S: Operand; `overflow_branch` 704/757 S: Stmt; `search` 871/924 S: Search, B: Expr, Operand, Ref, +layout; `expr_value` 1382/1439 S: Expr; `eval_fixed` 1419/1476 and `eval_float` 1461/1518 S: Expr, B: BinOp, Figurative; `arithmetic` 1491/1548 S: Expr, SizeError, Target; `condition` 1770/1827 S: Cond, B: Expr, Operand, Ref, RelOp, +layout; `comparand` 1906/1963 S: Expr |
| semantics | `activation` 198/206 S: Compiled; `initialize_values` 230/241 +layout; `occurrence_offset` 246/257 S: Item; `nest` 453/467; `occurrences` 594/611 B: Expr, Operand, +layout; `bytes` 605/622; `write` 609/626; `read` 636/653; `decode` -/674; `zoned_value` 655/680 S: SignClause, B: SignPosition; `natural_bytes` 686/739 S: Operand; `set_integer` 699/752 S: Ref; `string_stmt` 711/764 S: StringStmt, B: Delimiter, Expr, Operand; `unstring` 743/796 S: Unstring, B: Expr, Figurative, Operand; `inspect` 805/858 S: Inspect, B: InspectMode; `offset_of` 958/1011; `set` 1124/1181 S: SetStmt, B: Figurative, Ref, +layout; `function` 1219/1276 S: FunctionCall; `store_value` 1542/1599; `store_fixed` 1566/1623; `store_fixed_checked` 1570/1627 +layout; `zoned_image` 1635/1692 S: SignClause, B: SignPosition; `assign` 1657/1714 B: Figurative, +layout; `alnum_image` 1756/1813; `class` 1824/1881 S: Class, Expr, B: Operand; `compare` 1852/1909 S: Expr, B: Figurative; `image_len` 1896/1953 |
| service | `call` 973/1026 S: Call, +program; `call_nested` 998/1055 S: Call, Compiled, B: ArgMode, Operand; `bind` 1054/1111 and `bind_returning` 1064/1121 +program, +layout; `content_argument` 1078/1135 S: Operand; `value_argument` 1098/1155 S: Operand, B: Figurative; `cancel` 1111/1168; `accept` 1190/1247 S: AcceptFrom, Ref; `display` 1913/1970 S: Operand, B: Literal |
| resolution | `procedure` 308/319 S: ProcName, +program; `resolve` 528/542 and `locate` 538/552 S: Ref, +layout; `literal_value` 621/638 S: Literal; `region` 840/893 S: Bound; `phrases` 849/902 S: InspectPhrase, B: Literal, Operand; `address_of` 946/999 S: Ref, +layout; `program_name` 966/1019 S: Operand; `returned` 1071/1128 B: Ref; `operand_kind` 1393/1450 S: Operand; `uses_float` 1400/1457 S: Expr; `dmax` 1409/1466 S: Expr, B: BinOp, Literal, Operand; `initialize` 1968/2027 B: Figurative, +layout |

- **Split off.** `exec` keeps one arm per `Stmt`; its EXEC arms hand the `ExecBlock` to `cics` and
  `sql`. `integer` moves its Fixed-to-i64 conversion and abend to `rt`. `eval_fixed` and
  `eval_float` move their binary-operation core to `rt` as `arith::fixed_binop` and `float_binop`,
  and keep the `Expr` walk. `arithmetic` hands dmax and the float choice to lowering, and the
  remainder rule and size-error decision to `rt`; its locate passes stay (lir.md §7.4). `search`
  keeps SEARCH; SEARCH ALL's `flatten_and` and `key_term` move to lowering.
- **New signatures.** `activation` takes `Storage` (lir.md §4), and `initialize_values` builds that
  image once per program. `occurrence_offset` needs only `Item.dims`. `occurrences` takes the
  evaluated DEPENDING ON value, which the executor evaluates. `nest` becomes `RunUnit::enter`.
  `store_fixed_checked` takes the edit picture with the `Kind`, and the receiver's name for the
  TRUNC(OPT) report. `set` becomes the entry points of lir.md's `SetAddress` and `SetUpDown` ops,
  and MOVE for SET TO. `call_nested` builds the USING addresses (DRY-2) and runs the callee
  (DRY-1); `bind` and `bind_returning` take the linkage ordinals. `cancel` is already resolved.
- **Resolution.** `locate` turns a `Ref` into a `Loc`; its checks move to `rt::loc` (E10b).
  `operand_kind` locates only to read a kind. `uses_float` and `dmax` are arithmetic plan facts.
  `literal_value` is folded by lowering, and `initialize` unrolled into an `InitPlan`.

### 3.2 `machine/file_io.rs`: file verbs

| Class | Methods |
|---|---|
| walker | `conclude` 51 S: Handlers, +program |
| semantics | `set_status` 31, `io_status` 41, `relative_fits` 135 +program; `record_area` 71; `relative_value` 120 B: Expr, Operand, +program; `relative_number` 128 |
| service | `held` 146 S: OpenMode; `is_held` 157; `record_bytes` 162; `deliver` 185 S: Ref; `open_file` 200 S: OpenMode, B: Organization, +program; `close_file` 259; `read_stmt` 270 S: ReadStmt, B: Access, OpenMode, Organization, +program; `read_stream` 331 S: ReadStmt, B: OpenMode, +program; `write_stmt` 364 S: Advancing, Handlers, Operand, Ref; `write_record` -/370 S: Advancing, Handlers, B: OpenMode, +program; `write_stream` 419/424 S: Advancing, B: OpenMode, +program; `rewrite_stmt` 453/458 S: Handlers, Operand, Ref, B: OpenMode; `delete_stmt` 491/496 S: Handlers, B: OpenMode; `start_stmt` 517/522 S: Handlers, Ref, RelOp, B: Expr, OpenMode, Operand |
| resolution | `file_index` 27 +program; `area` 66 +layout; `sequential` 76 B: Access, Organization, +program; `record_span` 82 and `key_named` 111 S: Ref, +program; `keying` 91 B: Organization, +program; `record_of` 171 S: Operand, Ref, +layout |

`conclude` picks the AT END, INVALID KEY or NOT phrase from the status and runs its block; `rt`
returns `Step::Arm` and the status, and the walker runs the block. `io_status` is the abend policy
for a failing status and takes a `FileStatus` (DRY-3). The status and RELATIVE KEY readers take
their places from `FileDesc`. `write_record` is Report Writer's line write.

### 3.3 `machine/cics.rs`: dispatch, options, conditions, program control

Lines agree in both checkouts.

| Class | Methods |
|---|---|
| service | S: ExecBlock: `cics` 98, `deliver_record` 230 (B: Operand), `cics_ok` 252, `raise` 261, `handle_condition` 293 and `handle_abend` 308 (B: ExecArg), `commarea` 322, `cics_return` 332, `cics_link` 353 (B: Operand), `cics_abend` 401. No AST type: `eib_bytes` 196, `eib_halfword` 201, `eib_fullword` 205, `eib_packed` 210, `eib_text` 217, `eib_calen` 223, `begin_task` 413 |
| resolution | S: ExecBlock: `arg_bytes` 141, `store_bytes` 168, `store_int` 179, `store_pointer` 187 (B: Operand); `arg_int` 152 (B: Expr); `arg_text` 160 (B: ExecArg); `label` 288 (B: ProcName, +program) |

`cics` dispatches on the command name and becomes a match on `CicsCommand`. The `arg_*` and
`store_*` methods turn an option name into storage; lowering resolves each option to a `Datum`.
`raise` takes a `cics::Condition` (DRY-4). The HANDLE methods take their conditions with labels
already resolved. `cics_link` runs a callee (DRY-1).

### 3.4 `machine/cics_files.rs`: EXEC CICS file control

| Class | Methods |
|---|---|
| service | S: ExecBlock: `cics_file` 28, `file_read` 116, `file_write` 140, `file_rewrite` 157, `file_delete` 174, `file_startbr` 207, `file_browse` 235. `on_file` 63 B: OpenMode; `release_hold` 108 |
| resolution | S: ExecBlock: `ridfld` 84 (RIDFLD, RRN, KEYLENGTH, GENERIC), `set_ridfld` 99, `reqid` 112 |

### 3.5 `machine/cics_bms.rs`: terminal control through BMS

| Class | Methods |
|---|---|
| service | S: ExecBlock: `bms_map` 37, `with_terminal` 74, `wcc` 82, `send_map` 101 (B: Initial), `send_control` 164, `next_inbound` 177, `receive_map` 190 (B: Ref), `receive_raw` 240 |
| resolution | `area_bytes` 60 S: ExecBlock, B: Ref |

`bms_map` loads the mapset from the copy libraries with `syntax::bms::find_mapset`, which becomes
`Loader::mapset`. `area_bytes` falls back to a data item named after the map, by name.
`receive_map` writes the symbolic map through `Ref` locates.

### 3.6 `machine/cics_services.rs`: time, queues, ASSIGN and the rest

| Class | Methods |
|---|---|
| service | S: ExecBlock: `cics_service` 15, `cics_asktime` 45/44, `cics_formattime` 62/61, `cics_assign` 81/80 (+program), `cics_getmain` 108/107, `cics_address` 122/121 (+layout), `sent_bytes` 138/137, `cics_send_text` 148/147, `cics_write_operator` 155/154, `cics_writeq_ts` 170/169, `cics_readq_ts` 188/187, `cics_deleteq_ts` 201/200, `cics_writeq_td` 210/209, `cics_readq_td` 217/216, `cics_deleteq_td` 226/225. No AST type: `task` 37/36, `encoded` 41/40 |
| resolution | S: ExecBlock: `separator` 55/54, `queue_name` 162/161 |

The time commands use `unit::civil` (DRY-5). `cics_assign` reads `program.id`. `cics_address`
finds the EIB and COMMAREA by name through the layout.

### 3.7 `machine/sql.rs`: EXEC SQL, `codegen` only

| Class | Methods |
|---|---|
| semantics | `sql_inputs` 201, `sql_assign` 221 S: HostVar; `sqlca` 248 B: Expr, Literal, Operand, Ref, +layout |
| service | `sql` 24 S: ExecBlock, B: ChangeKind, +program; `cics_syncpoint` 135 S: ExecBlock, +program; `session` 147; `sql_call` 152 +program; `sql_single_row` 160 S: HostVar |
| resolution | `sql_targets` 176 S: HostVar, +layout; `whenever` 277 S: Whenever, B: Action, ProcName |

`sql` walks a `syntax::sql::Statement`; `rt` runs an `SqlEntry` (lir.md §9.7) against the
`Session`. `sql_single_row` assigns one row, gives +100 for none and -811 for more. `sql_targets`
locates each host variable and finds its `HostType` through `sql::host_type`, which reads the
`Layout`; it builds `HostPlace`s. `sqlca` builds `Ref`s to SQLCODE and the rest by name; `rt` takes
an `Sqlca`. `whenever` finds its paragraph by name at run time; lowering emits branches.
`cics_syncpoint` is a CICS service that calls `Session::settle`.

### 3.8 `machine/sort.rs`: SORT, MERGE, RELEASE and RETURN, `le-cics` only

| Class | Methods |
|---|---|
| walker | `run_sort_procedure` 487 S: ProcName; `procedure_range` 510 +program |
| semantics | `key_values` 195; `fixed_length` 207 +program; `entry` 214 |
| service | `sorting` 148 S: Sorting, +program; `register` 159 S: Ref; `sort_return` 163 B: Expr, Operand; `refuse_control_statements` 169; `status_failed` 229, `close_for_sort` 252, `next_input` 260, `by_dfsort` 388 +program; `is_open` 236; `open_for_sort` 241 S: OpenMode, +program; `gather` 295, `sort_end` 583 S: SortStmt; `read_using` 312 S: SortStmt, B: OpenMode, Organization, +program; `scatter` 350; `write_giving` 365 B: OpenMode, +program; `report_fastsrt` 452 S: SortStmt, B: Organization, +program; `sort_file` 526 S: SortStmt, B: SortIo; `release` 595 S: Operand, Ref, +layout; `return_record` 629 S: Handlers, Ref, +program; `sort_table` 673 S: SortStmt, B: Expr, Literal, Operand, Ref, +layout |
| resolution | `file_keys` 179 S: Ref, +layout; `fastsrt_plan` 412 S: SortStmt, B: SortIo, +program; `fastsrt_refusal` 427 S: SortStmt, B: Organization, +program |

The walker methods run an INPUT or OUTPUT PROCEDURE range, as a PERFORM does. `key_values` reads
each key as a `Val` or in DFSORT's decimal form, and `entry` builds a key entry by the
`SORT_DECIMAL_KEYS` rules. `sort_return` reads SORT-RETURN through `integer(Expr)`.
`report_fastsrt` writes a note to `unit.err`. The methods that read `program.files[k]` take a
`FileDesc`.

### 3.9 `machine/le_services.rs`: LE callable services, `le-cics` only

| Class | Methods |
|---|---|
| service | `le_call` 19 S: Call; `le_arguments` 31 S: Call, B: ArgMode, Operand. No AST type: `le_service` 51, `le_at` 90, `le_load` 97, `le_store` 103, `le_fullword` 110, `le_vstring` 117, `le_output` 124, `le_now` 130, `cee3abd` 135, `ceedays_or_secs` 153, `ceedate` 167, `ceedatm` 175, `ceedywk` 184, `ceegmt` 191, `ceeloct` 198, `ceegmto` 207, `ceemout` 214, `cee3dmp` 224, `ceegtst` 246, `ceefrst` 264, `le_write` 280 |

`le_call`'s ON EXCEPTION block is the walker's. `le_arguments` is a second copy of the CALL USING
address builder (DRY-2). `le_service` dispatches on the name against `le::PROVIDED`. `le_now`
reads the clock through `unit::now` and dates through le.rs's calendar code (DRY-5).

### 3.10 `machine/oo.rs`: INVOKE and the OO runtime, `le-cics` only

| Class | Methods |
|---|---|
| walker | `no_method` 284, `succeeded` 295 S: Invoke |
| service | `oo_register` 71 S: Ref, +layout; `room` 83; `keep` 92 S: Compiled; `jni_environment` 101; `part_storage` 120; `load_class` 130; `chain` 170; `object` 181; `find` 259; `invoke` 302 S: Invoke, B: InvokeMethod, +program, +layout; `new_object` 358, `run_method` 386 S: Invoke; `call_through_pointer` 446 S: Call, B: Figurative, Operand, +layout |
| resolution | `receiver` 192 S: Invoke, +program, +layout; `method_name` 217 S: InvokeMethod; `argument` 234 S: Operand, B: Figurative, Literal |

`keep`, `load_class` and `run_method` pass `Rc<ClassCode>` and `Rc<Compiled>`, which hold the AST
and the `Layout`. `load_class` calls `oo::find_class` (oo.rs:728) and `oo::class_code`, which
compile source; both become `Loader` methods. `invoke` calls `oo::operand_type` and `item_type`,
which read the `Layout`, so lowering computes the Java signatures. `jni_environment` and
`call_through_pointer` read the JNI table in `syntax::jni`. `run_method` runs a callee (DRY-1).

### 3.11 `machine/report.rs`: Report Writer, `le-cics` only

| Class | Methods |
|---|---|
| walker | `subtotal` 279; `accumulate` 295; `use_before_reporting` 375; `fill_field` 646 B: Expr |
| semantics | `add_to_total` 312 B: Figurative |
| service | `report_statement` 29 S: ReportStmt; `item_loc` 59, `state_offset` 64, `initiate` 126 +layout; `first_increment` 396, `print_lines` 404, `place_body` 419, `place_page_heading` 533, `place_page_footing` 544, `place_report_footing` 556 B: LineNumber; `place_report_heading` 505 B: LineNumber, NextGroup; `next_group` 592 B: NextGroup; `write_report_line` 688 B: Advancing; `write_report_record` 707 S: Advancing, +program. No AST type: `report_named` 50, `report` 54, `flag` 68, `set_flag` 72, `fullword` 77, `set_fullword` 82, `counter` 87, `set_counter` 94, `line_counter` 98, `set_line_counter` 102, `set_print_switch` 106, `arm_indicators` 113, `zero_total` 121, `generate` 142, `terminate` 168, `control_values` 197, `control_break` 207, `save_controls` 218, `swap_in_saved_controls` 230, `restore_controls` 241, `break_at` 249, `reset_totals_on` 268, `produce` 332, `span` 392, `open_page` 453, `advance_page` 472, `new_page` 492, `print_line` 621, `blank` 640 |

These methods pass `Report`, `Group`, `Line` and `Field` from exec/src/report.rs, so the columns
miss what those hold: `Control.reference` is a `Ref`, `Report.code` a `Literal`, and
`FieldContent::Source`, `Origin::Source` and `Subtotal.operand` an `Expr`. Hence `Writer<X>` (C4).
`control_values` reads each control through `locate` and takes the control places instead. The
walker methods evaluate an `Expr` or `Literal`; `rt` takes the value. `use_before_reporting` runs a
declarative range.

## 4. The AST leaks

Of 218 semantics and service methods (union), **96** take an AST type in their signature, or a
compile-side type that holds one (`Compiled`, `Item`), and **33** more reach one in their body.
These 129 change before they can live in `rt`; the other 89 already work on bytes, `Loc` and `Val`.

### 4.1 Size of each leak, and what replaces it

Semantics and service methods only, by the AST type in the signature. Body uses add 14 more methods
for `Operand`, 10 for `OpenMode`, 8 for `Expr` and 7 for `Figurative`.

| AST type in a signature | Methods | Body lines | Files | Replaced by |
|---|---:|---:|---|---|
| `ExecBlock` | 42 | 831 | cics, cics_bms, cics_files, cics_services, sql | `CicsCommand`; `SqlEntry` in sql.rs |
| `Ref` | 10 | 200 | file_io, machine, oo, sort | `Loc` |
| `Handlers` | 6 | 185 | file_io, sort | No block: `rt` returns `Step::Arm` and the status |
| `SortStmt` | 6 | 184 | sort | `SortPlan` |
| `Operand` | 7 | 172 | file_io, machine, sort | A `Val`, with its `Loc` where it has one |
| `Call` | 5 | 162 | le_services, machine, oo | `CallPlan` |
| `FunctionCall` | 1 | 162 | machine | `FunctionPlan` |
| `Invoke` | 3 | 135 | oo | `InvokePlan` |
| `Advancing` | 4 | 119 | file_io, report | `Advance` |
| `Compiled` | 3 | 94 | machine, oo | `ProgramFacts` and the program handle `H` |
| `ReadStmt` | 2 | 92 | file_io | `FileOp` |
| `OpenMode` | 3 | 78 | file_io, sort | `rt::OpenMode` |
| `Expr` | 2 | 70 | machine | An evaluated `i64` or `Val` |
| `SetStmt` | 1 | 65 | machine | The `SetAddress` and `SetUpDown` entry points, and MOVE |
| `Unstring` | 1 | 61 | machine | `UnstringPlan` |
| `HostVar` | 3 | 55 | sql | `HostPlace` |
| `SignClause` | 2 | 52 | machine | `rt::SignClause` |
| `Inspect` | 1 | 34 | machine | `InspectPlan` |
| `StringStmt` | 1 | 31 | machine | `StringPlan` |
| `AcceptFrom` | 1 | 28 | machine | `rt::AcceptFrom` |
| `Class` | 1 | 27 | machine | `rt::Class` |
| `RelOp` | 1 | 25 | file_io | `rt::RelOp` |
| `ReportStmt` | 1 | 20 | report | `ReportOp` |
| `Sorting` | 1 | 10 | sort | `SortPlan` |
| `Item` | 1 | 8 | machine | The item's dims, the only field read |

The payloads (`CicsCommand`, `SqlEntry`, `HostPlace`, `Sqlca`, `CallPlan`, `FunctionPlan`,
`InvokePlan`, `SortPlan`, `ReportOp`, `FileOp`, `FileDesc`, `Advance`, and the INSPECT, STRING and
UNSTRING plans) and `Step` are lir.md's (§8.1, §9), shared as C6 describes. The interpreter builds
them from the AST at each call; lowering builds them once. This document defines only:

| Type | Shape |
|---|---|
| `Loc` | Today's private `Loc` (machine.rs:69-75) made public: `offset`, `len`, `kind: Kind`. Today's also carries the layout item index, which only resolution reads, so it stays with the interpreter; the TRUNC(OPT) report takes the receiver's name instead |
| `Val` | Today's private `Val`: bytes, national, `Fixed`, `Hfp`, figurative, `ALL`, address |
| `ProgramFacts` | The part of a program the library reads: `ProgramOptions`, `Storage` and the edit pictures (lir.md §4), the `FileDesc`s, and the program id. The interpreter builds it from `Compiled`; the VM reads the same fields of the LIR `Program` |

### 4.2 What method bodies reach

The 33 body-only leaks read instead: `FileDesc` for `program.files[k]`; `Storage` for
`program.using`, `returning`, `layout.linkage_roots` and every VALUE; `ProgramFacts` for
`program.id` and `layout.edits`; the evaluated object for `Item.depending_on`; `Sqlca` for the
SQLCA fields found by name; `Loc`s in `rt::report::Report` for report offsets; and `rt`'s
`LineNumber`, `NextGroup`, `OpenMode`, `Advance`, `Figurative` and `SignClause` (§4.3).

### 4.3 Shared vocabulary

**The types live in `rt`.** The library, the LIR and the load module all name them, and `rt` may
not depend on `syntax`, so `rt` holds the one definition of each:

- `Pos` (`syntax::Pos`, crates/syntax/src/lib.rs:17): `{ file: u16, line: u32, col: u32 }`, in
  `Abend`, every `pos` parameter and the debug table;
- `Kind` (exec/src/layout.rs:11) and `Sym` (exec/src/picture.rs:11-32);
- the `syntax::ast` enums that hold no `Ref`, `Expr` or `Stmt`: `Figurative`, `SignClause`,
  `SignPosition`, `OpenMode`, `Organization`, `Access`, `InspectMode`, `RelOp`, `BinOp`, `Class`,
  `AcceptFrom`, `ArgMode`; and from `le-cics`'s `syntax::report`, `LineNumber`, `ColumnNumber`,
  `NextGroup`, `Footing` and `ControlName`, into `rt::report`;
- the CICS condition table (DRY-4), the JNI table (`syntax/src/jni.rs`) and the BMS model (§5.2).

`Advancing` is not vocabulary, since its `Lines` holds an `Expr`: `rt` has lir.md's `Advance`.
`ast::Literal` needs no `rt` copy: lowering parses numeric literals, and VALUE clauses reach the
runtime only as `Storage.image` (lir.md §4).

**Recommended: `syntax` depends on `rt`** and re-exports each type with `pub use`, so every
`syntax::ast::Figurative` path still resolves and no call site changes. codegen-runtime.md §6
forbids only the reverse. The alternative, `syntax` keeps its own definitions and lowering converts
them into `rt`'s, is open question 1: about 400 lines of copies (11 enums, `Pos`, two tables and
nine BMS types), `From` impls, tests that the copies agree, and a second RESP table, which DRY-4
removes; DFHRESP would stay a parser fold over its own table, or become a FUNCTION that `Check`
validates.

## 5. The `rt` module layout

### 5.1 The crates after step 1

| Crate | Holds | Depends on |
|---|---|---|
| `zarch` | Unchanged | none |
| `numeric` | Unchanged | `zarch` |
| `rt` | Everything in §5.2 marked `rt` | `numeric`, `zarch` |
| `syntax` | Unchanged apart from the re-exports of §4.3 | `rt` |
| `compile` | `layout.rs`, `Compiled`, `compile`, `Check`, PICTURE analysis, the `host_type` binding; later, lowering | `syntax`, `rt` |
| `exec` | The interpreter: `Machine` as the walker, the `ExecBlock` and statement adapters, the program loader, the test harness. Kept as `--interpret` (C1) | `compile`, `syntax`, `rt` |
| `cli` (`ironwork`) | The driver | `exec`, `compile`, `syntax`, `rt` |

### 5.2 Files: whole, split or stay

`codegen` line counts, with `le-cics` in brackets where they differ.

| File | Lines | Goes to | What changes |
|---|---:|---|---|
| `cics.rs` | 465 | `rt/cics.rs`, whole | `Task.mapsets` holds `rt::bms::Mapset`; `unit::civil` becomes `calendar`; condition names become `Condition` (DRY-4) |
| `codec.rs` | 47 | `rt/codec.rs`, whole | `SignClause`, `SignPosition` from `rt` |
| `edit.rs` | 201 | `rt/edit.rs`, whole | `Sym` from `rt::picture` |
| `files.rs` | 575 | `rt/files.rs`, whole | `OpenMode` from `rt`; status codes become `FileStatus` (DRY-3) |
| `strings.rs` | 155 | `rt/strings.rs`, whole | `InspectMode` from `rt` |
| `terminal.rs` | 545 | `rt/terminal.rs`, whole | none |
| `tn3270.rs` | 406 | `rt/tn3270.rs`, whole | none. It implements `cics::Terminal`; the spec's §6 does not name it (C2) |
| `sql/` (8 files) | 2212 | `rt/sql/`, whole | With it, `postgres/wire.rs`'s `Stream` and `Tls` traits (from f2afb52), whose rustls implementation the separate `tls/` build supplies; it imports them as `exec::sql::{Stream, Tls}` and names `rt::sql` after E7. `mod.rs:85` `host_type(&Layout, item)` and `:109` `structure` leave for `compile`; `fingerprint` moves in from `syntax/src/sql.rs:140`; `convert.rs:8` and `mod.rs:15` take `SignClause` from `rt` |
| `picture.rs` | 216 | Split | `Sym` (lines 11 to 32) to `rt/picture.rs`. `Category`, `Picture`, `analyse`, `edited` and `runs` to `compile/picture.rs` |
| `layout.rs` | 455 (513) | `compile/layout.rs` | `Kind` (lines 11 to 44) to `rt/kind.rs`. `Item`, `Condition`, `Layout`, `build`, `resolve` stay: `Item.depending_on` and `keys` hold `Ref`, `Item.value` holds `Literal` |
| `lib.rs` | 655 (633) | Split | `Compiled`, `compile`, `Check`, `section_end`, `procedure`, `FUNCTIONS` to `compile`. `Compiled::run*`, `execute*` (lines 122 to 231) to `exec`, which builds the `RunUnit` and the first `Machine` |
| `machine.rs` | 2075 (2134) | Split by §3.1 | `Abend`, `Ending`, `Loc`, `Val`, and the free functions `figurative_byte`, `figurative_unit`, `fixed`, `literal_fixed`, `pow10`, `align`, `compare_fixed`, `places_of`, `zoned_digits`, `utf16_text`, `compare_national`, `days_from_civil`, `days_in_month`, `numval` to `rt`. `Flow`, `Machine`, `flatten_and`, `key_term` stay |
| `machine/cics.rs`, `cics_bms.rs`, `cics_files.rs`, `cics_services.rs` | 1167 | `rt/cics/` (the services), `exec/src/cics_bind.rs` (the resolution methods) | Each service takes a `CicsCommand`, which `cics_bind.rs` builds from an `ExecBlock` |
| `machine/file_io.rs` | 542 (547) | `rt/fileio.rs`; `conclude` stays | Methods take `FileOp` and `FileDesc` and return a `FileStatus` |
| `machine/sql.rs` | 625 | `rt/sql/host.rs` and `rt/sql/run.rs`; the adapter stays | E11d. About 330 lines are tests |
| `unit.rs` | 237 (240) | Split | `RunUnit`, `Loaded`, `Clock`, `ADDRESS_BASE`, `RETURN_CODE`, `MAX_DEPTH`, `push_temporary`, `release_temporaries`, `close_all`, `return_code`, `now` to `rt/unit.rs`. `Library`, `LoadError`, `load`, `search_libraries`, `member_name` to `exec/src/loader.rs`. `civil` to `rt/calendar.rs` |
| `tests.rs` | 1467 (1521) | Stays in `exec` | Runs through the one harness (DRY-6) |
| `le.rs` (`le-cics`) | 686 | `rt/le.rs`, whole | Imports only `numeric` and `zarch`. Its calendar code becomes `calendar` (DRY-5). `le/tests.rs` splits: unit tests to `rt`, program tests to `exec` |
| `oo.rs` (`le-cics`) | 758 | Split | Lines 14 to 93 (`Objects`, `LoadedClass`, `Instance`, `ClassCode`, `Part`, `MethodCode`) to `rt/oo.rs`, generic over the program handle. Lines 95 to 757 (class compilation, `Check`, `Rules`) to `compile`. `find_class` (line 728) becomes a `Loader` method |
| `report.rs` (`le-cics`) | 971 | Split | Lines 14 to 183 (`state`, `Writer`, `Report`, `Group`, `Field`, `Sum` and the rest) to `rt/report.rs`, generic over the expression handle `X` of `FieldContent::Source`, `Origin::Source` and `Subtotal.operand`, with `span` (line 885) and `generate_target` (line 920), which `machine/report.rs:144, 393` call at run time. The rest (`Draft`, `prepare`, `resolve`, `check_statement`) to `compile` |
| `sort.rs` (`le-cics`) | 204 | `compile/sort.rs` | It is `Check` and register synthesis only |
| `machine/sort.rs` lines 1 to 146 (`le-cics`) | | `rt/sort.rs` | `Active`, `Key`, `Entry`, `KeyValue`, `order`, `float_order`, `dfsort_decimal` |
| `machine/le_services.rs`, `oo.rs`, `report.rs`, `sort.rs` (`le-cics`) | 2248 | `rt` (the services) | Take `CallPlan`, `SortPlan`, `InvokePlan` and `ReportOp` |
| `syntax/src/jni.rs` (`le-cics`) | | `rt/jni.rs` | Read by `machine/oo.rs:105, 111, 457` and by the JNI copybook in `syntax/src/system.rs` |
| `syntax/src/bms.rs` | 994 | Split | The model (`Mode`, `Initial`, `Protection`, `Intensity`, `Attrb`, `Field`, `Map`, `Mapset`, lines 10 to 88) and the run-time slot layout (`Slot`, `slots`, `extended_attributes`, lines 593 to 650) to `rt/bms.rs`. The parser, `find_mapset`, `picture_size` and `symbolic_map` stay (the spec's D4) |

The `rt` modules that result:

| Module | Holds | From |
|---|---|---|
| `vocab`, `Pos` | The shared vocabulary of §4.3 and `Pos` | `syntax::ast`, `syntax` |
| `kind`, `value`, `loc` | `Kind`, `Val`, `Loc`, and the location checks (subscript, OCCURS DEPENDING ON, reference modification, run-unit bound) over evaluated integers | `layout.rs`, `machine.rs` |
| `fixed`, `arith`, `codec`, `edit`, `picture`, `strings`, `calendar`, `intrinsic` | Number helpers, the arithmetic core, PACKED and ZONED reads, editing, `Sym`, STRING, UNSTRING and INSPECT bytes, dates, the FUNCTION table | `machine.rs`, `codec.rs`, `edit.rs`, `picture.rs`, `strings.rs`, `unit.rs` |
| `abend` | `Abend`, `AbendCode`, `Signal`, `FileStatus`, `Ending`, `Step` | `machine.rs`, `file_io.rs` |
| `unit`, `callee` | `RunUnit<'w, H>`, `Loader<H>`, `Activation`, `ProgramFacts`; the run-callee and USING code | `unit.rs`, `machine.rs` |
| `files`, `fileio` | The file store and the file verbs | `files.rs`, `machine/file_io.rs` |
| `cics/`, `bms`, `terminal`, `tn3270` | Task, handlers, conditions, commands, file control, services, map model and slots, terminals | `cics.rs`, `machine/cics*.rs`, `terminal.rs`, `tn3270.rs`, `syntax/src/bms.rs` |
| `sql/` | `Session`, `Database`, `convert`, `Replay`, `Recorder`, `Postgres` with `Stream` and `Tls`, `host`, `run`, `fingerprint` | `sql/`, `machine/sql.rs`, `syntax/src/sql.rs` |
| `le`, `sort`, `report`, `oo`, `jni` (`le-cics`) | LE services, the sort engine, the report writer, objects and classes, the JNI table | `le.rs`, `machine/*.rs`, `report.rs`, `oo.rs`, `syntax/src/jni.rs` |

### 5.3 The `RunUnit` fields

`RunUnit` (unit.rs:57) becomes `RunUnit<'w, H>`, where `H` is the executor's handle to a loaded
program: `Rc<Compiled>` in the interpreter, a module handle in the VM. `rt` never looks inside `H`.

| Field | Goes to | Note |
|---|---|---|
| `mem`, `depth`, `programs`, `names` | `rt` | `Loaded.compiled: Option<Rc<Compiled>>` becomes `code: Option<H>`; `Loaded`'s other fields are plain |
| `dds`, `sysin`, `clock`, `out`, `err` | `rt` | |
| `cics`, `eib`, `cics_files` | `rt` | |
| `sql` (`codegen`) | `rt` | `Session` is in `rt/sql` |
| `le` (`le-cics`) | `rt` | `le::State` |
| `oo` (`le-cics`) | `rt` | `Objects<H>`: `LoadedClass.code` held an `Rc<ClassCode>` that holds `Compiled` |
| `library: Library` | `exec` | It holds `Vec<Program>`, search directories, `copy::Libraries` and flags. `rt` gets `Box<dyn Loader<H>>` instead |

`Loader<H>` has two methods: `program(&mut self, name) -> Result<LoadedProgram<H>, LoadError>`,
where `LoadedProgram` carries the handle, the program id, its storage size and its file count; and
`mapset(&mut self, name) -> Option<Result<Mapset, String>>`, which replaces
`bms::find_mapset(self.unit.copy_libraries(), name)` in `cics_bms.rs:45`. The VM's loader reads
load modules ([load-module.md](load-module.md) §8.2).

### 5.4 The `Machine` fields

`Machine` is at machine.rs:89 (`le-cics`: 92).

| Field | Goes to |
|---|---|
| `options`, `ssrange`, `page`, `me`, `base`, `linkage`, `local_base`, `main`, `cics_handlers`, `unit` | `rt::Activation`, which `Machine` holds and derefs to |
| `oo: Frame`, `sort: Option<Active>` (`le-cics`) | `rt::Activation` |
| `report_writer` (`le-cics`) | `rt::ProgramFacts` |
| `program: &Program`, `layout: &Layout`, `resolved` | Stay in `Machine` |

### 5.5 The dependency rule and the `syntax::` imports

**Rule.** `rt` depends on `numeric` and `zarch`, and on nothing else. `numeric` depends on `zarch`,
and `zarch` on nothing. `syntax` may depend on `rt` (§4.3); the reverse is a build error and a test
failure (§6).

Every import of `syntax::` in a file that moves, and how it goes away. `codegen` lines; `le-cics`
where they differ or exist only there.

| Where | Import | Kind | How it goes away |
|---|---|---|---|
| `lib.rs:22-23` | `syntax::ast::*`, `Error`, `Pos` | AST types | `lib.rs` splits; the `compile` half keeps them. `Pos` is `rt::Pos` |
| `lib.rs:582` | `syntax::sql::{Sql, Statement::Malformed}` | AST type | In `Check`, which moves to `compile` |
| `layout.rs:6-7` | `DataEntry`, `Literal`, `SignClause`, `Usage`, `Error`, `Pos` (`le-cics` adds `Environment`, `FileDecl`, `Organization`, and `syntax::ast::Figurative` at line 348) | AST types | `layout.rs` moves to `compile`. `SignClause` and `Organization` are `rt`'s |
| `layout.rs:59, 64` | `syntax::ast::Ref` in `Item.depending_on` and `Item.keys` | AST type | Stays in `compile::Item`. `rt` never reads it: the executor evaluates the object (lir.md §5.4), and the LIR's item table holds item indices (lir.md §4) |
| `strings.rs:3`, `codec.rs:5`, `files.rs:13`, `sql/convert.rs:8`, `sql/mod.rs:15` | `InspectMode`, `SignClause`, `SignPosition`, `OpenMode` | vocabulary | `rt` (§4.3) |
| `cics.rs:147` | `syntax::bms::Mapset` | copybook model | `rt::bms::Mapset` |
| `machine/cics_bms.rs:8` | `bms::{Initial, Intensity, Map, Protection}`, and `bms::Attrb`, `bms::slots`, `bms::find_mapset` in the body | copybook model and parser | The model and `slots` are `rt::bms`. `find_mapset` becomes `Loader::mapset` |
| `machine/cics.rs:262` | `syntax::system::resp_code` | copybook table | `rt::cics::Condition::resp`, one table (DRY-4). `syntax/src/parser.rs:1652` and `syntax/src/system.rs:172` read `rt` |
| `machine/sql.rs:6` | `syntax::sql::{Action, ChangeKind, HostVar, Statement, Whenever}` | AST types | `SqlEntry`, `HostPlace` and WHENEVER branches (lir.md §9.7); the adapter that builds them stays in `exec` |
| `sql/replay.rs:14` | `syntax::sql::fingerprint` | parser helper | Moves to `rt/sql` (about 20 lines) |
| `unit.rs:11-12`, `:168` | `ast::Program`, `copy`, `parse_all_with` | AST type, parser call | `Library` and `load` move to `exec`. `RunUnit` sees only `Loader<H>` |
| `machine.rs:13-14` | `syntax::Pos`, `syntax::ast::*` | AST types | `Machine` keeps its imports. Every moved method drops them (§4) |
| `machine/sql.rs:357, 552, 599, 604, 613, 616`; `sql/mod.rs:142, 146`; `sql/postgres/mod.rs:232` | `syntax::parse`, `syntax::Pos`, `syntax::sql::fingerprint` | test code | The tests run through the harness (DRY-6) and live in `exec`; the fingerprint test uses `rt::sql::fingerprint` |
| `le-cics` `oo.rs:11-12, 468, 748-749` | `ast::*`, `Error`, `Pos`, `report::Content`, `copy::decode`, `parse_all_with` | AST types, parser calls | The compile half moves to `compile`. The class search becomes a `Loader` method |
| `le-cics` `report.rs:10-12`, `sort.rs:6-7` | `ast::*`, `report::*`, `Error`, `Pos` | AST types | The compile halves move to `compile` |
| `le-cics` `machine/oo.rs:105, 111, 457` | `syntax::jni::{RESERVED, FUNCTIONS, function}` | copybook table | `rt/jni.rs`; the JNI copybook reads it from `rt` |
| `le-cics` `machine/report.rs:9` | `LineNumber`, `NextGroup`, `ReportStmt` | AST types, vocabulary | `LineNumber` and `NextGroup` to `rt::report`. `ReportStmt` becomes `ReportOp` |

`le-cics` files with no `syntax::` import that move whole: `le.rs`, `edit.rs`, `terminal.rs`,
`tn3270.rs`, and the `Sym` half of `picture.rs`.

## 6. The boundary test

`crates/cli/tests/boundary.rs` reads Cargo manifests with `std` only. It lives in `cli`, which sees
every crate; a test inside `rt` would read its siblings' manifests, which a packaged crate does not
have. The second test proves the first can fail. Run with `rustc --test` against copies of the
`numeric` and `zarch` manifests and a stand-in `rt` manifest, it passes for the allowed set and
names the offending line when `syntax` is added to `rt`.

```rust
//! The runtime never depends on the compiler: `rt`, `numeric` and `zarch` reach no other ironwork
//! crate than the ones below. Reads Cargo manifests only; no dependency, no crate source.

use std::fs;
use std::path::Path;

/// Per runtime crate: its directory under `crates/`, the packages its `[dependencies]` may name,
/// and the packages its `[dev-dependencies]` may name. `[build-dependencies]` may name none.
const ALLOWED: &[(&str, &[&str], &[&str])] = &[
    ("zarch", &[], &[]),
    ("numeric", &["ironwork-zarch"], &[]),
    ("rt", &["ironwork-numeric", "ironwork-zarch"], &["ironwork-oracle"]),
];

/// What no runtime crate may reach, by package name and by directory: the parser, the compiler,
/// the driver, and the interpreter.
const FORBIDDEN_PACKAGES: &[&str] = &["ironwork-syntax", "ironwork-compile", "ironwork", "ironwork-exec"];
const FORBIDDEN_DIRS: &[&str] = &["syntax", "compile", "cli", "exec"];

const KINDS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

#[derive(Debug, PartialEq, Eq)]
struct Dependency {
    kind: &'static str,
    key: String,
    package: String,
    path: Option<String>,
}

/// The string a key is set to in an inline table: `quoted(r#"{ path = "../x" }"#, "path")`.
fn quoted(inline: &str, key: &str) -> Option<String> {
    let after = inline.split(&format!("{key} =")).nth(1).or_else(|| inline.split(&format!("{key}=")).nth(1))?;
    let start = after.find('"')? + 1;
    let end = start + after[start..].find('"')?;
    Some(after[start..end].to_owned())
}

/// The dependency kind a table header holds, as in `dependencies` or `target.'cfg(unix)'.dev-dependencies`.
fn kind_of(header: &str) -> Option<&'static str> {
    KINDS.iter().copied().find(|k| header == *k || header.ends_with(&format!(".{k}")))
}

/// The kind and name of a header that is one dependency's own table, as in `dependencies.syntax`.
fn own_table(header: &str) -> Option<(&'static str, &str)> {
    KINDS.iter().copied().find_map(|kind| {
        let marker = format!("{kind}.");
        let at = header.find(&marker)?;
        let starts_a_key = at == 0 || header.as_bytes()[at - 1] == b'.';
        starts_a_key.then(|| (kind, &header[at + marker.len()..]))
    })
}

fn dependencies(manifest: &str) -> Vec<Dependency> {
    let mut found: Vec<Dependency> = Vec::new();
    let (mut kind, mut in_own_table) = (None, false);
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']').trim();
            in_own_table = false;
            kind = kind_of(header);
            if let Some((k, name)) = own_table(header) {
                found.push(Dependency { kind: k, key: name.to_owned(), package: name.to_owned(), path: None });
                in_own_table = true;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if in_own_table {
            let Some(last) = found.last_mut() else { continue };
            match key {
                "package" => last.package = value.trim_matches('"').to_owned(),
                "path" => last.path = Some(value.trim_matches('"').to_owned()),
                _ => {}
            }
        } else if let Some(kind) = kind {
            let package = quoted(value, "package").unwrap_or_else(|| key.to_owned());
            found.push(Dependency { kind, key: key.to_owned(), package, path: quoted(value, "path") });
        }
    }
    found
}

/// Each way `manifest` breaks the rule for the crate whose allowed packages are given.
fn breaches(manifest: &str, allowed: &[&str], allowed_dev: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for d in dependencies(manifest) {
        let dir = d.path.as_deref().and_then(|p| p.trim_end_matches('/').rsplit('/').next()).unwrap_or("");
        let permitted = match d.kind {
            "dependencies" => allowed.contains(&d.package.as_str()),
            "dev-dependencies" => allowed_dev.contains(&d.package.as_str()),
            _ => false,
        };
        if FORBIDDEN_PACKAGES.contains(&d.package.as_str()) || FORBIDDEN_DIRS.contains(&dir) || !permitted {
            out.push(format!("{}: {} = {} (path {})", d.kind, d.key, d.package, d.path.as_deref().unwrap_or("none")));
        }
    }
    out
}

#[test]
fn the_runtime_reaches_no_compiler_crate() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut failures = Vec::new();
    for (dir, allowed, allowed_dev) in ALLOWED {
        let path = crates.join(dir).join("Cargo.toml");
        let manifest = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for breach in breaches(&manifest, allowed, allowed_dev) {
            failures.push(format!("crates/{dir}/Cargo.toml: {breach}"));
        }
    }
    assert!(failures.is_empty(), "the runtime depends on the compiler or on a crate outside its rule:\n{}", failures.join("\n"));
}

#[test]
fn the_check_sees_every_form_a_manifest_can_name_a_dependency_in() {
    let allowed: &[&str] = &["ironwork-numeric", "ironwork-zarch"];
    let named = |manifest: &str| breaches(manifest, allowed, &[]).len();
    assert_eq!(named("[dependencies]\nnumeric = { package = \"ironwork-numeric\", path = \"../numeric\" }\n"), 0);
    assert_eq!(named("[dependencies]\nsyntax = { package = \"ironwork-syntax\", path = \"../syntax\", version = \"0.1.1\" }\n"), 1);
    assert_eq!(named("[dependencies]\nparser = { package = \"ironwork-syntax\", version = \"0.1.1\" }\n"), 1);
    assert_eq!(named("[dependencies]\nsomething = { path = \"../compile\" }\n"), 1);
    assert_eq!(named("[dependencies.syntax]\npackage = \"ironwork-syntax\"\npath = \"../syntax\"\n"), 1);
    assert_eq!(named("[dev-dependencies]\nexec = { path = \"../exec\" }\n"), 1);
    assert_eq!(named("[target.'cfg(unix)'.dependencies]\ncli = { path = \"../cli\" }\n"), 1);
    assert_eq!(named("[build-dependencies]\nnumeric = { path = \"../numeric\" }\n"), 1);
    assert_eq!(named("[dependencies]\nlibc = \"0.2\"\n"), 1);
}
```

It also forbids `exec`, so the runtime cannot reach the interpreter. `rt` may take the oracle as a
dev-dependency and nothing else; `numeric` and `zarch` have no dev-dependencies today.

## 7. The held DRY findings

Each is a duplicate that would otherwise be moved twice, or a string that would otherwise be typed
at every new `rt` call site.

| Id | Finding | Sites today | Target | Step |
|---|---|---|---|---|
| DRY-1 | One run-callee sequence for CALL, INVOKE and LINK | `machine.rs:998` `call_nested`; `machine/cics.rs:353` `cics_link`; `le-cics` `machine/oo.rs:386` `run_method` (`le-cics` `machine.rs:1055`) | `rt::callee::run(unit, index, bindings, run)`: mark storage, bind arguments and RETURNING, activate, run through the executor's closure, mark inactive, reset an INITIAL program, release temporaries, pass STOP RUN up. LINK resets `initialized` first and restores EIBCALEN; INVOKE restores RETURN-CODE and binds the object's data records; CALL resets an INITIAL program after | E11b |
| DRY-2 | One CALL USING address builder | `machine.rs:1001-1022` (in `call_nested`); `le-cics` `machine/le_services.rs:31` `le_arguments`, a copy of it; `machine/cics.rs:368-378` (COMMAREA); `le-cics` `machine/oo.rs:408` | `rt::callee::addresses(unit, &[Arg]) -> Vec<Option<usize>>`, with `content_argument` and `value_argument` as `Val`-to-bytes functions beside it | E11b |
| DRY-3 | Typed abend codes and file status | `Abend.code: String`, built from typed-out strings at about 20 sites in each checkout; about 70 file-status string literals in `machine/`; `file_io.rs:8` `meaning(&str)`; `cics_files.rs:19` `condition_of(&str)`. Control flow by code string: `DIVIDE_BY_ZERO` (`machine.rs:47`, `:1512`), `CLOSED_OUTPUT` (`machine.rs:49`; `cli/src/main.rs:223, 549`), `strip_prefix("S0C")` (`lib.rs:223`); in `le-cics`: `STOPPED` (`machine/sort.rs:502`), `STOP_RUN` and `GO_BACK` (`machine/report.rs:44-45`), `starts_with("IO-")` (`machine/sort.rs:398`) | `AbendCode` (`Check(ProgramCheck)`, `Io(FileStatus)`, `Cics(Condition)`, `User(String)`, `Ironwork`, `Exec`, `Sql`, `Java`, and a `Signal` for the internal ones: divide by zero, stop run, go back, sort stopped, closed output), which lir.md's `AbendText` holds. `FileStatus` with `as_str`, `meaning`, and the class tests `covers(char)` that `conclude` uses. `Display` prints exactly today's code text | E3 (inside `exec`); the types move to `rt` in E4 |
| DRY-4 | A CICS condition type with one table of RESP and default abend | `syntax/src/system.rs:155` `RESP` (94 names), `:172` `resp_code`; `machine/cics.rs:55` `default_abend` (36 names and the default `AEIP`); `cics_files.rs:19` `condition_of`; `cics.rs:175, 194, 201, 228` returning `&'static str` names; 28 `raise(block, "NAME", ..)` calls; `Handlers.conditions: HashMap<String, Handler>` at `machine/cics.rs:10`; `syntax/src/parser.rs:1652` folding DFHRESP | `rt::cics::Condition`, which lir.md's `Cics::HandleCondition` names: an enum, one `CONDITIONS` table of `(variant, name, resp, default_abend)`, and `resp()`, `default_abend()`, `name()`, `from_name()`. Aliases (FILENOTFOUND and DSIDERR are both 12) are two names for one variant: `from_name` takes both, `name` gives the first. An unlisted default abend stays `AEIP` | E6 |
| DRY-5 | One calendar module | `unit.rs:207` `civil` (`le-cics`: 210); `machine.rs:2028` `days_from_civil` and `:2036` `days_in_month` (`le-cics`: 2087, 2095); `le-cics` `le.rs:146` `days_from_civil`, `:160` `leap`, `:164` `days_in_month`, `:173` `lilian`, `:178` `weekday`; `cics.rs:270-346`, whose ABSTIME and DAYCOUNT constants (`2_208_988_800`) sit beside them | `rt::calendar`: `days_from_civil` (const), `civil`, `is_leap`, `days_in_month`, `lilian`, `weekday`, and the epoch offsets as named constants. Two copies of `days_from_civil`, two of `days_in_month` and three leap-year tests become one each | E2 (inside `exec`); the module moves to `rt` in E4 |
| DRY-6 | One test runner | `tests.rs:11` `run_with`, `:20` `run`, `:196` `run_files`, `:782` `run_unit`, `:1125` `run_cics`; `machine/sql.rs:356` `run`, `:551` `run_task`; `sql/postgres/mod.rs:231` `run`; in `le-cics`: `tests/sort.rs:399` `run_flagged`, `tests/report.rs:29` `run_report`, `tests/oo.rs:35` `run_oo`, `le/tests.rs:31` `run` and `:317` `run_cics`. Each is parse, compile, run with two buffers, return a tuple | `exec/src/testing.rs`, `#[cfg(test)]`: `Harness::source(text).flags(..).dds(..).sysin(..).clock(..).task(..).database(..).run(Executor)`, returning an `Outcome` with `out`, `err`, `ending`, `return_code` and `task`. `Executor` has `Interpreter` now and gains `Vm` in step 3, which is what B2 needs. The old helper names stay as one-line wrappers so no test body changes | E1 |

## 8. Extraction order

Leaf helpers first, then the services, then the `Machine` split. Each step ends with `cargo test
--workspace` and the oracle cases passing, and changes no output byte. A method that moves leaves a
one-line forwarding method on `Machine` until the step that moves its last caller deletes it.
**Size** is lines touched, from the inventory's body counts: S under 300, M 300 to 1500, L over 1500.

**Gates.**

- **G0.** Step 1 starts on the merged tree: `codegen` and `le-cics` together, with M8 done (§14 of
  the spec).
- **G1.** The SQL runtime's owner is told before E7 and again before E11d. Until then
  `crates/exec/src/sql/` and `machine/sql.rs` are theirs, and both move.

| Step | Work | Size | Done when | Folds in |
|---|---|---|---|---|
| E1 | **The test harness.** `exec/src/testing.rs`; every runner in §7 becomes a wrapper over it | S, about 250 | Tests unchanged and green | DRY-6 |
| E2 | **The calendar module** inside `exec`: `exec/src/calendar.rs`; delete the copies in `unit.rs`, `machine.rs`, `le.rs` | S, about 200 | Date tests green, including `civil_dates` and the Lilian tests | DRY-5 |
| E3 | **Typed abend and file status** inside `exec`: `AbendCode`, `Signal`, `FileStatus`; replace the string literals; keep `Display` byte-identical; `cli`'s four uses of `abend.code` (`main.rs:223, 240, 443, 549`) | M, about 500 | Every abend line the tests and the oracle read is unchanged | DRY-3 |
| E4 | **Create `rt` and the boundary test.** Move `calendar.rs`, `codec.rs`, `edit.rs`, `strings.rs`, the `Sym` half of `picture.rs`, `Abend`, `AbendCode`, `FileStatus`, `Ending`, `Pos`, and the vocabulary of §4.3. `syntax` gains its `rt` dependency and re-exports (open question 1) | M, about 800 | Boundary test passes; `cargo tree -p ironwork-rt` names `numeric` and `zarch` only | |
| E5 | **Storage types.** `Kind` from `layout.rs`, `Val`, `Loc`, and the free numeric helpers of `machine.rs` (about 110 lines) into `rt`. About 600 `use` edits | M, about 800 | Green | |
| E6 | **Files and CICS state, whole.** `files.rs`, `cics.rs`, `terminal.rs`, `tn3270.rs`; the BMS model and `slots`; the `Condition` table, which `syntax::system::resp_code` and the DFHRESP fold read. `le-cics` adds `le.rs` and `jni.rs` | L, about 2300 (about 3000 with `le-cics`) | Green; `syntax/src/system.rs` holds no RESP table | DRY-4 |
| E7 | **The SQL library.** `sql/` whole to `rt/sql/`, with `Stream` and `Tls`; `fingerprint` in; `host_type` and `structure` out to `compile`; `tls/` imports from `rt::sql`. **G1 first** | L, about 2300 | Green; SQL replay, Postgres and `tools/pg-tls-test.sh` tests pass in their new home | |
| E8 | **Create `compile`.** `layout.rs`, `Compiled`, `compile`, `Check`, PICTURE analysis. `exec` depends on it. `le-cics` adds the compile halves of `sort.rs`, `report.rs`, `oo.rs` | M, about 1200 (L, about 2900, with `le-cics`) | Green; `exec/Cargo.toml` names `compile` | |
| E9 | **The run unit.** Split `unit.rs`; `RunUnit<'w, H>`; `Loader<H>`; `Objects<H>`; `exec/src/loader.rs`. `execute*` move to `exec` | M, about 450 | Green; `rt` holds no `Library`, `Program` or `copy` | |
| E10a | **Storage and MOVE.** `bytes`, `write`, `read`, `decode`, `zoned_value`, `alnum_image`, `image_len`, `assign`, `store_value`, `store_fixed`, `store_fixed_checked`, `zoned_image`, `compare`, `class`, `set_integer`, `natural_bytes`; `rt::Activation`. `compare` and `class` take `Val` and `Loc` | M, about 500 | Green | |
| E10b | **Locations and activation.** The location checks split from `locate` into `rt::loc`, over evaluated integers; `occurrences`, `occurrence_offset`, `offset_of`, `nest`, `activation`; `Storage` (lir.md §4), whose image `initialize_values` builds once per program | S, about 250 | Green; `locate`, left in `Machine`, calls `rt::loc` | |
| E10c | **Statement semantics.** `string_stmt`, `unstring`, `inspect`, `set`, `function`, `accept`, `display` become the entry points for `StringPlan`, `UnstringPlan`, `InspectPlan`, the `SetAddress` and `SetUpDown` ops, `FunctionPlan`, `rt::accept` and `rt::display`. `numval` and the date functions use `calendar` | M, about 550 | Green; no `FunctionCall` in `rt` | |
| E10d | **Arithmetic core.** `arith::fixed_binop`, `float_binop`, `pow`, the divide-remainder rule, the size-error decision. `Machine::arithmetic`, `eval_fixed` and `eval_float` keep the `Expr` walk and the locate passes; `dmax` and `uses_float` are marked for lowering | S, about 250 | Green | |
| E11a | **File verbs.** `file_io.rs` to `rt/fileio.rs` with `FileOp`, `FileDesc`, `Advance`; `conclude` stays and reads the returned `FileStatus`. `le-cics`: `write_record`, the SORT file methods and the Report Writer writes use the same entry points | M, about 600 | Green | |
| E11b | **CALL, CANCEL, LINK, INVOKE.** `rt::callee` and `CallPlan`; `call_nested`, `cics_link`, `run_method` and `le_arguments` become callers of it | M, about 300 | Green; one copy of each sequence | DRY-1, DRY-2 |
| E11c | **EXEC CICS.** `CicsCommand`; `cics_bind.rs` builds it from an `ExecBlock`; the 51 services of the four `cics_*.rs` files move to `rt/cics/`, and the 13 resolution methods go to the adapter. `raise` takes a `cics::Condition` | L, about 1600 | Green; `rt/cics` names no `ExecBlock` | |
| E11d | **EXEC SQL.** `SqlEntry`, `SqlStatement`, `HostPlace`, `Sqlca`; `rt::sql::run` holds `sql`, `sql_call`, `sql_single_row`, `session`; `sql_inputs`, `sql_assign` and `sqlca` move as `rt::sql::host`; `sql_targets` and `whenever` are resolution, so the adapter in `exec` builds the `HostPlace`s and takes the WHENEVER branch; SYNCPOINT is a CICS service that calls `Session::settle`. **G1 first** | M, about 450 | Green | |
| E11e | **`le-cics` services.** LE services (`CallPlan`), SORT and MERGE (`SortPlan`), Report Writer (`ReportOp`, `Writer<X>`), the OO runtime (`InvokePlan`). One sub-step each | L, about 2200 | Green | |
| E12 | **Close.** Delete the forwarding methods and dead code in `Machine`; `exec` is the walker and its adapters; `cargo tree` and the boundary test pass; update `codegen-runtime.md` §6 with the real module list | S, about 250 | Step 1 of §14 is done | |

**Why this order.**

- E1 to E3 change `exec` from inside, so a failure has no crate boundary to blame.
- E4 to E7 move code with no `Machine` in it: each is a file move plus import edits, and the
  compiler finds every edit.
- E8 and E9 cut the two things `rt` must not hold: the compiler's `Layout` and `Check`, and the
  program library.
- E10 and E11 are the extraction proper, smallest and most shared first: storage before
  statements, statements before services, and services in order of how much they call each other
  (files, then CALL, then CICS, whose LINK uses the callee code). Both SQL steps wait on G1.

## 9. Choices

| Id | Choice | Why |
|---|---|---|
| C1 | `exec` stays the interpreter crate | The spec's §6 does not say what `exec` becomes; E12 records it there |
| C2 | `tn3270.rs` goes to `rt` with `terminal.rs` | It implements `cics::Terminal` and imports no `syntax`. Open question 2 asks whether the runtime exception should cover a network server |
| C3 | `RunUnit<'w, H>` is generic over the program handle, not `dyn Any` | One monomorphised copy per executor and no downcast. It puts `H` on `Objects` and on `Loaded` |
| C4 | `Writer<X>` in the Report Writer is generic over the expression handle | The interpreter's `X` is `Expr`; lowering's is an `ExprId` |
| C5 | The interpreter keeps `locate`, `resolve` and `procedure` | They are resolution, but the interpreter needs them until step 5. Lowering calls the same code from `compile` |
| C6 | The payloads are lir.md's types, which lir.md writes with `PlaceId`, `ExprId` and `ConstId`, made generic over those handles: the LIR's ids in the VM, the interpreter's own references in the walker. The library asks the executor for a handle's `Loc` or value where the walker would locate or evaluate it | One set of types, so lowering translates nothing; the library sees `Loc`s and values only; and abends and side effects keep the walker's order, as when the INTO host variables are located after the database call |

## 10. Open questions

1. **Shared vocabulary.** The types of §4.3 live in `rt`. Should `syntax` depend on `rt` and
   re-export them, as recommended, or keep its own definitions and have lowering convert them?
2. **tn3270 and the runtime exception.** Should the exception cover `tn3270.rs`, a network server,
   or should it stay in `exec` under the AGPL alone (C2)?
