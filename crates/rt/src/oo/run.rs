//! INVOKE, SELF and SUPER, and the JNI environment, at run time. A COBOL class runs here: its
//! factory data, each object's instance data and each method's WORKING-STORAGE are storage of the
//! run unit that is never released. An object reference holds a local or global reference, as the
//! JNI hands them out; a method's local references are freed when it returns. A Java class, or a
//! JNI service that needs a JVM, ends the run with abend JAVA naming what was reached.

use super::{ClassCode, Instance, JAVA_LANG_OBJECT, LoadedClass, MAX_MEMORY, Objects, Part, Referent, Running};
use crate::abend::{Abend, AbendCode, Ending};
use crate::callee::{self, By, Callee};
use crate::display::utf16_text;
use crate::host::Values;
use crate::jni;
use crate::lir::{CallArg, InvokePlan, MethodName, Receiver, Step};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{ADDRESS_BASE, Loaded, Loader, RETURN_CODE, RunUnit, UnitHost};
use crate::vocab::{Figurative, Pos};
use numeric::assumptions::{EXPIRED_REFERENCE_ABENDS, LOCAL_FRAMES};
use numeric::precision::{Fixed, Places};
use std::cmp::Ordering;
use std::rc::Rc;

type R<T> = Result<T, Abend>;

/// What INVOKE and the JNI services ask of the executor beyond `Values` and the run unit: the method
/// the activation runs, names and Java signatures, an argument's bytes, and running a method.
pub trait OoHost<'w, P: Copy, O, S>: Values<P, O> + UnitHost<'w> {
    fn running(&self) -> Option<Running>;
    fn program_id(&self) -> String;
    /// A name, or a Java type signature, which an executor may work out only when it is read.
    fn symbol(&mut self, symbol: &S, pos: Pos) -> R<String>;
    /// A data item's name as written, which messages give.
    fn place_name(&self, place: P) -> String;
    /// An operand's name when it is a data item.
    fn operand_name(&self, operand: &O) -> Option<String>;
    /// An INVOKE argument as the method receives it, in the bytes of its Java type.
    fn invoke_argument(&mut self, operand: &O, java: &str, pos: Pos) -> R<Vec<u8>>;
    /// Gives loaded storage `index` the VALUE clauses of `data`, a FACTORY or OBJECT part.
    fn initialize_data(&mut self, data: Self::Program, index: usize) -> R<()>;
    /// Runs a method as a called program runs. An error is one given before or after it ran.
    fn run_method(&mut self, call: MethodCall<Self::Program>, pos: Pos) -> R<Returned>;
}

/// How a method's run ended, and when it ended normally, its RETURNING item's name and value.
pub struct Returned {
    pub ending: R<Ending>,
    pub value: Option<(String, Val)>,
}

/// A COBOL method's activation: its code and loaded WORKING-STORAGE, the LINKAGE ordinal and
/// address of each record of the object's or the factory's data, its arguments' addresses, and the
/// frame it runs in.
pub struct MethodCall<H> {
    pub code: H,
    pub storage: usize,
    pub records: Vec<(usize, usize)>,
    pub arguments: Vec<Option<usize>>,
    pub running: Running,
}

/// What an INVOKE is sent to: a class, or an object by its position plus one.
enum Target {
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

/// SELF in method `running`, given its local reference the first time the method reads it.
pub fn self_reference<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, running: Option<Running>, pos: Pos) -> R<Loc> {
    let Some(m) = running else { return Err(Abend::ironwork("SELF outside a method", pos)) };
    if unit.mem[m.cell..m.cell + 4] == [0; 4] {
        let made = unit.oo.event(format!("SELF of {}", unit.oo.told(m.invoked)));
        let reference = unit.oo.local_in(m.frame, m.this, made).map_err(|e| Abend::ironwork(e, pos))?;
        unit.mem[m.cell..m.cell + 4].copy_from_slice(&reference.to_be_bytes());
    }
    Ok(Loc { offset: m.cell, len: 4, kind: Kind::ObjectReference, item: usize::MAX })
}

/// `line 12 of CLIENT`, or `line 30 of Account.credit` in a method.
fn site<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, pos: Pos) -> String {
    let id = x.program_id();
    match x.running() {
        Some(m) => format!("line {} of {}.{}", pos.line, x.unit().oo.classes[m.class].external, id),
        None => format!("line {} of {}", pos.line, id),
    }
}

/// The object an object reference's value identifies, None for NULL. A reference that was
/// freed, or four bytes no reference was given, end the run (see [`EXPIRED_REFERENCE_ABENDS`]).
fn referent<C>(objects: &Objects<C>, value: u32, what: &str, holder: &str, pos: Pos) -> R<Option<u32>> {
    match objects.referent(value) {
        Referent::Null => Ok(None),
        Referent::Object(o) => Ok(Some(o)),
        Referent::Expired(told) => Err(Abend::ironwork(format!("{what}: {holder} holds {told}, and IBM leaves using it unpredictable (see {EXPIRED_REFERENCE_ABENDS})"), pos)),
        Referent::Unknown => Err(Abend::ironwork(format!("{what}: {holder} holds X'{value:08X}', which is not an object reference"), pos)),
    }
}

/// A new local reference in the innermost frame.
fn local_reference<C>(objects: &mut Objects<C>, object: u32, made: String, pos: Pos) -> R<u32> {
    let made = objects.event(made);
    objects.local(object, made).map_err(|m| Abend::ironwork(m, pos))
}

/// The object SELF refers to, as the method's SELF reference says once it has been read.
fn self_object<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>, m: Running, what: &str, pos: Pos) -> R<u32> {
    let value = u32::from_be_bytes(unit.mem[m.cell..m.cell + 4].try_into().unwrap_or_default());
    Ok(referent(&unit.oo, value, what, "SELF", pos)?.unwrap_or(m.this))
}

/// Two object references compare equal when they identify the same object (Language Reference
/// SC27-8713-03, p. 282), so each is looked up; one compared with the figurative constant NULL
/// is only tested for NULL. None when neither is an object reference; `names` names the two.
pub fn compare_references<C>(objects: &Objects<C>, names: impl FnOnce() -> (String, String), x: (&Val, Option<Loc>), y: (&Val, Option<Loc>), pos: Pos) -> R<Option<Ordering>> {
    let reference = |l: Option<Loc>| l.is_some_and(|l| l.kind == Kind::ObjectReference);
    let (Val::Address(p), Val::Address(q)) = (x.0, y.0) else { return Ok(None) };
    if !reference(x.1) && !reference(y.1) {
        return Ok(None);
    }
    let (a, b) = names();
    let what = format!("{a} = {b}");
    let same = referent(objects, *p, &what, &a, pos)? == referent(objects, *q, &what, &b, pos)?;
    Ok(Some(if same { Ordering::Equal } else { Ordering::Less }))
}

fn room<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>, size: usize, pos: Pos) -> R<()> {
    if unit.mem.len() + size > MAX_MEMORY {
        return Err(Abend::ironwork(format!("objects and classes take the run unit past ironwork's {MAX_MEMORY} bytes"), pos));
    }
    Ok(())
}

/// Storage the run unit keeps: a loaded entry no CALL can name holds it, so that releasing
/// arguments never reaches it.
fn keep<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, compiled: Option<H>, bytes: &[u8], pos: Pos) -> R<usize> {
    room(unit, bytes.len(), pos)?;
    let base = unit.push_temporary(bytes);
    let files = compiled.as_ref().map_or(0, |c| L::shape(c).0);
    unit.programs.push(Loaded::new(compiled, String::new(), base, bytes.len(), files));
    Ok(unit.programs.len() - 1)
}

/// The JNIENVPTR cell, pointing at the JNI environment, which points at the function table.
pub fn jni_environment<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, pos: Pos) -> R<usize> {
    if let Some(cell) = unit.oo.jni {
        return Ok(cell);
    }
    let slots = jni::RESERVED + jni::FUNCTIONS.len();
    let loaded = keep(unit, None, &vec![0; 8 + 4 * slots], pos)?;
    let base = unit.programs[loaded].base;
    let address = |offset: usize| ADDRESS_BASE + offset as u32;
    let mut bytes = [address(base + 4).to_be_bytes(), address(base + 8).to_be_bytes()].concat();
    for slot in 0..slots {
        let value = if slot < jni::RESERVED { 0 } else { JNI_TAG + slot as u32 };
        bytes.extend(value.to_be_bytes());
    }
    unit.mem[base..base + bytes.len()].copy_from_slice(&bytes);
    unit.oo.jni = Some(base);
    Ok(base)
}

/// FACTORY or OBJECT data, zeroed and given its VALUE clauses.
fn part_storage<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, part: &Part<X::Program>, pos: Pos) -> R<usize> {
    let data = part.data.clone();
    let size = X::Loader::shape(&data).1;
    let loaded = keep(x.unit(), Some(data.clone()), &vec![0; size], pos)?;
    x.initialize_data(data, loaded)?;
    x.unit().programs[loaded].active = false;
    Ok(loaded)
}

/// The run unit's class of this external name, loading its COBOL class definition, and those of
/// the classes it inherits, the first time.
fn load_class<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, external: &str, pos: Pos) -> R<usize> {
    if let Some(c) = x.unit().oo.find(external) {
        return Ok(c);
    }
    let (code, sources) = match x.unit().library.class(external).map_err(|m| Abend::ironwork(m, pos))? {
        None => (None, Vec::new()),
        Some(found) => (Some(found.code), found.sources),
    };
    let oo = &mut x.unit().oo;
    let index = oo.classes.len();
    let methods = code.as_ref().map_or(0, |c| c.methods.len());
    oo.classes.push(LoadedClass { external: external.to_owned(), code: code.clone(), parent: None, factory_object: 0, factory_data: None, methods: vec![None; methods], sources });
    let factory_object = oo.add_object(Instance { class: index, factory: true, parts: Vec::new() }).map_err(|m| Abend::ironwork(m, pos))?;
    oo.classes[index].factory_object = factory_object;
    let Some(code) = code else { return Ok(index) };
    let parent = load_class(x, &code.parent, pos)?;
    let oo = &mut x.unit().oo;
    let mut at = Some(parent);
    for _ in 0..=oo.classes.len() {
        match at {
            Some(c) if c == index => return Err(Abend::ironwork(format!("class {external} inherits from itself"), pos)),
            Some(c) => at = oo.classes[c].parent,
            None => break,
        }
    }
    oo.classes[index].parent = Some(parent);
    if let Some(part) = &code.factory {
        let loaded = part_storage(x, part, pos)?;
        x.unit().oo.classes[index].factory_data = Some(loaded);
    }
    Ok(index)
}

/// The class and its ancestors, the class first.
fn chain<C>(objects: &Objects<C>, class: usize) -> Vec<usize> {
    let mut out = vec![class];
    while let Some(p) = objects.classes[*out.last().unwrap()].parent {
        if out.contains(&p) {
            break;
        }
        out.push(p);
    }
    out
}

fn object<C>(objects: &Objects<C>, id: u32, what: &str, pos: Pos) -> R<(usize, bool)> {
    objects.object(id).map(|o| (o.class, o.factory)).ok_or_else(|| Abend::ironwork(format!("{what}: no object {id}"), pos))
}

fn target<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, receiver: &Receiver<P, S>, what: &str, pos: Pos) -> R<Target> {
    match receiver {
        Receiver::SelfRef | Receiver::Super => {
            let name = if matches!(receiver, Receiver::SelfRef) { "SELF" } else { "SUPER" };
            let Some(m) = x.running() else { return Err(Abend::ironwork(format!("INVOKE {name} outside a method"), pos)) };
            let unit = x.unit();
            let this = self_object(unit, m, what, pos)?;
            if matches!(receiver, Receiver::SelfRef) {
                return Ok(Target::Object(this));
            }
            let start = unit.oo.classes[m.class].parent.ok_or_else(|| Abend::ironwork("INVOKE SUPER: the class has no parent", pos))?;
            Ok(Target::Super { this, start, factory: m.factory })
        }
        Receiver::Class { external, .. } => {
            let external = x.symbol(external, pos)?;
            Ok(Target::Class(load_class(x, &external, pos)?))
        }
        Receiver::Object(place) => {
            let loc = x.locate(*place, false)?;
            let name = x.place_name(*place);
            let value = match <[u8; 4]>::try_from(store::bytes(x.mem(), loc)) {
                Ok(bytes) if loc.kind == Kind::ObjectReference => u32::from_be_bytes(bytes),
                _ => return Err(Abend::ironwork(format!("INVOKE {name}: not an object reference"), pos)),
            };
            match referent(&x.unit().oo, value, what, &name, pos)? {
                Some(object) => Ok(Target::Object(object)),
                None => Err(Abend::ironwork(format!("{what}: the object reference {name} is NULL"), pos)),
            }
        }
    }
}

/// The receiver as the INVOKE names it, which messages give.
fn target_name<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, receiver: &Receiver<P, S>, pos: Pos) -> R<String> {
    Ok(match receiver {
        Receiver::SelfRef => "SELF".into(),
        Receiver::Super => "SUPER".into(),
        Receiver::Class { name, .. } => x.symbol(name, pos)?,
        Receiver::Object(place) => x.place_name(*place),
    })
}

fn method_name<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, method: &MethodName<P, S>, pos: Pos) -> R<String> {
    Ok(match method {
        MethodName::New => "NEW".into(),
        MethodName::Named(n) => x.symbol(n, pos)?,
        MethodName::Dynamic(place) => {
            let loc = x.locate(*place, false)?;
            let page = x.facts().page();
            let bytes = store::bytes(x.mem(), loc);
            let text = if loc.kind == Kind::National { utf16_text(bytes) } else { page.decode(bytes) };
            let name = text.trim_end_matches(' ').to_owned();
            if name.is_empty() {
                return Err(Abend::ironwork(format!("INVOKE: {} holds no method name", x.place_name(*place)), pos));
            }
            name
        }
    })
}

fn find<H>(objects: &Objects<Rc<ClassCode<H>>>, start: usize, factory: bool, name: &str, params: &[String], returns: Option<&str>) -> Found {
    for class in chain(objects, start) {
        let c = &objects.classes[class];
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

fn no_method<P, O, S>(plan: &InvokePlan<P, O, S>, what: String, pos: Pos) -> R<Step> {
    if plan.on_exception {
        return Ok(Step::Arm(1));
    }
    Err(Abend {
        code: AbendCode::user(4038),
        message: format!("IGZ0045S Unable to invoke method on line number {}. ({what}: no method matches it, and the INVOKE has no ON EXCEPTION)", pos.line),
        pos,
        file: None,
    })
}

fn succeeded<P, O, S>(plan: &InvokePlan<P, O, S>) -> Step {
    if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next }
}

/// INVOKE: Arm(1) when no method matches and ON EXCEPTION is written, Arm(0) after a method ran
/// when a phrase is written, Next when none is, or End after a STOP RUN. A CICS task refuses it
/// (C147).
pub fn invoke<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, plan: &InvokePlan<P, O, S>, pos: Pos) -> R<Step> {
    if x.unit().cics.is_some() {
        return Err(Abend::ironwork("INVOKE was reached in a CICS task, where a COBOL program with object-oriented syntax cannot run", pos));
    }
    x.unit().unfollowed("object-oriented COBOL and calls through pointers");
    let name = method_name(x, &plan.method, pos)?;
    let written = target_name(x, &plan.receiver, pos)?;
    let what = format!("INVOKE {written} \"{name}\"");
    let receiver = target(x, &plan.receiver, &what, pos)?;
    let mut params = Vec::new();
    let mut arguments = Vec::new();
    for (n, (op, java)) in plan.args.iter().enumerate() {
        let java = x.symbol(java, pos)?;
        let bytes = x.invoke_argument(op, &java, pos)?;
        arguments.push(if is_reference(&java) {
            let holder = x.operand_name(op).unwrap_or_else(|| format!("argument {}", n + 1));
            Argument::Object(referent(&x.unit().oo, u32::from_be_bytes(bytes.as_slice().try_into().unwrap_or_default()), &what, &holder, pos)?)
        } else {
            Argument::Bytes(bytes)
        });
        params.push(java);
    }
    if matches!(plan.method, MethodName::New) {
        let Target::Class(class) = receiver else { return Err(Abend::ironwork("INVOKE ... NEW takes a class-name", pos)) };
        return new_object(x, plan, &written, class, &params, pos);
    }
    let returns = match &plan.returning {
        None => None,
        Some((_, java)) => Some(x.symbol(java, pos)?),
    };
    let (this, start, factory) = match receiver {
        Target::Class(c) => (x.unit().oo.classes[c].factory_object, c, true),
        Target::Object(o) => {
            let (class, factory) = object(&x.unit().oo, o, &what, pos)?;
            (o, class, factory)
        }
        Target::Super { this, start, factory } => (this, start, factory),
    };
    match find(&x.unit().oo, start, factory, &name, &params, returns.as_deref()) {
        Found::Missing => no_method(plan, what, pos),
        Found::Java(class) => Err(java(what, &class, pos)),
        Found::ObjectEquals => {
            let same = matches!(arguments.first(), Some(Argument::Object(Some(other))) if *other == this);
            if let Some((place, _)) = &plan.returning {
                let dest = x.locate(*place, false)?;
                x.assign(dest, Val::Bytes(vec![u8::from(same)]), None, pos)?;
            }
            Ok(succeeded(plan))
        }
        Found::Cobol { class, method } => {
            x.unit().enter(pos)?;
            let step = run_method(x, plan, class, method, this, arguments, &what, pos);
            x.unit().depth -= 1;
            step
        }
    }
}

/// INVOKE class NEW: a COBOL class gets its instance data from its VALUE clauses and takes no
/// arguments; a class with a Java ancestor other than java.lang.Object needs the JVM.
fn new_object<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, plan: &InvokePlan<P, O, S>, written: &str, class: usize, params: &[String], pos: Pos) -> R<Step> {
    let what = format!("INVOKE {written} NEW");
    let oo = &x.unit().oo;
    let chain = chain(oo, class);
    if let Some(&j) = chain.iter().find(|&&c| oo.classes[c].code.is_none() && oo.classes[c].external != JAVA_LANG_OBJECT) {
        return Err(java(what, &oo.classes[j].external.clone(), pos));
    }
    if !params.is_empty() {
        return no_method(plan, format!("{what} USING {} arguments", params.len()), pos);
    }
    let mut parts = Vec::new();
    for &c in chain.iter().rev() {
        if let Some(code) = x.unit().oo.classes[c].code.clone()
            && let Some(part) = &code.object
        {
            parts.push((c, part_storage(x, part, pos)?));
        }
    }
    let object = x.unit().oo.add_object(Instance { class, factory: false, parts }).map_err(|m| Abend::ironwork(m, pos))?;
    let here = site(x, pos);
    let made = format!("made by INVOKE {} NEW at {here}", x.unit().oo.classes[class].external);
    let reference = local_reference(&mut x.unit().oo, object, made, pos)?;
    if let Some((place, _)) = &plan.returning {
        let dest = x.locate(*place, false)?;
        x.assign(dest, Val::Address(reference), None, pos)?;
    }
    Ok(succeeded(plan))
}

/// Runs a COBOL method as a called program runs, with the data of its paragraph as the records
/// after its own LINKAGE, its arguments BY VALUE and SELF, in a local frame of its own: the
/// object references it receives are new local references there, and every local reference
/// made there is freed when it returns. A returned object reference reaches the invoker as a
/// new local reference of the invoker's frame.
#[allow(clippy::too_many_arguments)]
fn run_method<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, plan: &InvokePlan<P, O, S>, class: usize, k: usize, this: u32, arguments: Vec<Argument>, what: &str, pos: Pos) -> R<Step> {
    let code = x.unit().oo.classes[class].code.clone().ok_or_else(|| Abend::ironwork("not a COBOL class", pos))?;
    let method = &code.methods[k];
    let this = if method.factory { x.unit().oo.classes[class].factory_object } else { this };
    let storage = match x.unit().oo.classes[class].methods[k] {
        Some(s) => s,
        None => {
            let size = X::Loader::shape(&method.code).1;
            let s = keep(x.unit(), Some(method.code.clone()), &vec![0; size], pos)?;
            x.unit().oo.classes[class].methods[k] = Some(s);
            s
        }
    };
    let (part, data) = if method.factory {
        (code.factory.as_ref(), x.unit().oo.classes[class].factory_data)
    } else {
        let data = x.unit().oo.object(this).and_then(|o| o.parts.iter().find(|(c, _)| *c == class)).map(|(_, l)| *l);
        (code.object.as_ref(), data)
    };
    let here = site(x, pos);
    let unit = x.unit();
    let base = data.map(|d| unit.programs[d].base);
    let return_code = [unit.mem[RETURN_CODE], unit.mem[RETURN_CODE + 1]];
    let mark = unit.mem.len();
    let invoked_text = format!("method \"{}\" of {}, invoked at {here}", method.name, unit.oo.classes[class].external);
    let invoked = unit.oo.event(invoked_text.clone());
    let (depth, frame) = unit.oo.push_frame(false);
    let mut addresses = Vec::new();
    for (n, argument) in arguments.into_iter().enumerate() {
        let bytes = match argument {
            Argument::Bytes(b) => b,
            Argument::Object(None) => vec![0; 4],
            Argument::Object(Some(object)) => {
                let made = unit.oo.event(format!("received as argument {} by {invoked_text}", n + 1));
                unit.oo.local_in(frame, object, made).map_err(|m| Abend::ironwork(m, pos))?.to_be_bytes().to_vec()
            }
        };
        addresses.push(Some(unit.push_temporary(&bytes)));
    }
    let cell = unit.push_temporary(&[0; 4]);
    let records = match (part, base) {
        (Some(part), Some(base)) => part.records.iter().enumerate().map(|(n, offset)| (method.own_records + n, base + *offset as usize)).collect(),
        _ => Vec::new(),
    };
    let running = Running { class, factory: method.factory, this, cell, frame, invoked };
    let call = MethodCall { code: method.code.clone(), storage, records, arguments: addresses, running };
    let callee = Callee { index: storage, by: By::Invoke, mark: Some(mark), pos };
    let (ending, returned) = callee::run(x, &callee, |x| x.run_method(call, pos).map(|r| (r.ending, r.value)))?;
    let unit = x.unit();
    let ending = ending.map_err(|mut abend| {
        if abend.file.is_none() {
            abend.file = Some(unit.oo.classes[class].sources.get(abend.pos.file as usize).cloned().unwrap_or_default());
        }
        abend
    });
    if ending? == Ending::StopRun {
        return Ok(Step::End(Ending::StopRun));
    }
    let mut returned_object = None;
    if let Some((item, Val::Address(value))) = &returned
        && method.returns.as_deref().is_some_and(is_reference)
    {
        let holder = format!("{item}, the RETURNING item of method \"{}\",", method.name);
        returned_object = Some(referent(&unit.oo, *value, what, &holder, pos)?);
    }
    let mut returned = returned.map(|(_, value)| value);
    let expired = unit.oo.event(format!("expired when {invoked_text}, returned"));
    unit.oo.pop_frames(depth, expired);
    if let Some(object) = returned_object {
        let value = match object {
            None => 0,
            Some(o) => local_reference(&mut unit.oo, o, format!("returned by {invoked_text}"), pos)?,
        };
        returned = Some(Val::Address(value));
    }
    unit.mem[RETURN_CODE..RETURN_CODE + 2].copy_from_slice(&return_code);
    if let (Some((place, _)), Some(val)) = (&plan.returning, returned) {
        let dest = x.locate(*place, false)?;
        x.assign(dest, val, None, pos)?;
    }
    Ok(succeeded(plan))
}

/// CALL through a FUNCTION-POINTER or PROCEDURE-POINTER: a JNI service from the function table,
/// run here when it needs no JVM. Each argument is `Value` or `Omitted`, OMITTED as NULL. The
/// reference services keep the JNI's rules for local and global references (see [`LOCAL_FRAMES`]).
pub fn call_through_pointer<'w, P: Copy, O, S, X: OoHost<'w, P, O, S>>(x: &mut X, pointer: P, args: &[CallArg<P, O>], returning: Option<P>, pos: Pos) -> R<()> {
    x.unit().unfollowed("object-oriented COBOL and calls through pointers");
    let name = x.place_name(pointer);
    let loc = x.locate(pointer, false)?;
    let Ok(value) = <[u8; 4]>::try_from(store::bytes(x.mem(), loc)).map(u32::from_be_bytes) else {
        return Err(Abend::ironwork(format!("CALL {name}: a reference-modified function-pointer"), pos));
    };
    let service = value.checked_sub(JNI_TAG).and_then(|slot| jni::function(slot as usize));
    let Some(service) = service else {
        return Err(Abend::ironwork(format!("CALL {name}: X'{value:08X}' is not a JNI service, the only entry a function-pointer can hold here"), pos));
    };
    let mut values = Vec::new();
    for arg in args {
        values.push(match arg {
            CallArg::Value(op) => x.value(op, pos)?,
            CallArg::Omitted => Val::Address(0),
            CallArg::Reference(_) | CallArg::Content(_) => return Err(Abend::ironwork(format!("CALL {service}: a JNI service takes its arguments by value"), pos)),
        });
    }
    let what = format!("CALL {service}");
    let holder = |x: &X, n: usize| match args.get(n) {
        Some(CallArg::Value(op)) => x.operand_name(op),
        _ => None,
    }
    .unwrap_or_else(|| format!("argument {n}"));
    let value = |n: usize| match values.get(n) {
        Some(Val::Address(a)) => Ok(*a),
        Some(Val::Fig(Figurative::Null)) => Ok(0),
        _ => Err(Abend::ironwork(format!("CALL {service}: argument {n} is not an object reference"), pos)),
    };
    let referent_of = |x: &mut X, n: usize, value: u32| {
        let holder = holder(x, n);
        referent(&x.unit().oo, value, &what, &holder, pos)
    };
    let here = site(x, pos);
    let result = match service {
        "NewGlobalRef" | "NewLocalRef" => {
            let object = referent_of(x, 1, value(1)?)?;
            let reference = match object {
                None => 0,
                Some(o) => {
                    let oo = &mut x.unit().oo;
                    let made = oo.event(format!("made by {service} at {here}"));
                    let made = if service == "NewGlobalRef" { oo.global(o, made) } else { oo.local(o, made) };
                    made.map_err(|m| Abend::ironwork(m, pos))?
                }
            };
            Some(Val::Address(reference))
        }
        "DeleteGlobalRef" | "DeleteLocalRef" => {
            let reference = value(1)?;
            if referent_of(x, 1, reference)?.is_some() {
                let global = service == "DeleteGlobalRef";
                if x.unit().oo.is_global(reference) != global {
                    let kind = if global { "local" } else { "global" };
                    return Err(Abend::ironwork(format!("{what}: {} holds a {kind} reference, which {service} does not delete (see {LOCAL_FRAMES})", holder(x, 1)), pos));
                }
                let oo = &mut x.unit().oo;
                let event = oo.event(format!("was deleted by {service} at {here}"));
                oo.free(reference, event);
            }
            None
        }
        "IsSameObject" => {
            let same = referent_of(x, 1, value(1)?)? == referent_of(x, 2, value(2)?)?;
            Some(Val::Bytes(vec![u8::from(same)]))
        }
        "GetObjectRefType" => {
            let reference = value(1)?;
            let kind = match referent_of(x, 1, reference)? {
                None => 0,
                Some(_) if x.unit().oo.is_global(reference) => 2,
                Some(_) => 1,
            };
            Some(int(kind))
        }
        "PushLocalFrame" => {
            x.unit().oo.push_frame(true);
            Some(int(0))
        }
        "PopLocalFrame" => {
            let Some(depth) = x.unit().oo.pushed_frame() else {
                return Err(Abend::ironwork(format!("{what}: no frame PushLocalFrame pushed is open here (see {LOCAL_FRAMES})"), pos));
            };
            let object = referent_of(x, 1, value(1)?)?;
            let oo = &mut x.unit().oo;
            let expired = oo.event(format!("expired when PopLocalFrame at {here} freed its frame"));
            oo.pop_frames(depth, expired);
            let reference = match object {
                None => 0,
                Some(o) => local_reference(oo, o, format!("returned by PopLocalFrame at {here}"), pos)?,
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
                message: format!("CALL {name} was reached: {service} is a JNI service, and ironwork for COBOL has no JVM to run it"),
                pos,
                file: None,
            });
        }
    };
    if let (Some(target), Some(val)) = (returning, result) {
        let dest = x.locate(target, false)?;
        x.assign(dest, val, None, pos)?;
    }
    Ok(())
}
