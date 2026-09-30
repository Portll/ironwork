//! The run unit's objects and classes, and where a class definition is found. Compiling and
//! checking a class is `compile::oo`; running one is machine/oo.rs.

pub use compile::oo::*;

use numeric::assumptions::REFERENCES_KEPT;
use std::collections::HashMap;
use std::rc::Rc;
use syntax::ast::Program;

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

pub(crate) fn refuse_to_run(program: &Program) -> Result<(), crate::Abend> {
    match program.oo.as_ref().and_then(|o| o.class()) {
        Some(c) => Err(crate::Abend { code: crate::abend::AbendCode::Ironwork, message: format!("{} is a class definition: run a program that uses it", c.name), pos: c.pos }),
        None => Ok(()),
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
                    let mut programs = syntax::parse_all_with(&text, &library.copy.with_program(&path)).map_err(|e| format!("class {external} does not compile: {}", e.place(&path.display().to_string())))?;
                    if defined_class(&programs[0]).as_deref() == Some(external) {
                        return Ok(Some(programs.remove(0)));
                    }
                }
            }
        }
    }
    Ok(None)
}
