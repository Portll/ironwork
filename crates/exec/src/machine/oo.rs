//! INVOKE, SELF and SUPER, and the JNI environment, at run time. A COBOL class runs here: its
//! factory data, each object's instance data and each method's WORKING-STORAGE are storage of the
//! run unit that is never released. An object reference holds a local or global reference, as the
//! JNI hands them out; a method's local references are freed when it returns. A Java class, or a
//! JNI service that needs a JVM, ends the run with abend JAVA naming what was reached.

use super::*;
use crate::oo::{self as classes, Instance, JAVA_LANG_OBJECT, LoadedClass, MAX_MEMORY, Part, Referent};
use crate::unit::Loaded;
use numeric::assumptions::{EXPIRED_REFERENCE_ABENDS, LOCAL_FRAMES};
use std::rc::Rc;

/// The method an activation runs.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Frame {
    pub method: Option<Running>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Running {
    /// The run-unit class that defines the method.
    pub class: usize,
    pub factory: bool,
    /// The object SELF refers to, or the class's factory object.
    pub this: u32,
    /// Where SELF's four bytes are: zero until the method first reads SELF, then a local reference
    /// of the method's frame.
    pub cell: usize,
    /// The serial of the method's local frame.
    pub frame: u32,
    /// The event that tells which method this is and where it was invoked.
    pub invoked: u32,
}

/// What an INVOKE is sent to: a class, or an object by its position plus one.
enum Receiver {
    Class(usize),
    Object(u32),
    Super { this: u32, start: usize, factory: bool },
}

/// Where the method an INVOKE names was found.
enum Found {
    Cobol { class: usize, method: usize },
    /// java.lang.Object's equals, which is identity.
    ObjectEquals,
    Java(String),
    Missing,
}

/// An INVOKE argument: the bytes of a Java primitive, or the object an object reference identifies,
/// which the method receives as a new local reference.
enum Argument {
    Bytes(Vec<u8>),
    Object(Option<u32>),
}

/// A JNI function-table slot's value: this plus the slot, above any storage address.
const JNI_TAG: u32 = 0x7F00_0000;

/// java.lang.Object's public methods with their JNI signatures, which the JVM would run.
const OBJECT_METHODS: &[(&str, &[&str], Option<&str>)] = &[
    ("hashCode", &[], Some("I")),
    ("toString", &[], Some("Ljava/lang/String;")),
    ("getClass", &[], Some("Ljava/lang/Class;")),
    ("notify", &[], None),
    ("notifyAll", &[], None),
    ("wait", &[], None),
    ("wait", &["J"], None),
    ("wait", &["J", "I"], None),
    ("clone", &[], Some("Ljava/lang/Object;")),
    ("finalize", &[], None),
];

fn java(what: String, class: &str, pos: Pos) -> Abend {
    Abend {
        code: AbendCode::Java,
        message: format!("{what} was reached: {class} is a Java class, and ironwork for COBOL checks Java classes but has no JVM to run them"),
        pos,
        file: None,
    }
}

/// An object reference's Java type, as a JNI signature spells it.
fn is_reference(java: &str) -> bool {
    java.starts_with('L') || java.starts_with('[')
}

fn int(n: i128) -> Val {
    Val::Num(Fixed::new(n, Places::new(9, 0)))
}

impl<'p> Machine<'p, '_, '_> {
    /// SELF in a method, and JNIENVPTR; None for anything else.
    pub(super) fn oo_register(&mut self, r: &Ref) -> R<Option<Loc>> {
        if !r.qualifiers.is_empty() || !r.subscripts.is_empty() || !matches!(r.name.as_str(), "SELF" | "JNIENVPTR") || self.layout.resolve(&r.name, &[], r.pos).is_ok() {
            return Ok(None);
        }
        if r.name == "SELF" {
            let Some(m) = self.oo.method else { return Err(Abend::ironwork("SELF outside a method", r.pos)) };
            if self.unit.mem[m.cell..m.cell + 4] == [0; 4] {
                let made = self.unit.oo.event(format!("SELF of {}", self.unit.oo.told(m.invoked)));
                let reference = self.unit.oo.local_in(m.frame, m.this, made).map_err(|e| Abend::ironwork(e, r.pos))?;
                self.unit.mem[m.cell..m.cell + 4].copy_from_slice(&reference.to_be_bytes());
            }
            return Ok(Some(Loc { offset: m.cell, len: 4, kind: Kind::ObjectReference, item: usize::MAX }));
        }
        let cell = self.jni_environment(r.pos)?;
        Ok(Some(Loc { offset: cell, len: 4, kind: Kind::Pointer, item: usize::MAX }))
    }

    /// `line 12 of CLIENT`, or `line 30 of Account.credit` in a method.
    fn site(&self, pos: Pos) -> String {
        match self.oo.method {
            Some(m) => format!("line {} of {}.{}", pos.line, self.unit.oo.classes[m.class].external, self.program.id),
            None => format!("line {} of {}", pos.line, self.program.id),
        }
    }

    /// The object an object reference's value identifies, None for NULL. A reference that was
    /// freed, or four bytes no reference was given, end the run (see [`EXPIRED_REFERENCE_ABENDS`]).
    fn referent(&self, value: u32, what: &str, holder: &str, pos: Pos) -> R<Option<u32>> {
        match self.unit.oo.referent(value) {
            Referent::Null => Ok(None),
            Referent::Object(o) => Ok(Some(o)),
            Referent::Expired(told) => Err(Abend::ironwork(format!("{what}: {holder} holds {told}, and IBM leaves using it unpredictable (see {EXPIRED_REFERENCE_ABENDS})"), pos)),
            Referent::Unknown => Err(Abend::ironwork(format!("{what}: {holder} holds X'{value:08X}', which is not an object reference"), pos)),
        }
    }

    /// A new local reference in the innermost frame.
    fn local_reference(&mut self, object: u32, made: String, pos: Pos) -> R<u32> {
        let made = self.unit.oo.event(made);
        self.unit.oo.local(object, made).map_err(|m| Abend::ironwork(m, pos))
    }

    /// The object SELF refers to, as the method's SELF reference says once it has been read.
    fn self_object(&self, m: Running, what: &str, pos: Pos) -> R<u32> {
        let value = u32::from_be_bytes(self.unit.mem[m.cell..m.cell + 4].try_into().unwrap_or_default());
        Ok(self.referent(value, what, "SELF", pos)?.unwrap_or(m.this))
    }

    /// Two object references compare equal when they identify the same object (Language Reference
    /// SC27-8713-03, p. 282), so each is looked up; one compared with the figurative constant NULL
    /// is only tested for NULL.
    pub(super) fn compare_references(&self, a: &Expr, b: &Expr, x: (&Val, Option<Loc>), y: (&Val, Option<Loc>), pos: Pos) -> R<Option<Ordering>> {
        let reference = |l: Option<Loc>| l.is_some_and(|l| l.kind == Kind::ObjectReference);
        let (Val::Address(p), Val::Address(q)) = (x.0, y.0) else { return Ok(None) };
        if !reference(x.1) && !reference(y.1) {
            return Ok(None);
        }
        let name = |e: &Expr| match e {
            Expr::Operand(Operand::Ref(r)) => r.name.clone(),
            _ => String::new(),
        };
        let what = format!("{} = {}", name(a), name(b));
        let same = self.referent(*p, &what, &name(a), pos)? == self.referent(*q, &what, &name(b), pos)?;
        Ok(Some(if same { Ordering::Equal } else { Ordering::Less }))
    }

    fn room(&self, size: usize, pos: Pos) -> R<()> {
        if self.unit.mem.len() + size > MAX_MEMORY {
            return Err(Abend::ironwork(format!("objects and classes take the run unit past ironwork's {MAX_MEMORY} bytes"), pos));
        }
        Ok(())
    }

    /// Storage the run unit keeps: a loaded entry no CALL can name holds it, so that releasing
    /// arguments never reaches it.
    fn keep(&mut self, compiled: Option<Rc<Compiled>>, bytes: &[u8], pos: Pos) -> R<usize> {
        self.room(bytes.len(), pos)?;
        let base = self.unit.push_temporary(bytes);
        let files = compiled.as_ref().map_or(Vec::new(), |c| c.program.files.iter().map(|_| None).collect());
        self.unit.programs.push(Loaded { compiled, name: String::new(), base, files, initialized: false, active: false, entry: None, altered: Vec::new(), source: None });
        Ok(self.unit.programs.len() - 1)
    }

    /// The JNIENVPTR cell, pointing at the JNI environment, which points at the function table.
    fn jni_environment(&mut self, pos: Pos) -> R<usize> {
        if let Some(cell) = self.unit.oo.jni {
            return Ok(cell);
        }
        let slots = syntax::jni::RESERVED + syntax::jni::FUNCTIONS.len();
        let loaded = self.keep(None, &vec![0; 8 + 4 * slots], pos)?;
        let base = self.unit.programs[loaded].base;
        let address = |offset: usize| ADDRESS_BASE + offset as u32;
        let mut bytes = [address(base + 4).to_be_bytes(), address(base + 8).to_be_bytes()].concat();
        for slot in 0..slots {
            let value = if slot < syntax::jni::RESERVED { 0 } else { JNI_TAG + slot as u32 };
            bytes.extend(value.to_be_bytes());
        }
        self.unit.mem[base..base + bytes.len()].copy_from_slice(&bytes);
        self.unit.oo.jni = Some(base);
        Ok(base)
    }

    /// FACTORY or OBJECT data, zeroed and given its VALUE clauses.
    fn part_storage(&mut self, part: &Part, pos: Pos) -> R<usize> {
        let data = part.data.clone();
        let loaded = self.keep(Some(data.clone()), &vec![0; data.layout.size as usize], pos)?;
        Machine::activation(&data, loaded, &mut *self.unit, false)?;
        self.unit.programs[loaded].active = false;
        Ok(loaded)
    }

    /// The run unit's class of this external name, loading its COBOL class definition, and those of
    /// the classes it inherits, the first time.
    fn load_class(&mut self, external: &str, pos: Pos) -> R<usize> {
        if let Some(c) = self.unit.oo.find(external) {
            return Ok(c);
        }
        let found = classes::find_class(&mut self.unit.library, external).map_err(|m| Abend::ironwork(m, pos))?;
        let (code, sources) = match found {
            None => (None, Vec::new()),
            Some((program, path)) => {
                let flags = self.unit.library.flags.clone();
                let (code, _) = classes::class_code(&program, &flags).map_err(|errors| {
                    let first = syntax::most_severe(&errors).map(|e| e.place(external)).unwrap_or_default();
                    Abend::ironwork(format!("class {external} does not compile: {first}"), pos)
                })?;
                let mut sources = program.sources;
                if let (Some(own), Some(path)) = (sources.first_mut(), path) {
                    *own = path;
                }
                (Some(Rc::new(code)), sources)
            }
        };
        let index = self.unit.oo.classes.len();
        let methods = code.as_ref().map_or(0, |c| c.methods.len());
        self.unit.oo.classes.push(LoadedClass { external: external.to_owned(), code: code.clone(), parent: None, factory_object: 0, factory_data: None, methods: vec![None; methods], sources });
        let factory_object = self.unit.oo.add_object(Instance { class: index, factory: true, parts: Vec::new() }).map_err(|m| Abend::ironwork(m, pos))?;
        self.unit.oo.classes[index].factory_object = factory_object;
        let Some(code) = code else { return Ok(index) };
        let parent = self.load_class(&code.parent, pos)?;
        let mut at = Some(parent);
        for _ in 0..=self.unit.oo.classes.len() {
            match at {
                Some(c) if c == index => return Err(Abend::ironwork(format!("class {external} inherits from itself"), pos)),
                Some(c) => at = self.unit.oo.classes[c].parent,
                None => break,
            }
        }
        self.unit.oo.classes[index].parent = Some(parent);
        if let Some(part) = &code.factory {
            let loaded = self.part_storage(part, pos)?;
            self.unit.oo.classes[index].factory_data = Some(loaded);
        }
        Ok(index)
    }

    /// The class and its ancestors, the class first.
    fn chain(&self, class: usize) -> Vec<usize> {
        let mut out = vec![class];
        while let Some(p) = self.unit.oo.classes[*out.last().unwrap()].parent {
            if out.contains(&p) {
                break;
            }
            out.push(p);
        }
        out
    }

    fn object(&self, id: u32, what: &str, pos: Pos) -> R<(usize, bool)> {
        self.unit.oo.object(id).map(|o| (o.class, o.factory)).ok_or_else(|| Abend::ironwork(format!("{what}: no object {id}"), pos))
    }

    fn receiver(&mut self, i: &Invoke, what: &str) -> R<Receiver> {
        let t = &i.target;
        let plain = t.qualifiers.is_empty() && t.subscripts.is_empty() && t.refmod.is_none();
        if plain && matches!(t.name.as_str(), "SELF" | "SUPER") && self.layout.resolve(&t.name, &[], t.pos).is_err() {
            let Some(m) = self.oo.method else { return Err(Abend::ironwork(format!("INVOKE {} outside a method", t.name), i.pos)) };
            let this = self.self_object(m, what, i.pos)?;
            if t.name == "SELF" {
                return Ok(Receiver::Object(this));
            }
            let start = self.unit.oo.classes[m.class].parent.ok_or_else(|| Abend::ironwork("INVOKE SUPER: the class has no parent", i.pos))?;
            return Ok(Receiver::Super { this, start, factory: m.factory });
        }
        let program = self.program;
        if plain
            && let Some(external) = program.oo.as_deref().and_then(|o| o.external(&t.name))
            && self.layout.resolve(&t.name, &[], t.pos).is_err()
        {
            return Ok(Receiver::Class(self.load_class(external, i.pos)?));
        }
        let loc = self.locate(t)?;
        let value = match <[u8; 4]>::try_from(self.bytes(loc)) {
            Ok(bytes) if loc.kind == Kind::ObjectReference => u32::from_be_bytes(bytes),
            _ => return Err(Abend::ironwork(format!("INVOKE {}: not an object reference", t.name), i.pos)),
        };
        match self.referent(value, what, &t.name, i.pos)? {
            Some(object) => Ok(Receiver::Object(object)),
            None => Err(Abend::ironwork(format!("{what}: the object reference {} is NULL", t.name), i.pos)),
        }
    }

    fn method_name(&mut self, method: &InvokeMethod, pos: Pos) -> R<String> {
        Ok(match method {
            InvokeMethod::New => "NEW".into(),
            InvokeMethod::Named(n) => n.clone(),
            InvokeMethod::Identifier(r) => {
                let loc = self.locate(r)?;
                let text = if loc.kind == Kind::National { utf16_text(self.bytes(loc)) } else { self.page.decode(self.bytes(loc)) };
                let name = text.trim_end_matches(' ').to_owned();
                if name.is_empty() {
                    return Err(Abend::ironwork(format!("INVOKE: {} holds no method name", r.name), pos));
                }
                name
            }
        })
    }

    /// An argument as the method receives it, in the bytes of its Java type.
    fn argument(&mut self, op: &Operand, java: &str, pos: Pos) -> R<Vec<u8>> {
        Ok(match op {
            Operand::Ref(r) => {
                let loc = self.locate(r)?;
                let bytes = self.bytes(loc).to_vec();
                if r.refmod.is_some() && java == "C" {
                    let text = self.page.decode(&bytes);
                    text.encode_utf16().take(1).flat_map(u16::to_be_bytes).collect()
                } else {
                    bytes
                }
            }
            Operand::LengthOf(r) => (self.locate(r)?.len as i32).to_be_bytes().to_vec(),
            Operand::Literal(Literal::Number(t)) => {
                let v = t.parse::<i32>().map_err(|_| Abend::ironwork(format!("{t} is not an int"), pos))?;
                v.to_be_bytes().to_vec()
            }
            Operand::Literal(Literal::Figurative(Figurative::Zero)) => vec![0; 4],
            Operand::Literal(Literal::Figurative(f)) => vec![self.collating.figurative(*f)],
            Operand::Literal(Literal::Alnum(s)) => self.page.encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?,
            Operand::Literal(Literal::National(s)) => s.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            _ => return Err(Abend::ironwork("this INVOKE argument is not supported", pos)),
        })
    }

    fn find(&self, start: usize, factory: bool, name: &str, params: &[String], returns: Option<&str>) -> Found {
        for class in self.chain(start) {
            let c = &self.unit.oo.classes[class];
            match &c.code {
                Some(code) => {
                    if let Some(method) = code.methods.iter().position(|m| m.factory == factory && m.name == name && m.params == params && m.returns.as_deref() == returns) {
                        return Found::Cobol { class, method };
                    }
                }
                None if c.external == JAVA_LANG_OBJECT => {
                    if factory {
                        return Found::Missing;
                    }
                    if name == "equals" && params == ["Ljava/lang/Object;"] && returns == Some("Z") {
                        return Found::ObjectEquals;
                    }
                    let java = OBJECT_METHODS.iter().any(|(n, p, r)| *n == name && *p == params && *r == returns);
                    return if java { Found::Java(c.external.clone()) } else { Found::Missing };
                }
                None => return Found::Java(c.external.clone()),
            }
        }
        Found::Missing
    }

    fn no_method(&mut self, i: &'p Invoke, what: String) -> R<Flow> {
        match &i.on_exception {
            Some(body) => self.run_block(body),
            None => Err(Abend {
                code: AbendCode::user(4038),
                message: format!("{what}: no method matches it, and the INVOKE has no ON EXCEPTION (a severity-3 Language Environment condition)"),
                pos: i.pos,
                file: None,
            }),
        }
    }

    fn succeeded(&mut self, i: &'p Invoke) -> R<Flow> {
        match &i.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    pub(super) fn invoke(&mut self, i: &'p Invoke) -> R<Flow> {
        let pos = i.pos;
        let name = self.method_name(&i.method, pos)?;
        let what = format!("INVOKE {} \"{name}\"", i.target.name);
        let receiver = self.receiver(i, &what)?;
        let (program, layout) = (self.program, self.layout);
        let oo = program.oo.as_deref();
        let mut params = Vec::new();
        let mut arguments = Vec::new();
        for (n, op) in i.using.iter().enumerate() {
            let java = classes::operand_type(layout, oo, op).map_err(|m| Abend::ironwork(m, pos))?;
            let bytes = self.argument(op, &java, pos)?;
            arguments.push(if is_reference(&java) {
                let holder = match op {
                    Operand::Ref(r) => r.name.clone(),
                    _ => format!("argument {}", n + 1),
                };
                Argument::Object(self.referent(u32::from_be_bytes(bytes.as_slice().try_into().unwrap_or_default()), &what, &holder, pos)?)
            } else {
                Argument::Bytes(bytes)
            });
            params.push(java);
        }
        if i.method == InvokeMethod::New {
            let Receiver::Class(class) = receiver else { return Err(Abend::ironwork("INVOKE ... NEW takes a class-name", pos)) };
            return self.new_object(i, class, &params);
        }
        let returns = match &i.returning {
            None => None,
            Some(r) => match self.resolve(r)? {
                Resolved::Item(k) => Some(classes::item_type(layout, oo, k).map_err(|m| Abend::ironwork(m, pos))?),
                Resolved::Condition(_) => return Err(Abend::ironwork(format!("RETURNING {}: a condition-name", r.name), pos)),
            },
        };
        let (this, start, factory) = match receiver {
            Receiver::Class(c) => (self.unit.oo.classes[c].factory_object, c, true),
            Receiver::Object(object) => {
                let (class, factory) = self.object(object, &what, pos)?;
                (object, class, factory)
            }
            Receiver::Super { this, start, factory } => (this, start, factory),
        };
        match self.find(start, factory, &name, &params, returns.as_deref()) {
            Found::Missing => self.no_method(i, what),
            Found::Java(class) => Err(java(what, &class, pos)),
            Found::ObjectEquals => {
                let same = matches!(arguments.first(), Some(Argument::Object(Some(other))) if *other == this);
                if let Some(r) = &i.returning {
                    let dest = self.locate(r)?;
                    self.assign(dest, Val::Bytes(vec![u8::from(same)]), None, pos)?;
                }
                self.succeeded(i)
            }
            Found::Cobol { class, method } => {
                self.nest(pos)?;
                let flow = self.run_method(i, class, method, this, arguments, &what);
                self.unit.depth -= 1;
                flow
            }
        }
    }

    /// INVOKE class NEW: a COBOL class gets its instance data from its VALUE clauses and takes no
    /// arguments; a class with a Java ancestor other than java.lang.Object needs the JVM.
    fn new_object(&mut self, i: &'p Invoke, class: usize, params: &[String]) -> R<Flow> {
        let pos = i.pos;
        let what = format!("INVOKE {} NEW", i.target.name);
        let chain = self.chain(class);
        if let Some(&j) = chain.iter().find(|&&c| self.unit.oo.classes[c].code.is_none() && self.unit.oo.classes[c].external != JAVA_LANG_OBJECT) {
            return Err(java(what, &self.unit.oo.classes[j].external.clone(), pos));
        }
        if !params.is_empty() {
            return self.no_method(i, format!("{what} USING {} arguments", params.len()));
        }
        let mut parts = Vec::new();
        for &c in chain.iter().rev() {
            if let Some(code) = self.unit.oo.classes[c].code.clone()
                && let Some(part) = &code.object
            {
                parts.push((c, self.part_storage(part, pos)?));
            }
        }
        let object = self.unit.oo.add_object(Instance { class, factory: false, parts }).map_err(|m| Abend::ironwork(m, pos))?;
        let made = format!("made by INVOKE {} NEW at {}", self.unit.oo.classes[class].external, self.site(pos));
        let reference = self.local_reference(object, made, pos)?;
        if let Some(r) = &i.returning {
            let dest = self.locate(r)?;
            self.assign(dest, Val::Address(reference), None, pos)?;
        }
        self.succeeded(i)
    }

    /// Runs a COBOL method as a called program runs, with the data of its paragraph as the records
    /// after its own LINKAGE, its arguments BY VALUE and SELF, in a local frame of its own: the
    /// object references it receives are new local references there, and every local reference
    /// made there is freed when it returns. A returned object reference reaches the invoker as a
    /// new local reference of the invoker's frame.
    fn run_method(&mut self, i: &'p Invoke, class: usize, k: usize, this: u32, arguments: Vec<Argument>, what: &str) -> R<Flow> {
        let pos = i.pos;
        let code = self.unit.oo.classes[class].code.clone().ok_or_else(|| Abend::ironwork("not a COBOL class", pos))?;
        let method = &code.methods[k];
        let this = if method.factory { self.unit.oo.classes[class].factory_object } else { this };
        let storage = match self.unit.oo.classes[class].methods[k] {
            Some(s) => s,
            None => {
                let s = self.keep(Some(method.code.clone()), &vec![0; method.code.layout.size as usize], pos)?;
                self.unit.oo.classes[class].methods[k] = Some(s);
                s
            }
        };
        let (part, data) = if method.factory {
            (code.factory.as_ref(), self.unit.oo.classes[class].factory_data)
        } else {
            let data = self.unit.oo.object(this).and_then(|o| o.parts.iter().find(|(c, _)| *c == class)).map(|(_, l)| *l);
            (code.object.as_ref(), data)
        };
        let base = data.map(|d| self.unit.programs[d].base);
        let return_code = [self.unit.mem[RETURN_CODE], self.unit.mem[RETURN_CODE + 1]];
        let mark = self.unit.mem.len();
        let invoked_text = format!("method \"{}\" of {}, invoked at {}", method.name, self.unit.oo.classes[class].external, self.site(pos));
        let invoked = self.unit.oo.event(invoked_text.clone());
        let (depth, frame) = self.unit.oo.push_frame(false);
        let mut addresses = Vec::new();
        for (n, argument) in arguments.into_iter().enumerate() {
            let bytes = match argument {
                Argument::Bytes(b) => b,
                Argument::Object(None) => vec![0; 4],
                Argument::Object(Some(object)) => {
                    let made = self.unit.oo.event(format!("received as argument {} by {invoked_text}", n + 1));
                    self.unit.oo.local_in(frame, object, made).map_err(|m| Abend::ironwork(m, pos))?.to_be_bytes().to_vec()
                }
            };
            addresses.push(Some(self.unit.push_temporary(&bytes)));
        }
        let cell = self.unit.push_temporary(&[0; 4]);
        let compiled = method.code.clone();
        let outcome = {
            let mut callee = Machine::activation(&compiled, storage, &mut *self.unit, false)?;
            if let (Some(part), Some(base)) = (part, base) {
                for (n, offset) in part.records.iter().enumerate() {
                    if let Some(slot) = callee.linkage.get_mut(method.own_records + n) {
                        *slot = Some(base + *offset as usize);
                    }
                }
            }
            callee.bind(&addresses);
            callee.bind_returning();
            callee.oo = Frame { method: Some(Running { class, factory: method.factory, this, cell, frame, invoked }) };
            let ending = callee.run_procedure();
            let returned = match (&compiled.program.returning, &ending) {
                (Some(item), Ok(_)) => Some(callee.returned(item, pos)?),
                _ => None,
            };
            (ending, returned)
        };
        self.unit.programs[storage].active = false;
        self.unit.release_temporaries(mark);
        let (ending, mut returned) = outcome;
        let ending = ending.map_err(|mut abend| {
            if abend.file.is_none() {
                abend.file = Some(self.unit.oo.classes[class].sources.get(abend.pos.file as usize).cloned().unwrap_or_default());
            }
            abend
        });
        if ending? == Ending::StopRun {
            return Ok(Flow::End(Ending::StopRun));
        }
        let mut returned_object = None;
        if let (Some(Val::Address(value)), Some(item)) = (&returned, compiled.program.returning.as_ref().filter(|_| method.returns.as_deref().is_some_and(is_reference))) {
            let holder = format!("{item}, the RETURNING item of method \"{}\",", method.name);
            returned_object = Some(self.referent(*value, what, &holder, pos)?);
        }
        let expired = self.unit.oo.event(format!("expired when {invoked_text}, returned"));
        self.unit.oo.pop_frames(depth, expired);
        if let Some(object) = returned_object {
            let value = match object {
                None => 0,
                Some(o) => self.local_reference(o, format!("returned by {invoked_text}"), pos)?,
            };
            returned = Some(Val::Address(value));
        }
        self.unit.mem[RETURN_CODE..RETURN_CODE + 2].copy_from_slice(&return_code);
        if let (Some(target), Some(val)) = (&i.returning, returned) {
            let dest = self.locate(target)?;
            self.assign(dest, val, None, pos)?;
        }
        self.succeeded(i)
    }

    /// CALL through a FUNCTION-POINTER or PROCEDURE-POINTER: a JNI service from the function
    /// table, run here when it needs no JVM. None when the CALL names a program. The reference
    /// services keep the JNI's rules for local and global references (see [`LOCAL_FRAMES`]).
    pub(super) fn call_through_pointer(&mut self, c: &'p Call) -> R<Option<Flow>> {
        let Operand::Ref(r) = &c.target else { return Ok(None) };
        let Ok(Resolved::Item(item)) = self.resolve(r) else { return Ok(None) };
        if self.layout.items[item].kind != Kind::ProgramPointer {
            return Ok(None);
        }
        let pos = c.pos;
        let loc = self.locate(r)?;
        let Ok(value) = <[u8; 4]>::try_from(self.bytes(loc)).map(u32::from_be_bytes) else {
            return Err(Abend::ironwork(format!("CALL {}: a reference-modified function-pointer", r.name), pos));
        };
        let service = value.checked_sub(JNI_TAG).and_then(|slot| syntax::jni::function(slot as usize));
        let Some(service) = service else {
            return Err(Abend::ironwork(format!("CALL {}: X'{value:08X}' is not a JNI service, the only entry a function-pointer can hold here", r.name), pos));
        };
        let mut args = Vec::new();
        for arg in &c.using {
            args.push(match &arg.value {
                Some(op) => self.operand(op, pos)?,
                None => Val::Address(0),
            });
        }
        let what = format!("CALL {service}");
        let holder = |n: usize| match c.using.get(n).and_then(|a| a.value.as_ref()) {
            Some(Operand::Ref(r)) => r.name.clone(),
            _ => format!("argument {n}"),
        };
        let value = |n: usize| match args.get(n) {
            Some(Val::Address(a)) => Ok(*a),
            Some(Val::Fig(Figurative::Null)) => Ok(0),
            _ => Err(Abend::ironwork(format!("CALL {service}: argument {n} is not an object reference"), pos)),
        };
        let here = self.site(pos);
        let result = match service {
            "NewGlobalRef" | "NewLocalRef" => {
                let object = self.referent(value(1)?, &what, &holder(1), pos)?;
                let reference = match object {
                    None => 0,
                    Some(o) => {
                        let made = self.unit.oo.event(format!("made by {service} at {here}"));
                        let made = if service == "NewGlobalRef" { self.unit.oo.global(o, made) } else { self.unit.oo.local(o, made) };
                        made.map_err(|m| Abend::ironwork(m, pos))?
                    }
                };
                Some(Val::Address(reference))
            }
            "DeleteGlobalRef" | "DeleteLocalRef" => {
                let reference = value(1)?;
                if self.referent(reference, &what, &holder(1), pos)?.is_some() {
                    let global = service == "DeleteGlobalRef";
                    if self.unit.oo.is_global(reference) != global {
                        let kind = if global { "local" } else { "global" };
                        return Err(Abend::ironwork(format!("{what}: {} holds a {kind} reference, which {service} does not delete (see {LOCAL_FRAMES})", holder(1)), pos));
                    }
                    let event = self.unit.oo.event(format!("was deleted by {service} at {here}"));
                    self.unit.oo.free(reference, event);
                }
                None
            }
            "IsSameObject" => {
                let same = self.referent(value(1)?, &what, &holder(1), pos)? == self.referent(value(2)?, &what, &holder(2), pos)?;
                Some(Val::Bytes(vec![u8::from(same)]))
            }
            "GetObjectRefType" => {
                let reference = value(1)?;
                let kind = match self.referent(reference, &what, &holder(1), pos)? {
                    None => 0,
                    Some(_) if self.unit.oo.is_global(reference) => 2,
                    Some(_) => 1,
                };
                Some(int(kind))
            }
            "PushLocalFrame" => {
                self.unit.oo.push_frame(true);
                Some(int(0))
            }
            "PopLocalFrame" => {
                let Some(depth) = self.unit.oo.pushed_frame() else {
                    return Err(Abend::ironwork(format!("{what}: no frame PushLocalFrame pushed is open here (see {LOCAL_FRAMES})"), pos));
                };
                let object = self.referent(value(1)?, &what, &holder(1), pos)?;
                let expired = self.unit.oo.event(format!("expired when PopLocalFrame at {here} freed its frame"));
                self.unit.oo.pop_frames(depth, expired);
                let reference = match object {
                    None => 0,
                    Some(o) => self.local_reference(o, format!("returned by PopLocalFrame at {here}"), pos)?,
                };
                Some(Val::Address(reference))
            }
            "EnsureLocalCapacity" => Some(int(0)),
            "ExceptionOccurred" => Some(Val::Address(0)),
            "ExceptionCheck" => Some(Val::Bytes(vec![0])),
            "ExceptionClear" => None,
            _ => {
                return Err(Abend {
                    code: AbendCode::Java,
                    message: format!("CALL {} was reached: {service} is a JNI service, and ironwork for COBOL has no JVM to run it", r.name),
                    pos,
                    file: None,
                });
            }
        };
        if let (Some(target), Some(val)) = (&c.returning, result) {
            let dest = self.locate(target)?;
            self.assign(dest, val, None, pos)?;
        }
        Ok(Some(match &c.not_on_exception {
            Some(body) => self.run_block(body)?,
            None => Flow::Next,
        }))
    }
}
