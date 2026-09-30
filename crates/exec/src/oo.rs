//! Object-oriented COBOL, compiled and checked. A class written in COBOL becomes code the
//! interpreter runs within the run unit: each method is compiled as a program whose LINKAGE SECTION
//! ends with the records of the OBJECT or FACTORY WORKING-STORAGE it works on, and each
//! INVOKE finds its method by name and Java signature, as the JNI does. Running is in
//! machine/oo.rs; a Java class is checked here and never run.

use crate::layout::{Kind, Layout, Resolved};
use crate::{Check, Compiled};
use numeric::Options;
use numeric::assumptions::{OO_OPTIONS_REQUIRED, OO_OPTIONS_SEVERITY, REFERENCES_KEPT};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use syntax::ast::*;
use syntax::{Error, Pos};

pub const JAVA_LANG_OBJECT: &str = "java.lang.Object";

/// Objects a run unit may create before it abends; they are never freed.
pub const MAX_OBJECTS: usize = 1_000_000;
/// References a run unit may make before it abends; each is kept, so that an expired one can say
/// where it expired.
pub const MAX_REFERENCES: usize = 1 << 23;
/// Run-unit memory past which creating objects and classes abends.
pub const MAX_MEMORY: usize = 1 << 30;

/// The frame a global reference belongs to.
const GLOBAL: u32 = u32::MAX;

/// The run unit's classes and objects, the references to them, and the JNI environment once a
/// program uses JNIENVPTR.
#[derive(Default)]
pub struct Objects {
    pub(crate) classes: Vec<LoadedClass>,
    pub(crate) objects: Vec<Instance>,
    pub(crate) jni: Option<usize>,
    /// Every reference made, local or global; an object reference holds a reference's position
    /// plus one, and positions are never reused.
    references: Vec<Reference>,
    /// Local reference frames above the run unit's own, innermost last.
    frames: Vec<LocalFrame>,
    serials: u32,
    /// How references were made and how they expired, each told once.
    events: Vec<String>,
    event_index: HashMap<String, u32>,
}

struct Reference {
    /// The object's position plus one.
    object: u32,
    /// The serial of the local frame it belongs to (0 for the run unit's own), or GLOBAL.
    frame: u32,
    made: u32,
    /// The event that freed it, plus one; 0 while it is valid.
    freed: u32,
}

struct LocalFrame {
    serial: u32,
    /// The first reference made after the frame was pushed.
    first: u32,
    /// Pushed by PushLocalFrame, not by a method's invocation.
    pushed: bool,
}

/// What an object reference's four bytes name.
pub(crate) enum Referent {
    Null,
    Object(u32),
    /// A reference that was freed, and the message that says how.
    Expired(String),
    Unknown,
}

pub(crate) struct LoadedClass {
    pub external: String,
    /// None for a Java class, java.lang.Object among them.
    pub code: Option<Rc<ClassCode>>,
    pub parent: Option<usize>,
    /// The factory object's reference.
    pub factory_object: u32,
    /// Loaded storage of the factory data, and of each method's WORKING-STORAGE once it has run.
    pub factory_data: Option<usize>,
    pub methods: Vec<Option<usize>>,
}

pub(crate) struct Instance {
    pub class: usize,
    /// A factory object, on which INVOKE runs factory methods.
    pub factory: bool,
    /// The loaded storage of each COBOL class's instance data, by class.
    pub parts: Vec<(usize, usize)>,
}

impl Objects {
    pub(crate) fn find(&self, external: &str) -> Option<usize> {
        self.classes.iter().position(|c| c.external == external)
    }

    /// An object by its position plus one.
    pub(crate) fn object(&self, id: u32) -> Option<&Instance> {
        (id as usize).checked_sub(1).and_then(|i| self.objects.get(i))
    }

    pub(crate) fn add_object(&mut self, object: Instance) -> Result<u32, String> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(format!("the run unit created more than {MAX_OBJECTS} objects, which ironwork for COBOL never frees"));
        }
        self.objects.push(object);
        Ok(self.objects.len() as u32)
    }

    /// An event's number, the same for the same words.
    pub(crate) fn event(&mut self, text: String) -> u32 {
        if let Some(&n) = self.event_index.get(&text) {
            return n;
        }
        let n = self.events.len() as u32;
        self.event_index.insert(text.clone(), n);
        self.events.push(text);
        n
    }

    pub(crate) fn told(&self, event: u32) -> &str {
        self.events.get(event as usize).map_or("", String::as_str)
    }

    fn make(&mut self, object: u32, frame: u32, made: u32) -> Result<u32, String> {
        if self.references.len() >= MAX_REFERENCES {
            return Err(format!(
                "the run unit made more than {MAX_REFERENCES} object references, which ironwork for COBOL keeps so that an expired one can say where it expired (see {REFERENCES_KEPT})"
            ));
        }
        self.references.push(Reference { object, frame, made, freed: 0 });
        Ok(self.references.len() as u32)
    }

    /// A local reference in the innermost frame.
    pub(crate) fn local(&mut self, object: u32, made: u32) -> Result<u32, String> {
        let frame = self.frames.last().map_or(0, |f| f.serial);
        self.make(object, frame, made)
    }

    /// A local reference in the frame with this serial.
    pub(crate) fn local_in(&mut self, frame: u32, object: u32, made: u32) -> Result<u32, String> {
        self.make(object, frame, made)
    }

    pub(crate) fn global(&mut self, object: u32, made: u32) -> Result<u32, String> {
        self.make(object, GLOBAL, made)
    }

    /// A new local frame, for a method's invocation or PushLocalFrame: its depth and serial.
    pub(crate) fn push_frame(&mut self, pushed: bool) -> (usize, u32) {
        self.serials = if self.serials >= GLOBAL - 1 { 1 } else { self.serials + 1 };
        self.frames.push(LocalFrame { serial: self.serials, first: self.references.len() as u32, pushed });
        (self.frames.len() - 1, self.serials)
    }

    /// Pops the frame at `depth` and every frame above it, freeing their local references.
    pub(crate) fn pop_frames(&mut self, depth: usize, event: u32) {
        let Some(first) = self.frames.get(depth).map(|f| f.first as usize) else { return };
        let serials: Vec<u32> = self.frames.drain(depth..).map(|f| f.serial).collect();
        for r in self.references.iter_mut().skip(first) {
            if r.freed == 0 && serials.contains(&r.frame) {
                r.freed = event + 1;
            }
        }
    }

    /// The depth of the innermost frame when PushLocalFrame pushed it.
    pub(crate) fn pushed_frame(&self) -> Option<usize> {
        self.frames.last().filter(|f| f.pushed).map(|_| self.frames.len() - 1)
    }

    pub(crate) fn referent(&self, value: u32) -> Referent {
        if value == 0 {
            return Referent::Null;
        }
        match self.references.get(value as usize - 1) {
            None => Referent::Unknown,
            Some(r) if r.freed == 0 => Referent::Object(r.object),
            Some(r) => Referent::Expired(format!("{}; it {}", self.described(r), self.told(r.freed - 1))),
        }
    }

    pub(crate) fn is_global(&self, value: u32) -> bool {
        (value as usize).checked_sub(1).and_then(|i| self.references.get(i)).is_some_and(|r| r.frame == GLOBAL)
    }

    /// Frees a valid reference, as DeleteLocalRef or DeleteGlobalRef does.
    pub(crate) fn free(&mut self, value: u32, event: u32) {
        if let Some(r) = (value as usize).checked_sub(1).and_then(|i| self.references.get_mut(i)) {
            r.freed = event + 1;
        }
    }

    /// `a local reference to an Account object, made by ...`
    fn described(&self, r: &Reference) -> String {
        let kind = if r.frame == GLOBAL { "global" } else { "local" };
        let object = match self.object(r.object) {
            Some(o) if o.factory => format!("the factory object of {}", self.classes[o.class].external),
            Some(o) => {
                let class = &self.classes[o.class].external;
                let article = if class.starts_with(['A', 'E', 'I', 'O', 'U', 'a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
                format!("{article} {class} object")
            }
            None => "an object".into(),
        };
        format!("a {kind} reference to {object}, {}", self.told(r.made))
    }
}

/// A COBOL class, compiled.
pub(crate) struct ClassCode {
    /// The external name of the class it inherits.
    pub parent: String,
    pub factory: Option<Part>,
    pub object: Option<Part>,
    pub methods: Vec<MethodCode>,
}

/// FACTORY or OBJECT WORKING-STORAGE, laid out as a program's, and where each record starts in it.
pub(crate) struct Part {
    pub data: Rc<Compiled>,
    pub records: Vec<u32>,
}

pub(crate) struct MethodCode {
    pub name: String,
    pub factory: bool,
    /// Java types of the parameters and of the returned item, as a JNI signature spells them.
    pub params: Vec<String>,
    pub returns: Option<String>,
    pub code: Rc<Compiled>,
    /// LINKAGE records the method declares; the records of its paragraph's data follow them.
    pub own_records: usize,
}

/// A class definition's source compiles when each of its methods does.
pub(crate) fn compile_class_definition(program: Program, flags: &[String]) -> Result<Compiled, Vec<Error>> {
    let mut shell = program.clone();
    shell.oo = None;
    let mut compiled = crate::compile_program(shell, flags, false)?;
    let (_, diagnostics) = class_code(&program, flags)?;
    compiled.diagnostics.extend(diagnostics);
    compiled.program = program;
    Ok(compiled)
}

/// A class definition, INVOKE or an object reference; the JNI reached through JNIENVPTR alone is
/// not object-oriented syntax.
fn object_oriented(program: &Program) -> bool {
    let files = program.files.iter().flat_map(|f| f.records.iter());
    let mut data = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(files);
    let mut invoke = false;
    for p in &program.paragraphs {
        each(&p.statements, &mut |s| invoke |= matches!(s, Stmt::Invoke(_)));
    }
    program.oo.as_deref().is_some_and(|o| o.class().is_some()) || invoke || data.any(|e| e.usage == Some(Usage::ObjectReference))
}

/// IBM's rules for the options a program is compiled with (see [`OO_OPTIONS_REQUIRED`]): object-
/// oriented syntax needs THREAD, DLL, RENT and DBCS, NORENT conflicts with THREAD and DLL, and under
/// THREAD a program is RECURSIVE, not INITIAL, contains no program, and SORTs or MERGEs no file. A
/// method answers only for its statements; its class answers for the options. A missing option and
/// the NORENT conflict are warnings, the rest errors (see [`OO_OPTIONS_SEVERITY`]).
pub(crate) fn option_rules(program: &Program, options: &Options, errors: &mut Vec<Error>) {
    let oo = program.oo.as_deref();
    let method = oo.and_then(Oo::method).is_some();
    let who = match oo.and_then(Oo::class) {
        Some(c) => format!("class {}", c.name),
        None => format!("program {}", program.id),
    };
    if !method {
        let forcing: Vec<&str> = [(options.thread, "THREAD"), (options.dll, "DLL")].into_iter().filter(|(on, _)| *on).map(|(_, o)| o).collect();
        if !options.rent && !forcing.is_empty() {
            errors.push(Error::warning(Pos::default(), format!("NORENT conflicts with {}, which IBM compiles only as RENT (see {OO_OPTIONS_REQUIRED})", forcing.join(" and "))));
        }
        if object_oriented(program) {
            let missing: Vec<&str> = [(options.thread, "THREAD"), (options.dll, "DLL"), (options.rent || !forcing.is_empty(), "RENT"), (options.dbcs, "DBCS")]
                .into_iter()
                .filter(|(on, _)| !*on)
                .map(|(_, o)| o)
                .collect();
            if !missing.is_empty() {
                errors.push(Error::warning(
                    Pos::default(),
                    format!(
                        "{who} uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: {} missing from its CBL or PROCESS cards (see {OO_OPTIONS_REQUIRED} and {OO_OPTIONS_SEVERITY})",
                        missing.join(", ")
                    ),
                ));
            }
        }
    }
    if !options.thread {
        return;
    }
    if !method && oo.and_then(Oo::class).is_none() {
        if !program.recursive {
            errors.push(Error::at(Pos::default(), format!("{who} is compiled with THREAD, which requires RECURSIVE in its PROGRAM-ID paragraph")));
        }
        if program.initial {
            errors.push(Error::at(Pos::default(), format!("{who} is INITIAL, which THREAD does not allow")));
        }
        if let Some(inner) = program.nested.first() {
            errors.push(Error::at(Pos::default(), format!("{who} contains program {inner}, and THREAD does not allow nested programs")));
        }
    }
    for p in &program.paragraphs {
        each(&p.statements, &mut |s| {
            if let Stmt::Sorting(so) = s
                && let Sorting::Sort(st) = &**so
                && (st.merge || program.files.iter().any(|f| f.name == st.subject.name))
            {
                let verb = if st.merge { "MERGE" } else { "SORT of a file" };
                errors.push(Error::at(st.pos, format!("{verb} is not allowed in a program compiled with THREAD")));
            }
        });
    }
}

pub(crate) fn refuse_to_run(program: &Program) -> Result<(), crate::Abend> {
    match program.oo.as_ref().and_then(|o| o.class()) {
        Some(c) => Err(crate::Abend { code: crate::abend::AbendCode::Ironwork, message: format!("{} is a class definition: run a program that uses it", c.name), pos: c.pos }),
        None => Ok(()),
    }
}

/// The external name a class definition defines.
pub(crate) fn defined_class(program: &Program) -> Option<String> {
    let oo = program.oo.as_ref()?;
    let class = oo.class()?;
    Some(oo.external(&class.name).map_or_else(|| external_class_name(&class.name), str::to_owned))
}

fn records(entries: &[DataEntry]) -> usize {
    entries.iter().filter(|e| e.level == 1 || e.level == 77).count()
}

/// Every name a method declares, which hides a name of its paragraph's data.
fn declared_names(program: &Program) -> HashSet<String> {
    let files = program.files.iter().flat_map(|f| f.records.iter());
    program
        .working_storage
        .iter()
        .chain(&program.local_storage)
        .chain(&program.linkage)
        .chain(files)
        .flat_map(|e| e.name.iter().chain(&e.indexed_by))
        .cloned()
        .collect()
}

/// A class definition's code, and the warnings and informational messages it compiled with.
pub(crate) fn class_code(program: &Program, flags: &[String]) -> Result<(ClassCode, Vec<Error>), Vec<Error>> {
    let Some(oo) = program.oo.as_deref() else { return Err(vec![Error::at(Pos::default(), "not a class definition")]) };
    let Some(def) = oo.class() else { return Err(vec![Error::at(Pos::default(), "not a class definition")]) };
    let mut errors = Vec::new();
    let mut options = Options::default();
    for option in &program.options {
        options.apply(option).ok();
    }
    for flag in flags {
        options.apply_flag(flag).ok();
    }
    option_rules(program, &options, &mut errors);
    let external = defined_class(program).unwrap_or_default();
    let parent = match oo.external(&def.inherits) {
        Some(e) => e.to_owned(),
        None => {
            errors.push(Error::at(def.pos, format!("{}: the class a class INHERITS must be named in its REPOSITORY paragraph", def.inherits)));
            String::new()
        }
    };
    if def.inherits == def.name || parent == external {
        errors.push(Error::at(def.pos, format!("class {} cannot inherit from itself", def.name)));
    }
    let mut base = program.clone();
    base.oo = None;
    let mut code = ClassCode { parent, factory: None, object: None, methods: Vec::new() };
    for (factory, part) in [(true, &def.factory), (false, &def.object)] {
        let Some(part) = part else { continue };
        if let Some(e) = part.working_storage.iter().find(|e| !e.indexed_by.is_empty()) {
            errors.push(Error::at(e.pos, "INDEXED BY in FACTORY or OBJECT data is not supported yet"));
            continue;
        }
        let mut data = base.clone();
        data.working_storage = part.working_storage.clone();
        data.oo = Some(Box::new(Oo { repository: oo.repository.clone(), unit: OoUnit::Program }));
        match crate::compile_program(data, flags, false) {
            Ok(c) => {
                let offsets = c.layout.items.iter().filter(|i| i.parent.is_none()).take(records(&part.working_storage)).map(|i| i.offset).collect();
                let compiled = Part { data: Rc::new(c), records: offsets };
                if factory { code.factory = Some(compiled) } else { code.object = Some(compiled) }
            }
            Err(e) => errors.extend(e),
        }
        for m in &part.methods {
            match method_code(program, m, part, factory, flags) {
                Ok(mc) => {
                    errors.extend(mc.code.diagnostics.iter().cloned());
                    code.methods.push(mc);
                }
                Err(e) => errors.extend(e),
            }
        }
    }
    for (k, m) in code.methods.iter().enumerate() {
        if let Some(twin) = code.methods[..k].iter().find(|o| o.name == m.name && o.params == m.params) {
            let kind = |f: bool| if f { "factory" } else { "instance" };
            let pos = m.code.program.oo.as_deref().and_then(Oo::method).map_or(def.pos, |m| m.pos);
            errors.push(Error::at(pos, format!("{} method \"{}\" has the same parameter types as {} method \"{}\"", kind(m.factory), m.name, kind(twin.factory), twin.name)));
        }
    }
    if crate::refused(&errors, &options) { Err(errors) } else { Ok((code, errors)) }
}

/// A method compiled with its paragraph's data after its own LINKAGE records; a name the method
/// declares itself hides the paragraph's.
fn method_code(class: &Program, method: &Program, part: &ClassPart, factory: bool, flags: &[String]) -> Result<MethodCode, Vec<Error>> {
    let mut p = method.clone();
    p.sources = class.sources.clone();
    p.options = class.options.clone();
    let own_records = records(&p.linkage);
    let hidden = declared_names(&p);
    let hide = |n: &mut String| {
        if hidden.contains(n) {
            n.insert(0, ' ');
        }
    };
    for e in &part.working_storage {
        let mut e = e.clone();
        e.value = None;
        e.name.iter_mut().for_each(hide);
        e.redefines.iter_mut().for_each(hide);
        e.depending_on.iter_mut().for_each(|r| hide(&mut r.name));
        p.linkage.push(e);
    }
    let pos = method.oo.as_deref().and_then(Oo::method).map_or(part.pos, |m| m.pos);
    let name = method.id.clone();
    let compiled = crate::compile(p, flags)?;
    let mut errors = Vec::new();
    let layout = &compiled.layout;
    let oo = compiled.program.oo.as_deref();
    let own = |n: &str| layout.linkage_roots.iter().take(own_records).copied().find(|&i| layout.items[i].name.as_deref() == Some(n));
    let mut params = Vec::new();
    for param in &compiled.program.using {
        if !param.by_value {
            errors.push(Error::at(pos, format!("method \"{name}\" receives {} BY REFERENCE: a method's parameters are BY VALUE", param.name)));
        }
        match own(&param.name).map(|i| item_type(layout, oo, i)) {
            Some(Ok(t)) => params.push(t),
            Some(Err(m)) => errors.push(Error::at(pos, format!("method \"{name}\" parameter {}: {m}", param.name))),
            None => errors.push(Error::at(pos, format!("method \"{name}\" parameter {}: not a record of the method's own LINKAGE SECTION", param.name))),
        }
    }
    let mut shared = Vec::new();
    for paragraph in &compiled.program.paragraphs {
        each(&paragraph.statements, &mut |s| {
            if let Stmt::Set { set: SetStmt::AddressOf { targets, .. }, pos } = s {
                for r in targets {
                    if let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos)
                        && layout.items[i].linkage.is_some_and(|l| l as usize >= own_records)
                    {
                        shared.push(Error::at(*pos, format!("SET ADDRESS OF {}: FACTORY and OBJECT data is WORKING-STORAGE, not LINKAGE", r.name)));
                    }
                }
            }
        });
    }
    errors.extend(shared);
    let returns = match &compiled.program.returning {
        None => None,
        Some(r) => match own(r).map(|i| item_type(layout, oo, i)) {
            Some(Ok(t)) => Some(t),
            Some(Err(m)) => {
                errors.push(Error::at(pos, format!("method \"{name}\" RETURNING {r}: {m}")));
                None
            }
            None => {
                errors.push(Error::at(pos, format!("method \"{name}\" RETURNING {r}: not a record of the method's own LINKAGE SECTION")));
                None
            }
        },
    };
    if !errors.is_empty() {
        return Err(errors.into_iter().map(|e| e.in_files(&class.sources)).collect());
    }
    Ok(MethodCode { name, factory, params, returns, code: Rc::new(compiled), own_records })
}

/// The Java type of a class-name's objects, as a JNI signature spells it.
pub(crate) fn class_type(external: &str) -> String {
    match external {
        "jstring" | "java.lang.String" => "Ljava/lang/String;".into(),
        "jbooleanArray" => "[Z".into(),
        "jbyteArray" => "[B".into(),
        "jshortArray" => "[S".into(),
        "jintArray" => "[I".into(),
        "jlongArray" => "[J".into(),
        "jcharArray" => "[C".into(),
        "jfloatArray" => "[F".into(),
        "jdoubleArray" => "[D".into(),
        "jobjectArray" => "[Ljava/lang/Object;".into(),
        e => match e.strip_prefix("jobjectArray:") {
            Some(element) => format!("[L{};", element.replace('.', "/")),
            None => format!("L{};", e.replace('.', "/")),
        },
    }
}

/// A PIC X item with exactly the two condition-names IBM's boolean needs.
fn boolean(layout: &Layout, item: usize) -> bool {
    let byte = |l: &Literal| match l {
        Literal::Hex(b) if b.len() == 1 => Some(b[0]),
        Literal::Figurative(Figurative::LowValue) => Some(0x00),
        Literal::Figurative(Figurative::HighValue) => Some(0xFF),
        _ => None,
    };
    let conditions: Vec<_> = layout.conditions.iter().filter(|c| c.item == item).collect();
    let is = |low: u8, high: Option<u8>| {
        conditions.iter().any(|c| matches!(c.values.as_slice(), [(l, h)] if byte(l) == Some(low) && h.as_ref().and_then(byte) == high))
    };
    conditions.len() == 2 && is(0x00, None) && is(0x01, Some(0xFF))
}

/// The Java type an item passes as, as a JNI signature spells it; a universal object reference
/// passes as java.lang.Object.
pub(crate) fn item_type(layout: &Layout, oo: Option<&Oo>, item: usize) -> Result<String, String> {
    let it = &layout.items[item];
    Ok(match it.kind {
        Kind::Alnum { .. } if it.size == 1 => (if boolean(layout, item) { "Z" } else { "B" }).into(),
        Kind::Binary { digits, scale: 0, signed: true, .. } => (match digits {
            1..=4 => "S",
            5..=9 => "I",
            _ => "J",
        })
        .into(),
        Kind::Float(zarch::hfp::Precision::Short) => "F".into(),
        Kind::Float(_) => "D".into(),
        Kind::National if it.size == 2 => "C".into(),
        Kind::ObjectReference => match &it.object_class {
            None => format!("L{};", JAVA_LANG_OBJECT.replace('.', "/")),
            Some(c) => class_type(oo.and_then(|o| o.external(c)).ok_or_else(|| format!("class {c} is not named in the REPOSITORY paragraph"))?),
        },
        _ => return Err("not a type Java shares with COBOL: PIC X for byte or boolean, a signed binary integer, COMP-1, COMP-2, PIC N for char, or an object reference".into()),
    })
}

/// The Java type of an INVOKE argument: an item, a one-character reference modification, LENGTH OF,
/// or one of the literals IBM lists.
pub(crate) fn operand_type(layout: &Layout, oo: Option<&Oo>, op: &Operand) -> Result<String, String> {
    match op {
        Operand::Ref(r) => {
            let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return Err(format!("{} is not a data item", r.name)) };
            match &r.refmod {
                None => item_type(layout, oo, i),
                Some(_) if layout.items[i].kind == Kind::National => Err("a reference-modified national argument is not supported yet".into()),
                Some(rm) if matches!(rm.length.as_deref(), Some(Expr::Operand(Operand::Literal(Literal::Number(n)))) if n == "1") => Ok("C".into()),
                Some(_) => Err("a reference-modified argument must be one character long".into()),
            }
        }
        Operand::LengthOf(_) => Ok("I".into()),
        Operand::Literal(Literal::Number(t)) if !t.contains('.') && t.trim_start_matches(['+', '-']).len() <= 9 => Ok("I".into()),
        Operand::Literal(Literal::Figurative(Figurative::Zero)) => Ok("I".into()),
        Operand::Literal(Literal::Figurative(Figurative::Space | Figurative::Quote | Figurative::HighValue | Figurative::LowValue)) => Ok("B".into()),
        Operand::Literal(Literal::Alnum(s)) if s.chars().count() == 1 => Ok("B".into()),
        Operand::Literal(Literal::National(s)) if s.chars().count() == 1 => Ok("C".into()),
        _ => Err("not an argument Java takes: an item of a type Java shares, LENGTH OF, an integer literal of up to nine digits, or a one-character literal".into()),
    }
}

fn is_named(r: &Ref, name: &str) -> bool {
    r.name == name && r.qualifiers.is_empty() && r.subscripts.is_empty() && r.refmod.is_none()
}

/// SELF and JNIENVPTR, which are not data items of the program.
pub(crate) fn special_register(layout: &Layout, r: &Ref) -> bool {
    (is_named(r, "SELF") || is_named(r, "JNIENVPTR")) && layout.resolve(&r.name, &r.qualifiers, r.pos).is_err()
}

/// A class-name of the REPOSITORY used where a data item could also be named.
fn class_name<'o>(layout: &Layout, oo: Option<&'o Oo>, r: &Ref) -> Option<&'o str> {
    let external = oo?.external(&r.name)?;
    (r.qualifiers.is_empty() && r.subscripts.is_empty() && r.refmod.is_none() && layout.resolve(&r.name, &r.qualifiers, r.pos).is_err()).then_some(external)
}

impl Check<'_> {
    pub(crate) fn invoke(&mut self, i: &Invoke) {
        let oo = self.program.oo.as_deref();
        let in_method = oo.and_then(Oo::method).is_some();
        let err = |m: String| Error::at(i.pos, m);
        let special = is_named(&i.target, "SELF") || is_named(&i.target, "SUPER");
        let class = class_name(self.layout, oo, &i.target).is_some();
        let mut typed = false;
        if special && self.layout.resolve(&i.target.name, &[], i.pos).is_err() {
            if !in_method {
                self.errors.push(err(format!("INVOKE {}: SELF and SUPER can be used only in a method", i.target.name)));
            }
        } else if !class {
            self.reference(&i.target);
            match self.layout.resolve(&i.target.name, &i.target.qualifiers, i.target.pos) {
                Ok(Resolved::Item(k)) if self.layout.items[k].kind == Kind::ObjectReference => typed = self.layout.items[k].object_class.is_some(),
                Ok(_) => self.errors.push(err(format!("INVOKE {}: not an object reference or a class named in the REPOSITORY paragraph", i.target.name))),
                Err(_) => {}
            }
        }
        match &i.method {
            InvokeMethod::New => {
                if !class {
                    self.errors.push(err(format!("INVOKE {} NEW: NEW takes a class-name from the REPOSITORY paragraph", i.target.name)));
                }
                match &i.returning {
                    None => self.errors.push(err("INVOKE ... NEW needs RETURNING an object reference".into())),
                    Some(r) if !matches!(self.layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(Resolved::Item(k)) if self.layout.items[k].kind == Kind::ObjectReference) => {
                        self.errors.push(err(format!("INVOKE ... NEW RETURNING {}: not an object reference", r.name)));
                    }
                    Some(_) => {}
                }
            }
            InvokeMethod::Named(name) if name.is_empty() => self.errors.push(err("INVOKE with an empty method name".into())),
            InvokeMethod::Named(_) => {}
            InvokeMethod::Identifier(r) => {
                self.reference(r);
                if let Ok(Resolved::Item(k)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
                    && !matches!(self.layout.items[k].kind, Kind::Alnum { .. } | Kind::National | Kind::Group)
                {
                    self.errors.push(err(format!("INVOKE ... {}: a method name is held in an alphanumeric or national item", r.name)));
                }
                if typed {
                    self.errors.push(err(format!("INVOKE {} {}: a method named by a data item is invoked on a universal object reference", i.target.name, r.name)));
                }
            }
        }
        for op in &i.using {
            self.operand(op);
            if let Err(m) = operand_type(self.layout, oo, op) {
                self.errors.push(err(format!("INVOKE argument: {m}")));
            }
        }
        if let Some(r) = &i.returning {
            self.reference(r);
            if r.refmod.is_some() {
                self.errors.push(err(format!("INVOKE ... RETURNING {}: not reference-modified", r.name)));
            }
            if let (false, Ok(Resolved::Item(k))) = (i.method == InvokeMethod::New, self.layout.resolve(&r.name, &r.qualifiers, r.pos))
                && let Err(m) = item_type(self.layout, oo, k)
            {
                self.errors.push(err(format!("INVOKE ... RETURNING {}: {m}", r.name)));
            }
        }
        self.statements(i.on_exception.as_deref().unwrap_or_default());
        self.statements(i.not_on_exception.as_deref().unwrap_or_default());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Object,
    ProgramPointer,
    Null,
    Other,
}

/// The rules for object references, SELF, EXIT METHOD and the rest of the object-oriented syntax,
/// over a program or method that has otherwise checked.
struct Rules<'a> {
    layout: &'a Layout,
    method: bool,
    uses_oo: bool,
    errors: &'a mut Vec<Error>,
}

pub(crate) fn check(layout: &Layout, program: &Program, errors: &mut Vec<Error>) {
    let oo = program.oo.as_deref();
    let method = oo.and_then(Oo::method).is_some();
    let mut rules = Rules { layout, method, uses_oo: oo.is_some_and(|o| !o.repository.is_empty()), errors };
    for item in &layout.items {
        if item.kind == Kind::ObjectReference {
            rules.uses_oo = true;
        }
        if let Some(c) = &item.object_class
            && oo.and_then(|o| o.external(c)).is_none()
        {
            rules.errors.push(Error::at(item.pos, format!("OBJECT REFERENCE {c}: the class must be named in the REPOSITORY paragraph")));
        }
    }
    if method && let Some(f) = program.files.first() {
        rules.errors.push(Error::at(f.pos, "a method's FILE SECTION can define only EXTERNAL files, which ironwork for COBOL does not support yet"));
    }
    for p in &program.paragraphs {
        rules.statements(&p.statements);
    }
    // The Report Writer precompiler's code MOVEs each CONTROL and SOURCE item.
    for report in &program.report_writer.reports {
        report.controls.iter().for_each(|c| rules.plain(&Operand::Ref(c.clone()), c.pos));
        for e in report.groups.iter().flat_map(|g| &g.entries) {
            if let Some(syntax::report::Content::Source(x)) = &e.content {
                rules.expr(x, e.pos);
            }
        }
    }
    let exec = program.exec_declarations.iter().map(|b| (b.kind, b.pos)).chain(execs(&program.paragraphs));
    for (kind, pos) in exec {
        if method {
            rules.errors.push(Error::at(pos, "a class definition cannot contain EXEC statements"));
        } else if kind == ExecKind::Cics && rules.uses_oo {
            rules.errors.push(Error::at(pos, "a program that uses object-oriented syntax cannot contain EXEC CICS"));
        }
    }
}

fn execs(paragraphs: &[Paragraph]) -> Vec<(ExecKind, Pos)> {
    let mut out = Vec::new();
    for p in paragraphs {
        each(&p.statements, &mut |s| {
            if let Stmt::Exec(b) = s {
                out.push((b.kind, b.pos));
            }
        });
    }
    out
}

/// Every statement, and every statement nested in it.
fn each(stmts: &[Stmt], f: &mut dyn FnMut(&Stmt)) {
    for s in stmts {
        f(s);
        for body in bodies(s) {
            each(body, f);
        }
    }
}

fn opt(o: &Option<Vec<Stmt>>) -> &[Stmt] {
    o.as_deref().unwrap_or_default()
}

fn handlers(h: &Handlers) -> [&[Stmt]; 2] {
    [opt(&h.on), opt(&h.not_on)]
}

pub(crate) fn bodies(s: &Stmt) -> Vec<&[Stmt]> {
    match s {
        Stmt::If { then, otherwise, .. } => vec![then, otherwise],
        Stmt::PerformInline { body, .. } => vec![body],
        Stmt::Evaluate { whens, other, .. } => whens.iter().map(|w| w.body.as_slice()).chain([other.as_slice()]).collect(),
        Stmt::Compute { size_error: Some(se), .. } => vec![&se.on, &se.not_on],
        Stmt::Arith(a) => a.size_error.iter().flat_map(|se| [se.on.as_slice(), se.not_on.as_slice()]).collect(),
        Stmt::Read(r) => handlers(&r.at_end).into_iter().chain(handlers(&r.invalid)).collect(),
        Stmt::Write { invalid, .. } | Stmt::Rewrite { invalid, .. } | Stmt::Delete { invalid, .. } | Stmt::Start { invalid, .. } => handlers(invalid).to_vec(),
        Stmt::Call(c) => vec![opt(&c.on_exception), opt(&c.not_on_exception)],
        Stmt::Invoke(i) => vec![opt(&i.on_exception), opt(&i.not_on_exception)],
        Stmt::String(st) => vec![opt(&st.on_overflow), opt(&st.not_on_overflow)],
        Stmt::Unstring(u) => vec![opt(&u.on_overflow), opt(&u.not_on_overflow)],
        Stmt::Search(se) => se.whens.iter().map(|(_, b)| b.as_slice()).chain([opt(&se.at_end)]).collect(),
        Stmt::Sorting(so) => match &**so {
            Sorting::Return { at_end, .. } => handlers(at_end).to_vec(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

impl Rules<'_> {
    fn statements(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.statement(s);
            for body in bodies(s) {
                self.statements(body);
            }
        }
    }

    fn statement(&mut self, s: &Stmt) {
        match s {
            Stmt::Move { from, to, pos } => {
                self.plain(from, *pos);
                to.iter().for_each(|r| self.receiver(r, *pos));
            }
            Stmt::Display { items, pos, .. } => items.iter().for_each(|o| self.plain(o, *pos)),
            Stmt::Compute { targets, expr, pos, .. } => {
                targets.iter().for_each(|t| self.receiver(&t.r, *pos));
                self.expr(expr, *pos);
            }
            Stmt::Arith(a) => {
                for (t, e) in &a.computations {
                    self.receiver(&t.r, a.pos);
                    self.expr(e, a.pos);
                }
            }
            Stmt::Initialize { targets, pos } | Stmt::Set { set: SetStmt::UpDown { targets, .. }, pos } => targets.iter().for_each(|r| self.receiver(r, *pos)),
            Stmt::Accept { target, pos, .. } => self.receiver(target, *pos),
            Stmt::String(st) => {
                st.sources.iter().for_each(|(o, _)| self.plain(o, st.pos));
                self.receiver(&st.into, st.pos);
            }
            Stmt::Unstring(u) => {
                self.plain(&Operand::Ref(u.source.clone()), u.pos);
                u.into.iter().for_each(|i| self.receiver(&i.target, u.pos));
            }
            Stmt::Inspect(i) => self.receiver(&i.target, i.pos),
            Stmt::If { cond, pos, .. } => self.cond(cond, *pos),
            Stmt::PerformInline { repeat, pos, .. } | Stmt::PerformProc { repeat, pos, .. } => match repeat {
                Loop::Until { cond, .. } => self.cond(cond, *pos),
                Loop::Varying { varying, .. } => self.cond(&varying.until, *pos),
                _ => {}
            },
            Stmt::Evaluate { subjects, whens, pos, .. } => {
                for subject in subjects {
                    match subject {
                        Subject::Cond(c) => self.cond(c, *pos),
                        Subject::Expr(e) => self.expr(e, *pos),
                        Subject::Bool(_) => {}
                    }
                }
                for w in whens {
                    for object in w.alternatives.iter().flatten() {
                        match object {
                            Object::Cond(c) => self.cond(c, *pos),
                            Object::Value { from, thru, .. } => thru.iter().chain([from]).for_each(|e| self.expr(e, *pos)),
                            _ => {}
                        }
                    }
                }
            }
            Stmt::Search(se) => se.whens.iter().for_each(|(c, _)| self.cond(c, se.pos)),
            Stmt::Set { set, pos } => self.set(set, *pos),
            Stmt::ExitMethod { pos } if !self.method => self.errors.push(Error::at(*pos, "EXIT METHOD can be used only in a method")),
            Stmt::ExitProgram { pos } if self.method => self.errors.push(Error::at(*pos, "EXIT PROGRAM cannot be used in a method: use EXIT METHOD or GOBACK")),
            Stmt::Invoke(_) => self.uses_oo = true,
            Stmt::Sorting(so) => match &**so {
                Sorting::Release { from: Some(op), pos, .. } => self.plain(op, *pos),
                Sorting::Return { into: Some(r), pos, .. } => self.receiver(r, *pos),
                _ => {}
            },
            _ => {}
        }
    }

    fn is_self(&self, r: &Ref) -> bool {
        is_named(r, "SELF") && self.layout.resolve(&r.name, &[], r.pos).is_err()
    }

    fn side_of_ref(&mut self, r: &Ref, pos: Pos) -> Side {
        if self.is_self(r) {
            if !self.method {
                self.errors.push(Error::at(pos, "SELF can be used only in a method"));
            }
            return Side::Object;
        }
        match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) if self.layout.items[i].kind == Kind::ObjectReference => Side::Object,
            Ok(Resolved::Item(i)) if self.layout.items[i].kind == Kind::ProgramPointer => Side::ProgramPointer,
            _ => Side::Other,
        }
    }

    fn side(&mut self, op: &Operand, pos: Pos) -> Side {
        match op {
            Operand::Ref(r) => self.side_of_ref(r, pos),
            Operand::Literal(Literal::Figurative(Figurative::Null)) => Side::Null,
            _ => Side::Other,
        }
    }

    /// An operand of a statement that takes no object reference or function-pointer.
    fn plain(&mut self, op: &Operand, pos: Pos) {
        let what = match self.side(op, pos) {
            Side::Object => "an object reference",
            Side::ProgramPointer => "a function-pointer or procedure-pointer",
            _ => return,
        };
        let name = if let Operand::Ref(r) = op { r.name.as_str() } else { "" };
        self.errors.push(Error::at(pos, format!("{name} is {what}: it can be used only in SET, INVOKE, CALL and a relation condition")));
    }

    fn receiver(&mut self, r: &Ref, pos: Pos) {
        if is_named(r, "JNIENVPTR") && self.layout.resolve(&r.name, &[], r.pos).is_err() {
            self.errors.push(Error::at(pos, "JNIENVPTR cannot receive a value"));
        } else if self.is_self(r) {
            self.errors.push(Error::at(pos, "SELF cannot receive a value"));
        } else {
            self.plain(&Operand::Ref(r.clone()), pos);
        }
    }

    fn expr(&mut self, e: &Expr, pos: Pos) {
        match e {
            Expr::Operand(op) => self.plain(op, pos),
            Expr::Neg(inner) => self.expr(inner, pos),
            Expr::Bin(a, _, b) => {
                self.expr(a, pos);
                self.expr(b, pos);
            }
        }
    }

    fn cond(&mut self, c: &Cond, pos: Pos) {
        match c {
            Cond::Rel(a, op, b) => {
                let side = |r: &mut Self, e: &Expr| match e {
                    Expr::Operand(o) => r.side(o, pos),
                    _ => Side::Other,
                };
                let (x, y) = (side(self, a), side(self, b));
                let handle = |s: Side| matches!(s, Side::Object | Side::ProgramPointer);
                if !handle(x) && !handle(y) {
                    self.expr(a, pos);
                    self.expr(b, pos);
                    return;
                }
                if !matches!(op, RelOp::Eq | RelOp::Ne) {
                    self.errors.push(Error::at(pos, "object references and function-pointers compare only as equal or not equal"));
                }
                let fits = |s: Side, other: Side| s == other || s == Side::Null || other == Side::Null;
                if !fits(x, y) {
                    self.errors.push(Error::at(pos, "an object reference compares with another object reference, SELF or NULL; a function-pointer with another or NULL"));
                }
            }
            Cond::Class(e, _) => self.expr(e, pos),
            Cond::Not(inner) => self.cond(inner, pos),
            Cond::And(a, b) | Cond::Or(a, b) => {
                self.cond(a, pos);
                self.cond(b, pos);
            }
            Cond::Name(_) | Cond::NameOrRel { .. } => {}
        }
    }

    fn set(&mut self, set: &SetStmt, pos: Pos) {
        let SetStmt::To { targets, value } = set else { return };
        let value_side = self.side(value, pos);
        for r in targets {
            if is_named(r, "JNIENVPTR") && self.layout.resolve(&r.name, &[], r.pos).is_err() {
                self.errors.push(Error::at(pos, "JNIENVPTR cannot receive a value"));
                continue;
            }
            if self.is_self(r) {
                self.errors.push(Error::at(pos, "SELF cannot receive a value"));
                continue;
            }
            let message = match (self.side_of_ref(r, pos), value_side) {
                (Side::Object, Side::Object | Side::Null) | (Side::ProgramPointer, Side::ProgramPointer | Side::Null) => continue,
                (Side::Object, _) => "an object reference takes another object reference, SELF or NULL",
                (Side::ProgramPointer, _) => "a function-pointer takes another function-pointer or NULL (SET TO ENTRY is not supported yet)",
                (_, Side::Object | Side::ProgramPointer) => "an object reference or function-pointer can be set only into its own kind",
                _ => continue,
            };
            self.errors.push(Error::at(pos, format!("SET {} TO: {message}", r.name)));
        }
    }
}

/// Where a COBOL class definition is found: among the programs already read, then in the program
/// libraries, as a member named with the class's simple name or its full name with periods as
/// underscores. None means a Java class. See [`numeric::assumptions::CLASS_SEARCH`].
pub(crate) fn find_class(library: &mut crate::unit::Library, external: &str) -> Result<Option<Program>, String> {
    if external == JAVA_LANG_OBJECT {
        return Ok(None);
    }
    if let Some(i) = library.programs.iter().position(|p| defined_class(p).as_deref() == Some(external)) {
        return Ok(Some(library.programs.remove(i)));
    }
    let member = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    let simple = external.rsplit('.').next().unwrap_or(external).to_owned();
    let mut names = vec![simple, external.replace('.', "_")];
    names.dedup();
    names.retain(|n| member(n));
    for dir in library.dirs.clone() {
        for name in &names {
            for variant in [name.clone(), name.to_ascii_lowercase(), name.to_ascii_uppercase()] {
                for ext in ["", ".cbl", ".CBL", ".cob", ".COB"] {
                    let path = dir.join(format!("{variant}{ext}"));
                    if !path.is_file() {
                        continue;
                    }
                    let text = std::fs::read(&path).map(|b| syntax::copy::decode(&b)).map_err(|e| format!("{}: {e}", path.display()))?;
                    let mut programs = syntax::parse_all_with(&text, &library.copy).map_err(|e| format!("class {external} does not compile: {}", e.place(&path.display().to_string())))?;
                    if defined_class(&programs[0]).as_deref() == Some(external) {
                        return Ok(Some(programs.remove(0)));
                    }
                }
            }
        }
    }
    Ok(None)
}
