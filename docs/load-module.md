# The load module

The `.iwm` file format, and how a run unit loads it. It details §8 of
[codegen-runtime.md](codegen-runtime.md) and serves invariants 6 and 7 of its §10.

**Status:** draft, for the operator's review. The container, the encoding rules and every section's
codec (§3 to §7, §9) are built in `rt::module`; `ironwork compile` writes modules and `ironwork
dump` (§11) prints them. The loader (§8.2) is not built: running a module needs the VM. The writer
always writes an empty `BMS` section, so a program that uses maps loses them (L2.5). The types a
module holds are [lir.md](lir.md)'s; this document gives the container, the encoding rules, which
apply to any of them, and the program directory.

## 1. Scope and constraints

- **One file per source file.** It holds every program of that source as lir.md's `Program`, and
  the BMS maps its programs use.
- **No dependency, no `unsafe`** (codegen-runtime.md §3). ironwork's own code writes and reads the
  format, with no serialisation crate. The reader works on `&[u8]` with checked slicing, and never
  casts a byte buffer to a struct.
- **Where it lives.** The reader, the codec traits and the types they encode live in `rt`. The
  container writer, `rt::module::ModuleWriter` (section order, string interning, checksums), lives
  in `rt` too, because no `compile` crate exists yet. The reader never calls it. Once `compile`
  exists it drives the writer: it chooses what each section holds, and `rt` lays the bytes out.
- **Untrusted input.** A `.iwm` may be truncated, corrupt or hostile. The reader bounds every count
  and index, never allocates from a length it has not checked (§4.8), and never panics.

## 2. Ubiquitous language

| Term | Meaning |
|---|---|
| Module | The bytes of one `.iwm` file, and `rt::module::Module`, those bytes with the header and section table checked |
| Section | A contiguous block of a module holding one kind of data |
| Program ordinal | A program's position in the directory, from 0, in source order with nested programs after their container (`syntax::parse_all_with`, lib.rs:77) |
| String table | The one table of strings every other section refers to by index |
| Canonical | Having exactly one byte encoding for a value, so equal values give equal bytes |

## 3. Container

### 3.1 Layout

    +----------------------+ 0
    | header (32 bytes)    |
    +----------------------+ 32
    | section table        |  section_count x 28 bytes
    +----------------------+
    | section bodies       |  in section-id order, no gaps, no padding
    +----------------------+ file_len

Header and table fields are fixed-width little-endian integers, because the reader must find the
sections before it can decode anything else. Section bodies use the rules of §4.

### 3.2 Header

| Offset | Size | Field | Rule |
|---|---|---|---|
| 0 | 8 | `magic` | `89 49 57 4D 0D 0A 1A 0A`: a high byte, `IWM`, then CR LF, SUB, LF. The high byte and the line-ending bytes make a text-mode transfer or a mistaken `cat` show up as a bad magic, as PNG's do |
| 8 | 2 | `major` | Format major version |
| 10 | 2 | `minor` | Format minor version |
| 12 | 4 | `features` | Bit set of required features. Zero in every version so far |
| 16 | 4 | `section_count` | Number of table entries |
| 20 | 8 | `file_len` | Total bytes. A shorter file is truncated, and a longer one has trailing bytes |
| 28 | 4 | `header_crc` | CRC-32 of bytes 0 to 27 followed by the whole section table, as one run |

### 3.3 Section table entry

| Offset | Size | Field | Rule |
|---|---|---|---|
| 0 | 4 | `id` | A section id (§3.4) |
| 4 | 4 | `flags` | Bit 0: optional, so a reader that does not know `id` skips it. Other bits are zero, and the reader refuses a set one |
| 8 | 8 | `offset` | From the start of the file |
| 16 | 8 | `length` | In bytes |
| 24 | 4 | `crc` | CRC-32 of the section's bytes |

- **Canonical table.** Entries ascend strictly by `id`. The first body begins where the table ends,
  and each later one where the previous ends, and the last ends at `file_len`, so no two writers
  can lay a module out differently. The table must fit inside the file. A reader rejects any other
  layout as malformed.
- **Flags and known sections.** A section this version knows (§3.4) is required, so the reader
  refuses one flagged optional. A section it does not know is skipped if flagged optional, and
  refused otherwise. Any flag bit other than bit 0 is refused.
- **Required sections.** Every section of §3.4 with a required "yes" must be in the table. The
  reader refuses a module that lacks one, naming the lowest id missing.
- **Checksum.** CRC-32, the IEEE polynomial (reflected, `0xEDB88320`, initial and final XOR
  `0xFFFFFFFF`): a 256-entry table built by a `const fn`, about fifteen lines in `rt`, with
  `extend` to continue a checksum over more bytes. It detects
  the burst errors and truncations of transit. It does not resist tampering (question 2).
- **Order of checks.** The reader stops at the first that fails:
  1. Magic. A file that is a prefix of the magic, or shorter than it, is compared as far as it goes.
  2. A file shorter than the 32-byte header is truncated. This comes before the version, which sits
     inside the header.
  3. Version (§8.1).
  4. `file_len`: a shorter file is truncated, and a longer one has trailing bytes.
  5. `header_crc`, over the header and the whole section table.
  6. `features`: any set bit is refused.
  7. The section table (§3.3): its size, order, layout, flags and ids, then the required sections.
  8. Each section's `crc`, when the reader first touches the section. A reader that wants only the
     directory need not checksum the LIR.

  A file that fails an earlier check reports it whatever a later field says, so a changed magic,
  version or `file_len` byte is not reported as a bad `header_crc`.

### 3.5 Errors

| Error | Raised when | Message |
|---|---|---|
| `NotAModule` | The magic differs (check 1) | `not an ironwork load module` |
| `Truncated` | Fewer than 32 bytes, or fewer than `file_len` (checks 2, 4) | `truncated: 100 bytes of 240` |
| `Version` | A version the reader does not read (check 3, §8.1) | `load module format 1.0; this ironwork reads 0.1. Compile the source again` |
| `TrailingBytes` | More bytes than `file_len` (check 4) | `4 bytes after the end of the module at 240` |
| `HeaderChecksum` | `header_crc` differs (check 5) | `header is corrupt (checksum 1234ABCD, expected 5678EF01)` |
| `Feature` | Any `features` bit is set (check 6) | `load module needs features 0x00000004, which this ironwork lacks` |
| `UnknownSection` | A section the reader does not know, not flagged optional (check 7) | `required section 0x9 is unknown to this ironwork` |
| `MissingSection` | A required section is absent (check 7) | `required section DEBUG is missing` |
| `SectionChecksum` | A section's `crc` differs (check 8) | `section LAYOUT is corrupt (checksum 1234ABCD, expected 5678EF01)` |
| `Malformed` | Anything else: a table or a body breaks a rule of §3 or §4 | `LAYOUT is malformed at byte 2: bool 7` |

- **`Malformed` reasons in the header and table** (section `section table`, offset from the start of
  the file): the table runs past the end; entries not strictly ascending; a section not beginning
  where the last ended, or running past the end; sections that end before `file_len`; a flag bit
  other than bit 0; a known section flagged optional.
- **`Malformed` reasons in a body** (offset from the start of the body): an integer over 64 bits,
  over-long, or cut short; a value that overflows its type; a `bool`, `Option` or `Result` byte
  other than 0 or 1; a surrogate or out-of-range `char`; a NaN other than the canonical one; a
  count larger than the bytes that remain; an index outside the string table; an unknown enum tag;
  map keys that do not strictly ascend; a `check` that fails; bytes left after the last value;
  and, in the string table, invalid UTF-8 and a text stored twice.
- **File name.** The reader's messages carry no file name. The caller puts it in front, as
  `X: `.

### 3.4 Sections

| Id | Name | Body | Required |
|---|---|---|---|
| 1 | `STRINGS` | The string table (§4.2) | yes |
| 2 | `DIRECTORY` | The program directory (§6) | yes |
| 3 | `OPTIONS` | Per program: `Program.options` (§5.1) | yes |
| 4 | `LAYOUT` | Per program: `Program.storage`, `items` and `edits` (§5.2) | yes |
| 5 | `LIR` | Per program: the rest of `Program` | yes |
| 6 | `SQL` | Per program: `Program.sql`, the SQL statement table (§7) | yes |
| 7 | `BMS` | The map models of the mapsets the module's programs use (§5.3) | yes |
| 8 | `DEBUG` | Per program: `Program.debug` (§9) | yes |
| 0x8000 up | reserved | Extension sections, written with flag bit 0 set | no |
| any other | unknown | Skipped if flagged optional, and refused otherwise (§3.3) | no |

- **Per program** means a count equal to the directory's program count, then one record per program
  in ordinal order. A section never repeats a program's name. A program's records are decoded
  together, on its first CALL.
- **Order of writing.** `ModuleWriter::section` takes the known sections in ascending id order and
  panics on any other order, and on `STRINGS`, which `finish` writes. `finish` panics if a required
  section is missing. `extension` takes an id from `0x8000` up and always sets the optional flag.
- **Always present.** A module with no SQL or no maps still carries the section, with zero records,
  so every module's section table has the same shape.
- **Debug is required.** Every abend names its COBOL position (codegen-runtime.md §10, invariant 3),
  so a module without a debug table could not abend correctly (question 3).

## 4. Encoding rules

Everything inside a section body is built from these rules and nothing else. A type is encodable if
its fields are, so encoding any type built from them is mechanical.

### 4.1 Integers

| Type | Encoding |
|---|---|
| `u8`, `bool` | One byte. A `bool` is 0 or 1, and any other byte is malformed |
| `u16`, `u32`, `u64`, `usize` | Unsigned LEB128: seven bits per byte, low group first, high bit set on all but the last byte |
| `i16`, `i32`, `i64` | Zigzag (`(n << 1) ^ (n >> bits-1)`), then unsigned LEB128 |
| `char` | Its scalar value as unsigned LEB128. A surrogate or a value over `0x10FFFF` is malformed |
| `f64` | Its bit pattern as eight little-endian bytes, with any NaN written as `0x7FF8000000000000`. The reader refuses any other NaN. No encoded type holds a float today |

- **Why LEB128.** Nearly every integer in a module is a small index, count, length or offset, so a
  variable width makes a module much smaller than fixed `u32`, with no alignment. Nothing needs to
  skip integers: the table indexes the sections, and a program is decoded whole.
- **Canonical only.** The reader rejects an over-long encoding (a final zero byte after a
  continuation, as `0x80 0x00`), a value of more than 64 bits, and a value that overflows the
  target type, so each value has one byte form.
- **Sizes.** `usize` is read as `u64` and checked to fit the target's `usize`.

### 4.2 Strings

- **By index.** A `String` field is a LEB128 index into the string table, which stores each text
  once. lir.md's `SymId`s index the program's `symbols`, whose entries are themselves string indices.
- **Bytes.** A `Vec<u8>` is a count then the raw bytes, and does not enter the string table. It
  holds binary data: a `Const::Bytes`, the storage image, a BMS `Initial::Bytes`.
- **Table body.** A count, then per string a byte length and its UTF-8 bytes. Invalid UTF-8 is
  malformed, and so is a text stored twice, since the table stores each text once. The empty string
  is an ordinary entry.
- **Order.** Strings are numbered by first use, in the fixed order the writer visits the sections
  (§3.4) and, within a section, the fields. The writer builds the section bodies first, interning as
  it goes, then writes the table, which sits first in the file.

### 4.3 Enums

- **Tag then fields.** A tag as unsigned LEB128, then the variant's fields in declaration order.
- **Explicit tags.** Each variant's tag is written where the type's codec is declared, not taken
  from its position. A tag is never reused or renumbered, and a removed variant's number stays
  retired. A new variant takes a new tag; a change to an existing one needs a major version (§8.1).
- **Unknown tag.** The reader stops with `Malformed`, naming the type and the tag.
- **Fieldless enums** (`Arith`, `Trunc`) are the same rule with no fields.

### 4.4 Composites

| Rust | Encoding |
|---|---|
| Struct | Its fields in declaration order, with no names, no length and no padding |
| Tuple | Its elements in order. Tuples of two, three and four elements are encodable |
| `[T; N]` | Its elements in order, with no count (the type gives `N`). The reader refuses an `N` larger than the bytes that remain |
| `Vec<T>` | A count, then the elements |
| `Option<T>` | One byte, 0 for `None` or 1 for `Some`, then the `T` for `Some`. Any other byte is malformed |
| `Result<T, E>` | One byte, 0 for `Ok` or 1 for `Err`, then the `T` or the `E`. Any other byte is malformed |
| `Box<T>` | Just the `T` |
| `BTreeMap<K, V>` | A count, then each `(K, V)` in ascending key order. The reader rejects an order that does not strictly ascend |
| `HashMap`, `HashSet` | Not encodable. No codec exists, so a type holding one does not compile (§10) |
| `&T`, `Rc<T>`, `Box<dyn Trait>`, `fn` | Not encodable. Lowering replaces the value with an index or a value |

A field derivable from another is still written, since a codec that skips fields is not mechanical,
and the reader checks the two agree.

### 4.5 The two traits

    // rt::module::codec
    pub trait Encode { fn encode(&self, w: &mut Writer); }
    pub trait Decode: Sized { fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError>; }

- **`Writer`** owns a `Vec<u8>` and the string interner. `w.string(&str)` writes the index. It has
  `leb`, `zigzag`, `byte`, `bytes` and `count`, and `take`, which returns the bytes written so far
  and keeps the string table.
- **`Reader`** borrows a `&[u8]`, and holds a position, the section's name for error messages, and a
  reference to the decoded string table. Every read is bounds-checked and returns an error instead
  of panicking. `finish` refuses bytes left after the last value.
- **`decode_all::<T>(section, bytes, strings)`** decodes one `T` that fills `bytes` exactly.
- **Foreign types.** `Encode` and `Decode` are `rt`'s traits, so `rt` may implement them for
  `numeric::options::Options`, `numeric`'s `Fixed` and `zarch::hfp::Precision` without breaking the
  orphan rule.

### 4.6 Hand-written or generated

**A small `macro_rules!` in `rt`, and hand-written code for the primitives only.**

- **One field list gives both directions.** Hand-written encoders and decoders must list the same
  fields in the same order, and a swapped pair round-trips against itself while being wrong against
  the format.
- **A new field cannot be forgotten.** The struct form expands to an exhaustive destructure
  (`let Options { arith, trunc, .., dbcs } = self;`, listing every field and with no `..`), and the
  enum form to an exhaustive `match`, so a new field or variant is a compile error until listed.
- **Not a proc-macro,** which is a separate crate and needs a Rust parser. `macro_rules!` covers
  structs with named fields and enums with unit, named-field and tuple variants in about sixty
  lines.

The macros are exported from `rt` as `codec_struct!` and `codec_enum!`. Their grammar:

    codec_struct!(Type { field, ... } [check path]);
    codec_enum!(Type { Variant [{ field, ... } | (elem, ...)] = tag, ... });

| Part | Rule |
|---|---|
| `Type { field, ... }` | The type's named fields, in the order they encode. Every field is listed, and a tuple struct is not accepted |
| `check path` | Optional. A `fn(&T) -> Result<(), String>` run on the decoded value, for a rule the shape cannot state (a code page the tables carry, §5.1). Its `String` becomes the reason of a `Malformed` at the byte where the value began. Without it, no check runs |
| `Variant` | A unit variant, written bare |
| `Variant { field, ... }` | A variant with named fields, encoded in the order listed |
| `Variant(elem, ...)` | A tuple variant, such as `Float(p)`. Each `elem` is a binding name for one position, encoded in order |
| `= tag` | An integer literal, unsigned LEB128 on the wire. Every variant has one; a variant has named fields or tuple elements, not both |
| Trailing comma | Allowed after the last field, element and variant |

Every field and element must itself be encodable (§4.1 to §4.4).

    codec_struct!(Options { arith, trunc, numproc, codepage, trunc_check, fastsrt, fastsrt_adv_print,
        sort_keys, adv, thread, dll, rent, dbcs, warnings, compile, dynam, debug, cics_return_warning,
        invdata, zwb, quote, currency, nsymbol, dispsign, intdate, qualify, initial, vlr, vsamopenfs,
        numcheck, parmcheck, initcheck, optimize } check options_valid);
    codec_enum!(Arith { Compat = 0, Extend = 1 });
    codec_enum!(Kind {
        Group = 0,
        Alnum { justified } = 1,
        Zoned { digits, scale, signed, sign } = 3,
        Float(precision) = 6,
    });

Hand-written code is confined to the integer, `bool`, `char`, `f64`, string, `Vec`, `Option`,
`Result`, `Box`, tuple, array and `BTreeMap` implementations in `codec.rs`, about 270 lines without
tests, and the debug positions (§9).

### 4.7 A worked example

`Options` with `arith: Extend` and `trunc: Opt`, and every other field at `Options::default()`:

| Field | Value | Bytes |
|---|---|---|
| `arith` | tag 1 | `01` |
| `trunc` | tag 1 | `01` |
| `numproc` | tag 0 | `00` |
| `codepage` | 1140 as LEB128 (`0x474`) | `F4 08` |
| `trunc_check` | tag 0 | `00` |
| `fastsrt` | false | `00` |
| `fastsrt_adv_print` | `Exclude`, tag 0 | `00` |
| `sort_keys` | `Dfsort`, tag 0 | `00` |
| `adv` | true | `01` |
| `thread` | false | `00` |
| `dll` | false | `00` |
| `rent` | true | `01` |
| `dbcs` | true | `01` |
| `warnings` | `Proceed`, tag 0 | `00` |
| `compile` | `None` | `00` |
| `dynam` | false | `00` |
| `debug` | false | `00` |
| `cics_return_warning` | `Once`, tag 0 | `00` |
| `invdata` | `None` | `00` |
| `zwb` | true | `01` |
| `quote` | `Quote`, tag 0 | `00` |
| `currency` | `None` | `00` |
| `nsymbol` | `National`, tag 0 | `00` |
| `dispsign` | `Compat`, tag 0 | `00` |
| `intdate` | `Ansi`, tag 0 | `00` |
| `qualify` | `Compat`, tag 0 | `00` |
| `initial` | false | `00` |
| `vlr` | `Standard`, tag 0 | `00` |
| `vsamopenfs` | `Compat`, tag 0 | `00` |
| `numcheck` | `None` | `00` |
| `parmcheck` | `None` | `00` |
| `initcheck` | `None` | `00` |
| `optimize` | 0 as LEB128 | `00` |

Thirty-four bytes: `01 01 00 F4 08 00 00 00 00 01 00 00 01 01 00 00 00 00 00 00 01 00 00 00 00 00 00
00 00 00 00 00 00 00`.

### 4.8 Bounds on decoding

- **A count is at most the bytes that remain.** Every element takes at least one byte, so a larger
  count is malformed. A four-byte file cannot ask for four billion elements.
- **An index is checked where the type is decoded:** a string index against the string count, an
  item index against `Program.items`, a program ordinal against the directory. Indices into the
  LIR's own tables are checked by the LIR verifier (lir.md §12.2) when a program is first decoded.
- **Depth.** The LIR nests by index, not by `Box`, so no encoded type recurses; one added later is
  decoded with a depth limit of 64.
- **Trailing bytes.** A section whose decode ends before its `length` is malformed.

## 5. Options, storage and maps

Line numbers are for the tree at 79a199e, except those of `crates/numeric/src/options.rs`, which are
for the tree that added `Options::vsamopenfs`.

### 5.1 Options

`Program.options` is lir.md's `ProgramOptions`: `numeric::options::Options`
(crates/numeric/src/options.rs:373-421), which is `Copy`, holds only enums, options, integers, a
`char` and bools, and encodes as it is, its fields in declaration order; then `ssrange`, the option cards, the
collating sequence, `decimal_point_comma` (a bool) and `numval_currency` (a string), the last two
from SPECIAL-NAMES (lir.md §9.11), and `when_compiled`. Options are per program, because a CBL
card is.

`when_compiled` is an `Option` of lir.md's `CompileTime`, the time FUNCTION WHEN-COMPILED gives:
`Some` in a program that uses WHEN-COMPILED, and `None` in any other, so that only such a program's
module depends on when it was compiled (§10). A class definition's methods and data are programs of
their own, each `Some` only when it uses the function. `CompileTime` is `seconds` since
1970-01-01T00:00:00Z as zigzag LEB128, `hundredths` as LEB128, and `source`, a tag:
`SourceDateEpoch` 0, `Clock` 1. Its `check` refuses seconds outside 0 to 253402300799
(9999-12-31T23:59:59Z), hundredths over 99, and hundredths other than 0 from `SourceDateEpoch`. The
compiler takes it from the build's SOURCE_DATE_EPOCH when set, and from the clock otherwise.

The spellings a card or PARM may use come from IBM's option table, vendored as
`crates/numeric/data/enterprise-options.tsv` and read by `Options::apply`. Six fields have no IBM
compiler option: `trunc_check`, `fastsrt_adv_print`, `sort_keys`, `warnings`, `debug` and
`cics_return_warning` are set by this compiler's own flags.

| Field | Type | Encoding | Set by |
|---|---|---|---|
| `arith` | `Arith` (:54) | tag: `Compat` 0, `Extend` 1 | `ARITH`, `AR`, with `COMPAT`, `C`, `EXTEND` or `E` |
| `trunc` | `Trunc` (:84) | tag: `Std` 0, `Opt` 1, `Bin` 2 | `TRUNC(STD\|OPT\|BIN)` |
| `numproc` | `Numproc` (:92) | tag: `Nopfd` 0, `Pfd` 1 | `NUMPROC(NOPFD\|PFD)` |
| `codepage` | `u16` | LEB128. The `check` function refuses a CCSID `CodePage::by_ccsid` does not carry, because `Options::code_page` (:816) would otherwise panic | `CODEPAGE(n)`, `CP(n)` |
| `trunc_check` | `TruncCheck` (:102) | tag: `Report` 0, `Silent` 1 | `-silent` |
| `fastsrt` | `bool` | 0 or 1 | `FASTSRT`, `FSRT`, and `NOFASTSRT`, `NOFSRT` |
| `fastsrt_adv_print` | `FastsrtAdvPrint` (:122) | tag: `Exclude` 0, `Include` 1 | `--fastsrt-adv-print=exclude\|include` |
| `sort_keys` | `SortKeys` (:112) | tag: `Dfsort` 0, `Strict` 1 | `-strict-sort-keys` |
| `adv` | `bool` | 0 or 1 | `ADV`, `NOADV` |
| `thread` | `bool` | 0 or 1 | `THREAD`, `NOTHREAD` |
| `dll` | `bool` | 0 or 1 | `DLL`, `NODLL` |
| `rent` | `bool` | 0 or 1 | `RENT`, `NORENT` |
| `dbcs` | `bool` | 0 or 1 | `DBCS`, `NODBCS` |
| `warnings` | `Warnings` (:326) | tag: `Proceed` 0, `Block` 1 | `-warnings-block` |
| `compile` | `Option<Compile>` (:335) | `None`, or `Some` then the tag: `Full` 0, `Until` 1 followed by the `Stop` (:346) tag (`W` 0, `E` 1, `S` 2), `SyntaxOnly` 2. `None` when no card gives the option; `Options::object_code` (:809) resolves it with `warnings` (assumption C47) | `COMPILE`, `C`, and `NOCOMPILE`, `NOC`, alone or with `(W)`, `(E)` or `(S)` |
| `dynam` | `bool` | 0 or 1 | `DYNAM`, `DYN`, and `NODYNAM`, `NODYN` |
| `debug` | `bool` | 0 or 1 | `-debug`, the Language Environment runtime option DEBUG |
| `cics_return_warning` | `CicsReturnWarning` (:214) | tag: `Once` 0, `Always` 1, `Never` 2. What a program with no STOP RUN, GOBACK or EXIT PROGRAM that ends with EXEC CICS RETURN or XCTL gets (assumption C124) | `--cics-return-warning=once\|always\|never` |
| `invdata` | `Option<Invdata>` (:142) | `None` for NOINVDATA, or `Some` then `forcenumcmp` and `cleansign` as bools | `INVDATA`, `INVD`, with `FORCENUMCMP`, `FNC`, `NOFORCENUMCMP`, `NOFNC`, `CLEANSIGN`, `CS`, `NOCLEANSIGN`, `NOCS`; `NOINVDATA`, `NOINVD`; `ZONEDATA(PFD\|NOPFD\|MIG)`, `ZD`, as INVDATA's equivalents |
| `zwb` | `bool` | 0 or 1 | `ZWB`, `NOZWB` |
| `quote` | `Quote` (:234) | tag: `Quote` 0, `Apost` 1. The figurative constant QUOTE's character | `QUOTE`, `Q`, `APOST` |
| `currency` | `Option<Currency>` (:261) | `None`, or `Some` then the tag: `Char` 0 followed by the `char`, `Hex` 1 followed by the byte. `Options::currency_symbol` (:822) reads a `Hex` byte in the program's code page | `CURRENCY(literal)`, `CURR(literal)`, and `NOCURRENCY`, `NOCURR` |
| `nsymbol` | `Nsymbol` (:269) | tag: `National` 0, `Dbcs` 1 | `NSYMBOL`, `NS`, with `NATIONAL`, `NAT` or `DBCS` |
| `dispsign` | `DispSign` (:279) | tag: `Compat` 0, `Sep` 1 | `DISPSIGN`, `DS`, with `COMPAT`, `C`, `SEP` or `S` |
| `intdate` | `IntDate` (:288) | tag: `Ansi` 0, `Lilian` 1 | `INTDATE(ANSI\|LILIAN)` |
| `qualify` | `Qualify` (:297) | tag: `Compat` 0, `Extend` 1 | `QUALIFY`, `QUA`, with `COMPAT`, `C`, `EXTEND` or `E` |
| `initial` | `bool` | 0 or 1 | `INITIAL`, `NOINITIAL` |
| `vlr` | `Vlr` (:307) | tag: `Standard` 0, `Compat` 1 | `VLR`, with `STANDARD`, `S`, `COMPAT` or `C` |
| `vsamopenfs` | `VsamOpenFs` (:317) | tag: `Compat` 0, `Succ` 1 | `VSAMOPENFS`, `VS`, with `COMPAT`, `C`, `SUCC` or `S` |
| `numcheck` | `Option<Numcheck>` (:158) | `None` for NONUMCHECK, or `Some` then `zon` (`Option` of `ZonCheck`: `alphnum` and `lax` as bools), `pac` as a bool, `bin` (`Option` of `BinCheck`: `truncbin` as a bool) and `abd` as a bool | `NUMCHECK`, `NC`, with `ZON`, `NOZON`, `PAC`, `NOPAC`, `BIN`, `NOBIN`, `MSG` or `ABD`, ZON taking `ALPHNUM`, `NOALPHNUM`, `LAX` or `STRICT` and BIN `TRUNCBIN` or `NOTRUNCBIN`; `NONUMCHECK`, `NONC`; `ZONECHECK(MSG\|ABD)`, `ZC`, as NUMCHECK(ZON,MSG\|ABD), and `NOZONECHECK`, `NOZC` |
| `parmcheck` | `Option<Parmcheck>` (:195) | `None` for NOPARMCHECK, or `Some` then `abd` as a bool and `bytes` as LEB128 | `PARMCHECK`, `PC`, with `MSG` or `ABD` and a size from 1 to 9999; `NOPARMCHECK`, `NOPC` |
| `initcheck` | `Option<Initcheck>` (:204) | `None` for NOINITCHECK, or `Some` then the tag: `Lax` 0, `Strict` 1 | `INITCHECK`, `IC`, with `LAX` or `STRICT`; `NOINITCHECK`, `NOIC` |
| `optimize` | `u8` | LEB128, 0 to 2. The `check` function refuses any other level. Under NOINVDATA a level above 0 compares some zoned items by their bytes (assumption C262) | `OPTIMIZE(0\|1\|2)`, `OPT(n)`; `NOOPTIMIZE` as 0, and `OPTIMIZE`, `OPTIMIZE(STD)` and `OPTIMIZE(FULL)` as 2 (Programming Guide SC27-8714-03, Table 51, p. 395) |

`ADV`, `APOST`, `DBCS`, `DLL`, `INITIAL`, `INTDATE`, `NUMPROC`, `RENT`, `THREAD`, `TRUNC` and `ZWB`
have no abbreviations. The defaults are `Compat`, `Std`, `Nopfd`, 1140, `Report`, false, `Exclude`,
`Dfsort`, true, false, false, true, true, `Proceed`, `None` (IBM's default NOCOMPILE(S) in force),
false, false, `Once`, `None` (NOINVDATA), true, `Quote`, `None` (NOCURRENCY), `National`, `Compat`,
`Ansi`, `Compat`, false, `Standard`, `Compat`, `None` for NONUMCHECK, NOPARMCHECK and
NOINITCHECK, and 0.

### 5.2 Storage and the item table

The `LAYOUT` section holds lir.md's `Storage`, `Item` table and edit pictures, each an `Edit` of its
symbols and the currency sign value it shows, which encode by §4 with these tags:

- **`Kind`** (layout.rs:11-26, an `rt` type) tags in declaration order: `Group` 0,
  `Alnum{justified}` 1, `National` 2, `Zoned{digits,scale,signed,sign}` 3,
  `Packed{digits,scale,signed}` 4, `Binary{digits,scale,signed,native}` 5, `Float(Precision)` 6,
  `NumericEdited{edit,digits,scale,blank_when_zero}` 7, `AlnumEdited{edit}` 8, `Pointer` 9,
  `Index` 10. `edit` is checked against `edits`.
- **`Precision`** (crates/zarch/src/hfp.rs:10) is `Short` 0, `Long` 1, `Extended` 2.
- **`Sym`** (exec/src/picture.rs:12-32) has fourteen variants, four of them carrying a `char`
  (`FloatLead`, `Float`, `Sign`, `Insert`), and encodes as an enum.

The runtime may not depend on `syntax`, so the compile-side `Layout` (crates/exec/src/layout.rs:93-105)
is not encoded. Its AST fields reach the module as follows, and no encoded type names `syntax::`:

| Compile's field | In the module |
|---|---|
| `Item.depending_on: Option<ast::Ref>` | `Item.depending_on: Option<u32>`, the object's item index. The places that depend on it evaluate the object as the walker does (lir.md §5.4) |
| `Item.keys: Vec<(bool, ast::Ref)>` | `Item.keys: Vec<(bool, u32)>`, ascending flag and item index. SEARCH ALL matches WHEN terms to keys at lowering, where the walker matches by name (machine.rs:916) |
| `Item.value`, `Condition.values`: `ast::Literal` | Not encoded: VALUE reaches the module as `Storage.image`, and 88-level values as constants of `Cond::Name` |
| `Kind::Zoned.sign: Option<ast::SignClause>`, `Item.pos: syntax::Pos` | `rt::SignClause`; `Item.at`, a debug id |

`SignClause`, `Pos`, `Kind` and `Sym` are `rt`'s (semantics-library.md §4.3).

### 5.3 BMS map models

The map model (crates/syntax/src/bms.rs) holds owned strings, integers, bools and enums only, and
encodes as it is. `rt` owns the types (codegen-runtime.md §6, D4; semantics-library.md §5.2).

| Type | Fields (line) | Encoding |
|---|---|---|
| `Mapset` (:81) | `name: String`, `mode: Mode`, `ctrl: Vec<String>`, `maps: Vec<Map>` | in order |
| `Map` (:68) | `name`, `lines`, `columns`, `line`, `column: u16`, `ctrl: Vec<String>`, `tioapfx: bool`, `dsatts: Vec<String>`, `fields: Vec<Field>` | in order; `u16` as LEB128 |
| `Field` (:47) | `name: Option<String>`, `line`, `column`, `length: u16`, `attrb: Attrb`, `initial: Option<Initial>`, `picin`, `picout: Option<String>`, `occurs: u16`, `group: Option<String>`, `justify_right`, `fill_zero: bool`, `color`, `hilight: Option<String>` | in order |
| `Attrb` (:37) | `protection`, `numeric`, `intensity`, `detectable`, `cursor`, `fset` | in order |
| `Mode` (:10) | `In` 0, `Out` 1, `InOut` 2 | tag |
| `Protection` (:23) | `Askip` 0, `Prot` 1, `Unprot` 2 | tag |
| `Intensity` (:30) | `Norm` 0, `Brt` 1, `Drk` 2 | tag |
| `Initial` (:17) | `Text(String)` 0, `Bytes(Vec<u8>)` 1 | tag, string index or raw bytes |

- **Not encoded:** `Slot` (:593) and `slots`, which place a field in the symbolic map. SEND MAP and
  RECEIVE MAP call `slots` at run time (machine/cics_bms.rs:110, :200), so it runs on the decoded
  model and no offset is stored. Nor is `cics::Task.mapsets` (cics.rs:147), a run-time cache.
- **Order.** The section holds each mapset the module's programs use once, sorted by name in ASCII
  order; names are unique in a module. lir.md's `MapId` is a mapset's position here and a map's
  within it.
- **Where it comes from.** Compilation reads `NAME.bms` from the COPY libraries with `find_mapset`
  (bms.rs:90), so the module needs no map source at run time.

## 6. The program directory

The `DIRECTORY` section is the module's table of contents. The reader parses it first, and
`RunUnit` answers name lookups from it without decoding any program.

```rust
pub struct DirectoryEntry {
    /// PROGRAM-ID exactly as written.
    pub id: String,
    /// The program ordinal of the containing program.
    pub parent: Option<u32>,
    pub common: bool,
    /// ENTRY names and the paragraphs they enter.
    pub entries: Vec<(String, ParaId)>,
    /// USING: true for BY VALUE, in order.
    pub params: Vec<bool>,
    pub returning: bool,
    /// Visible to a dynamic CALL (§8.3).
    pub dynamic: bool,
}
```

- **`id` is written as it stands in the source**, because the SQL recording identity uses it
  unchanged (machine/sql.rs:153). Lookups compare case-insensitively, as `RunUnit::add` and `load`
  do (unit.rs:111, :144). Where two programs share an `id`, the lowest ordinal wins, as the first
  match does today (question 6).
- **`params` and `returning`** are `Program.using` (ast.rs:16, :28) reduced to what a CALL checks.
  INITIAL, RECURSIVE and the entry paragraph are in the program's LIR. **`entries`** is empty until
  the parser reads ENTRY.
- **`parent` and `common`** need the parser to record them: it flattens nested programs
  (parser.rs:264-290), so `ast::Program` gains `parent: Option<usize>` and `common: bool`.
- **`dynamic`** is true for every program, since today's search finds nested programs by name too
  (question 6).
- **Main** is ordinal 0, the first top-level program, which `parse_with` returns (lib.rs:72).
  `ironwork run x.iwm` starts it.

## 7. The SQL statement table

The `SQL` section holds each program's `Program.sql`, the table lir.md §9.7 defines: one `SqlEntry`
per EXEC SQL block by ordinal, holding the block's command word, the typed statement, the canonical
text, its fingerprint and WITH HOLD. WHENEVER is not in it: lowering emits it as branches in the
LIR. It is a section of its own because the SQL runtime ([sql-runtime.md](sql-runtime.md)) reads it
and no other. A recording made by the interpreter answers a module, and the reverse, because the
identity `PROGRAM:ORDINAL:HASH` uses only `DirectoryEntry.id`, `ordinal` and `fingerprint`
(sql/replay.rs:40); a recording writes the fingerprint in eight hex digits.

The reader refuses a module whose SQL table breaks lir.md's definition:

- **Ordinals** that do not run densely from 1, so entry *k* has ordinal *k*.
- **A fingerprint** that differs from `fingerprint(text)`, 32-bit FNV-1a with offset basis
  `0x811C9DC5` and prime `0x01000193`, which the reader recomputes.
- **WITH HOLD** set other than on an OPEN whose `text` says `WITH HOLD`, or an OPEN whose `text`
  says it with the flag clear.
- **A `HostPlace`** whose type is `HostType::Structure`, which lowering expands.

`Statement::Malformed` never reaches a module, since the compiler refuses it; the writer returns an
error if it meets one. `HostType::Zoned`'s sign is `rt::SignClause`.

## 8. Versioning, loading and CALL

### 8.1 Versions

The format version is `major.minor`, starting at `0.1`.

| The reader finds | It does |
|---|---|
| Bad magic | Refuses: `X: not an ironwork load module` |
| A different `major` | Refuses: `X: load module format 1.0; this ironwork reads 0.1. Compile the source again`. A reader of major 1 or more names `1.x` |
| The same `major`, a lower `minor` | Reads it. A minor version only adds, and a section body's shape never changes inside a major (new data goes in a new section) |
| The same `major`, a higher `minor` | Reads it, ignoring sections with the optional flag it does not know. Refuses on an unknown required section or a set `features` bit, naming it |
| `major` 0 | Requires the same `minor` as well. The format is not frozen until 1.0 |

- **No older readers.** A new major version does not keep the last one's reader: the source is the
  durable artefact, and compiling again is the remedy (question 1).
- **A major bump** is needed for any change to an existing encoding, tag or section body, and any
  change to the LIR that lir.md marks as breaking.
- **No compiler version is recorded.** Two compilers that produce the same LIR produce the same
  module; a version string would make every upgrade change every module.

### 8.2 Loading

`RunUnit::load` (unit.rs:136) finds the program a CALL names, in this order:

1. **A loaded module.** `RunUnit::find` (unit.rs:131) looks today in the names of programs already
   added. It now looks in the directories of the modules already read, by upper-cased `id`, over
   the programs whose `dynamic` is set. A program found here is added to the run unit (given its
   storage) if it is not yet.
2. **`NAME.iwm` in the `-L` directories**, in their order, as `NAME.iwm` then `name.iwm`, as
   `search_libraries` tries both cases (unit.rs:159). The module is read, checksummed and its
   directory registered, then step 1's lookup runs against it. Every `-L` directory is searched for
   `.iwm` before any is searched for source.
3. **Source, compiled in memory**, exactly as today: `Library.programs`, then the directories for
   `NAME`, `.cbl`, `.CBL`, `.cob`, `.COB` (unit.rs:144, :164).

- **The name check comes first.** `member_name` (unit.rs:77) refuses a name outside the member
  character set before any path is built, so `CALL '../X'` never reaches the filesystem.
- **Storage is allocated on a program's first CALL,** in call order by `RunUnit::add` as now, so
  addresses, which programs can observe as pointers, are those of interpreted programs. Two modules
  holding one top-level `id`: the first registered wins.
- **A module that fails** (unreadable, corrupt, the wrong version, or holding no program `NAME`) is
  not "not found": `LoadError` gains `Module(String)`, which abends as `Compile` does
  (machine.rs:984), outside ON EXCEPTION. CICS LINK draws the same line: `NotFound` raises PGMIDERR
  and the rest abend (machine/cics.rs:359-362).
- **Shadowing.** A stale `NAME.iwm` beside newer source is used, because step 2 precedes step 3,
  and the format carries no time to compare: the compile time a program using WHEN-COMPILED holds
  is that function's value, not a build stamp, and the loader does not read it (question 4).

### 8.3 Static and dynamic CALL

Today every CALL is dynamic: `Machine::call` (machine.rs:974) reads the name, calls `unit.load`, and
resolves by a flat name map (`RunUnit.names`, unit.rs:62). `Library.programs` holds the first
source's other programs, nested or not, and a match removes the first of that `id` (unit.rs:144).
Nesting and COMMON are not modelled.

| CALL | Compile option | Resolved | By |
|---|---|---|---|
| `CALL 'X'` | NODYNAM | Compile time | The first program of the module named `X`, to a program ordinal |
| `CALL 'X'` | DYNAM | Run time | `RunUnit::load` (§8.2) |
| `CALL ID` (identifier) | either | Run time | `RunUnit::load` |

- **An unresolved NODYNAM literal**, naming no program of the module, is compiled as a call by
  name, resolved at run time as under DYNAM, with a compile warning. IBM would fail at link time; a
  module cannot link (question 5).
- **IBM's scope rule** would resolve a static CALL from *P* to `X` among the programs directly
  contained in *P*, then, walking outward, among those contained in each enclosing program where a
  match counts only if it is COMMON, and finally among the module's top-level programs; a nested
  program that is not COMMON would be unreachable from outside its container and by dynamic CALL.
  That changes what runs today, so it is question 6.
- **A bundle.** `ironwork compile A.cbl B.cbl -o out/ --bundle x` reads several sources into one
  module with one directory, and a static CALL resolves across them. Debug file names keep each
  program's source (§9).
- **Shared memory.** A static callee shares the run unit's memory and keeps its WORKING-STORAGE
  between CALLs, as a dynamic one does.

### 8.4 CANCEL, ON EXCEPTION and S806

- **CANCEL** (machine.rs:380, `cancel` at :1111) looks the program up with `RunUnit::find`, so it
  reaches only programs already loaded. It does nothing for a name not loaded, abends if the
  program is active, closes its files, and clears `initialized`, so the next CALL initialises
  WORKING-STORAGE again from `Storage.image`. The module is data, and does not change. Whether IBM
  lets CANCEL reach a statically called program is question 7.
- **Not found** is `LoadError::NotFound`: no loaded module, no `NAME.iwm`, no source. A CALL with ON
  EXCEPTION runs that block, and one without abends S806 with today's message (machine.rs:978-983).
- **A LINK or XCTL** through EXEC CICS gets PGMIDERR for the same case.

## 9. The debug table

The `DEBUG` section holds each program's `Program.debug` (lir.md §10). `sources`, `ops` and
`statements` encode by §4, in that order with `positions` between `sources` and `ops`.
`positions` is written in debug-id order, each position as its difference from the one before
(file, line and column as zigzag LEB128), so a run of positions from one statement costs a few
bytes each. The reader decodes a program's table on its first abend, not on load.

### 9.1 Source names

`sources` holds no absolute path and no path relative to where the compiler was started.

- **The main source** is its file name alone, `PAYROLL.cbl`, which is what B1's abend line shows.
- **A COPY member** is its path relative to the library directory that supplied it, with `/` as
  separator: `CUSTREC.cpy`, `sys/SQLCA.cpy`. The directory itself is not recorded.
- **Why.** The same source compiled from `/build/a` as `src/PAYROLL.cbl` and from `/build/b` as
  `PAYROLL.cbl` must give one module.
- **The cost.** Two sources of a bundle with one file name, in different directories, are not told
  apart in an abend; the position and the program name still are. `--source-prefix` adds a
  directory to the name, written as given, and is one of the inputs that decide the bytes
  (question 8).
- **Rejected input.** A source or library-relative name with a `..` component, a leading `/`, or a
  Windows drive letter is a compile error, since it would put a path into the module.

## 10. Reproducibility

Invariant 6 says the same source, libraries and options give a byte-identical module. It holds
because:

1. **Nothing depends on time, path, host, compiler version or a random seed** (§8.1, §9.1), with
   one exception: a program that uses FUNCTION WHEN-COMPILED holds its compile time (§5.1), which
   is the build's SOURCE_DATE_EPOCH when set, the reproducible-builds convention, and the clock's
   otherwise. A module is reproducible with no environment unless one of its programs uses
   WHEN-COMPILED; then the build sets SOURCE_DATE_EPOCH.
2. **Nothing iterates unordered.** The codec has no `HashMap` or `HashSet` (§4.4). The maps in the
   tree are run-time caches, never encoded: `RunUnit.names` (unit.rs:62), `RunUnit.cics_files`
   (:72), `Machine.resolved` (machine.rs:95), `cics::Task.mapsets` (cics.rs:147). Lowering collects
   the maps it builds as it works (a label table, a picture cache) into a `Vec` or a `BTreeMap`.
3. **Every value has one form** (§3.3, §4.1, §4.2, §4.3). Set-like lists are sorted (the mapsets,
   by name); where order carries meaning (programs, items, paragraphs), source order is kept.
4. **The inputs are named:** the source bytes, every COPY member and BMS file the compile read (by
   content), the options, including `-L` (which decides what a static CALL can resolve),
   `--source-prefix` and the option cards, and, for a program using WHEN-COMPILED,
   SOURCE_DATE_EPOCH.

A test compiles every corpus program twice in separate processes and compares the bytes, and a
second compiles once from two working directories. One that runs today (lower/tests.rs) compiles a
program without WHEN-COMPILED at two compile times and finds the modules identical, and one with it
and finds them different.

## 11. `ironwork dump`

    ironwork dump [--section NAME]... [--strings] [--no-check] file.iwm

- **Output** is text, one fact per line, in section order, and stable: no address, timestamp or
  path, so tests compare it as text. A program's `when_compiled` prints with its options; it is
  present only where the program uses WHEN-COMPILED, and fixed by SOURCE_DATE_EPOCH.
- **What it prints.** The version and file length. The section table with each section's id, name,
  offset, length and whether its checksum matched. The directory (id, ordinal, parent, COMMON,
  dynamic). Each program's options. The item table (level, name, offset, size, occurs, kind, ODO
  item, keys). Each program's SQL entries as `PAYROLL:3:9f2a41c0 SELECT ...`, the identity a
  recording uses. The BMS maps. The LIR, through its printer (lir.md §12.2). The debug table as
  `#12 PAYROLL.cbl:47:12`.
- **Strings** print only with `--strings`, since every other section prints its strings inline.
- **Checksums.** A bad section prints `CHECKSUM MISMATCH`, and the dump exits non-zero after
  printing what it can; `--no-check` prints regardless. A section whose body will not decode prints
  the byte offset and reason, and the rest are still printed.
- **A test tool.** Round-trip tests compare `dump` text before and after writing, which shows a
  difference in words where a byte comparison shows an offset.
- **Where it lives.** In `cli`, on `rt`'s reader, with no compiler crate.

## 12. Specification (BDD)

### L1: Round trip

- **Given** every test program of `exec/src/tests.rs`, compiled to a module **when** the module is
  read and written again **then** the bytes are identical, **and** `dump` of the two agree.
- **Given** any value of every codec type, generated by a fixed sequence **when** it is encoded and
  decoded **then** it equals the original, **and** a value with a NaN encodes as the canonical NaN.
- **Given** `Options` with `arith: Extend`, `trunc: Opt` and every other field at its default
  **then** it encodes as `01 01 00 F4 08 00 00 00 00 01 00 00 01 01 00 00 00 00 00 00 01 00 00 00 00
  00 00 00 00 00 00 00 00`.
- **Given** an OCCURS DEPENDING ON table **when** the module loads **then** the item holds the
  object's item index, **and** the module contains no `Ref`.

### L2: Reproducibility

- **Given** `PAYROLL.cbl`, which does not use WHEN-COMPILED, and the same libraries and options
  **when** it is compiled twice, in two processes, with no SOURCE_DATE_EPOCH **then** the two
  `.iwm` files are byte-identical.
- **Given** a program that uses WHEN-COMPILED **when** it is compiled twice with one
  SOURCE_DATE_EPOCH **then** the modules are byte-identical; **given** none **then** its
  `when_compiled` is the clock's, `source` `Clock`, and the modules differ in `OPTIONS` only.
- **Given** the same source compiled from two working directories **then** the modules are
  byte-identical.
- **Given** a copy of the source under another directory **then** the module is byte-identical.
- **Given** a BMS mapset used by two programs **then** the `BMS` section holds it once, in name
  order.
- **Given** an option changed, such as `TRUNC(BIN)` **then** the modules differ, and only in
  `OPTIONS`, in `STRINGS` (which holds the option card's text) and in whatever the option changes
  in `LIR`.

### L3: Version mismatch

- **Given** a module whose `major` is higher than the reader's **when** it is run **then** the run
  stops with `X: load module format 1.0; this ironwork reads 0.1. Compile the source again`, and
  exit status is non-zero, **and** no program runs.
- **Given** a module with a higher `minor` and an unknown optional section **then** it runs, and the
  section is ignored. **Given** an unknown required section **then** it is refused, naming the
  section.
- **Given** a file that does not start with the magic **then** `X: not an ironwork load module`.
- **Given** a module with a `features` bit set **then** it is refused, naming the bits.
- **Given** a `major` 0 reader and a module with another `minor` **then** it is refused.

### L4: Corruption

- **Given** a module with one byte of the `LAYOUT` section changed **when** it is loaded **then** it
  is refused with `X: section LAYOUT is corrupt (checksum 1234ABCD, expected 5678EF01)`.
- **Given** a module truncated anywhere, including to fewer than 32 bytes **then** it is refused as
  truncated, never a panic.
- **Given** a `features`, `section_count`, `header_crc` or section-table byte changed **then** it is
  refused as a bad header checksum. **Given** a magic, version or `file_len` byte changed **then**
  it is refused by that field's own check.
- **Given** a string table that stores one text twice, a non-canonical NaN, a known section flagged
  optional, a flag bit other than bit 0, or a module without a required section **then** it is
  refused.
- **Given** a section whose checksum was recomputed after a count was set beyond the remaining
  bytes **then** it is refused as malformed, and no memory is allocated from the count.
- **Given** a `SQL` entry whose stored fingerprint differs from its text **then** the module is
  refused.
- **Given** `dump` of a module with one bad section **then** it prints `CHECKSUM MISMATCH` for that
  section, prints the others, and exits non-zero.

### L5: A missing program, and CALL

- **Given** `MAIN.iwm` calling `SUB` with no `SUB` in a loaded module, in `-L` as `.iwm`, or as
  source **when** the CALL has ON EXCEPTION **then** that block runs, **and** without it the run
  abends S806.
- **Given** a `SUB.iwm` that holds no program `SUB` **then** the run abends naming the module, and
  ON EXCEPTION does not run.
- **Given** `SUB.iwm` and `SUB.cbl` in one `-L` directory **then** the module is used.
- **Given** a source of three programs, one nested in the second **when** a dynamic CALL names the
  nested one **then** it is not found. **Given** a static CALL from its container **then** it runs.
  (These two hold once question 6 adopts the scope rule; until then the nested program is found.)
- **Given** a module of two programs A and B and a DYNAM CALL from A to B **then** B is added to
  the run unit on that CALL and not before, **and** its storage address is the one the interpreter
  gives it.
- **Given** SUB CANCELled and CALLed again **then** its WORKING-STORAGE has its `value`s again,
  **and** the module's bytes have not changed.

### L6: SQL replay

- **Given** a recording made by `ironwork run PAYROLL.cbl --sql-record rec.txt` **when**
  `ironwork run PAYROLL.iwm --sql-replay rec.txt` runs **then** every call matches by
  `PROGRAM:ORDINAL:HASH`, **and** the output, RETURN-CODE and storage equal those of
  `ironwork run PAYROLL.cbl --sql-replay rec.txt`.
- **Given** a program with a WHENEVER, a DECLARE CURSOR WITH HOLD and an INCLUDE before an INSERT
  **then** the module's SQL table holds one entry per block with dense ordinals, **and** the
  INSERT's ordinal is the one the interpreter reports.
- **Given** the entries of a module **then** each `fingerprint` equals `fingerprint(text)` and each
  `text` equals what the interpreter passes for that block.

### L7: The boundary

- **Given** the reader in `rt` **then** `rt`'s manifest depends on neither `syntax` nor `compile`,
  **and** no type it decodes names `syntax::`.
- **Given** the codec module **then** it contains no `unsafe`, and no `HashMap` implementation of
  `Encode`.

## 13. Out of scope

- **The LIR and its verifier,** which lir.md defines.
- **Signing and encryption.** The checksum detects damage and does not resist an attacker
  (question 2).
- **Linking against a load module from z/OS.** The format is ironwork's own.
- **Memory-mapping.** The reader copies what it decodes, so no alignment is guaranteed.
- **Partial loading.** A program is decoded whole on its first CALL.

## 14. Open questions

1. **Old modules.** Should a new major version keep a reader for the previous one? Estates keep load
   modules for years and recompile rarely; this document keeps none, and recompiling is the remedy.
2. **Integrity.** Is a checksum enough, or should a module carry a keyed signature (a hash written
   in-tree), so a shop can check that a module is the one it built?
3. **Stripping.** Should a `--strip-debug` module exist for size, with abends naming only the
   program and instruction? Invariant 3 forbids it as stated.
4. **Stale modules.** A `NAME.iwm` beside a newer `NAME.cbl` is used, its compile time not compared.
   A compile that fails leaves an existing `NAME.iwm` in place, as a failed compile on z/OS leaves
   the old member in the load library, so the loader would run the older module. Is that right, or
   should the `-L` search prefer source, or the newer file?
5. **Unresolved static CALL.** A NODYNAM literal naming no program of the module compiles as a
   run-time call, with a warning, where IBM fails at link time. Should compiling fail instead,
   unless the caller passes `--allow-unresolved`? And where the name is an LE service, should a
   static CALL bind the service at compile time, or still let a program of that name found at run
   time win, as assumption L1 `LE_SERVICE_AFTER_PROGRAMS` (int) records?
6. **Program scope.** Adopt IBM's rules, each a change from today: a nested program that is not
   COMMON reachable only from its container and never by dynamic CALL (`dynamic` false), static
   CALL resolved by the scope rule of §8.3, and a duplicate `id` a compile error? If so, how far
   does COMMON reach: siblings of its container only, or every program contained anywhere in its
   container? That needs an Enterprise COBOL run, and is recorded as a V-series assumption until
   then.
7. **CANCEL of a static callee, and of a nested program.** This document keeps today's behaviour:
   reset for a loaded program, nothing for one not registered by name. Does Enterprise COBOL agree?
8. **Directory in abend lines.** Is a bare `PAYROLL.cbl` in an abend acceptable, or should the
   default keep a path relative to the compile invocation, at the price that a module depends on
   where it was built?
