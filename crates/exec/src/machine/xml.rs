//! XML PARSE: each event the scanner reports is put in the XML special registers and the processing
//! procedure is performed for it (Language Reference SC27-8713-03, pp. 489-494).

use super::*;
use rt::xml::{Encoding, Event, EventKind, Scanner, Step, UTF8};

mod generate;

/// Where XML-TEXT and the other registers whose length varies hold the current event's fragments:
/// an offset in run-unit storage and a length, empty outside a processing procedure.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Registers {
    text: (usize, usize),
    ntext: (usize, usize),
    namespace: (usize, usize),
    nnamespace: (usize, usize),
    prefix: (usize, usize),
    nprefix: (usize, usize),
}

fn special(name: &str) -> Ref {
    Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: Pos::default() }
}

fn national(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

impl<'p> Machine<'p, '_, '_> {
    /// XML-TEXT, XML-NTEXT and the namespace registers, with any reference modification.
    pub(super) fn xml_register(&mut self, r: &Ref) -> R<Option<Loc>> {
        if !compile::markup::xml_register(self.layout, r) {
            return Ok(None);
        }
        let x = self.xml;
        let ((offset, len), kind) = match r.name.as_str() {
            "XML-TEXT" => (x.text, Kind::Alnum { justified: false }),
            "XML-NTEXT" => (x.ntext, Kind::National),
            "XML-NAMESPACE" => (x.namespace, Kind::Alnum { justified: false }),
            "XML-NNAMESPACE" => (x.nnamespace, Kind::National),
            "XML-NAMESPACE-PREFIX" => (x.prefix, Kind::Alnum { justified: false }),
            _ => (x.nprefix, Kind::National),
        };
        let mut loc = Loc { offset, len, kind, item: usize::MAX };
        if let Some(rm) = &r.refmod {
            let unit = if kind == Kind::National { 2 } else { 1 };
            let start = self.integer(&rm.start, r.pos)?;
            let length = match &rm.length {
                Some(l) => self.integer(l, r.pos)?,
                None => (len / unit) as i64 - start + 1,
            };
            if start < 1 || length < 0 || (start - 1 + length) as usize * unit > len {
                return Err(Abend::ironwork(format!("reference modification ({start}:{length}) of {} is outside its {len} bytes", r.name), r.pos));
            }
            loc = Loc { offset: offset + (start as usize - 1) * unit, len: length as usize * unit, kind, item: usize::MAX };
        }
        Ok(Some(loc))
    }

    fn xml_code(&mut self, pos: Pos) -> R<i64> {
        let r = special("XML-CODE");
        self.integer(&Expr::Operand(Operand::Ref(r)), pos)
    }

    fn fragment(&mut self, bytes: &[u8]) -> (usize, usize) {
        if bytes.is_empty() {
            return (0, 0);
        }
        (self.unit.push_temporary(bytes), bytes.len())
    }

    /// Sets the registers for one event and performs the processing procedure; returns its flow.
    fn xml_event(&mut self, e: &Event, code: i64, encoding: Encoding, national_out: bool, range: (usize, usize), pos: Pos) -> R<Flow> {
        let name = self.page.encode_lossy(&format!("{:<30}", e.kind.name()));
        let event = special("XML-EVENT");
        let loc = self.locate(&event)?;
        self.write(loc, &name[..30]);
        self.set_integer(&special("XML-CODE"), code, pos)?;
        self.set_integer(&special("XML-INFORMATION"), e.information.into(), pos)?;
        let national_character = matches!(e.kind, EventKind::ContentNationalCharacter | EventKind::AttributeNationalCharacter);
        let mut registers = Registers::default();
        if national_out || national_character {
            registers.ntext = self.fragment(&national(&e.text));
            registers.nnamespace = self.fragment(&national(&e.namespace));
            registers.nprefix = self.fragment(&national(&e.prefix));
        } else {
            registers.text = self.fragment(&encoding.encode(&e.text));
            registers.namespace = self.fragment(&encoding.encode(&e.namespace));
            registers.prefix = self.fragment(&encoding.encode(&e.prefix));
        }
        self.xml = registers;
        self.perform_range(range.0, range.1, None, None)
    }

    fn xml_document(&mut self, x: &XmlParse, encoding: Encoding, carry: &mut Vec<u8>) -> R<String> {
        let loc = self.locate(&x.document)?;
        Ok(encoding.decode_segment(carry, self.bytes(loc)))
    }

    pub(super) fn xml_parse(&mut self, x: &'p XmlParse) -> R<Flow> {
        self.unit.unfollowed("XML PARSE");
        let (start, first_end) = self.procedure(&x.procedure, x.pos)?;
        let end = match &x.thru {
            Some(t) => self.procedure(t, x.pos)?.1,
            None => first_end,
        };
        let document = self.locate(&x.document)?;
        let ccsid = match &x.encoding {
            Some(op) => match self.operand(op, x.pos)? {
                Val::Num(n) => n.to_i128().and_then(|c| u16::try_from(c).ok()),
                _ => None,
            },
            None => None,
        };
        let encoding = match (document.kind, ccsid) {
            (Kind::National, _) => Encoding::National,
            (_, Some(UTF8)) => Encoding::Utf8,
            (_, Some(c)) => Encoding::Page(CodePage::by_ccsid(c).ok_or_else(|| Abend::ironwork(format!("XML PARSE: CCSID {c} is not a code page ironwork for COBOL carries"), x.pos))?),
            (_, None) => Encoding::Page(self.page),
        };
        let national_out = matches!(encoding, Encoding::National) || x.returning_national;
        let representable: Box<dyn Fn(char) -> bool> = match encoding {
            Encoding::Page(page) if !national_out => Box::new(move |c| page.encode_char(c).is_some()),
            _ => Box::new(|_| true),
        };
        let mut carry = Vec::new();
        let text = self.xml_document(x, encoding, &mut carry)?;
        let mut seen = text.clone();
        // An EXCEPTION's XML-TEXT is the current segment up to the error (Language Reference
        // SC27-8713-03, p. 33, note 4).
        let mut segment_start = 0;
        let mut scanner = Scanner::new(&text, &*representable);
        let mark = self.unit.mem.len();
        let code = loop {
            // An EXCEPTION the scanner reports as an event is a warning the parse can go on from.
            let (event, exception, warning) = match scanner.advance() {
                Step::Event(e) if e.kind == EventKind::Exception => {
                    let code = i64::from(e.code);
                    (e, Some(code), true)
                }
                Step::Event(e) => (e, None, false),
                Step::EndOfInput => (Event { kind: EventKind::EndOfInput, text: String::new(), namespace: String::new(), prefix: String::new(), information: 0, code: 0 }, None, false),
                Step::Error(m) => {
                    let upto: String = seen.chars().take(m.offset).skip(segment_start).collect();
                    let code = m.why.code();
                    (Event { kind: EventKind::Exception, text: upto, namespace: String::new(), prefix: String::new(), information: 0, code }, Some(i64::from(code)), false)
                }
                Step::Done => break 0,
            };
            let flow = self.xml_event(&event, exception.unwrap_or(0), encoding, national_out, (start, end), x.pos);
            self.xml = Registers::default();
            self.unit.release_temporaries(mark);
            match flow? {
                Flow::Next => {}
                other => return Ok(other),
            }
            // What the procedure left in XML-CODE decides what follows (Programming Guide
            // SC27-8714-03, p. 630, Table 75).
            let code = self.xml_code(x.pos)?;
            match event.kind {
                EventKind::Exception if warning && code == 0 => {}
                EventKind::Exception => break exception.unwrap_or(code),
                _ if code == -1 => break -1,
                EventKind::EndOfInput if code == 1 => {
                    let segment = self.xml_document(x, encoding, &mut carry)?;
                    segment_start = seen.chars().count();
                    seen.push_str(&segment);
                    scanner.feed(&segment);
                }
                EventKind::EndOfInput if code == 0 => {
                    if !carry.is_empty() {
                        let rest = encoding.decode(&std::mem::take(&mut carry));
                        seen.push_str(&rest);
                        scanner.feed(&rest);
                    }
                    scanner.finish();
                }
                EventKind::EndOfDocument if code == 0 => break 0,
                _ if code == 0 => {}
                kind => {
                    let message = format!("IGZ0230S XML PARSE: the processing procedure set XML-CODE to {code} at event {}", kind.name());
                    return Err(Abend { code: AbendCode::user(4038), message, pos: x.pos, file: None });
                }
            }
        };
        self.set_integer(&special("XML-CODE"), code, x.pos)?;
        let handler = if code == 0 { &x.not_on_exception } else { &x.on_exception };
        match handler {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }
}
