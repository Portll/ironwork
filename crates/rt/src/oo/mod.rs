//! The run unit's classes and objects, and the references a program holds to them. `C` is the
//! executor's handle to a loaded class definition, which the run unit keeps without looking inside.
//! `run` runs INVOKE, SELF and the JNI services, with `C` an `Rc<ClassCode<H>>`.

mod run;

pub use run::{MethodCall, OoHost, Returned, call_through_pointer, compare_references, invoke, jni_environment, self_reference};

use numeric::assumptions::REFERENCES_KEPT;
use std::collections::HashMap;

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
pub struct Objects<C> {
    pub classes: Vec<LoadedClass<C>>,
    pub objects: Vec<Instance>,
    pub jni: Option<usize>,
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
pub enum Referent {
    Null,
    Object(u32),
    /// A reference that was freed, and the message that says how.
    Expired(String),
    Unknown,
}

pub struct LoadedClass<C> {
    pub external: String,
    /// None for a Java class, java.lang.Object among them.
    pub code: Option<C>,
    pub parent: Option<usize>,
    /// The factory object's reference.
    pub factory_object: u32,
    /// Loaded storage of the factory data, and of each method's WORKING-STORAGE once it has run.
    pub factory_data: Option<usize>,
    pub methods: Vec<Option<usize>>,
    /// The class's source table, its own source first by path when a program library supplied it.
    pub sources: Vec<String>,
}

pub struct Instance {
    pub class: usize,
    /// A factory object, on which INVOKE runs factory methods.
    pub factory: bool,
    /// The loaded storage of each COBOL class's instance data, by class.
    pub parts: Vec<(usize, usize)>,
}

/// A COBOL class, compiled; `H` is the executor's handle to a compiled program.
pub struct ClassCode<H> {
    /// The external name of the class it inherits.
    pub parent: String,
    pub factory: Option<Part<H>>,
    pub object: Option<Part<H>>,
    pub methods: Vec<MethodCode<H>>,
}

/// FACTORY or OBJECT WORKING-STORAGE, laid out as a program's, and where each record starts in it.
pub struct Part<H> {
    pub data: H,
    pub records: Vec<u32>,
}

pub struct MethodCode<H> {
    pub name: String,
    pub factory: bool,
    /// Java types of the parameters and of the returned item, as a JNI signature spells them.
    pub params: Vec<String>,
    pub returns: Option<String>,
    pub code: H,
    /// LINKAGE records the method declares; the records of its paragraph's data follow them.
    pub own_records: usize,
}

/// The method an activation runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub method: Option<Running>,
}

#[derive(Clone, Copy, Debug)]
pub struct Running {
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

impl<C> Default for Objects<C> {
    fn default() -> Self {
        Self { classes: Vec::new(), objects: Vec::new(), jni: None, references: Vec::new(), frames: Vec::new(), serials: 0, events: Vec::new(), event_index: HashMap::new() }
    }
}

impl<C> Objects<C> {
    pub fn find(&self, external: &str) -> Option<usize> {
        self.classes.iter().position(|c| c.external == external)
    }

    /// An object by its position plus one.
    pub fn object(&self, id: u32) -> Option<&Instance> {
        (id as usize).checked_sub(1).and_then(|i| self.objects.get(i))
    }

    pub fn add_object(&mut self, object: Instance) -> Result<u32, String> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(format!("the run unit created more than {MAX_OBJECTS} objects, which ironwork for COBOL never frees"));
        }
        self.objects.push(object);
        Ok(self.objects.len() as u32)
    }

    /// An event's number, the same for the same words.
    pub fn event(&mut self, text: String) -> u32 {
        if let Some(&n) = self.event_index.get(&text) {
            return n;
        }
        let n = self.events.len() as u32;
        self.event_index.insert(text.clone(), n);
        self.events.push(text);
        n
    }

    pub fn told(&self, event: u32) -> &str {
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
    pub fn local(&mut self, object: u32, made: u32) -> Result<u32, String> {
        let frame = self.frames.last().map_or(0, |f| f.serial);
        self.make(object, frame, made)
    }

    /// A local reference in the frame with this serial.
    pub fn local_in(&mut self, frame: u32, object: u32, made: u32) -> Result<u32, String> {
        self.make(object, frame, made)
    }

    pub fn global(&mut self, object: u32, made: u32) -> Result<u32, String> {
        self.make(object, GLOBAL, made)
    }

    /// A new local frame, for a method's invocation or PushLocalFrame: its depth and serial.
    pub fn push_frame(&mut self, pushed: bool) -> (usize, u32) {
        self.serials = if self.serials >= GLOBAL - 1 { 1 } else { self.serials + 1 };
        self.frames.push(LocalFrame { serial: self.serials, first: self.references.len() as u32, pushed });
        (self.frames.len() - 1, self.serials)
    }

    /// Pops the frame at `depth` and every frame above it, freeing their local references.
    pub fn pop_frames(&mut self, depth: usize, event: u32) {
        let Some(first) = self.frames.get(depth).map(|f| f.first as usize) else { return };
        let serials: Vec<u32> = self.frames.drain(depth..).map(|f| f.serial).collect();
        for r in self.references.iter_mut().skip(first) {
            if r.freed == 0 && serials.contains(&r.frame) {
                r.freed = event + 1;
            }
        }
    }

    /// The depth of the innermost frame when PushLocalFrame pushed it.
    pub fn pushed_frame(&self) -> Option<usize> {
        self.frames.last().filter(|f| f.pushed).map(|_| self.frames.len() - 1)
    }

    pub fn referent(&self, value: u32) -> Referent {
        if value == 0 {
            return Referent::Null;
        }
        match self.references.get(value as usize - 1) {
            None => Referent::Unknown,
            Some(r) if r.freed == 0 => Referent::Object(r.object),
            Some(r) => Referent::Expired(format!("{}; it {}", self.described(r), self.told(r.freed - 1))),
        }
    }

    pub fn is_global(&self, value: u32) -> bool {
        (value as usize).checked_sub(1).and_then(|i| self.references.get(i)).is_some_and(|r| r.frame == GLOBAL)
    }

    /// Frees a valid reference, as DeleteLocalRef or DeleteGlobalRef does.
    pub fn free(&mut self, value: u32, event: u32) {
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
