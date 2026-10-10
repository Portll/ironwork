//! The LIR as text (lir.md §13): a program's generated code as `ironwork dump` prints it, for any
//! tool that shows it. The same program prints the same bytes; nothing reads the text back.

use super::{
    AbendId, AbendText, Access, Advance, Argument, ArithId, ArithPlan, ArithStep, Base, Binding, Block, BlockId, Bound, CallArg, CallId, CallTarget, Ccsid,
    Chars, CicsId, Class, Comparand, Compare, Cond, CondId, Const, ConstId, Convert, ConvertTable, Count, Debug, DebugId, DisplayId, DisplayItem,
    Expr, ExprId, FileDesc, FileOpId, FileVerb, Flag, FloatFrom, FromMove, FunctionId, GlobalAt, Image, Indicator, InitValue, InspectId,
    InspectPhrase, Inspected, IntExpr, InvokeId, Item, JsonLeaf, JsonNode, JsonValue, Marker, MarkupId, MethodName, Mode, MovePlan, Named,
    NationalFrom, NumberInto, NumericFrom, Odo, Op, Operand, Organization, ParaId, Paragraph, ParseLeaf, ParseNode, ParseValue, Phrase, Place,
    PlaceId, Plans, Program, Range, RangeId, RangeKind, Receiver, Replacement, ReportOp, ReturnId, SearchAllId, Section, SenderCheck, Services,
    SetTo, SignTest, SortIo, SortKeys, SortPlan, Spacing, SqlEntry, SqlStatement, SqlTest, SqlcaField, StartKey, StartRel, StepPlan,
    StorePlan, StringId, SymId, Terminator, TrimSide, UnstringId, UpDown, UserArgument, UserFunctionId, XmlForm, XmlNode, XmlRegister,
    XmlValue,
};
use super::{Markup, ReleaseId, ScreenPlan, ScreenPosition, SortId, SqlId};
use crate::abend::Ending;
use crate::cics::{Cics, CicsCommand, Datum, Handles};
use crate::files::Format;
use crate::report::{Adding, Field, FieldContent, GroupKind, LineNumber, NextGroup, Origin};
use crate::sql::HostType;
use crate::storage::Kind;
use crate::store::LaxRedefinition;
use crate::vocab::{AcceptFrom, BinOp, Closing, Figurative, InspectMode, OpenMode, Pos, RelOp, SignClause, SignPosition};
use numeric::{Arith, Native};
use numeric::precision::Fixed;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;
use zarch::ebcdic::CodePage;
use zarch::hfp::Precision;
use zarch::wide::U256;

/// What a program's LIR section holds, borrowed: what the printer prints.
#[derive(Clone, Copy)]
pub struct Code<'a> {
    pub id: SymId,
    pub initial: bool,
    pub recursive: bool,
    pub paragraphs: &'a [Paragraph],
    pub procedure_start: ParaId,
    pub ranges: &'a [Range],
    pub blocks: &'a [Block],
    pub places: &'a [Place],
    pub exprs: &'a [Expr],
    pub conds: &'a [Cond],
    pub consts: &'a [Const],
    pub plans: &'a Plans,
    pub services: &'a Services,
    pub abends: &'a [AbendText],
    pub symbols: &'a [String],
}

/// A program's code as text. The other sections name what the code refers to: `items` qualifies a
/// data name several items share, `debug` gives source positions, `sql` the EXEC SQL statements and
/// `ccsid` the code page literals are shown in. Each may be left empty, and is then not shown.
#[derive(Clone, Copy)]
pub struct Listing<'a> {
    pub code: Code<'a>,
    pub items: &'a [Item],
    pub debug: Option<&'a Debug>,
    pub sql: &'a [SqlEntry],
    pub ccsid: Option<u16>,
}

impl Program {
    pub fn code(&self) -> Code<'_> {
        Code {
            id: self.id,
            initial: self.initial,
            recursive: self.recursive,
            paragraphs: &self.paragraphs,
            procedure_start: self.procedure_start,
            ranges: &self.ranges,
            blocks: &self.blocks,
            places: &self.places,
            exprs: &self.exprs,
            conds: &self.conds,
            consts: &self.consts,
            plans: &self.plans,
            services: &self.services,
            abends: &self.abends,
            symbols: &self.symbols,
        }
    }
}

impl<'a> Listing<'a> {
    pub fn of(program: &'a Program) -> Self {
        Listing {
            code: program.code(),
            items: &program.items,
            debug: Some(&program.debug),
            sql: &program.sql,
            ccsid: Some(program.options.options.codepage),
        }
    }
}

impl fmt::Display for Listing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Printer::new(self).program(f)
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Listing::of(self).fmt(f)
    }
}

/// How deep one reference may nest, and how many references it may print, before the rest is
/// "...": a module is untrusted, and its tables may refer to each other in a cycle.
const DEPTH: u32 = 256;
const BUDGET: u32 = 100_000;

struct Printer<'a> {
    c: Code<'a>,
    items: &'a [Item],
    debug: Option<&'a Debug>,
    sql: &'a [SqlEntry],
    page: Option<&'static CodePage>,
    /// Each data name more than one item has, with those items.
    shared: BTreeMap<&'a str, Vec<usize>>,
    /// Each paragraph name more than one paragraph has.
    repeated: BTreeMap<&'a str, usize>,
    depth: Cell<u32>,
    spent: Cell<u32>,
}

fn join(parts: impl IntoIterator<Item = String>, separator: &str) -> String {
    parts.into_iter().collect::<Vec<_>>().join(separator)
}

/// ` [a, b]` or ` {a, b}`, nothing when there is nothing to say.
fn attrs(open: char, parts: Vec<String>) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let close = if open == '[' { ']' } else { '}' };
    format!(" {open}{}{close}", parts.join(", "))
}

fn yes(on: bool, text: &str) -> Option<String> {
    on.then(|| text.to_owned())
}

fn label(id: BlockId) -> String {
    format!("b{id}")
}

fn abend_ref(id: AbendId) -> String {
    format!("a{id}")
}

impl<'a> Printer<'a> {
    fn new(l: &Listing<'a>) -> Self {
        let symbol = |id: SymId| l.code.symbols.get(id as usize).map(String::as_str);
        let mut shared: BTreeMap<&'a str, Vec<usize>> = BTreeMap::new();
        for (k, item) in l.items.iter().enumerate() {
            if let Some(name) = item.name.and_then(symbol) {
                shared.entry(name).or_default().push(k);
            }
        }
        shared.retain(|_, items| items.len() > 1);
        let mut repeated: BTreeMap<&'a str, usize> = BTreeMap::new();
        for p in l.code.paragraphs {
            if let Some(name) = symbol(p.name) {
                *repeated.entry(name).or_default() += 1;
            }
        }
        repeated.retain(|_, n| *n > 1);
        Printer {
            c: l.code,
            items: l.items,
            debug: l.debug,
            sql: l.sql,
            page: l.ccsid.and_then(CodePage::by_ccsid),
            shared,
            repeated,
            depth: Cell::new(0),
            spent: Cell::new(0),
        }
    }

    fn nested(&self, render: impl FnOnce() -> String) -> String {
        let depth = self.depth.get();
        if depth == 0 {
            self.spent.set(0);
        }
        let spent = self.spent.get();
        if depth >= DEPTH || spent >= BUDGET {
            return "...".to_owned();
        }
        self.depth.set(depth + 1);
        self.spent.set(spent + 1);
        let text = render();
        self.depth.set(depth);
        text
    }

    fn symbol(&self, id: SymId) -> Option<&'a str> {
        self.c.symbols.get(id as usize).map(String::as_str)
    }

    /// A data, paragraph, file or program name as a word.
    fn name(&self, id: SymId) -> String {
        self.symbol(id).map_or_else(|| format!("symbol{id}?"), word)
    }

    /// Text that is not a name: a message, a statement, a signature.
    fn string(&self, id: SymId) -> String {
        self.symbol(id).map_or_else(|| format!("symbol{id}?"), |s| format!("{s:?}"))
    }

    /// Text that is quoted already, as a JSON string is.
    fn raw(&self, id: SymId) -> String {
        self.symbol(id).map_or_else(|| format!("symbol{id}?"), |s| if s.chars().any(char::is_control) { format!("{s:?}") } else { s.to_owned() })
    }

    /// A literal's text, as COBOL writes it.
    fn literal(&self, id: SymId) -> String {
        self.symbol(id).map_or_else(|| format!("symbol{id}?"), quote)
    }

    fn bytes(&self, bytes: &[u8]) -> String {
        let decoded = self.page.and_then(|page| {
            bytes
                .iter()
                .map(|&b| {
                    let ch = page.decode_byte(b);
                    (!ch.is_control() && page.encode_char(ch) == Some(b)).then_some(ch)
                })
                .collect::<Option<String>>()
        });
        decoded.map_or_else(|| hex("X", bytes), |text| quote(&text))
    }

    fn position(&self, pos: &Pos) -> String {
        if pos.file == 0 {
            return format!("{}:{}", pos.line, pos.col);
        }
        let file = self.debug.and_then(|d| d.sources.get(usize::from(pos.file))).map_or_else(|| format!("file{}", pos.file), |&s| self.name(s));
        format!("{file}:{}:{}", pos.line, pos.col)
    }

    fn pos(&self, at: DebugId) -> Option<String> {
        self.debug.and_then(|d| d.positions.get(at as usize)).map(|p| self.position(p))
    }

    fn at(&self, at: DebugId) -> String {
        self.pos(at).map_or_else(String::new, |p| format!("  @{p}"))
    }

    fn para(&self, id: ParaId) -> String {
        let Some(p) = self.c.paragraphs.get(id as usize) else { return format!("paragraph{id}?") };
        let name = self.name(p.name);
        if !self.symbol(p.name).is_some_and(|n| self.repeated.contains_key(n)) {
            return name;
        }
        let section = (0..id).rev().map(|s| (s, &self.c.paragraphs[s as usize])).find(|(_, s)| s.is_section && s.section_end >= id);
        match section {
            Some((_, s)) if !self.symbol(s.name).is_some_and(|n| self.repeated.contains_key(n)) => format!("{name} OF {}", self.name(s.name)),
            _ => format!("{name} (paragraph {id})"),
        }
    }

    /// The paragraph control falls into, or "end" past the last.
    fn next_para(&self, id: ParaId) -> String {
        if id as usize == self.c.paragraphs.len() { "end".to_owned() } else { self.para(id) }
    }

    fn range(&self, id: RangeId) -> String {
        match self.c.ranges.get(id as usize) {
            None => format!("r{id}?"),
            Some(r) if r.first == r.last => format!("r{id} {}", self.para(r.first)),
            Some(r) => format!("r{id} {} thru {}", self.para(r.first), self.para(r.last)),
        }
    }

    fn file(&self, file: u16) -> String {
        self.c.services.files.get(usize::from(file)).map_or_else(|| format!("file{file}?"), |d| self.name(d.name))
    }

    fn file_at(&self, file: usize) -> String {
        u16::try_from(file).map_or_else(|_| format!("file{file}?"), |f| self.file(f))
    }

    /// LINKAGE record `n` by its 01 or 77's name.
    fn linkage(&self, n: u16) -> String {
        let record = self.items.iter().find(|i| i.linkage == Some(n) && i.parent.is_none()).and_then(|i| i.name);
        record.map_or_else(|| format!("linkage{n}"), |s| self.name(s))
    }

    /// A place as a reference is written: its name, qualified when other items share it, then
    /// its subscripts and reference modification.
    fn place(&self, id: PlaceId) -> String {
        self.nested(|| match self.c.places.get(id as usize) {
            None => format!("p{id}?"),
            Some(p) => self.reference(p, &[]),
        })
    }

    fn places(&self, ids: &[PlaceId]) -> String {
        join(ids.iter().map(|&p| self.place(p)), " ")
    }

    /// `all` holds the subscripts written ALL, by position.
    fn reference(&self, p: &Place, all: &[u32]) -> String {
        let mut text = self.data_name(p);
        if !p.subscripts.is_empty() {
            let subscripts = p.subscripts.iter().enumerate().map(|(k, s)| if all.contains(&(k as u32)) { "ALL".to_owned() } else { self.int(&s.value) });
            text += &format!("({})", join(subscripts, ", "));
        }
        if let Some(r) = &p.refmod {
            text += &format!("({}:{})", self.int(&r.start), r.length.as_ref().map_or_else(String::new, |l| self.int(l)));
        }
        text
    }

    fn data_name(&self, p: &Place) -> String {
        let name = self.name(p.name);
        let Some(same) = self.symbol(p.name).and_then(|n| self.shared.get(n)) else { return name };
        let held = |i: &Item| match p.base {
            Base::Program => !i.local && i.linkage.is_none(),
            Base::Local => i.local,
            Base::Linkage(n) => i.linkage == Some(n),
            Base::ReturnCode | Base::Eib | Base::SelfRef | Base::JniEnv | Base::Xml(_) => false,
        };
        let Some(&own) = same.iter().find(|&&k| self.items[k].offset == p.offset && held(&self.items[k])) else { return name };
        let mut others: Vec<usize> = same.iter().copied().filter(|&k| k != own).collect();
        let mut qualifiers: Vec<&str> = Vec::new();
        for ancestor in self.ancestors(own) {
            if others.is_empty() {
                break;
            }
            let Some(q) = self.items[ancestor].name.and_then(|s| self.symbol(s)) else { continue };
            qualifiers.push(q);
            others.retain(|&k| self.qualifies(k, &qualifiers));
        }
        qualifiers.iter().fold(name, |text, q| format!("{text} OF {}", word(q)))
    }

    /// An item's ancestors, innermost first; a chain that loops stops.
    fn ancestors(&self, k: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut next = self.items.get(k).and_then(|i| i.parent);
        while let Some(p) = next.map(|p| p as usize).filter(|&p| p < self.items.len() && out.len() < self.items.len()) {
            out.push(p);
            next = self.items[p].parent;
        }
        out
    }

    /// Item `k` has ancestors named by `qualifiers`, in order.
    fn qualifies(&self, k: usize, qualifiers: &[&str]) -> bool {
        let mut wanted = qualifiers.iter().peekable();
        for a in self.ancestors(k) {
            if wanted.peek().is_some_and(|&&q| self.items[a].name.and_then(|s| self.symbol(s)) == Some(q)) {
                wanted.next();
            }
        }
        wanted.peek().is_none()
    }

    fn konst(&self, id: ConstId) -> String {
        match self.c.consts.get(id as usize) {
            None => format!("c{id}?"),
            Some(c) => self.constant(c),
        }
    }

    fn constant(&self, c: &Const) -> String {
        match c {
            Const::Bytes(b) => self.bytes(b),
            Const::National(units) => national(units),
            Const::Number(n) => decimal(n),
            Const::Figurative(f) => figurative(*f).to_owned(),
            Const::All(b) => format!("ALL {}", self.bytes(b)),
            Const::AllNational(units) => format!("ALL {}", national(units)),
            Const::Dbcs(b) => hex("GX", b),
            Const::Float(b) => hex("float X", b),
            Const::Refused(a) => format!("refused {}", abend_ref(*a)),
        }
    }

    fn operand(&self, o: &Operand) -> String {
        match *o {
            Operand::Load(p) => self.place(p),
            Operand::Const(c) => self.konst(c),
            Operand::LengthOf(p) => format!("LENGTH OF {}", self.place(p)),
            Operand::AddressOf(p) => format!("ADDRESS OF {}", self.place(p)),
            Operand::Function(f) => self.function(f),
            Operand::UserFunction(u) => self.user_function(u),
        }
    }

    fn function(&self, id: FunctionId) -> String {
        self.nested(|| {
            let Some(f) = self.c.plans.function.get(id as usize) else { return format!("function{id}?") };
            let args = f.args.iter().map(|a| match a {
                Argument::Value(v) => self.comparand(v),
                Argument::All { element, all } => match self.c.places.get(*element as usize) {
                    None => format!("p{element}?"),
                    Some(p) => self.reference(p, &all.iter().map(|(k, _)| *k).collect::<Vec<_>>()),
                },
            });
            let side = match f.side {
                None => "",
                Some(TrimSide::Leading) => " LEADING",
                Some(TrimSide::Trailing) => " TRAILING",
            };
            let mut text = format!("FUNCTION {}({}{side})", f.func.name(), join(args, ", "));
            if let Some(r) = &f.refmod {
                text += &format!("({}:{})", self.int(&r.start), r.length.as_ref().map_or_else(String::new, |l| self.int(l)));
            }
            let how = [f.integer.as_ref().map(|i| format!("integer {}", self.int(i))), f.arity.map(|a| format!("arity {}", abend_ref(a)))];
            text + &attrs('{', how.into_iter().flatten().collect())
        })
    }

    fn user_function(&self, id: UserFunctionId) -> String {
        self.nested(|| {
            let Some(u) = self.c.services.user_functions.get(id as usize) else { return format!("user-function{id}?") };
            let args = u.args.iter().map(|a| match a {
                UserArgument::Reference(p) => self.place(*p),
                UserArgument::Value(v) => format!("value {}", self.comparand(v)),
                UserArgument::Literal(b) => format!("literal {}", self.bytes(b)),
            });
            let mut text = format!("FUNCTION {}({})", self.name(u.name), join(args, ", "));
            if let Some(r) = &u.refmod {
                text += &format!("({}:{})", self.int(&r.start), r.length.as_ref().map_or_else(String::new, |l| self.int(l)));
            }
            let external = (u.external != u.name).then(|| format!("external {}", self.name(u.external)));
            text + &attrs('{', external.into_iter().collect())
        })
    }

    fn expr(&self, id: ExprId) -> String {
        self.nested(|| match self.c.exprs.get(id as usize) {
            None => format!("e{id}?"),
            Some(Expr::Operand(o)) => self.operand(o),
            Some(Expr::Neg(e)) => format!("-{}", self.term(*e)),
            Some(Expr::Bin(a, op, b)) => format!("{} {} {}", self.term(*a), binop(*op), self.term(*b)),
            Some(Expr::Pow(a, n)) => {
                let exponent = self.int(n);
                let compound = matches!(n, IntExpr::Fixed { expr, .. } if matches!(self.c.exprs.get(*expr as usize), Some(Expr::Bin(..) | Expr::Pow(..))));
                if compound { format!("{} ** ({exponent})", self.term(*a)) } else { format!("{} ** {exponent}", self.term(*a)) }
            }
        })
    }

    /// An operand of an operator, in parentheses when it is an operation itself.
    fn term(&self, id: ExprId) -> String {
        match self.c.exprs.get(id as usize) {
            Some(Expr::Bin(..) | Expr::Pow(..)) => format!("({})", self.expr(id)),
            _ => self.expr(id),
        }
    }

    fn int(&self, e: &IntExpr) -> String {
        match e {
            IntExpr::Const(n) => n.to_string(),
            IntExpr::Item(p) => self.place(*p),
            IntExpr::Fixed { expr, dmax, prepass } => {
                let how = self.how(*dmax, &Mode::Fixed, prepass);
                if how.is_empty() { self.expr(*expr) } else { self.term(*expr) + &attrs('{', how) }
            }
            IntExpr::Walk(k) => format!("walk{k}"),
        }
    }

    /// How an expression is evaluated where that is not plain: dmax, floating point, and the
    /// places located before it.
    fn how(&self, dmax: u32, mode: &Mode, prepass: &[PlaceId]) -> Vec<String> {
        let mut how = Vec::new();
        if dmax != 0 {
            how.push(format!("dmax {dmax}"));
        }
        match mode {
            Mode::Fixed => {}
            Mode::Float(p) => how.push(format!("float {}", precision(*p))),
        }
        if !prepass.is_empty() {
            how.push(format!("prepass {}", self.places(prepass)));
        }
        how
    }

    fn comparand(&self, c: &Comparand) -> String {
        match c {
            Comparand::Operand(o) => self.operand(o),
            Comparand::Expr { expr, dmax, mode, prepass } => self.term(*expr) + &attrs('{', self.how(*dmax, mode, prepass)),
        }
    }

    fn cond(&self, id: CondId) -> String {
        self.nested(|| match self.c.conds.get(id as usize) {
            None => format!("k{id}?"),
            Some(Cond::Rel { a, op, b, how }) => format!("{} {} {} [{}]", self.comparand(a), relop(*op), self.comparand(b), compare(*how)),
            Some(Cond::Class { place, test }) => format!("{} is {}", self.place(*place), byte_class(*test)),
            Some(Cond::Sign { value, test }) => format!("{} is {}", self.comparand(value), sign_test(*test)),
            Some(Cond::Name { subject, values, how }) => {
                let values = values.iter().map(|(low, high)| match high {
                    None => self.konst(*low),
                    Some(h) => format!("{} thru {}", self.konst(*low), self.konst(*h)),
                });
                format!("{} in ({}) [{}]", self.place(*subject), join(values, ", "), compare(*how))
            }
            Some(Cond::Not(c)) => format!("not ({})", self.cond(*c)),
            Some(Cond::And(a, b)) => format!("{} and {}", self.logical(*a), self.logical(*b)),
            Some(Cond::Or(a, b)) => format!("{} or {}", self.logical(*a), self.logical(*b)),
            Some(Cond::Counter(t)) => format!("t{t} > 0"),
            Some(Cond::InTable { index, count }) => format!("{} within {}", self.place(*index), self.count(count)),
            Some(Cond::Sql(test)) => format!("sql {}", sql_test(*test)),
        })
    }

    /// A side of AND or OR, in parentheses when it is AND or OR itself.
    fn logical(&self, id: CondId) -> String {
        match self.c.conds.get(id as usize) {
            Some(Cond::And(..) | Cond::Or(..)) => format!("({})", self.cond(id)),
            _ => self.cond(id),
        }
    }

    fn count(&self, c: &Count) -> String {
        match c {
            Count::Fixed(n) => n.to_string(),
            Count::Odo(o) => self.odo(o),
            Count::Temp(t) => format!("t{t}"),
        }
    }

    fn odo(&self, o: &Odo) -> String {
        format!("({} max {} element {}{})", self.int(&o.object), o.max, o.element, if o.check { " check" } else { "" })
    }

    fn chars(&self, c: &Chars) -> String {
        match c {
            Chars::Literal(b) => self.bytes(b),
            Chars::Place(p) => self.place(*p),
            Chars::Value(o) => self.operand(o),
        }
    }

    fn store(&self, s: &StorePlan) -> String {
        match *s {
            StorePlan::Zoned { digits, scale, signed, sign } => format!("zoned {}{}", pic(digits, scale, signed), sign_clause(sign)),
            StorePlan::Packed { digits, scale, signed } => format!("packed {}", pic(digits, scale, signed)),
            StorePlan::Binary { digits, scale, signed, native, name: _ } => format!("binary {}{}", pic(digits, scale, signed), native_word(native)),
            StorePlan::NumericEdited { edit, digits, scale, blank_when_zero } => {
                format!("numeric-edited edit {edit} {}{}", pic(digits, scale, false), if blank_when_zero { " blank-when-zero" } else { "" })
            }
            StorePlan::Float(p) => format!("float {}", precision(p)),
            StorePlan::Index => "index".to_owned(),
            StorePlan::Refused(a) => format!("refused {}", abend_ref(a)),
        }
    }

    fn step(&self, s: &StepPlan) -> String {
        format!("{}, dmax {}", self.store(&s.store), s.dmax)
    }

    fn move_plan(&self, m: &MovePlan) -> String {
        match *m {
            MovePlan::Alnum { image: i, justified } => format!("alnum{}{}", image(i), if justified { " justified" } else { "" }),
            MovePlan::AlnumEdited { image: i, edit, positions } => format!("alnum-edited edit {edit} positions {positions}{}", image(i)),
            MovePlan::National(from) => format!("national {}", national_from(from)),
            MovePlan::Dbcs { justified, edit } => {
                format!("dbcs{}{}", if justified { " justified" } else { "" }, edit.map_or(String::new(), |e| format!(" edit {e}")))
            }
            MovePlan::Numeric { from, store } => format!("numeric {} to {}", numeric_from(from), self.store(&store)),
            MovePlan::Float { from, precision: p } => format!("float {} to {}", float_from(from), precision(p)),
            MovePlan::Address => "address".to_owned(),
            MovePlan::Index => "index".to_owned(),
            MovePlan::Refused(a) => format!("refused {}", abend_ref(a)),
        }
    }

    /// ` [plan]` and NUMCHECK's test of the sender.
    fn moved(&self, plan: &MovePlan, check: SenderCheck) -> String {
        let check = match check {
            SenderCheck::None => None,
            SenderCheck::Item => Some("check item".to_owned()),
            SenderCheck::Integer => Some("check integer".to_owned()),
        };
        attrs('[', std::iter::once(self.move_plan(plan)).chain(check).collect())
    }

    fn stored(&self, (place, store): &(PlaceId, StorePlan)) -> String {
        format!("{} [{}]", self.place(*place), self.store(store))
    }

    fn program(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = (!self.c.paragraphs.is_empty()).then(|| format!("start {}", self.para(self.c.procedure_start)));
        let flags = [start, yes(self.c.initial, "initial"), yes(self.c.recursive, "recursive")];
        writeln!(f, "program {}{}", self.name(self.c.id), join(flags.into_iter().flatten().map(|s| format!(" {s}")), ""))?;
        for (k, r) in self.c.ranges.iter().enumerate() {
            let span = if r.first == r.last { self.para(r.first) } else { format!("{} thru {}", self.para(r.first), self.para(r.last)) };
            writeln!(f, "range r{k} {} {span}", range_kind(r.kind))?;
        }
        self.code(f)?;
        self.tables(f)
    }

    fn code(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entries: BTreeMap<BlockId, Vec<ParaId>> = BTreeMap::new();
        for (k, p) in self.c.paragraphs.iter().enumerate() {
            entries.entry(p.entry).or_default().push(k as ParaId);
        }
        for (b, block) in self.c.blocks.iter().enumerate() {
            for &p in entries.get(&(b as BlockId)).into_iter().flatten() {
                writeln!(f, "{}", self.paragraph(p, false))?;
            }
            writeln!(f, "{}:", label(b as BlockId))?;
            let ats = self.debug.and_then(|d| d.ops.get(b));
            let starts = self.debug.and_then(|d| d.statements.get(b)).map_or(&[][..], Vec::as_slice);
            let mut statement = None;
            let starting = |f: &mut fmt::Formatter<'_>, k: usize, statement: &mut Option<String>| -> fmt::Result {
                for (_, at) in starts.iter().filter(|(i, _)| *i as usize == k) {
                    let pos = self.pos(*at);
                    writeln!(f, "    statement {}", pos.as_deref().unwrap_or("?"))?;
                    *statement = pos;
                }
                Ok(())
            };
            let suffix = |k: usize, statement: &Option<String>| match ats.and_then(|a| a.get(k)).and_then(|&at| self.pos(at)) {
                Some(pos) if Some(&pos) != statement.as_ref() => format!("  @{pos}"),
                _ => String::new(),
            };
            for (k, op) in block.ops.iter().enumerate() {
                starting(f, k, &mut statement)?;
                writeln!(f, "    {}{}", self.op(op), suffix(k, &statement))?;
            }
            let k = block.ops.len();
            starting(f, k, &mut statement)?;
            let at = match block.end {
                Terminator::Jump(_) | Terminator::Select(_) => String::new(),
                _ => suffix(k, &statement),
            };
            writeln!(f, "    {}{at}", self.terminator(&block.end))?;
        }
        for (&b, paragraphs) in entries.range(self.c.blocks.len() as BlockId..) {
            for &p in paragraphs {
                writeln!(f, "{} entry {}?", self.paragraph(p, true), label(b))?;
            }
        }
        Ok(())
    }

    fn paragraph(&self, k: ParaId, bare: bool) -> String {
        let p = &self.c.paragraphs[k as usize];
        let mut text = format!("paragraph {k} {}", self.name(p.name));
        if p.is_section {
            text += &format!(" section thru {}", self.para(p.section_end));
        }
        if p.priority != 0 {
            text += &format!(" priority {}", p.priority);
        }
        if let Some(a) = p.abandoned {
            text += &format!(" abandoned {}", abend_ref(a));
        }
        if bare { text } else { text + &self.at(p.at) }
    }

    fn terminator(&self, t: &Terminator) -> String {
        match t {
            Terminator::Jump(b) => format!("Jump {}", label(*b)),
            Terminator::Branch { cond, then, otherwise } => format!("Branch ({}) {} else {}", self.cond(*cond), label(*then), label(*otherwise)),
            Terminator::Select(arms) => format!("Select {}", join(arms.iter().map(|&b| label(b)), " ")),
            Terminator::ParagraphEnd { next } => format!("ParagraphEnd next {}", self.next_para(*next)),
            Terminator::GoTo(p) => format!("GoTo {}", self.para(*p)),
            Terminator::Switch { value, targets, otherwise } => {
                format!("Switch {} {} else {}", self.int(value), join(targets.iter().map(|&p| self.para(p)), " "), label(*otherwise))
            }
            Terminator::PerformEnter { range, ret, resume } => {
                let resume = resume.map_or_else(String::new, |r| format!(" resume {} {}", self.para(r.para), label(r.block)));
                format!("PerformEnter {} -> {}{resume}", self.range(*range), label(*ret))
            }
            Terminator::ExitProgram { next } => format!("ExitProgram next {}", label(*next)),
            Terminator::End(e) => format!("End {}", ending(*e)),
            Terminator::Abend(a) => match self.c.abends.get(*a as usize) {
                Some(text) => format!("Abend {} {} {}", abend_ref(*a), text.code, self.string(text.message)),
                None => format!("Abend {}?", abend_ref(*a)),
            },
            Terminator::AlteredGoTo { para, otherwise } => format!("AlteredGoTo {} else {}", self.para(*para), label(*otherwise)),
            Terminator::Debug { range, name, next } => format!("Debug {} name {} next {}", self.range(*range), self.literal(*name), label(*next)),
        }
    }

    fn op(&self, op: &Op) -> String {
        match op {
            Op::Move { from, to, plan, check } => format!("Move {} <- {}{}", self.place(*to), self.operand(from), self.moved(plan, *check)),
            Op::Set { from, to, plan } => format!("Set {} <- {}{}", self.place(*to), self.operand(from), self.moved(plan, SenderCheck::None)),
            Op::Initialize { target, plan } => format!("Initialize {} init {plan}", self.place(*target)),
            Op::Arith(id) => self.arith(*id),
            Op::SetAddress { records, address } => format!("SetAddress {} <- {}", join(records.iter().map(|&r| self.linkage(r)), " "), self.operand(address)),
            Op::SetUpDown { by, down, targets } => {
                let targets = targets.iter().map(|(p, how)| {
                    let how = match how {
                        UpDown::Pointer => "pointer".to_owned(),
                        UpDown::Number(step) => self.step(step),
                        UpDown::Refused(a) => format!("refused {}", abend_ref(*a)),
                    };
                    format!("{} [{how}]", self.place(*p))
                });
                format!("SetUpDown {} {} by {}", join(targets, ", "), if *down { "down" } else { "up" }, self.int(by))
            }
            Op::SetEntry { entry, targets } => format!("SetEntry {} <- {}", self.places(targets), self.operand(entry)),
            Op::Step { var, by, plan, prepass } => {
                let prepass = (!prepass.is_empty()).then(|| format!("prepass {}", self.places(prepass)));
                format!("Step {} by {} [{}]{}", self.place(*var), self.expr(*by), self.step(plan), attrs('{', prepass.into_iter().collect()))
            }
            Op::SetInt { target, value } => format!("SetInt {} <- {}", self.place(*target), self.int(value)),
            Op::Inspect(id) => self.inspect(*id),
            Op::String(id) => self.string_op(*id),
            Op::Unstring(id) => self.unstring(*id, None),
            Op::UnstringValue { value, plan } => self.unstring(*plan, Some(value)),
            Op::SearchAll(id) => self.search_all(*id),
            Op::Nest => "Nest".to_owned(),
            Op::Unnest(n) => format!("Unnest {n}"),
            Op::SetTemp(t, value) => format!("SetTemp t{t} <- {}", self.int(value)),
            Op::DecTemp(t) => format!("DecTemp t{t}"),
            Op::SetCount(t, odo) => format!("SetCount t{t} <- {}", self.odo(odo)),
            Op::Display(id) => self.display(*id),
            Op::DisplayError(id) => format!("Error{}", self.display(*id)),
            Op::ArgumentNumber(value) => format!("ArgumentNumber <- {}", self.int(value)),
            Op::Environment { display, value } => format!("Environment{} {}", if *value { "Value" } else { "Name" }, self.display(*display)),
            Op::ScreenDisplay { display, screen } => format!("Screen{} {}", self.display(*display), self.screen(screen)),
            Op::ScreenAccept { inputs, handled } => {
                let inputs = inputs.iter().map(|i| {
                    let flags = format!("{}{}", if i.update { " update" } else { "" }, if i.secure { " secure" } else { "" });
                    format!("{} <- {} [{}{flags}]", self.place(i.target), self.place(i.field), self.screen_position(&i.at))
                });
                format!("ScreenAccept {}{}", join(inputs, ", "), if *handled { " handled" } else { "" })
            }
            Op::Accept { target, from, plan } => format!("Accept {} <- {}{}", self.place(*target), accept_from(*from), self.moved(plan, SenderCheck::None)),
            Op::File(id) => self.file_op(*id),
            Op::Call(id) => self.call(*id),
            Op::Cancel(name) => format!("Cancel {}", self.operand(name)),
            Op::Sort(id) => self.sort(*id),
            Op::Release(id) => self.release(*id),
            Op::Return(id) => self.return_op(*id),
            Op::Report(r) => self.report_op(r),
            Op::Invoke(id) => self.invoke(*id),
            Op::Cics(id) => self.cics(*id),
            Op::Sql(ordinal) => self.sql(*ordinal),
            Op::Alter { para, to } => format!("Alter {} to {}", self.para(*para), self.para(*to)),
            Op::EnterSegment(priority) => format!("EnterSegment {priority}"),
            Op::DebugLine(line) => format!("DebugLine {line}"),
            Op::DebugAlter { range, name, contents } => format!("DebugAlter {} name {} contents {}", self.range(*range), self.literal(*name), self.literal(*contents)),
            Op::Markup(id) => self.markup(*id),
        }
    }

    fn arith(&self, id: ArithId) -> String {
        let Some(ArithPlan { dmax, arith, prepass, steps, remainder, handled, per_receiver, inner_dmax }) = self.c.plans.arith.get(id as usize) else {
            return format!("Arith arith{id}?");
        };
        let mut steps: Vec<String> = steps.iter().map(|s| self.arith_step(s)).collect();
        if let Some(r) = remainder {
            steps.push(format!(
                "remainder {} <- {} / {} [{}, quotient scale {}]",
                self.place(r.target),
                self.term(r.dividend),
                self.term(r.divisor),
                self.store(&r.store),
                r.quotient_scale
            ));
        }
        let mut how = vec![format!("dmax {dmax}")];
        if inner_dmax != dmax {
            how.push(format!("inner dmax {inner_dmax}"));
        }
        match arith {
            Arith::Compat => {}
            Arith::Extend => how.push("arith extend".to_owned()),
        }
        if !prepass.is_empty() {
            how.push(format!("prepass {}", self.places(prepass)));
        }
        how.extend(yes(*per_receiver, "per receiver"));
        how.extend(yes(*handled, "size error"));
        format!("Arith {}{}", steps.join("; "), attrs('{', how))
    }

    fn arith_step(&self, s: &ArithStep) -> String {
        let mut how = vec![self.store(&s.store)];
        how.extend(yes(s.rounded, "rounded"));
        match s.mode {
            Mode::Fixed => {}
            Mode::Float(p) => how.push(format!("float {}", precision(p))),
        }
        if !s.probe.is_empty() {
            how.push(format!("probe {}", self.places(&s.probe)));
        }
        format!("{} <- {}{}", self.place(s.target), self.expr(s.expr), attrs('[', how))
    }

    fn screen_position(&self, at: &ScreenPosition) -> String {
        match at {
            ScreenPosition::Cursor => "cursor".to_owned(),
            ScreenPosition::Combined(at) => format!("at {}", self.int(at)),
            ScreenPosition::LineColumn { line, column } => {
                format!("line {} column {}", line.as_ref().map_or("cursor".to_owned(), |l| self.int(l)), column.as_ref().map_or("1".to_owned(), |c| self.int(c)))
            }
        }
    }

    fn screen(&self, s: &ScreenPlan) -> String {
        let at = self.screen_position(&s.at);
        let flags = [(s.blank_screen, " blank-screen"), (s.blank_line, " blank-line"), (s.erase_eol, " erase-eol"), (s.erase_eos, " erase-eos"), (s.update, " update"), (s.secure, " secure")];
        format!("[{at}{}]", flags.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect::<String>())
    }

    fn display(&self, id: DisplayId) -> String {
        let Some(d) = self.c.plans.display.get(id as usize) else { return format!("Display display{id}?") };
        let items = d.items.iter().map(|item| match item {
            DisplayItem::Bytes(p) => self.place(*p),
            DisplayItem::National(p) => format!("{} [national]", self.place(*p)),
            DisplayItem::Digits { place, digits, signed } => format!("{} [digits {digits}{}]", self.place(*place), if *signed { " signed" } else { "" }),
            DisplayItem::Refused { place, abend } => format!("{} [refused {}]", self.place(*place), abend_ref(*abend)),
            DisplayItem::Text(text) => self.literal(*text),
            DisplayItem::Value(o) => self.operand(o),
        });
        format!("Display {}{}", join(items, ", "), attrs('{', yes(d.no_advancing, "no advancing").into_iter().collect()))
    }

    fn inspect(&self, id: InspectId) -> String {
        let Some(i) = self.c.plans.inspect.get(id as usize) else { return format!("Inspect inspect{id}?") };
        let mut text = format!(
            "Inspect {}",
            match &i.target {
                Inspected::Item(p) => self.place(*p),
                Inspected::Value(o) => self.operand(o),
            }
        );
        for phrase in &i.tallying {
            text += &format!(" tallying {}", self.inspect_phrase(phrase));
        }
        for phrase in &i.replacing {
            text += &format!(" replacing {}", self.inspect_phrase(phrase));
        }
        if let Some(c) = &i.converting {
            text += &match &c.table {
                ConvertTable::Built(pairs) => {
                    let (from, to): (Vec<u8>, Vec<u8>) = pairs.iter().copied().unzip();
                    format!(" converting {} to {} {{built}}", self.bytes(&from), self.bytes(&to))
                }
                ConvertTable::Operands { from, to } => format!(" converting {} to {}", self.chars(from), self.chars(to)),
            };
            text += &self.bounds(&c.bounds);
        }
        text
    }

    fn inspect_phrase(&self, phrase: &InspectPhrase) -> String {
        let mut text = String::new();
        if let Some((p, step)) = &phrase.counter {
            text += &format!("{} [{}] for ", self.place(*p), self.step(step));
        }
        text += match phrase.mode {
            InspectMode::Characters => "CHARACTERS",
            InspectMode::All => "ALL",
            InspectMode::Leading => "LEADING",
            InspectMode::Trailing => "TRAILING",
            InspectMode::First => "FIRST",
        };
        if let Some(pattern) = &phrase.pattern {
            text += &format!(" {}", self.chars(pattern));
        }
        match &phrase.by {
            None => {}
            Some(Replacement::Chars(c)) => text += &format!(" by {}", self.chars(c)),
            Some(Replacement::Fill(b)) => text += &format!(" by fill {}", hex("X", &[*b])),
        }
        text + &self.bounds(&phrase.bounds)
    }

    fn bounds(&self, bounds: &[Bound]) -> String {
        join(bounds.iter().map(|b| format!(" {} {}", if b.after { "after" } else { "before" }, self.chars(&b.value))), "")
    }

    fn string_op(&self, id: StringId) -> String {
        let Some(s) = self.c.plans.string.get(id as usize) else { return format!("String string{id}?") };
        let sources = s.sources.iter().map(|source| {
            let delimiter = source.delimiter.as_ref().map_or_else(|| "size".to_owned(), |d| self.chars(d));
            format!("{} delimited by {delimiter}", self.chars(&source.chars))
        });
        let pointer = s.pointer.as_ref().map_or_else(String::new, |p| format!(" pointer {}", self.stored(p)));
        format!("String {} <- {}{pointer}", self.place(s.into), join(sources, ", "))
    }

    fn unstring(&self, id: UnstringId, value: Option<&Operand>) -> String {
        let Some(u) = self.c.plans.unstring.get(id as usize) else { return format!("Unstring unstring{id}?") };
        let mut text = match value {
            Some(v) => format!("UnstringValue {}", self.operand(v)),
            None => format!("Unstring {}", self.place(u.source)),
        };
        if !u.delimiters.is_empty() {
            let delimiters = u.delimiters.iter().map(|(all, c)| format!("{}{}", if *all { "ALL " } else { "" }, self.chars(c)));
            text += &format!(" delimited by {}", join(delimiters, ", "));
        }
        let into = u.into.iter().map(|i| {
            let mut field = format!("{} [{}]", self.place(i.target), self.move_plan(&i.plan));
            if let Some(d) = &i.delimiter {
                field += &format!(" delimiter in {} [found {}, none {}]", self.place(d.target), self.move_plan(&d.found), self.move_plan(&d.none));
            }
            if let Some(c) = &i.count {
                field += &format!(" count in {}", self.stored(c));
            }
            field
        });
        text += &format!(" into {}", join(into, ", "));
        if let Some(p) = &u.pointer {
            text += &format!(" pointer {}", self.stored(p));
        }
        if let Some((p, step)) = &u.tallying {
            text += &format!(" tallying {} [{}]", self.place(*p), self.step(step));
        }
        text
    }

    fn search_all(&self, id: SearchAllId) -> String {
        let Some(s) = self.c.plans.search_all.get(id as usize) else { return format!("SearchAll search-all{id}?") };
        let keys = s.keys.iter().map(|k| {
            format!("{} = {} [{}{}]", self.comparand(&k.key), self.comparand(&k.value), compare(k.how), if k.ascending { "" } else { ", descending" })
        });
        format!("SearchAll {} [{}] within {} when {}", self.place(s.index), self.store(&s.store), self.count(&s.count), join(keys, " and "))
    }

    fn file_op(&self, id: FileOpId) -> String {
        let Some(op) = self.c.services.file_ops.get(id as usize) else { return format!("File file-op{id}?") };
        let file = self.file(op.file);
        let (text, at_end) = match &op.verb {
            FileVerb::Open(mode) => (format!("OPEN {} {file}", open_mode(*mode)), false),
            FileVerb::Close => (format!("CLOSE {file}"), false),
            FileVerb::CloseWith(closing) => (format!("CLOSE {file} {}", closing_text(*closing)), false),
            FileVerb::Read { sequential, previous, into, key } => {
                let mut text = format!("READ {file}");
                text += &if *sequential { " sequential".to_owned() } else { format!(" key {key}") };
                if *previous {
                    text += " previous";
                }
                if let Some((p, plan)) = into {
                    text += &format!(" into {}{}", self.place(*p), self.moved(plan, SenderCheck::None));
                }
                (text, *sequential)
            }
            FileVerb::Write { record, from, advancing } => {
                let mut text = format!("WRITE {}{}", self.place(*record), self.sender(from.as_ref()));
                if let Some(a) = advancing {
                    text += &format!(" advancing {}", self.advance(a));
                }
                (text, false)
            }
            FileVerb::Rewrite { record, from } => (format!("REWRITE {}{}", self.place(*record), self.sender(from.as_ref())), false),
            FileVerb::Delete => (format!("DELETE {file}"), false),
            FileVerb::DeleteFile => (format!("DELETE FILE {file}"), false),
            FileVerb::Start { rel, key } => {
                let rel = match rel {
                    StartRel::Equal => "=",
                    StartRel::Greater => ">",
                    StartRel::NotLess => ">=",
                    StartRel::Less => "<",
                    StartRel::NotGreater => "<=",
                };
                let key = match key {
                    StartKey::Prime => "prime".to_owned(),
                    StartKey::Named { key, span } => format!("{key} +{} len {}", span.offset, span.len),
                    StartKey::Relative(value) => format!("relative {}", self.int(value)),
                    StartKey::RelativeKey => "relative key".to_owned(),
                };
                (format!("START {file} key {rel} {key}"), false)
            }
        };
        let (on, not_on) = if at_end { ("at end", "not at end") } else { ("invalid key", "not invalid key") };
        let mut phrases = phrase_words(op.phrase, on, not_on);
        phrases.extend(phrase_words(op.end_of_page, "end-of-page", "not end-of-page"));
        format!("File {text}{}", attrs('{', phrases))
    }

    fn sender(&self, from: Option<&FromMove>) -> String {
        from.map_or_else(String::new, |m| format!(" <- {}{}", self.operand(&m.from), self.moved(&m.plan, m.check)))
    }

    fn advance(&self, a: &Advance) -> String {
        let side = |before: bool| if before { "before" } else { "after" };
        match a {
            Advance::Lines { before, count } => format!("{} {} lines", side(*before), self.int(count)),
            Advance::Page { before } => format!("{} page", side(*before)),
            Advance::Mnemonic { before, space } => format!(
                "{} {}",
                side(*before),
                match space {
                    Spacing::Lines(n) => format!("{n} lines"),
                    Spacing::Channel(c) => format!("channel {c}"),
                    Spacing::PageMode => "page mode".to_owned(),
                }
            ),
        }
    }

    fn call(&self, id: CallId) -> String {
        let Some(c) = self.c.services.calls.get(id as usize) else { return format!("Call call{id}?") };
        let target = match &c.target {
            CallTarget::Named { name, le: None } => self.literal(*name),
            CallTarget::Named { name, le: Some(service) } => {
                let service = format!("{service:?}").to_ascii_uppercase();
                format!("{} or {service}", self.literal(*name))
            }
            CallTarget::Dynamic(o) => self.operand(o),
            CallTarget::Pointer(p) => format!("pointer {}", self.place(*p)),
            CallTarget::Entry(p) => format!("entry {}", self.place(*p)),
        };
        let mut text = format!("Call {target}");
        if !c.args.is_empty() {
            let args = c.args.iter().map(|a| match a {
                CallArg::Reference(p) => self.place(*p),
                CallArg::Content(chars) => format!("content {}", self.chars(chars)),
                CallArg::Value(o) => format!("value {}", self.operand(o)),
                CallArg::Omitted => "omitted".to_owned(),
            });
            text += &format!(" using {}", join(args, ", "));
        }
        if let Some(r) = c.returning {
            text += &format!(" returning {}", self.place(r));
        }
        text + &attrs('{', [yes(c.on_exception, "on exception"), yes(c.not_on_exception, "not on exception")].into_iter().flatten().collect())
    }

    fn sort_keys(&self, keys: &SortKeys) -> String {
        let named = keys.keys.iter().map(|k| {
            let item = self.items.get(k.item as usize).and_then(|i| i.name).map_or_else(|| format!("+{} len {}", k.offset, k.len), |s| self.name(s));
            format!("{} {item}{}", if k.ascending { "ascending" } else { "descending" }, if k.collated { " collated" } else { "" })
        });
        format!("keys {}{}", join(named, ", "), if keys.collating.is_some() { " {collating sequence}" } else { "" })
    }

    fn sort(&self, id: SortId) -> String {
        match self.c.services.sorts.get(id as usize) {
            None => format!("Sort sort{id}?"),
            Some(SortPlan::File(s)) => {
                let io = |io: &Option<SortIo>, files: &str, procedure: &str| match io {
                    None => format!("{procedure} none"),
                    Some(SortIo::Files(list)) => format!("{files} {}", join(list.iter().map(|&k| self.file(k)), " ")),
                    Some(SortIo::Procedure(r)) => format!("{procedure} procedure {}", self.range(*r)),
                };
                format!(
                    "Sort {} {} {} {} {}{}",
                    if s.merge { "merge" } else { "file" },
                    self.file(s.sd),
                    self.sort_keys(&s.keys),
                    io(&s.input, "using", "input"),
                    io(&s.output, "giving", "output"),
                    attrs('{', vec![format!("sort-return {}", self.place(s.sort_return)), format!("sort-control {}", self.place(s.sort_control))])
                )
            }
            Some(SortPlan::Table(t)) => {
                format!("Sort table {} first {} within {} stride {} {}", self.name(t.name), self.place(t.first), self.count(&t.count), t.stride, self.sort_keys(&t.keys))
            }
        }
    }

    fn release(&self, id: ReleaseId) -> String {
        let Some(r) = self.c.services.releases.get(id as usize) else { return format!("Release release{id}?") };
        let file = r.file.map_or_else(|| self.name(r.name), |k| self.file(k));
        format!("Release {}{}{}", self.place(r.record), self.sender(r.from.as_ref()), attrs('{', vec![format!("file {file}"), format!("sort-return {}", self.place(r.sort_return))]))
    }

    fn return_op(&self, id: ReturnId) -> String {
        let Some(r) = self.c.services.returns.get(id as usize) else { return format!("Return return{id}?") };
        let file = r.file.map_or_else(|| self.name(r.name), |k| self.file(k));
        let into = r.into.as_ref().map_or_else(String::new, |(p, plan)| format!(" into {}{}", self.place(*p), self.moved(plan, SenderCheck::None)));
        format!("Return {file}{into}{}", attrs('{', vec![format!("sort-return {}", self.place(r.sort_return))]))
    }

    fn report_name(&self, report: u32) -> String {
        self.c.services.report.reports.get(report as usize).map_or_else(|| format!("report{report}?"), |r| word(&r.name))
    }

    fn report_op(&self, op: &ReportOp) -> String {
        match *op {
            ReportOp::Initiate(r) => format!("Report Initiate {}", self.report_name(r)),
            ReportOp::Generate { report, detail } => {
                let group = detail.map_or_else(String::new, |d| {
                    let name = self.c.services.report.reports.get(report as usize).and_then(|r| r.groups.get(d as usize)).and_then(|g| g.name.as_deref());
                    format!(" detail {}", name.map_or_else(|| format!("group{d}"), word))
                });
                format!("Report Generate {}{group}", self.report_name(report))
            }
            ReportOp::Terminate(r) => format!("Report Terminate {}", self.report_name(r)),
            ReportOp::Suppress => "Report Suppress".to_owned(),
        }
    }

    fn invoke(&self, id: InvokeId) -> String {
        let Some(i) = self.c.services.invokes.get(id as usize) else { return format!("Invoke invoke{id}?") };
        let receiver = match &i.receiver {
            Receiver::SelfRef => "SELF".to_owned(),
            Receiver::Super => "SUPER".to_owned(),
            Receiver::Class { name, external } if name == external => format!("class {}", self.name(*name)),
            Receiver::Class { name, external } => format!("class {} external {}", self.name(*name), self.string(*external)),
            Receiver::Object(p) => self.place(*p),
        };
        let method = match &i.method {
            MethodName::New => "NEW".to_owned(),
            MethodName::Named(s) => self.literal(*s),
            MethodName::Dynamic(p) => self.place(*p),
        };
        let mut text = format!("Invoke {receiver} {method}");
        if !i.args.is_empty() {
            text += &format!(" using {}", join(i.args.iter().map(|(o, sig)| format!("{} as {}", self.operand(o), self.string(*sig))), ", "));
        }
        if let Some((p, sig)) = &i.returning {
            text += &format!(" returning {} as {}", self.place(*p), self.string(*sig));
        }
        text + &attrs('{', [yes(i.on_exception, "on exception"), yes(i.not_on_exception, "not on exception")].into_iter().flatten().collect())
    }

    fn cics(&self, id: CicsId) -> String {
        let Some(written) = self.c.services.cics.get(id as usize) else { return format!("Cics cics{id}?") };
        let Ok(CicsCommand { name: _, command, resp, sinks }) = written.clone().map(&mut Names(self));
        let mut words = vec![command.name().to_owned()];
        self.cics_options(&command, &mut words);
        option(&mut words, "RESP", &resp.resp);
        option(&mut words, "RESP2", &resp.resp2);
        flag(&mut words, "NOHANDLE", resp.nohandle);
        let mut how = Vec::new();
        if self.symbol(written.name) != Some(command.name()) {
            how.push(format!("written {}", self.string(written.name)));
        }
        if !sinks.is_empty() {
            how.push(format!("sinks {}", join(sinks.iter().map(|(p, s)| format!("{p} {}", s.kind())), " ")));
        }
        format!("Cics {}{}", words.join(" "), attrs('{', how))
    }

    fn sql(&self, ordinal: SqlId) -> String {
        let Some(e) = (ordinal as usize).checked_sub(1).and_then(|k| self.sql.get(k)) else { return format!("Sql {ordinal}") };
        let text = if self.symbol(e.text).is_some_and(|t| !t.is_empty()) { self.string(e.text) } else { self.name(e.verb) };
        let hosts = |key: &str, list: &[super::HostPlace]| if list.is_empty() { String::new() } else { format!(" {key} ({})", join(list.iter().map(|h| self.host(h)), ", ")) };
        let statement = match &e.statement {
            SqlStatement::Query { inputs, into } => format!("query{}{}", hosts("inputs", inputs), hosts("into", into)),
            SqlStatement::Change { delete, inputs, current_of } => {
                let current = current_of.map_or_else(String::new, |c| format!(" current of {}", self.name(c)));
                format!("{}{}{current}", if *delete { "delete" } else { "change" }, hosts("inputs", inputs))
            }
            SqlStatement::Open { cursor, inputs } => format!("open {}{}", self.name(*cursor), hosts("inputs", inputs)),
            SqlStatement::Fetch { cursor, into } => format!("fetch {}{}", self.name(*cursor), hosts("into", into)),
            SqlStatement::Close { cursor } => format!("close {}", self.name(*cursor)),
            SqlStatement::Commit => "commit".to_owned(),
            SqlStatement::Rollback => "rollback".to_owned(),
            SqlStatement::Declaration => "declaration".to_owned(),
            SqlStatement::Unsupported(what) => format!("unsupported {}", self.string(*what)),
            SqlStatement::Connect { what, location } => format!("connect {}{}", self.string(*what), hosts("location", location)),
            SqlStatement::Prepare { name, source } => format!("prepare {}{}", self.name(*name), hosts("from", source)),
            SqlStatement::ExecuteImmediate { source } => format!("execute immediate{}", hosts("from", source)),
            SqlStatement::Execute { name, inputs } => format!("execute {}{}", self.name(*name), hosts("using", inputs)),
            SqlStatement::OpenPrepared { cursor, statement, inputs } => format!("open {} for {}{}", self.name(*cursor), self.name(*statement), hosts("using", inputs)),
            SqlStatement::Describe { name, descriptor, names } => format!("describe {} into {} {names:?}", self.name(*name), self.place(*descriptor)),
            SqlStatement::PrepareInto { name, source, descriptor, names } => format!("prepare {} into {} {names:?}{}", self.name(*name), self.place(*descriptor), hosts("from", source)),
            SqlStatement::ExecuteDescriptor { name, descriptor } => format!("execute {} using descriptor {}", self.name(*name), self.place(*descriptor)),
            SqlStatement::OpenDescriptor { cursor, statement, descriptor } => format!("open {} for {} using descriptor {}", self.name(*cursor), self.name(*statement), self.place(*descriptor)),
            SqlStatement::FetchDescriptor { cursor, descriptor } => format!("fetch {} using descriptor {}", self.name(*cursor), self.place(*descriptor)),
            SqlStatement::FetchRowset { cursor, rows, into, enabled } => {
                let disabled = if *enabled { "" } else { " (no rowset positioning)" };
                format!("fetch rowset {}{disabled}{}{}", self.name(*cursor), self.rows(rows), self.arrays("into", into))
            }
            SqlStatement::InsertRows { inputs, rows, atomic } => format!("insert rows{}{}{}", if *atomic { " atomic" } else { " not atomic" }, self.rows(rows), self.arrays("inputs", inputs)),
            SqlStatement::Call { procedure, args } => format!("call {}{}", self.name(*procedure), hosts("arguments", args)),
        };
        format!("Sql {ordinal} {text} {statement}{}", attrs('{', yes(e.with_hold, "with hold").into_iter().collect()))
    }

    fn rows(&self, rows: &super::RowCount) -> String {
        match rows {
            super::RowCount::Implicit => String::new(),
            super::RowCount::Constant(n) => format!(" for {n} rows"),
            super::RowCount::Host(h) => format!(" for ({}) rows", self.host(h)),
        }
    }

    fn arrays(&self, key: &str, list: &[super::HostArray]) -> String {
        if list.is_empty() {
            return String::new();
        }
        let array = |a: &super::HostArray| match a.array {
            Some(d) => format!("{} array {} by {} indicator by {}", self.host(&a.place), d.count, d.stride, d.indicator_stride),
            None => self.host(&a.place),
        };
        format!(" {key} ({})", join(list.iter().map(array), ", "))
    }

    fn host(&self, h: &super::HostPlace) -> String {
        let mut text = self.place(h.var);
        if let Some((offset, len)) = h.member {
            text += &format!(" member +{offset} len {len}");
        }
        text += &match &h.ty {
            Ok(ty) => format!(" [{}]", host_type(ty)),
            Err(a) => format!(" [refused {}]", abend_ref(*a)),
        };
        if let Some((p, offset)) = h.indicator {
            text += &format!(" indicator {} +{offset}", self.place(p));
        }
        text
    }

    fn ccsid(&self, c: &Ccsid) -> Option<String> {
        match c {
            Ccsid::Unnamed => None,
            Ccsid::CodePage => Some("encoding codepage".to_owned()),
            Ccsid::Operand(o) => Some(format!("encoding {}", self.operand(o))),
        }
    }

    fn markup(&self, id: MarkupId) -> String {
        let Some(m) = self.c.services.markup.get(id as usize) else { return format!("Markup markup{id}?") };
        let subscripts = |s: &[IntExpr]| (!s.is_empty()).then(|| format!("subscripts ({})", join(s.iter().map(|e| self.int(e)), ", ")));
        let (verb, words) = match m {
            Markup::JsonGenerate(g) => {
                let name = g.name.map_or_else(|| "name omitted".to_owned(), |n| format!("name {}", self.raw(n)));
                let mut words = vec![format!("{} <- {}", self.place(g.receiver), self.place(g.from))];
                words.extend(subscripts(&g.subscripts));
                words.push(name);
                words.extend(self.ccsid(&g.encoding));
                words.extend(g.count.as_ref().map(|c| format!("count {}", self.stored(c))));
                words.push(format!("code {}", self.stored(&g.code)));
                ("JSON GENERATE", words)
            }
            Markup::XmlGenerate(g) => {
                let mut words = vec![format!("{} <- {}", self.place(g.receiver), self.place(g.from))];
                words.extend(subscripts(&g.subscripts));
                words.extend(self.ccsid(&g.encoding));
                words.extend(g.namespace.as_ref().map(|o| format!("namespace {}", self.operand(o))));
                words.extend(g.prefix.as_ref().map(|o| format!("prefix {}", self.operand(o))));
                words.extend(yes(g.declaration, "declaration"));
                words.extend(yes(g.suppressing, "suppress"));
                words.extend(g.count.as_ref().map(|c| format!("count {}", self.stored(c))));
                words.push(format!("code {}", self.stored(&g.code)));
                ("XML GENERATE", words)
            }
            Markup::XmlParse(p) => {
                let mut words = vec![self.place(p.document)];
                words.extend(p.encoding.as_ref().map(|o| format!("encoding {}", self.operand(o))));
                words.extend(yes(p.national, "returning national"));
                words.push(format!("procedure {}", self.range(p.procedure)));
                words.push(format!("event {}", self.place(p.event)));
                words.push(format!("code {}", self.stored(&p.code)));
                words.push(format!("information {}", self.stored(&p.information)));
                words.push(format!("code-value {}", self.int(&p.code_value)));
                ("XML PARSE", words)
            }
            Markup::JsonParse(p) => {
                let mut words = vec![format!("{} <- {}", self.place(p.into), self.place(p.source))];
                words.extend(subscripts(&p.subscripts));
                words.extend(self.ccsid(&p.encoding));
                words.extend(yes(p.ignore_all, "ignoring null for all"));
                words.push(format!("code {}", self.stored(&p.code)));
                words.push(format!("status {}", self.stored(&p.status)));
                ("JSON PARSE", words)
            }
        };
        let (on, not_on) = m.phrases();
        let phrases = [yes(on, "on exception"), yes(not_on, "not on exception")].into_iter().flatten().collect();
        format!("Markup m{id} {verb} {}{}", words.join(" "), attrs('{', phrases))
    }

    fn tables(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (k, p) in self.c.places.iter().enumerate() {
            writeln!(f, "{}", self.place_row(k, p))?;
        }
        for (k, c) in self.c.consts.iter().enumerate() {
            writeln!(f, "const c{k} {}", self.constant(c))?;
        }
        for (k, a) in self.c.abends.iter().enumerate() {
            writeln!(f, "abend a{k} {} {}{}", a.code, self.string(a.message), a.at.map_or_else(String::new, |at| self.at(at)))?;
        }
        let s = self.c.services;
        for (k, d) in s.files.iter().enumerate() {
            writeln!(f, "file {k} {}", self.file_row(d))?;
        }
        for (k, e) in s.entries.iter().enumerate() {
            let using = join(e.using.iter().map(|&r| self.linkage(r)), " ");
            writeln!(f, "entry {k} {} paragraph {} block {} using ({using})", self.name(e.name), self.para(e.paragraph), label(e.block))?;
        }
        for (k, plan) in self.c.plans.init.iter().enumerate() {
            for field in &plan.fields {
                let value = match &field.value {
                    InitValue::Default(v) => figurative(*v).to_owned(),
                    InitValue::Value(c) => self.konst(*c),
                    InitValue::Replacing(o) => format!("replacing {}", self.operand(o)),
                };
                let scaling = if field.scaling == 0 { String::new() } else { format!(" scaling {}", field.scaling) };
                writeln!(f, "init {k} +{} len {} <- {value}{}{scaling}", field.offset, field.len, self.moved(&field.store, SenderCheck::None))?;
            }
        }
        for (field, p, ty) in &s.sqlca.fields {
            writeln!(f, "sqlca {} {} [{}]", sqlca_field(*field), self.place(*p), host_type(ty))?;
        }
        let modes = s.declaratives.modes.iter().zip(MODES).filter_map(|(r, mode)| r.map(|r| format!("{mode} {}", self.range(r))));
        for mode in modes {
            writeln!(f, "declaratives {mode}")?;
        }
        if let Some((offset, len)) = s.declaratives.debug_item {
            writeln!(f, "declaratives debug-item program+{offset} len {len}")?;
        }
        self.scope(f)?;
        if let Some(d) = &s.function {
            writeln!(f, "function params ({}) returning {}", self.places(&d.params), self.place(d.returning))?;
        }
        self.reports(f)?;
        for (k, m) in s.markup.iter().enumerate() {
            self.markup_nodes(f, k, m)?;
        }
        if let Some(class) = &s.class {
            self.class(f, class)?;
        }
        Ok(())
    }

    fn place_row(&self, k: usize, p: &Place) -> String {
        let mut text = format!("place p{k} {} {}+{} len {} {}", self.reference(p, &[]), base(p.base), p.offset, p.len, kind(&p.kind));
        if p.scaling != 0 {
            text += &format!(" scaling {}", p.scaling);
        }
        for o in &p.moved {
            text += &format!(" moved {}", self.odo(o));
        }
        for s in &p.subscripts {
            text += &format!(" stride {}", s.stride);
        }
        if let Some(t) = p.table {
            text += &format!(" table check {}+{}", t.displacement, t.extent);
        }
        for o in &p.odo {
            text += &format!(" odo {}", self.odo(o));
        }
        if p.refmod.as_ref().is_some_and(|r| r.check) {
            text += " refmod check";
        }
        match p.numcheck.lax {
            None => {}
            Some(LaxRedefinition::Signed) => text += " numcheck lax signed",
            Some(LaxRedefinition::LeadingSpaces(n)) => text += &format!(" numcheck lax {n} leading spaces"),
        }
        if p.numcheck.removed {
            text += " numcheck removed";
        }
        text + &self.at(p.at)
    }

    fn file_row(&self, d: &FileDesc) -> String {
        let organization = match d.organization {
            Organization::Sequential => "sequential",
            Organization::LineSequential => "line-sequential",
            Organization::Indexed => "indexed",
            Organization::Relative => "relative",
        };
        let access = match d.access {
            Access::Sequential => "sequential",
            Access::Random => "random",
            Access::Dynamic => "dynamic",
        };
        let format = match d.format {
            Format::Fixed => "fixed",
            Format::Variable => "variable",
            Format::Text => "text",
        };
        let mut text = format!("{} assign {} {organization} access {access} format {format} read {} to {}", self.name(d.name), self.name(d.assign), d.read_lengths.0, d.read_lengths.1);
        let flags = [yes(d.optional, "optional"), yes(d.fixed, "fixed-length"), yes(d.sort, "sort")];
        text += &join(flags.into_iter().flatten().map(|s| format!(" {s}")), "");
        if let Some(min) = d.record_min {
            text += &format!(" record-min {min}");
        }
        if let Some(dep) = &d.depending {
            text += &format!(" depending {} from {} to {}", self.place(dep.item), dep.lengths.0, dep.lengths.1);
        }
        if let Some((p, plan)) = &d.status {
            text += &format!(" status {}{}", self.place(*p), self.moved(plan, SenderCheck::None));
        }
        if let Some(keys) = &d.keys {
            text += &format!(" key +{} len {}", keys.prime.offset, keys.prime.len);
            for (span, duplicates) in &keys.alternates {
                text += &format!(" alternate +{} len {}{}", span.offset, span.len, if *duplicates { " duplicates" } else { "" });
            }
            for (key, pieces) in &keys.split {
                let pieces: Vec<String> = pieces.iter().map(|p| format!("+{} len {}", p.offset, p.len)).collect();
                text += &format!(" split {key} = {}", pieces.join(", "));
            }
        }
        if let Some(r) = &d.relative {
            let digits = r.digits.map_or_else(String::new, |n| format!(" digits {n}"));
            text += &format!(" relative-key {} [{}] value {}{digits}", self.place(r.place), self.store(&r.store), self.int(&r.value));
        }
        if let Some(l) = &d.linage {
            text += &format!(" linage {}", self.int(&l.lines));
            for (key, value) in [("footing", &l.footing), ("top", &l.top), ("bottom", &l.bottom)] {
                if let Some(v) = value {
                    text += &format!(" {key} {}", self.int(v));
                }
            }
            if let Some(c) = &l.counter {
                text += &format!(" counter {}", self.stored(c));
            }
        }
        if let Some(c) = d.carriage {
            text += &format!(" carriage {}{}", if c.machine { "machine" } else { "asa" }, if c.reserved { " reserved" } else { "" });
        }
        if let Some(r) = d.error {
            text += &format!(" error {}", self.range(r));
        }
        if let Some(a) = d.assign_item {
            text += &format!(" assign-item {} @{}", self.place(a.place), a.select);
        }
        text
    }

    fn scope(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = &self.c.services.scope;
        for &c in &s.containers {
            writeln!(f, "scope container {}", self.name(c))?;
        }
        for &c in &s.callable {
            writeln!(f, "scope callable {}", self.name(c))?;
        }
        for &c in &s.hidden {
            writeln!(f, "scope hidden {}", self.name(c))?;
        }
        for (record, binding) in &s.records {
            let binding = match binding {
                Binding::External { name, size } => format!("external {} size {size}", self.name(*name)),
                Binding::ExternalFile(k) => format!("external file {}", self.file(*k)),
                Binding::Global { program, section, name } => format!("global {} of {} in {}", self.name(*name), scope_section(*section), self.name(*program)),
            };
            writeln!(f, "scope record {} {binding}", self.linkage(*record))?;
        }
        for shared in &s.files {
            let from = match (shared.external, shared.declared_in) {
                (true, _) => "external".to_owned(),
                (false, Some(p)) => format!("global in {}", self.name(p)),
                (false, None) => "global".to_owned(),
            };
            writeln!(f, "scope file {} {from}", self.file(shared.file))?;
        }
        for (file, record) in &s.areas {
            writeln!(f, "scope area {} {}", self.file(*file), self.linkage(*record))?;
        }
        for g in &s.globals {
            let at = match g.at {
                GlobalAt::Program(offset) => format!("program+{offset}"),
                GlobalAt::Local(offset) => format!("local+{offset}"),
                GlobalAt::Linkage(record) => self.linkage(record),
            };
            writeln!(f, "scope global {} of {} at {at}", self.name(g.name), scope_section(g.section))?;
        }
        for (file, range) in &s.global_files {
            writeln!(f, "scope global-error {} {}", self.file(*file), self.range(*range))?;
        }
        for (mode, r) in MODES.iter().zip(&s.global_modes) {
            if let Some(r) = r {
                writeln!(f, "scope global-error {mode} {}", self.range(*r))?;
            }
        }
        Ok(())
    }

    fn reports(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let w = &self.c.services.report;
        if let Some(p) = w.print_switch {
            writeln!(f, "report print-switch {}", self.place_at(p))?;
        }
        for (k, r) in w.reports.iter().enumerate() {
            let mut text = format!("report {k} {} file {} width {}", word(&r.name), self.file_at(r.file), r.width);
            if let Some(c) = r.code {
                text += &format!(" code {}", self.konst(c));
            }
            if let Some(p) = r.page {
                text += &format!(" page limit {} heading {} first-detail {} last-detail {} footing {}", p.limit, p.heading, p.first_detail, p.last_detail, p.footing);
            }
            text += &format!(" page-counter {} line-counter {} state {}", self.place_at(r.page_counter), self.place_at(r.line_counter), self.place_at(r.state));
            let headings = [("report-heading", r.report_heading), ("page-heading", r.page_heading), ("page-footing", r.page_footing), ("report-footing", r.report_footing)];
            for (key, group) in headings {
                if let Some(g) = group {
                    text += &format!(" {key} group {g}");
                }
            }
            for (key, list) in [("control-headings", &r.control_headings), ("control-footings", &r.control_footings)] {
                if list.iter().any(Option::is_some) {
                    text += &format!(" {key} ({})", join(list.iter().map(|g| g.map_or_else(|| "-".to_owned(), |g| g.to_string())), " "));
                }
            }
            if let Some(n) = r.first_detail_written {
                text += &format!(" first-detail-written {n}");
            }
            writeln!(f, "{text}")?;
            for (c, control) in r.controls.iter().enumerate() {
                writeln!(f, "report {k} control {c} {} saved +{} len {}", self.place(control.reference), control.saved, control.len)?;
            }
            for (g, group) in r.groups.iter().enumerate() {
                let name = group.name.as_deref().map_or_else(|| "-".to_owned(), word);
                let mut text = format!("report {k} group {g} {name} {} level {}", group_kind(group.kind), group.level);
                if let Some(n) = group.next_group {
                    text += &format!(" next-group {}", next_group(n));
                }
                if let Some(i) = group.indicate {
                    text += &format!(" indicate +{i}");
                }
                if let Some(r) = group.declarative {
                    text += &format!(" declarative {}", self.range(r));
                }
                if !group.totals.is_empty() {
                    text += &format!(" totals ({})", join(group.totals.iter().map(|t| format!("sum{t}")), " "));
                }
                writeln!(f, "{text}")?;
                for (l, line) in group.lines.iter().enumerate() {
                    writeln!(f, "report {k} group {g} line {l} {}", line_number(line.number))?;
                    for field in &line.fields {
                        writeln!(f, "report {k} group {g} line {l} {}", self.report_field(field))?;
                    }
                }
                for field in &group.unprinted {
                    writeln!(f, "report {k} group {g} unprinted {}", self.report_field(field))?;
                }
                for (key, list) in [("cross", &group.cross), ("roll", &group.rolls)] {
                    for (sum, origin) in list {
                        writeln!(f, "report {k} group {g} {key} sum{sum} <- {}", self.origin(origin))?;
                    }
                }
            }
            for (s, sum) in r.sums.iter().enumerate() {
                let reset = sum.reset.map_or_else(String::new, |l| format!(" reset on control {l}"));
                writeln!(f, "report {k} sum{s} {}{reset}", self.place_at(sum.total))?;
            }
            for t in &r.subtotals {
                let adding = match &t.adding {
                    Adding::EveryGenerate => "every generate".to_owned(),
                    Adding::Upon(groups) => format!("upon groups ({})", join(groups.iter().map(usize::to_string), " ")),
                    Adding::Correlated(groups) => format!("correlated groups ({})", join(groups.iter().map(usize::to_string), " ")),
                };
                writeln!(f, "report {k} subtotal sum{} <- {} {adding}", t.sum, self.comparand(&t.operand))?;
            }
        }
        Ok(())
    }

    fn place_at(&self, place: usize) -> String {
        u32::try_from(place).map_or_else(|_| format!("p{place}?"), |p| self.place(p))
    }

    fn report_field(&self, field: &Field) -> String {
        let content = match &field.content {
            FieldContent::Source(x) => format!("source {}", self.comparand(x)),
            FieldContent::Value(v) => format!("value {}", self.konst(*v)),
            FieldContent::Sum(s) => format!("sum{s}"),
            FieldContent::Program => "program".to_owned(),
        };
        let flags = [yes(field.group_indicate, "group indicate"), yes(field.blank_when_zero, "blank-when-zero"), yes(field.rounded, "rounded")];
        format!("column {} {} <- {content}{}  @{}", field.column, self.place_at(field.item), attrs('{', flags.into_iter().flatten().collect()), self.position(&field.pos))
    }

    fn origin(&self, origin: &Origin) -> String {
        match origin {
            Origin::Source(x) => self.comparand(x),
            Origin::Value(v) => self.konst(*v),
            Origin::Total(s) => format!("sum{s}"),
        }
    }

    fn markup_nodes(&self, f: &mut fmt::Formatter<'_>, k: usize, m: &Markup) -> fmt::Result {
        let rows: Vec<String> = match m {
            Markup::JsonGenerate(g) => g.nodes.iter().map(|n| self.json_node(n)).collect(),
            Markup::XmlGenerate(g) => g.nodes.iter().map(|n| self.xml_node(n)).collect(),
            Markup::XmlParse(_) => Vec::new(),
            Markup::JsonParse(p) => p.nodes.iter().map(|n| self.parse_node(n)).collect(),
        };
        for (n, row) in rows.iter().enumerate() {
            writeln!(f, "markup m{k} node {n} {row}")?;
        }
        Ok(())
    }

    /// What every node of a markup tree has: where it is, its kind, and how often it occurs.
    fn node(&self, name: String, offset: u32, moved: &[Odo], len: u32, kind_of: &Kind, occurs: Option<&Count>) -> String {
        let mut text = format!("{name} +{offset} len {len} {}", kind(kind_of));
        if let Some(c) = occurs {
            text += &format!(" occurs {}", self.count(c));
        }
        for o in moved {
            text += &format!(" moved {}", self.odo(o));
        }
        text
    }

    fn marker(&self, m: &Marker) -> String {
        match m {
            Marker::Byte(Some(b)) => format!("byte {}", hex("X", &[*b])),
            Marker::Byte(None) => "byte none".to_owned(),
            Marker::Condition(c) => format!("condition ({})", self.cond(*c)),
            Marker::Refused(a) => format!("refused {}", abend_ref(*a)),
        }
    }

    fn convert(c: &Convert) -> String {
        match c {
            Convert::Chars { justified } => if *justified { "chars justified" } else { "chars" }.to_owned(),
            Convert::National => "national".to_owned(),
            Convert::Dbcs => "dbcs".to_owned(),
            Convert::Float(p) => format!("float {}", precision(*p)),
            Convert::Fixed { integers } => format!("fixed {integers} integers"),
            Convert::Scaled { integers, scaling } => format!("fixed {integers} integers scaled {scaling}"),
            Convert::Refused(a) => format!("refused {}", abend_ref(*a)),
        }
    }

    fn suppress(list: &[Figurative]) -> String {
        if list.is_empty() { String::new() } else { format!(" suppress {}", join(list.iter().map(|f| figurative(*f).to_owned()), " ")) }
    }

    fn json_node(&self, n: &JsonNode) -> String {
        let mut text = self.node(self.raw(n.name), n.offset, &n.moved, n.len, &n.kind, n.occurs.as_ref());
        if let Some((place, marker)) = &n.indicator {
            let place = place.map_or_else(|a| format!("refused {}", abend_ref(a)), |p| self.place(p));
            text += &format!(" indicator {place} {}", self.marker(marker));
        }
        if let Some(null) = n.null {
            text += &format!(" null {}", figurative(null));
        }
        text + &match &n.value {
            JsonValue::Object { members, eligible } => format!(" object ({}){}", members_list(members), if *eligible { " eligible" } else { "" }),
            JsonValue::Leaf(JsonLeaf { suppress, boolean, convert }) => {
                let boolean = boolean.as_ref().map_or_else(String::new, |m| format!(" boolean {}", self.marker(m)));
                format!(" leaf{}{boolean} {}", Self::suppress(suppress), Self::convert(convert))
            }
        }
    }

    fn xml_node(&self, n: &XmlNode) -> String {
        let text = self.node(self.string(n.name), n.offset, &n.moved, n.len, &n.kind, n.occurs.as_ref());
        text + &match &n.value {
            XmlValue::Element { members } => format!(" element ({})", members_list(members)),
            XmlValue::Members { members } => format!(" members ({})", members_list(members)),
            XmlValue::Leaf { form, suppress, convert } => {
                let form = match form {
                    XmlForm::Attribute => "attribute",
                    XmlForm::Element => "element",
                    XmlForm::Content => "content",
                };
                format!(" leaf {form}{} {}", Self::suppress(suppress), Self::convert(convert))
            }
        }
    }

    fn set_to(&self, s: &SetTo) -> String {
        match s {
            SetTo::Nothing => "nothing".to_owned(),
            SetTo::Move { place, value, plan } => format!("{} <- {}{}", self.place(*place), self.konst(*value), self.moved(plan, SenderCheck::None)),
            SetTo::Refused(a) => format!("refused {}", abend_ref(*a)),
        }
    }

    fn flag(&self, flag: &Flag) -> String {
        match flag {
            Flag::Set { on, off } => format!("set on {} off {}", self.set_to(on), self.set_to(off)),
            Flag::Literals { on, off } => format!(
                "literals on {}{} off {}{}",
                self.konst(on.0),
                self.moved(&on.1, SenderCheck::None),
                self.konst(off.0),
                self.moved(&off.1, SenderCheck::None)
            ),
        }
    }

    fn parse_node(&self, n: &ParseNode) -> String {
        let name = match n.name {
            Named::Exactly(s) => self.string(s),
            Named::Folded(s) => format!("folded {}", self.name(s)),
            Named::Omitted => "omitted".to_owned(),
        };
        let mut text = self.node(name, n.offset, &n.moved, n.len, &n.kind, n.occurs.as_ref());
        if n.ignored {
            text += " ignoring null";
        }
        if let Some(Indicator { place, flag }) = &n.indicator {
            let place = match place {
                None => "-".to_owned(),
                Some(Ok(p)) => self.place(*p),
                Some(Err(a)) => format!("refused {}", abend_ref(*a)),
            };
            text += &format!(" indicator {place} {}", self.flag(flag));
        }
        if let Some((null, plan)) = &n.null {
            text += &format!(" null {}{}", figurative(*null), self.moved(plan, SenderCheck::None));
        }
        text + &match &n.value {
            ParseValue::Object { members } => format!(" object ({})", members_list(members)),
            ParseValue::Suppressed => " suppressed".to_owned(),
            ParseValue::Leaf(ParseLeaf { boolean, text, number }) => {
                let mut leaf = " leaf".to_owned();
                if let Some(b) = boolean {
                    leaf += &format!(" boolean {}", self.flag(b));
                }
                if let Some(plan) = text {
                    leaf += &format!(" text{}", self.moved(plan, SenderCheck::None));
                }
                leaf + &match number {
                    NumberInto::Float(plan) => format!(" number float{}", self.moved(plan, SenderCheck::None)),
                    NumberInto::Store(store) => format!(" number [{}]", self.store(store)),
                    NumberInto::Edited(plan) => format!(" number edited{}", self.moved(plan, SenderCheck::None)),
                    NumberInto::Digits => " number digits".to_owned(),
                    NumberInto::Incompatible => " number incompatible".to_owned(),
                    NumberInto::StoreScaled { store, scaling } => format!(" number [{}] scaled {scaling}", self.store(store)),
                    NumberInto::EditedScaled { plan, scaling } => format!(" number edited{} scaled {scaling}", self.moved(plan, SenderCheck::None)),
                }
            }
        }
    }

    /// A class's data and methods are programs of their own, each printed after its heading.
    fn class(&self, f: &mut fmt::Formatter<'_>, class: &Class) -> fmt::Result {
        writeln!(f, "class {} inherits {}", self.string(class.external), self.string(class.parent))?;
        for (key, part) in [("factory", &class.factory), ("object", &class.object)] {
            if let Some(part) = part {
                writeln!(f, "class {key} data records ({})", join(part.records.iter().map(|r| format!("+{r}")), " "))?;
                write!(f, "{}", Listing::of(&part.data))?;
            }
        }
        for (k, m) in class.methods.iter().enumerate() {
            let params = join(m.params.iter().map(|&s| self.string(s)), " ");
            let returns = m.returns.map_or_else(String::new, |s| format!(" returns {}", self.string(s)));
            let factory = if m.factory { " factory" } else { "" };
            writeln!(f, "class method {k} {}{factory} params ({params}){returns} own-records {}", self.name(m.name), m.own_records)?;
            write!(f, "{}", Listing::of(&m.code))?;
        }
        Ok(())
    }
}

/// Maps a CICS command's handles to their text.
struct Names<'p, 'a>(&'p Printer<'a>);

impl Handles<PlaceId, Operand, SymId> for Names<'_, '_> {
    type Place = String;
    type Value = String;
    type Text = String;
    type Error = Infallible;

    fn place(&mut self, place: PlaceId) -> Result<String, Infallible> {
        Ok(self.0.place(place))
    }

    fn value(&mut self, value: Operand) -> Result<String, Infallible> {
        Ok(self.0.operand(&value))
    }

    fn text(&mut self, text: SymId) -> Result<String, Infallible> {
        Ok(self.0.name(text))
    }
}

type CicsOpt = Option<Datum<String, String, String>>;

fn option(words: &mut Vec<String>, key: &str, value: &CicsOpt) {
    match value {
        None => {}
        Some(Datum::Place(v) | Datum::Value(v) | Datum::Text(v)) => words.push(format!("{key}({v})")),
        Some(Datum::Bare) => words.push(key.to_owned()),
    }
}

fn flag(words: &mut Vec<String>, key: &str, on: bool) {
    if on {
        words.push(key.to_owned());
    }
}

impl Printer<'_> {
    /// A command's options as EXEC CICS writes them, in the order of its fields.
    fn cics_options(&self, command: &Cics<String, String, String>, w: &mut Vec<String>) {
        match command {
            Cics::File { verb: _, file, options: o } => {
                option(w, "FILE", file);
                for (key, value) in [("RIDFLD", &o.ridfld), ("KEYLENGTH", &o.keylength), ("REQID", &o.reqid), ("FROM", &o.from), ("NUMREC", &o.numrec)] {
                    option(w, key, value);
                }
                for (key, value) in [("INTO", &o.record.into), ("SET", &o.record.set), ("LENGTH", &o.record.length)] {
                    option(w, key, value);
                }
                for (key, on) in [("GENERIC", o.generic), ("RRN", o.rrn), ("GTEQ", o.gteq), ("EQUAL", o.equal), ("UPDATE", o.update)] {
                    flag(w, key, on);
                }
            }
            Cics::Return { transid, commarea, length, channel, immediate } => {
                for (key, value) in [("TRANSID", transid), ("COMMAREA", commarea), ("LENGTH", length), ("CHANNEL", channel)] {
                    option(w, key, value);
                }
                flag(w, "IMMEDIATE", *immediate);
            }
            Cics::Link(t) | Cics::Xctl(t) => {
                for (key, value) in [("PROGRAM", &t.program), ("COMMAREA", &t.commarea), ("LENGTH", &t.length)] {
                    option(w, key, value);
                }
            }
            Cics::Abend { abcode, cancel } => {
                option(w, "ABCODE", abcode);
                flag(w, "CANCEL", *cancel);
            }
            Cics::HandleCondition(labels) => {
                w.extend(labels.iter().map(|(c, label)| label.map_or_else(|| c.name().to_owned(), |p| format!("{}({})", c.name(), self.para(p)))));
            }
            Cics::IgnoreCondition(conditions) => w.extend(conditions.iter().map(|c| c.name().to_owned())),
            Cics::PushHandle | Cics::PopHandle | Cics::HandleAid | Cics::Freemain | Cics::Enq | Cics::Deq | Cics::Delay | Cics::Unsupported => {}
            Cics::HandleAbend { program, label, reset } => {
                option(w, "PROGRAM", program);
                if let Some(p) = label {
                    w.push(format!("LABEL({})", self.para(*p)));
                }
                flag(w, "RESET", *reset);
            }
            Cics::SendMap { map, mapset, from, maponly, dataonly, cursor, control } => {
                for (key, value) in [("MAP", map), ("MAPSET", mapset), ("FROM", from), ("CURSOR", cursor)] {
                    option(w, key, value);
                }
                flag(w, "MAPONLY", *maponly);
                flag(w, "DATAONLY", *dataonly);
                control_flags(w, control);
            }
            Cics::ReceiveMap { map, mapset, into, set } => {
                for (key, value) in [("MAP", map), ("MAPSET", mapset), ("INTO", into), ("SET", set)] {
                    option(w, key, value);
                }
            }
            Cics::SendControl { cursor, control } => {
                option(w, "CURSOR", cursor);
                control_flags(w, control);
            }
            Cics::Receive(r) => {
                for (key, value) in [("INTO", &r.into), ("SET", &r.set), ("LENGTH", &r.length)] {
                    option(w, key, value);
                }
            }
            Cics::Asktime { abstime } => option(w, "ABSTIME", abstime),
            Cics::Formattime { abstime, datesep, timesep, outputs } => {
                for (key, value) in [("ABSTIME", abstime), ("DATESEP", datesep), ("TIMESEP", timesep)] {
                    option(w, key, value);
                }
                for (key, value) in outputs {
                    option(w, key, &Some(value.clone()));
                }
            }
            Cics::Assign(a) => {
                let options = [
                    ("APPLID", &a.applid),
                    ("SYSID", &a.sysid),
                    ("USERID", &a.userid),
                    ("NETNAME", &a.netname),
                    ("FACILITY", &a.facility),
                    ("STARTCODE", &a.startcode),
                    ("ABCODE", &a.abcode),
                    ("PROGRAM", &a.program),
                    ("CWALENG", &a.cwaleng),
                    ("TWALENG", &a.twaleng),
                ];
                for (key, value) in options {
                    option(w, key, value);
                }
            }
            Cics::Getmain { flength, length, initimg, set } => {
                for (key, value) in [("FLENGTH", flength), ("LENGTH", length), ("INITIMG", initimg), ("SET", set)] {
                    option(w, key, value);
                }
            }
            Cics::Syncpoint { rollback } => flag(w, "ROLLBACK", *rollback),
            Cics::Address { eib, commarea, cwa, twa } => {
                for (key, value) in [("EIB", eib), ("COMMAREA", commarea), ("CWA", cwa), ("TWA", twa)] {
                    option(w, key, value);
                }
            }
            Cics::SendText { from, length } => {
                option(w, "FROM", from);
                option(w, "LENGTH", length);
            }
            Cics::WriteOperator { text, textlength } => {
                option(w, "TEXT", text);
                option(w, "TEXTLENGTH", textlength);
            }
            Cics::WriteqTs { queue, from, length, rewrite, item, numitems } => {
                for (key, value) in [("QUEUE", queue), ("FROM", from), ("LENGTH", length), ("ITEM", item), ("NUMITEMS", numitems)] {
                    option(w, key, value);
                }
                flag(w, "REWRITE", *rewrite);
            }
            Cics::ReadqTs { queue, next, item, numitems, record } => {
                option(w, "QUEUE", queue);
                flag(w, "NEXT", *next);
                for (key, value) in [("ITEM", item), ("NUMITEMS", numitems), ("INTO", &record.into), ("SET", &record.set), ("LENGTH", &record.length)] {
                    option(w, key, value);
                }
            }
            Cics::DeleteqTs { queue } | Cics::DeleteqTd { queue } => option(w, "QUEUE", queue),
            Cics::WriteqTd { queue, from, length } => {
                for (key, value) in [("QUEUE", queue), ("FROM", from), ("LENGTH", length)] {
                    option(w, key, value);
                }
            }
            Cics::ReadqTd { queue, record } => {
                for (key, value) in [("QUEUE", queue), ("INTO", &record.into), ("SET", &record.set), ("LENGTH", &record.length)] {
                    option(w, key, value);
                }
            }
            Cics::Refused(why) => w.push(format!("{why:?}")),
        }
    }
}

fn control_flags(w: &mut Vec<String>, c: &crate::cics::Control) {
    for (key, on) in [("ERASE", c.erase), ("FREEKB", c.freekb), ("ALARM", c.alarm), ("FRSET", c.frset)] {
        flag(w, key, on);
    }
}

/// A name as it is, or quoted and escaped when it is not one word of printable characters.
fn word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| !c.is_control() && !c.is_whitespace()) { s.to_owned() } else { format!("{s:?}") }
}

/// A literal in quotes, a quote doubled within it, as COBOL writes one; escaped when it holds a
/// control character.
fn quote(s: &str) -> String {
    if s.chars().any(char::is_control) { format!("{s:?}") } else { format!("'{}'", s.replace('\'', "''")) }
}

fn hex(prefix: &str, bytes: &[u8]) -> String {
    format!("{prefix}'{}'", join(bytes.iter().map(|b| format!("{b:02X}")), ""))
}

/// UTF-16 units, big-endian, as N'...'; NX'...' when they are not printable text.
fn national(units: &[u8]) -> String {
    let (pairs, odd) = units.as_chunks::<2>();
    let text = odd
        .is_empty()
        .then(|| char::decode_utf16(pairs.iter().map(|&u| u16::from_be_bytes(u))).collect::<Result<String, _>>().ok())
        .flatten()
        .filter(|t| !t.chars().any(char::is_control));
    text.map_or_else(|| hex("NX", units), |t| format!("N{}", quote(&t)))
}

/// A number with all its decimal places.
fn decimal(n: &Fixed) -> String {
    const MOST: u32 = 100;
    if n.places.dec > MOST {
        return format!("{:?}", n.magnitude);
    }
    let mut digits = Vec::new();
    let mut rest = n.magnitude;
    while !rest.is_zero() {
        let (q, r) = rest.div_rem(U256::from_u128(10));
        digits.push(char::from(b'0' + r.to_u128().and_then(|d| u8::try_from(d).ok()).unwrap_or(0)));
        rest = q;
    }
    let dec = n.places.dec as usize;
    while digits.len() <= dec {
        digits.push('0');
    }
    digits.reverse();
    let (int, frac): (String, String) = (digits[..digits.len() - dec].iter().collect(), digits[digits.len() - dec..].iter().collect());
    let sign = if n.negative { "-" } else { "" };
    if frac.is_empty() { format!("{sign}{int}") } else { format!("{sign}{int}.{frac}") }
}

fn native_word(native: Native) -> &'static str {
    match native {
        Native::No => "",
        Native::Comp5 => " native",
        Native::BinaryChar => " binary-char",
        Native::CompX => " comp-x",
        Native::Comp5Bytes => " native-bytes",
    }
}

fn pic(digits: u32, scale: u32, signed: bool) -> String {
    let int = digits.saturating_sub(scale);
    let mut text = if signed { "S".to_owned() } else { String::new() };
    if int > 0 || scale == 0 {
        text += &format!("9({int})");
    }
    if scale > 0 {
        text += &format!("V9({scale})");
    }
    text
}

fn sign_clause(sign: Option<SignClause>) -> String {
    sign.map_or_else(String::new, |s| {
        let position = match s.position {
            SignPosition::Leading => "leading",
            SignPosition::Trailing => "trailing",
        };
        format!(" sign {position}{}", if s.separate { " separate" } else { "" })
    })
}

fn kind(k: &Kind) -> String {
    match *k {
        Kind::Group => "group".to_owned(),
        Kind::Alnum { justified } => if justified { "alnum justified" } else { "alnum" }.to_owned(),
        Kind::National => "national".to_owned(),
        Kind::Dbcs { justified, edit } => format!("dbcs{}{}", if justified { " justified" } else { "" }, edit.map_or(String::new(), |e| format!(" edit {e}"))),
        Kind::Zoned { digits, scale, signed, sign } => format!("zoned {}{}", pic(digits, scale, signed), sign_clause(sign)),
        Kind::Packed { digits, scale, signed } => format!("packed {}", pic(digits, scale, signed)),
        Kind::Binary { digits, scale, signed, native } => format!("binary {}{}", pic(digits, scale, signed), native_word(native)),
        Kind::Float(p) => format!("float {}", precision(p)),
        Kind::NumericEdited { edit, digits, scale, blank_when_zero } => {
            format!("numeric-edited edit {edit} {}{}", pic(digits, scale, false), if blank_when_zero { " blank-when-zero" } else { "" })
        }
        Kind::AlnumEdited { edit } => format!("alnum-edited edit {edit}"),
        Kind::Pointer => "pointer".to_owned(),
        Kind::Index => "index".to_owned(),
        Kind::ObjectReference => "object-reference".to_owned(),
        Kind::ProgramPointer => "program-pointer".to_owned(),
    }
}

fn base(b: Base) -> String {
    match b {
        Base::Program => "program".to_owned(),
        Base::Local => "local".to_owned(),
        Base::Linkage(n) => format!("linkage{n}"),
        Base::ReturnCode => "return-code".to_owned(),
        Base::Eib => "eib".to_owned(),
        Base::SelfRef => "self".to_owned(),
        Base::JniEnv => "jnienv".to_owned(),
        Base::Xml(r) => match r {
            XmlRegister::Text => "xml-text",
            XmlRegister::NText => "xml-ntext",
            XmlRegister::Namespace => "xml-namespace",
            XmlRegister::NNamespace => "xml-nnamespace",
            XmlRegister::Prefix => "xml-namespace-prefix",
            XmlRegister::NPrefix => "xml-nnamespace-prefix",
        }
        .to_owned(),
    }
}

fn precision(p: Precision) -> &'static str {
    match p {
        Precision::Short => "short",
        Precision::Long => "long",
        Precision::Extended => "extended",
    }
}

fn figurative(f: Figurative) -> &'static str {
    match f {
        Figurative::Zero => "ZERO",
        Figurative::Space => "SPACE",
        Figurative::HighValue => "HIGH-VALUE",
        Figurative::LowValue => "LOW-VALUE",
        Figurative::Quote => "QUOTE",
        Figurative::Null => "NULL",
    }
}

fn binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Pow => "**",
    }
}

fn relop(op: RelOp) -> &'static str {
    match op {
        RelOp::Eq => "=",
        RelOp::Ne => "<>",
        RelOp::Lt => "<",
        RelOp::Le => "<=",
        RelOp::Gt => ">",
        RelOp::Ge => ">=",
    }
}

fn compare(c: Compare) -> String {
    match c {
        Compare::PackedPfd => "packed-pfd".to_owned(),
        Compare::Address => "address".to_owned(),
        Compare::Float => "float".to_owned(),
        Compare::Fixed => "fixed".to_owned(),
        Compare::National => "national".to_owned(),
        Compare::Dbcs => "dbcs".to_owned(),
        Compare::Alphanumeric => "alnum".to_owned(),
        Compare::Refused(a) => format!("refused {}", abend_ref(a)),
        Compare::References => "references".to_owned(),
        Compare::ZonedBytes { zoned_first } => format!("zoned-bytes {}", if zoned_first { "left" } else { "right" }),
    }
}

fn byte_class(c: super::ByteClass) -> String {
    use super::ByteClass;
    let name = match c {
        ByteClass::Packed { signed: true } => "numeric [packed signed]",
        ByteClass::Packed { signed: false } => "numeric [packed]",
        ByteClass::Zoned { signed: true } => "numeric [zoned signed]",
        ByteClass::Zoned { signed: false } => "numeric [zoned]",
        ByteClass::Digits => "numeric [digits]",
        ByteClass::Alphabetic => "alphabetic",
        ByteClass::AlphabeticLower => "alphabetic-lower",
        ByteClass::AlphabeticUpper => "alphabetic-upper",
        ByteClass::Dbcs => "dbcs",
        ByteClass::Kanji => "kanji",
        ByteClass::Set { bits } => return format!("class [{}]", (0..=255u8).filter(|&b| bits[usize::from(b / 8)] >> (b % 8) & 1 == 1).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")),
    };
    name.to_owned()
}

fn sign_test(t: SignTest) -> &'static str {
    match t {
        SignTest::Positive => "positive",
        SignTest::Negative => "negative",
        SignTest::Zero => "zero",
    }
}

fn sql_test(t: SqlTest) -> &'static str {
    match t {
        SqlTest::Error => "error",
        SqlTest::NotFound => "not found",
        SqlTest::Warning => "warning",
    }
}

fn ending(e: Ending) -> &'static str {
    match e {
        Ending::Goback => "goback",
        Ending::StopRun => "stop-run",
        Ending::EndOfProgram => "end-of-program",
    }
}

fn range_kind(k: RangeKind) -> &'static str {
    match k {
        RangeKind::Perform => "perform",
        RangeKind::SortProcedure => "sort-procedure",
        RangeKind::UseBeforeReporting => "use-before-reporting",
        RangeKind::UseProcedure => "use-procedure",
        RangeKind::Debugging => "debugging",
        RangeKind::Processing => "processing",
    }
}

fn image(i: Image) -> String {
    match i {
        Image::Bytes => String::new(),
        Image::All => " all".to_owned(),
        Image::Figurative => " figurative".to_owned(),
        Image::Digits { digits } => format!(" digits {digits}"),
        Image::Stored => " stored".to_owned(),
    }
}

fn national_from(n: NationalFrom) -> &'static str {
    match n {
        NationalFrom::Units => "units",
        NationalFrom::Decoded => "decoded",
        NationalFrom::Figurative => "figurative",
        NationalFrom::Dbcs => "dbcs",
    }
}

fn numeric_from(n: NumericFrom) -> String {
    match n {
        NumericFrom::Value => "value".to_owned(),
        NumericFrom::PackedCopy => "packed-copy".to_owned(),
        NumericFrom::Float => "float".to_owned(),
        NumericFrom::Zero => "zero".to_owned(),
        NumericFrom::Fill => "fill".to_owned(),
        NumericFrom::Zoned => "zoned".to_owned(),
        NumericFrom::DeEdit { edit, digits, scale } => format!("de-edit edit {edit} {}", pic(digits, scale, false)),
    }
}

fn float_from(f: FloatFrom) -> &'static str {
    match f {
        FloatFrom::Float => "float",
        FloatFrom::Fixed => "fixed",
        FloatFrom::Zero => "zero",
    }
}

fn accept_from(a: AcceptFrom) -> &'static str {
    match a {
        AcceptFrom::Sysin => "SYSIN",
        AcceptFrom::Date { four_digit_year: false } => "DATE",
        AcceptFrom::Date { four_digit_year: true } => "DATE YYYYMMDD",
        AcceptFrom::Day { four_digit_year: false } => "DAY",
        AcceptFrom::Day { four_digit_year: true } => "DAY YYYYDDD",
        AcceptFrom::DayOfWeek => "DAY-OF-WEEK",
        AcceptFrom::Time => "TIME",
        AcceptFrom::CommandLine => "COMMAND-LINE",
        AcceptFrom::ArgumentNumber => "ARGUMENT-NUMBER",
        AcceptFrom::ArgumentValue => "ARGUMENT-VALUE",
        AcceptFrom::EnvironmentValue => "ENVIRONMENT-VALUE",
    }
}

fn open_mode(m: OpenMode) -> &'static str {
    match m {
        OpenMode::Input => "INPUT",
        OpenMode::Output => "OUTPUT",
        OpenMode::Extend => "EXTEND",
        OpenMode::InputOutput => "I-O",
    }
}

fn closing_text(c: Closing) -> &'static str {
    match c {
        Closing::Volume => "volume",
        Closing::NoRewind => "no rewind",
        Closing::Lock => "lock",
    }
}

fn phrase_words(p: Option<Phrase>, on: &str, not_on: &str) -> Vec<String> {
    p.map_or_else(Vec::new, |p| [yes(p.on, on), yes(p.not_on, not_on)].into_iter().flatten().collect())
}

fn host_type(t: &HostType) -> String {
    let unsigned = |signed: bool| if signed { "" } else { " unsigned" };
    match t {
        HostType::SmallInt { signed } => format!("smallint{}", unsigned(*signed)),
        HostType::Integer { signed } => format!("integer{}", unsigned(*signed)),
        HostType::BigInt { signed } => format!("bigint{}", unsigned(*signed)),
        HostType::Decimal { digits, scale, signed } => format!("decimal({digits},{scale}){}", unsigned(*signed)),
        HostType::Zoned { digits, scale, signed, sign } => format!("zoned({digits},{scale}){}{}", unsigned(*signed), sign_clause(*sign)),
        HostType::Real => "real".to_owned(),
        HostType::Double => "double".to_owned(),
        HostType::Char(n) => format!("char({n})"),
        HostType::VarChar(n) => format!("varchar({n})"),
        HostType::Graphic(n) => format!("graphic({n})"),
        HostType::VarGraphic(n) => format!("vargraphic({n})"),
        HostType::Structure(members) => format!("structure of {}", members.len()),
    }
}

fn sqlca_field(f: SqlcaField) -> String {
    match f {
        SqlcaField::CaId => "SQLCAID".to_owned(),
        SqlcaField::CaBc => "SQLCABC".to_owned(),
        SqlcaField::Code => "SQLCODE".to_owned(),
        SqlcaField::ErrMl => "SQLERRML".to_owned(),
        SqlcaField::ErrMc => "SQLERRMC".to_owned(),
        SqlcaField::ErrP => "SQLERRP".to_owned(),
        SqlcaField::State => "SQLSTATE".to_owned(),
        SqlcaField::ErrD(n) => format!("SQLERRD({n})"),
        SqlcaField::Warn(n) => format!("SQLWARN{}", if n == 10 { "A".to_owned() } else { n.to_string() }),
    }
}

fn scope_section(s: Section) -> &'static str {
    match s {
        Section::WorkingStorage => "working-storage",
        Section::LocalStorage => "local-storage",
        Section::Linkage => "linkage",
        Section::File => "file",
    }
}

fn group_kind(k: GroupKind) -> &'static str {
    match k {
        GroupKind::ReportHeading => "report-heading",
        GroupKind::PageHeading => "page-heading",
        GroupKind::ControlHeading => "control-heading",
        GroupKind::Detail => "detail",
        GroupKind::ControlFooting => "control-footing",
        GroupKind::PageFooting => "page-footing",
        GroupKind::ReportFooting => "report-footing",
    }
}

fn next_group(n: NextGroup) -> String {
    match n {
        NextGroup::Line(l) => format!("line {l}"),
        NextGroup::Plus(l) => format!("plus {l}"),
        NextGroup::NextPage => "next page".to_owned(),
    }
}

fn line_number(n: LineNumber) -> String {
    match n {
        LineNumber::Line(l) => format!("at {l}"),
        LineNumber::Plus(l) => format!("plus {l}"),
        LineNumber::NextPage(None) => "next page".to_owned(),
        LineNumber::NextPage(Some(l)) => format!("next page at {l}"),
    }
}

fn members_list(members: &[u32]) -> String {
    join(members.iter().map(u32::to_string), " ")
}

/// The open modes `Declaratives.modes` and `Scope.global_modes` are in.
const MODES: [&str; 4] = ["input", "output", "i-o", "extend"];

#[cfg(test)]
mod tests {
    use super::*;
    use numeric::precision::Places;

    #[test]
    fn a_number_prints_with_every_decimal_place_it_holds() {
        let n = |value: i128, int: u32, dec: u32| decimal(&Fixed::new(value, Places::new(int, dec)));
        assert_eq!([n(125, 3, 1), n(-5, 1, 2), n(0, 1, 0), n(1250, 3, 2), n(7, 1, 0)], ["12.5", "-0.05", "0", "12.50", "7"]);
    }

    #[test]
    fn text_prints_on_one_line_whatever_it_holds() {
        assert_eq!(quote("IT'S"), "'IT''S'");
        assert_eq!(quote("A\nB"), "\"A\\nB\"");
        assert_eq!(word("WS-A"), "WS-A");
        assert_eq!(word("A B"), "\"A B\"");
        assert_eq!(national(&[0, 0x41, 0, 0x42]), "N'AB'");
        assert_eq!(national(&[0xD8, 0x00]), "NX'D800'");
        assert_eq!(pic(9, 2, true), "S9(7)V9(2)");
        assert_eq!(pic(2, 2, false), "V9(2)");
        assert_eq!(pic(4, 0, false), "9(4)");
    }

    #[test]
    fn tables_that_refer_to_each_other_in_a_cycle_print_and_stop() {
        let (plans, services) = (Plans::default(), Services::default());
        let exprs = [Expr::Bin(0, BinOp::Add, 0)];
        let conds = [Cond::Rel { a: Comparand::Expr { expr: 0, dmax: 0, mode: Mode::Fixed, prepass: Vec::new() }, op: RelOp::Eq, b: Comparand::Operand(Operand::Const(9)), how: Compare::Fixed }];
        let blocks = [Block { ops: vec![Op::Display(4)], end: Terminator::Branch { cond: 0, then: 0, otherwise: 1 } }];
        let code = Code {
            id: 0,
            initial: false,
            recursive: false,
            paragraphs: &[],
            procedure_start: 0,
            ranges: &[],
            blocks: &blocks,
            places: &[],
            exprs: &exprs,
            conds: &conds,
            consts: &[],
            plans: &plans,
            services: &services,
            abends: &[],
            symbols: &[],
        };
        let text = Listing { code, items: &[], debug: None, sql: &[], ccsid: None }.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[..3], ["program symbol0?", "b0:", "    Display display4?"]);
        assert!(lines[3].starts_with("    Branch (((((") && lines[3].contains("...") && lines[3].ends_with(" = c9? [fixed]) b0 else b1"), "{text}");
    }
}
