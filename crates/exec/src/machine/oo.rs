//! INVOKE, SELF and SUPER, and the JNI environment, at run time. A COBOL class runs here: its
//! factory data, each object's instance data and each method's WORKING-STORAGE are storage of the
//! run unit that is never released. A Java class, or a JNI service that needs a JVM, ends the run
//! with abend JAVA naming what was reached.

use super::*;
use crate::oo::{self as classes, Instance, JAVA_LANG_OBJECT, LoadedClass, MAX_MEMORY, Part};
use crate::unit::Loaded;
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
    /// SELF: the object, or the class's factory object.
    pub this: u32,
    /// Where SELF's four bytes are.
    pub cell: usize,
}

/// What an INVOKE is sent to.
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
        code: "JAVA".into(),
        message: format!("{what} was reached: {class} is a Java class, and ironwork for COBOL checks Java classes but has no JVM to run them"),
        pos,
    }
}

impl<'p> Machine<'p, '_, '_> {
    /// SELF in a method, and JNIENVPTR; None for anything else.
    pub(super) fn oo_register(&mut self, r: &Ref) -> R<Option<Loc>> {
        if !r.qualifiers.is_empty() || !r.subscripts.is_empty() || !matches!(r.name.as_str(), "SELF" | "JNIENVPTR") || self.layout.resolve(&r.name, &[], r.pos).is_ok() {
            return Ok(None);
        }
        if r.name == "SELF" {
            let Some(m) = self.oo.method else { return Err(Abend::ironwork("SELF outside a method", r.pos)) };
            return Ok(Some(Loc { offset: m.cell, len: 4, kind: Kind::ObjectReference, item: usize::MAX }));
        }
        let cell = self.jni_environment(r.pos)?;
        Ok(Some(Loc { offset: cell, len: 4, kind: Kind::Pointer, item: usize::MAX }))
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
        self.unit.programs.push(Loaded { compiled, name: String::new(), base, files, initialized: false, active: false });
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
        let code = match found {
            None => None,
            Some(program) => {
                let flags = self.unit.library.flags.clone();
                let code = classes::class_code(&program, &flags).map_err(|errors| {
                    let first = errors.first().map(|e| e.place(external)).unwrap_or_default();
                    Abend::ironwork(format!("class {external} does not compile: {first}"), pos)
                })?;
                Some(Rc::new(code))
            }
        };
        let index = self.unit.oo.classes.len();
        let methods = code.as_ref().map_or(0, |c| c.methods.len());
        self.unit.oo.classes.push(LoadedClass { external: external.to_owned(), code: code.clone(), parent: None, factory_object: 0, factory_data: None, methods: vec![None; methods] });
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

    fn object(&self, reference: u32, what: &str, pos: Pos) -> R<(usize, bool)> {
        if reference == 0 {
            return Err(Abend::ironwork(format!("INVOKE {what}: the object reference is NULL"), pos));
        }
        self.unit
            .oo
            .object(reference)
            .map(|o| (o.class, o.factory))
            .ok_or_else(|| Abend::ironwork(format!("INVOKE {what}: X'{reference:08X}' is not a reference to an object"), pos))
    }

    fn receiver(&mut self, i: &Invoke) -> R<Receiver> {
        let t = &i.target;
        let plain = t.qualifiers.is_empty() && t.subscripts.is_empty() && t.refmod.is_none();
        if plain && matches!(t.name.as_str(), "SELF" | "SUPER") && self.layout.resolve(&t.name, &[], t.pos).is_err() {
            let Some(m) = self.oo.method else { return Err(Abend::ironwork(format!("INVOKE {} outside a method", t.name), i.pos)) };
            if t.name == "SELF" {
                return Ok(Receiver::Object(m.this));
            }
            let start = self.unit.oo.classes[m.class].parent.ok_or_else(|| Abend::ironwork("INVOKE SUPER: the class has no parent", i.pos))?;
            return Ok(Receiver::Super { this: m.this, start, factory: m.factory });
        }
        let program = self.program;
        if plain
            && let Some(external) = program.oo.as_deref().and_then(|o| o.external(&t.name))
            && self.layout.resolve(&t.name, &[], t.pos).is_err()
        {
            return Ok(Receiver::Class(self.load_class(external, i.pos)?));
        }
        let loc = self.locate(t)?;
        match <[u8; 4]>::try_from(self.bytes(loc)) {
            Ok(bytes) if loc.kind == Kind::ObjectReference => Ok(Receiver::Object(u32::from_be_bytes(bytes))),
            _ => Err(Abend::ironwork(format!("INVOKE {}: not an object reference", t.name), i.pos)),
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
            Operand::Literal(Literal::Figurative(f)) => vec![figurative_byte(*f)],
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
                code: "U4038".into(),
                message: format!("{what}: no method matches it, and the INVOKE has no ON EXCEPTION (a severity-3 Language Environment condition)"),
                pos: i.pos,
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
        let receiver = self.receiver(i)?;
        let name = self.method_name(&i.method, pos)?;
        let what = format!("INVOKE {} \"{name}\"", i.target.name);
        let (program, layout) = (self.program, self.layout);
        let oo = program.oo.as_deref();
        let mut params = Vec::new();
        let mut arguments = Vec::new();
        for op in &i.using {
            let java = classes::operand_type(layout, oo, op).map_err(|m| Abend::ironwork(m, pos))?;
            arguments.push(self.argument(op, &java, pos)?);
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
            Receiver::Object(reference) => {
                let (class, factory) = self.object(reference, &i.target.name, pos)?;
                (reference, class, factory)
            }
            Receiver::Super { this, start, factory } => (this, start, factory),
        };
        match self.find(start, factory, &name, &params, returns.as_deref()) {
            Found::Missing => self.no_method(i, what),
            Found::Java(class) => Err(java(what, &class, pos)),
            Found::ObjectEquals => {
                let other = u32::from_be_bytes(arguments[0].as_slice().try_into().unwrap_or_default());
                let same = other != 0 && self.unit.oo.object(other).is_some() && other == this;
                if let Some(r) = &i.returning {
                    let dest = self.locate(r)?;
                    self.assign(dest, Val::Bytes(vec![u8::from(same)]), None, pos)?;
                }
                self.succeeded(i)
            }
            Found::Cobol { class, method } => {
                self.nest(pos)?;
                let flow = self.run_method(i, class, method, this, arguments);
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
        let reference = self.unit.oo.add_object(Instance { class, factory: false, parts }).map_err(|m| Abend::ironwork(m, pos))?;
        if let Some(r) = &i.returning {
            let dest = self.locate(r)?;
            self.assign(dest, Val::Address(reference), None, pos)?;
        }
        self.succeeded(i)
    }

    /// Runs a COBOL method as a called program runs, with the data of its paragraph as the records
    /// after its own LINKAGE, its arguments BY VALUE and SELF.
    fn run_method(&mut self, i: &'p Invoke, class: usize, k: usize, this: u32, arguments: Vec<Vec<u8>>) -> R<Flow> {
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
        let addresses: Vec<Option<usize>> = arguments.iter().map(|b| Some(self.unit.push_temporary(b))).collect();
        let cell = self.unit.push_temporary(&this.to_be_bytes());
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
            callee.oo = Frame { method: Some(Running { class, factory: method.factory, this, cell }) };
            let ending = callee.run_procedure();
            let returned = match (&compiled.program.returning, &ending) {
                (Some(item), Ok(_)) => Some(callee.returned(item, pos)?),
                _ => None,
            };
            (ending, returned)
        };
        self.unit.programs[storage].active = false;
        self.unit.release_temporaries(mark);
        let (ending, returned) = outcome;
        if ending? == Ending::StopRun {
            return Ok(Flow::End(Ending::StopRun));
        }
        self.unit.mem[RETURN_CODE..RETURN_CODE + 2].copy_from_slice(&return_code);
        if let (Some(target), Some(val)) = (&i.returning, returned) {
            let dest = self.locate(target)?;
            self.assign(dest, val, None, pos)?;
        }
        self.succeeded(i)
    }

    /// CALL through a FUNCTION-POINTER or PROCEDURE-POINTER: a JNI service from the function
    /// table, run here when it needs no JVM. None when the CALL names a program.
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
        let reference = |n: usize| match args.get(n) {
            Some(Val::Address(a)) => Ok(*a),
            Some(Val::Fig(Figurative::Null)) => Ok(0),
            _ => Err(Abend::ironwork(format!("CALL {service}: argument {n} is not an object reference"), pos)),
        };
        let result = match service {
            "NewGlobalRef" | "NewLocalRef" | "PopLocalFrame" => Some(Val::Address(reference(1)?)),
            "DeleteGlobalRef" | "DeleteLocalRef" | "ExceptionClear" => None,
            "IsSameObject" => Some(Val::Bytes(vec![u8::from(reference(1)? == reference(2)?)])),
            "ExceptionOccurred" => Some(Val::Address(0)),
            "ExceptionCheck" => Some(Val::Bytes(vec![0])),
            "EnsureLocalCapacity" | "PushLocalFrame" => Some(Val::Num(Fixed::new(0, Places::new(9, 0)))),
            _ => {
                return Err(Abend {
                    code: "JAVA".into(),
                    message: format!("CALL {} was reached: {service} is a JNI service, and ironwork for COBOL has no JVM to run it", r.name),
                    pos,
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
