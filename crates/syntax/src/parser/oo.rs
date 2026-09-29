//! The object-oriented syntax: class definitions with their FACTORY and OBJECT paragraphs and
//! methods, the REPOSITORY paragraph, USAGE OBJECT REFERENCE and INVOKE.

use super::*;

/// A program's `oo`, when its REPOSITORY names classes.
pub(super) fn program_oo(repository: Vec<ClassEntry>) -> Option<Box<Oo>> {
    (!repository.is_empty()).then(|| Box::new(Oo { repository, unit: OoUnit::Program }))
}

/// Contained programs work with their outermost program's REPOSITORY and may not have their own.
pub(super) fn share_repository(repository: &[ClassEntry], nested: &mut [Program]) -> R<()> {
    for p in nested {
        match &p.oo {
            Some(o) if !o.repository.is_empty() && o.repository.as_slice() != repository => {
                return Err(Error::at(o.repository[0].pos, "a REPOSITORY paragraph belongs to the outermost program only"));
            }
            _ if !repository.is_empty() => p.oo = program_oo(repository.to_vec()),
            _ => {}
        }
    }
    Ok(())
}

/// A method name as Java forms one.
fn java_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

impl Parser<'_> {
    fn at_end_of(&self, what: &str) -> bool {
        self.is_word("END") && self.word_at(1) == Some(what)
    }

    /// `IDENTIFICATION DIVISION. FACTORY.`, `... OBJECT.` or `... METHOD-ID.` ahead.
    fn at_header(&self, what: &str) -> bool {
        self.at_division(&["IDENTIFICATION", "ID"]) && self.peek_at(2) == Some(&Tok::Period) && self.word_at(3) == Some(what)
    }

    /// A class definition, from CLASS-ID to END CLASS, alone in its source as IBM requires.
    pub(super) fn class_definition(&mut self, options: &[String], out: &mut Vec<Program>) -> R<()> {
        let pos = self.pos();
        if self.at != 3 || !out.is_empty() {
            return Err(Error::at(pos, "a class definition must be alone in its source file"));
        }
        self.at += 1;
        self.accept(&Tok::Period);
        let name = self.name("a class name")?;
        if !self.accept_word("INHERITS") {
            return Err(self.error("INHERITS and the parent class: every class derives from java.lang.Object"));
        }
        if self.is_word("FROM") {
            return Err(self.error("the parent class after INHERITS (INHERITS FROM is not Enterprise COBOL)"));
        }
        let inherits = self.name("the parent class")?;
        self.expect(&Tok::Period, "a period after the CLASS-ID paragraph")?;
        while self.peek().is_some() && !self.at_division(&["ENVIRONMENT", "IDENTIFICATION", "ID", "DATA", "PROCEDURE"]) && !self.at_end_of("CLASS") {
            self.at += 1;
        }
        let repository = if self.at_division(&["ENVIRONMENT"]) { self.class_environment()? } else { Vec::new() };
        if self.at_division(&["DATA", "PROCEDURE"]) {
            return Err(self.error("a FACTORY or OBJECT paragraph: a class's data and methods belong to them"));
        }
        let mut def = ClassDef { name: name.clone(), inherits, factory: None, object: None, pos };
        if self.at_header("FACTORY") {
            def.factory = Some(self.class_part("FACTORY", &name, &repository, options)?);
        }
        if self.at_header("OBJECT") {
            def.object = Some(self.class_part("OBJECT", &name, &repository, options)?);
        }
        if !self.at_end_of("CLASS") {
            return Err(self.error("END CLASS, after the FACTORY paragraph and then the OBJECT paragraph"));
        }
        self.at += 2;
        let end = self.name("the class name after END CLASS")?;
        if end != name {
            return Err(Error::at(self.tokens[self.at - 1].pos, format!("END CLASS {end} ends class {name}")));
        }
        self.accept(&Tok::Period);
        if self.peek().is_some() {
            return Err(self.error("the end of the source: a class definition must be alone in its source file"));
        }
        out.push(Program {
            id: name,
            options: options.to_vec(),
            initial: false,
            recursive: false,
            working_storage: Vec::new(),
            local_storage: Vec::new(),
            linkage: Vec::new(),
            using: Vec::new(),
            returning: None,
            paragraphs: Vec::new(),
            files: Vec::new(),
            sources: Vec::new(),
            exec_declarations: Vec::new(),
            report_writer: Default::default(),
            oo: Some(Box::new(Oo { repository, unit: OoUnit::Class(Box::new(def)) })),
            environment: Environment::default(),
            nested: Vec::new(),
        });
        Ok(())
    }

    /// A class's ENVIRONMENT DIVISION: a CONFIGURATION SECTION only, whose REPOSITORY is kept.
    fn class_environment(&mut self) -> R<Vec<ClassEntry>> {
        self.at += 2;
        self.expect(&Tok::Period, "a period")?;
        let mut repository = Vec::new();
        while self.peek().is_some() && !self.at_division(&["IDENTIFICATION", "ID", "DATA", "PROCEDURE"]) && !self.at_end_of("CLASS") {
            if self.section_header() {
                match self.word() {
                    Some("CONFIGURATION") => {}
                    Some("INPUT-OUTPUT") => return Err(self.error("the CONFIGURATION SECTION: a class has no INPUT-OUTPUT SECTION; a method declares its own files")),
                    _ => return Err(self.error("the CONFIGURATION SECTION: a class's ENVIRONMENT DIVISION has no other")),
                }
            }
            if self.is_word("CLASS-CONTROL") {
                return Err(self.error("the REPOSITORY paragraph (CLASS-CONTROL is not Enterprise COBOL)"));
            }
            if self.accept_word("REPOSITORY") {
                repository = self.repository()?;
                continue;
            }
            self.at += 1;
        }
        Ok(repository)
    }

    /// The REPOSITORY paragraph's CLASS entries; FUNCTION entries are passed over.
    pub(super) fn repository(&mut self) -> R<Vec<ClassEntry>> {
        self.accept(&Tok::Period);
        let mut entries: Vec<ClassEntry> = Vec::new();
        loop {
            if self.accept(&Tok::Period) || self.peek().is_none() || self.section_header() || self.at_division(&["DATA", "PROCEDURE", "IDENTIFICATION", "ID"]) {
                return Ok(entries);
            }
            if self.accept_word("CLASS") {
                let pos = self.pos();
                let name = self.name("a class name")?;
                self.accept_word("IS");
                let external = match self.peek() {
                    Some(Tok::Alnum(s)) => {
                        let s = s.clone();
                        self.at += 1;
                        if s.is_empty() || s.contains(' ') {
                            return Err(Error::at(pos, format!("CLASS {name} IS \"{s}\": not a Java class name")));
                        }
                        s
                    }
                    Some(Tok::Word(w)) if w == "AS" => return Err(self.error("IS and the external class name (AS is not Enterprise COBOL)")),
                    _ => external_class_name(&name),
                };
                if entries.iter().any(|e| e.name == name) {
                    return Err(Error::at(pos, format!("class {name} is named twice in the REPOSITORY paragraph")));
                }
                entries.push(ClassEntry { name, external, pos });
                continue;
            }
            if self.accept_word("FUNCTION") {
                while self.peek().is_some()
                    && self.peek() != Some(&Tok::Period)
                    && !self.is_word("CLASS")
                    && !self.is_word("FUNCTION")
                    && !self.section_header()
                    && !self.at_division(&["DATA", "PROCEDURE", "IDENTIFICATION", "ID"])
                {
                    self.at += 1;
                }
                continue;
            }
            if self.paragraph_header() {
                return Ok(entries);
            }
            return Err(self.error("CLASS or FUNCTION in the REPOSITORY paragraph"));
        }
    }

    /// A FACTORY or OBJECT paragraph: WORKING-STORAGE, then method definitions, then END FACTORY
    /// or END OBJECT.
    fn class_part(&mut self, kind: &str, class: &str, repository: &[ClassEntry], options: &[String]) -> R<ClassPart> {
        let pos = self.pos();
        self.at += 4;
        self.expect(&Tok::Period, "a period")?;
        let mut part = ClassPart { pos, ..ClassPart::default() };
        if self.at_division(&["DATA"]) {
            self.at += 2;
            self.expect(&Tok::Period, "a period")?;
            while self.peek().is_some() && !self.at_division(&["PROCEDURE", "IDENTIFICATION", "ID"]) && !self.at_end_of(kind) {
                let section = self.name("WORKING-STORAGE SECTION")?;
                if section != "WORKING-STORAGE" {
                    return Err(Error::at(self.tokens[self.at - 1].pos, format!("{section}: the DATA DIVISION of a {kind} paragraph has only a WORKING-STORAGE SECTION")));
                }
                self.expect_word("SECTION")?;
                self.expect(&Tok::Period, "a period")?;
                part.working_storage.extend(self.data_entries()?);
                if let Some(block) = self.exec_declarations.first() {
                    return Err(Error::at(block.pos, "a class definition cannot contain EXEC statements"));
                }
            }
        }
        if self.at_division(&["PROCEDURE"]) {
            self.at += 2;
            self.expect(&Tok::Period, "a period: a FACTORY or OBJECT PROCEDURE DIVISION holds only methods")?;
            while self.at_header("METHOD-ID") {
                part.methods.push(self.method(class, kind == "FACTORY", repository, options)?);
            }
        }
        if !self.at_end_of(kind) {
            return Err(self.error(format!("a method definition or END {kind}")));
        }
        self.at += 2;
        self.accept(&Tok::Period);
        Ok(part)
    }

    /// One method: IDENTIFICATION DIVISION. METHOD-ID. "name". through END METHOD "name".
    fn method(&mut self, class: &str, factory: bool, repository: &[ClassEntry], options: &[String]) -> R<Program> {
        let pos = self.pos();
        self.at += 4;
        self.accept(&Tok::Period);
        let name = match self.peek().cloned() {
            Some(Tok::Alnum(s) | Tok::National(s)) => s,
            Some(Tok::Word(w)) => return Err(self.error(format!("the method name as a literal, METHOD-ID. \"{w}\" (Enterprise COBOL names methods with literals)"))),
            _ => return Err(self.error("the method name as a literal")),
        };
        if !java_identifier(&name) {
            return Err(self.error(format!("a method name that is a Java identifier, not \"{name}\"")));
        }
        let name_pos = self.pos();
        self.at += 1;
        self.expect(&Tok::Period, "a period after the method name")?;
        let outer = (std::mem::take(&mut self.exec_declarations), std::mem::take(&mut self.cics), std::mem::take(&mut self.sql.blocks));
        let mut out = Vec::new();
        let parsed = self.program_body(name.clone(), false, false, options, &mut out, true);
        (self.exec_declarations, self.cics, self.sql.blocks) = outer;
        parsed?;
        if out.len() != 1 {
            return Err(Error::at(pos, format!("method \"{name}\" contains a program: a method cannot contain nested programs")));
        }
        if !self.at_end_of("METHOD") {
            return Err(self.error(format!("END METHOD \"{name}\"")));
        }
        self.at += 2;
        match self.peek() {
            Some(Tok::Alnum(s) | Tok::National(s)) if *s == name => self.at += 1,
            _ => return Err(self.error(format!("\"{name}\" after END METHOD"))),
        }
        self.accept(&Tok::Period);
        let mut method = out.remove(0);
        if method.oo.is_some() {
            return Err(Error::at(pos, format!("method \"{name}\" has a REPOSITORY paragraph: the class's applies to its methods")));
        }
        method.oo = Some(Box::new(Oo { repository: repository.to_vec(), unit: OoUnit::Method(MethodOf { class: class.to_owned(), factory, name, pos: name_pos }) }));
        Ok(method)
    }

    /// OBJECT REFERENCE [class-name], after USAGE OBJECT or OBJECT.
    pub(super) fn object_reference(&mut self, e: &mut DataEntry) -> R<()> {
        self.expect_word("REFERENCE")?;
        e.usage = Some(Usage::ObjectReference);
        if let Some(w) = self.word()
            && !is_clause_word(w)
        {
            e.object_class = Some(w.to_owned());
            self.at += 1;
        }
        Ok(())
    }

    pub(super) fn invoke(&mut self, pos: Pos) -> R<Invoke> {
        let target = self.reference()?;
        let method = if self.accept_word("NEW") {
            InvokeMethod::New
        } else {
            match self.peek().cloned() {
                Some(Tok::Alnum(s) | Tok::National(s)) => {
                    self.at += 1;
                    InvokeMethod::Named(s)
                }
                _ if self.starts_ref() => InvokeMethod::Identifier(self.reference()?),
                _ => return Err(self.error("NEW, or the method's name as a literal or in a data item")),
            }
        };
        let mut using = Vec::new();
        if self.accept_word("USING") {
            loop {
                let by = self.accept_word("BY");
                match self.accept_any(&["VALUE", "REFERENCE", "CONTENT"]).as_deref() {
                    Some("VALUE") => {}
                    Some(other) => return Err(Error::at(self.tokens[self.at - 1].pos, format!("INVOKE passes its arguments BY VALUE, not BY {other}"))),
                    None if by => return Err(self.error("VALUE after BY")),
                    None => break,
                }
                let before = using.len();
                while self.starts_operand() {
                    using.push(self.operand()?);
                }
                if using.len() == before {
                    return Err(self.error("an argument after BY VALUE"));
                }
            }
            if using.is_empty() {
                return Err(Error::at(self.pos(), "INVOKE passes its arguments BY VALUE: write USING BY VALUE"));
            }
        }
        let returning = if self.accept_word("RETURNING") { Some(self.reference()?) } else { None };
        let (mut on_exception, mut not_on_exception) = (None, None);
        loop {
            let negated = self.is_word("NOT") && matches!(self.word_at(1), Some("ON" | "EXCEPTION"));
            if !negated && !(self.is_word("ON") || self.is_word("EXCEPTION")) {
                break;
            }
            if negated {
                self.at += 1;
            }
            self.accept_word("ON");
            self.expect_word("EXCEPTION")?;
            let body = self.block(&["NOT", "END-INVOKE"])?;
            if negated { not_on_exception = Some(body) } else { on_exception = Some(body) }
        }
        self.accept_word("END-INVOKE");
        Ok(Invoke { target, method, using, returning, on_exception, not_on_exception, pos })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::*;

    const CLASS: &str = "       CBL THREAD,DLL
       IDENTIFICATION DIVISION.
       CLASS-ID. Account INHERITS Base.
       ENVIRONMENT DIVISION.
       CONFIGURATION SECTION.
       REPOSITORY.
           CLASS Base IS \"java.lang.Object\"
           CLASS Account IS \"com.acme.Account\".
       IDENTIFICATION DIVISION.
       FACTORY.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  N PIC S9(9) BINARY VALUE 0.
       PROCEDURE DIVISION.
       IDENTIFICATION DIVISION.
       METHOD-ID. \"count\".
       DATA DIVISION.
       LINKAGE SECTION.
       01  R PIC S9(9) BINARY.
       PROCEDURE DIVISION RETURNING R.
           MOVE N TO R.
       END METHOD \"count\".
       END FACTORY.
       IDENTIFICATION DIVISION.
       OBJECT.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  BALANCE PIC S9(9) VALUE 0.
       01  OTHER-ACCOUNT USAGE OBJECT REFERENCE Account.
       PROCEDURE DIVISION.
       IDENTIFICATION DIVISION.
       METHOD-ID. \"credit\".
       DATA DIVISION.
       LINKAGE SECTION.
       01  AMOUNT PIC S9(9) BINARY.
       PROCEDURE DIVISION USING BY VALUE AMOUNT.
           ADD AMOUNT TO BALANCE
           INVOKE SELF \"audit\" USING BY VALUE AMOUNT 1 LENGTH OF AMOUNT
               ON EXCEPTION CONTINUE
           END-INVOKE
           EXIT METHOD.
       END METHOD \"credit\".
       END OBJECT.
       END CLASS Account.
";

    #[test]
    fn a_class_with_factory_and_object_paragraphs() {
        let p = crate::parse(CLASS).unwrap_or_else(|e| panic!("{e}"));
        let oo = p.oo.as_ref().unwrap();
        assert_eq!(oo.external("BASE"), Some("java.lang.Object"));
        assert_eq!(oo.external("ACCOUNT"), Some("com.acme.Account"));
        let class = oo.class().unwrap();
        assert_eq!((class.name.as_str(), class.inherits.as_str()), ("ACCOUNT", "BASE"));
        let factory = class.factory.as_ref().unwrap();
        assert_eq!(factory.working_storage.len(), 1);
        assert_eq!(factory.methods[0].id, "count");
        assert!(factory.methods[0].oo.as_ref().unwrap().method().unwrap().factory);
        let object = class.object.as_ref().unwrap();
        assert_eq!(object.working_storage[1].usage, Some(Usage::ObjectReference));
        assert_eq!(object.working_storage[1].object_class.as_deref(), Some("ACCOUNT"));
        let credit = &object.methods[0];
        assert!(credit.using[0].by_value);
        let Stmt::Invoke(i) = &credit.paragraphs[0].statements[1] else { panic!("{:?}", credit.paragraphs[0].statements) };
        assert_eq!(i.target.name, "SELF");
        assert_eq!(i.method, InvokeMethod::Named("audit".into()));
        assert_eq!(i.using.len(), 3);
        assert!(i.on_exception.is_some());
        assert!(matches!(credit.paragraphs[0].statements[2], Stmt::ExitMethod { .. }));
    }

    #[test]
    fn repository_names_and_invoke_forms() {
        let text = "       IDENTIFICATION DIVISION.
       PROGRAM-ID. CLIENT RECURSIVE.
       ENVIRONMENT DIVISION.
       CONFIGURATION SECTION.
       REPOSITORY.
           FUNCTION ALL INTRINSIC
           CLASS Account IS \"com.acme.Account\"
           CLASS Orders.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  A OBJECT REFERENCE Account.
       01  U USAGE IS OBJECT REFERENCE.
       01  M PIC X(10).
       PROCEDURE DIVISION.
           INVOKE Account NEW RETURNING A
           INVOKE U M
           INVOKE A \"credit\" USING BY VALUE 5 RETURNING M
               NOT ON EXCEPTION DISPLAY 'OK'
           END-INVOKE
           SET U TO NULL
           GOBACK.
";
        let p = crate::parse(text).unwrap_or_else(|e| panic!("{e}"));
        let oo = p.oo.as_ref().unwrap();
        assert_eq!(oo.external("ACCOUNT"), Some("com.acme.Account"));
        assert_eq!(oo.external("ORDERS"), Some("ORDERS"));
        assert_eq!(p.working_storage[1].object_class, None);
        let s = &p.paragraphs[0].statements;
        assert!(matches!(&s[0], Stmt::Invoke(i) if i.method == InvokeMethod::New && i.returning.is_some()));
        assert!(matches!(&s[1], Stmt::Invoke(i) if matches!(i.method, InvokeMethod::Identifier(_))));
        assert!(matches!(&s[2], Stmt::Invoke(i) if i.not_on_exception.is_some()));
    }

    #[test]
    fn what_enterprise_cobol_does_not_accept_is_refused_by_name() {
        let refused = |text: &str| crate::parse(text).unwrap_err().message;
        let head = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n";
        assert!(refused(&format!("{head}           INVOKE A \"m\" USING B.\n")).contains("BY VALUE"));
        assert!(refused(&format!("{head}           INVOKE A \"m\" USING BY REFERENCE B.\n")).contains("not BY REFERENCE"));
        let class = |body: &str| format!("       IDENTIFICATION DIVISION.\n       CLASS-ID. C INHERITS B.\n{body}       END CLASS C.\n");
        assert!(refused(&class("       IDENTIFICATION DIVISION.\n       OBJECT.\n       PROCEDURE DIVISION.\n       IDENTIFICATION DIVISION.\n       METHOD-ID. m.\n       END METHOD m.\n       END OBJECT.\n")).contains("literal"));
        assert!(refused(&class("       ENVIRONMENT DIVISION.\n       OBJECT SECTION.\n       CLASS-CONTROL.\n")).contains("CONFIGURATION"));
        assert!(refused("       IDENTIFICATION DIVISION.\n       CLASS-ID. C INHERITS FROM B.\n").contains("INHERITS FROM"));
        assert!(refused(&format!("{}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. X.\n", class(""))).contains("alone"));
        assert!(refused(&format!("{head}           GOBACK.\n       END PROGRAM P.\n{}", class(""))).contains("alone"));
        let method = "       IDENTIFICATION DIVISION.\n       OBJECT.\n       PROCEDURE DIVISION.\n       IDENTIFICATION DIVISION.\n       METHOD-ID. \"m\".\n       PROCEDURE DIVISION.\n           CONTINUE.\n       END METHOD \"n\".\n       END OBJECT.\n";
        assert!(refused(&class(method)).contains("END METHOD"));
    }
}
