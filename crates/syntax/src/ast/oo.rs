//! Object-oriented COBOL as Enterprise COBOL has it, for Java interoperability: class definitions
//! with factory and object paragraphs, methods, the REPOSITORY paragraph and INVOKE.

use super::{DataEntry, Operand, Program, Ref, Stmt};
use crate::Pos;

/// What a program, class or method knows of classes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Oo {
    /// REPOSITORY CLASS entries: the outermost program's for it and its contained programs, the
    /// class's for the class and each of its methods.
    pub repository: Vec<ClassEntry>,
    pub unit: OoUnit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OoUnit {
    #[default]
    Program,
    Class(Box<ClassDef>),
    Method(MethodOf),
}

/// `CLASS name [IS "external"]` in a REPOSITORY paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassEntry {
    pub name: String,
    /// As written, case kept; or the name made into one as IBM makes it.
    pub external: String,
    pub pos: Pos,
}

/// The external class-name IBM forms from a class-name with no literal: uppercase, hyphens as
/// zeros, a leading digit 1-9 as A-I and 0 as J.
pub fn external_class_name(name: &str) -> String {
    name.to_ascii_uppercase()
        .chars()
        .enumerate()
        .map(|(i, c)| match c {
            '-' => '0',
            '0' if i == 0 => 'J',
            '1'..='9' if i == 0 => (b'A' + (c as u8 - b'1')) as char,
            c => c,
        })
        .collect()
}

impl Oo {
    /// The external name of a class-name in the REPOSITORY.
    pub fn external(&self, name: &str) -> Option<&str> {
        self.repository.iter().find(|e| e.name == name).map(|e| e.external.as_str())
    }

    pub fn class(&self) -> Option<&ClassDef> {
        match &self.unit {
            OoUnit::Class(c) => Some(c),
            _ => None,
        }
    }

    pub fn method(&self) -> Option<&MethodOf> {
        match &self.unit {
            OoUnit::Method(m) => Some(m),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub name: String,
    /// The class-name after INHERITS, which the REPOSITORY must name.
    pub inherits: String,
    pub factory: Option<ClassPart>,
    pub object: Option<ClassPart>,
    pub pos: Pos,
}

/// A FACTORY or OBJECT paragraph: its data, which the methods share, and its methods.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassPart {
    pub working_storage: Vec<DataEntry>,
    pub methods: Vec<Program>,
    pub pos: Pos,
}

/// A method: the class that defines it, whether it is a factory method, and its name as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodOf {
    pub class: String,
    pub factory: bool,
    pub name: String,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvokeMethod {
    New,
    /// A literal method name, case kept.
    Named(String),
    /// A data item holding the method name.
    Identifier(Ref),
}

/// INVOKE: the object, class-name, SELF or SUPER in `target`, then what to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invoke {
    pub target: Ref,
    pub method: InvokeMethod,
    /// The BY VALUE arguments: identifiers, literals and LENGTH OF.
    pub using: Vec<Operand>,
    pub returning: Option<Ref>,
    pub on_exception: Option<Vec<Stmt>>,
    pub not_on_exception: Option<Vec<Stmt>>,
    pub pos: Pos,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_names_as_ibm_forms_them() {
        assert_eq!(external_class_name("Account"), "ACCOUNT");
        assert_eq!(external_class_name("my-class_1"), "MY0CLASS_1");
        assert_eq!(external_class_name("1st-class"), "AST0CLASS");
        assert_eq!(external_class_name("0ne"), "JNE");
    }
}
