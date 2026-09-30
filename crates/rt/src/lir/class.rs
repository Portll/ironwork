//! A COBOL class definition (lir.md §9.8): its FACTORY and OBJECT data and its methods, each lowered
//! as a program of its own.

use super::{Program, SymId};
use crate::codec_struct;
use crate::module::ModuleError;
use crate::module::codec::{Decode, Encode, Reader, Writer};
use std::cell::Cell;

/// Names are the defining program's symbols. `parent` is the external name of the class it
/// inherits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    pub external: SymId,
    pub parent: SymId,
    pub factory: Option<ClassPart>,
    pub object: Option<ClassPart>,
    pub methods: Vec<Method>,
}

/// FACTORY or OBJECT data: a program whose storage is the data as its VALUE clauses leave it, and
/// the offset of each 01 or 77 record in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassPart {
    pub data: Program,
    pub records: Vec<u32>,
}

/// `params` and `returns` are Java type signatures. The method's LINKAGE records after its first
/// `own_records` are its part's records, bound at each invocation to the object's or the factory's
/// data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Method {
    pub name: SymId,
    pub factory: bool,
    pub params: Vec<SymId>,
    pub returns: Option<SymId>,
    pub own_records: u16,
    pub code: Program,
}

codec_struct!(ClassPart { data, records });
codec_struct!(Method { name, factory, params, returns, own_records, code });

thread_local! {
    static IN_CLASS: Cell<bool> = const { Cell::new(false) };
}

impl Encode for Class {
    fn encode(&self, w: &mut Writer) {
        let Class { external, parent, factory, object, methods } = self;
        external.encode(w);
        parent.encode(w);
        factory.encode(w);
        object.encode(w);
        methods.encode(w);
    }
}

/// A method or part is a program and never a class, so a class inside one is refused before it is
/// read, which keeps decoding from nesting deeper than one class.
impl Decode for Class {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        if IN_CLASS.get() {
            return Err(r.malformed(r.position(), "a class definition inside a class definition"));
        }
        IN_CLASS.set(true);
        let class = (|| {
            Ok(Class {
                external: Decode::decode(r)?,
                parent: Decode::decode(r)?,
                factory: Decode::decode(r)?,
                object: Decode::decode(r)?,
                methods: Decode::decode(r)?,
            })
        })();
        IN_CLASS.set(false);
        class
    }
}
