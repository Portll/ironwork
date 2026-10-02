use super::*;
use rt::lir::{Ccsid, Convert, Count, Flag, JsonValue, Marker, Markup, Named, NumberInto, ParseValue, RangeKind, SetTo, XmlForm, XmlRegister, XmlValue};
use rt::storage::Kind;
use rt::vocab::Figurative;

fn markup(p: &Program) -> &[Markup] {
    &p.services.markup
}

#[test]
fn json_generate_lays_out_the_items_its_walk_reaches_and_the_phrases_decided_for_each() {
    let data = concat!(
        "       01  D PIC X(200).\n       01  N PIC 9(4).\n       01  J PIC 9 VALUE 2.\n",
        "       01  Grp.\n           05 Ac-No PIC AA9999.\n           05 FILLER.\n              10 Inner PIC S9(4) COMP-5.\n",
        "           05 FILLER PIC X(3).\n           05 R REDEFINES Ac-No PIC X(6).\n           05 Flag PIC X.\n              88 Flag-On VALUE 'Y'.\n",
        "           05 Gone PIC X.\n           05 More OCCURS 0 TO 2 DEPENDING J.\n              10 Stuff PIC S99V9 OCCURS 2.\n",
    );
    let generate = "JSON GENERATE D FROM Grp COUNT N ENCODING 1140\n    NAME OF Ac-No IS 'acct'\n    SUPPRESS Gone EVERY NUMERIC WHEN ZERO\n    CONVERTING Flag TO BOOLEAN USING Flag-On\n    ON EXCEPTION DISPLAY 'NO'\nEND-JSON";
    let body: String = generate.split('\n').map(line).chain([line("GOBACK.")]).collect();
    let p = lowered(&program("", data, &body));
    let [Markup::JsonGenerate(g)] = markup(&p) else { panic!("{:?}", markup(&p)) };
    assert!(matches!((g.encoding, g.on_exception, g.not_on_exception), (Ccsid::Operand(LirOperand::Const(_)), true, false)));
    assert_eq!((symbol(&p, g.name.unwrap()), g.subscripts.len(), g.count.is_some()), ("\"Grp\"", 0, true));
    let names: Vec<&str> = g.nodes.iter().map(|n| symbol(&p, n.name)).collect();
    assert_eq!(names, ["\"Grp\"", "\"acct\"", "\"Inner\"", "\"Flag\"", "\"More\"", "\"Stuff\""]);
    let JsonValue::Object { members, eligible: true } = &g.nodes[0].value else { panic!("{:?}", g.nodes[0]) };
    assert_eq!(members, &[1, 2, 3, 4]);
    assert_eq!((g.nodes[2].offset, g.nodes[3].offset, g.nodes[4].offset, g.nodes[5].offset, g.nodes[5].len), (6, 11, 13, 0, 3));
    assert!(matches!(&g.nodes[4].occurs, Some(Count::Odo(_))) && g.nodes[5].occurs == Some(Count::Fixed(2)));
    let JsonValue::Leaf(inner) = &g.nodes[2].value else { panic!() };
    assert_eq!((&inner.suppress[..], inner.convert), (&[Figurative::Zero][..], Convert::Fixed { integers: 5 }));
    let JsonValue::Leaf(stuff) = &g.nodes[5].value else { panic!() };
    assert_eq!((&stuff.suppress[..], stuff.convert), (&[Figurative::Zero][..], Convert::Fixed { integers: 2 }));
    let JsonValue::Leaf(flag) = &g.nodes[3].value else { panic!() };
    assert!(matches!((flag.boolean, &flag.suppress[..], flag.convert), (Some(Marker::Condition(_)), [], Convert::Chars { justified: false })));
    assert!(matches!(p.blocks[0].end, Terminator::Select(ref arms) if arms.len() == 2));
}

#[test]
fn an_indicator_in_a_table_is_located_with_the_walk_s_subscripts_and_a_whole_table_is_the_root() {
    let data = "       01  D PIC X(200).\n       01  T.\n           02 G OCCURS 2.\n              03 IS-NULL PIC X.\n              03 V PIC X(10).\n";
    let body = [line("JSON GENERATE D FROM G"), line("    INDICATING V IS JSON NULL USING 'Y' IN IS-NULL"), line("GOBACK.")].concat();
    let p = lowered(&program("", data, &body));
    let [Markup::JsonGenerate(g)] = markup(&p) else { panic!() };
    assert_eq!((g.nodes.len(), g.nodes[0].occurs.clone(), g.subscripts.len()), (2, Some(Count::Fixed(2)), 0));
    assert!(matches!(&p.places[g.from as usize].subscripts[..], [lir::Subscript { value: IntExpr::Const(1), .. }]));
    let Some((Ok(at), Marker::Byte(Some(0xE8)))) = g.nodes[1].indicator else { panic!("{:?}", g.nodes[1]) };
    assert!(matches!(&p.places[at as usize].subscripts[..], [lir::Subscript { value: IntExpr::Walk(0), stride: 11, .. }]));
}

#[test]
fn a_phrase_naming_a_condition_name_abends_where_the_walker_would() {
    let data = "       01  D PIC X(80).\n       01  G.\n           05 A PIC X.\n              88 A-ON VALUE 'Y'.\n";
    let p = lowered(&program("", data, &[line("JSON GENERATE D FROM G SUPPRESS A-ON"), line("GOBACK.")].concat()));
    let Terminator::Abend(a) = p.blocks[0].end else { panic!("{:?}", p.blocks[0].end) };
    assert_eq!(symbol(&p, p.abends[a as usize].message), "A-ON is a condition-name, not a data item");
    assert!(markup(&p).is_empty());
}

#[test]
fn xml_generate_keeps_forms_unnamed_tables_and_each_leaf_s_suppression() {
    let data = concat!(
        "       01  D PIC X(300).\n       01  G.\n           05 A PIC X(3).\n           05 FILLER OCCURS 2.\n              10 B PIC 9(3).\n",
        "           05 C PIC X(3).\n           05 S PIC X.\n",
    );
    let generate = "XML GENERATE D FROM G WITH ATTRIBUTES\n    TYPE OF C IS ELEMENT\n    SUPPRESS S EVERY NUMERIC ATTRIBUTE WHEN ZERO\n        A WHEN SPACE";
    let body: String = generate.split('\n').map(line).chain([line("GOBACK.")]).collect();
    let p = lowered(&program("", data, &body));
    let [Markup::XmlGenerate(x)] = markup(&p) else { panic!() };
    assert!(x.suppressing && x.encoding == Ccsid::Unnamed && x.namespace.is_none());
    let XmlValue::Element { members } = &x.nodes[0].value else { panic!() };
    assert_eq!(members, &[1, 2, 4]);
    assert!(matches!(&x.nodes[2], lir::XmlNode { occurs: Some(Count::Fixed(2)), value: XmlValue::Members { .. }, len: 3, .. }));
    let leaf = |k: usize| match &x.nodes[k].value {
        XmlValue::Leaf { form, suppress, .. } => (*form, suppress.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(leaf(1), (XmlForm::Attribute, vec![Figurative::Space]));
    assert_eq!(leaf(3), (XmlForm::Attribute, vec![Figurative::Zero]));
    assert_eq!(leaf(4), (XmlForm::Element, vec![]));
    let code = &p.places[x.code.0 as usize];
    assert_eq!((symbol(&p, code.name), code.base), ("XML-CODE", Base::Program));
}

#[test]
fn xml_parse_runs_its_processing_procedure_as_a_range_and_its_fragments_are_registers() {
    let data = "       01  DOC PIC X(40).\n       01  N PIC 9(4).\n";
    let procedure = [
        "       MAIN.\n",
        &line("XML PARSE DOC PROCESSING PROCEDURE P THRU Q"),
        &line("    ON EXCEPTION DISPLAY 'NO'"),
        &line("END-XML"),
        &line("GOBACK."),
        "       P.\n",
        &line("MOVE LENGTH OF XML-TEXT TO N"),
        &line("IF XML-TEXT(2:1) = 'b' GO TO Z END-IF."),
        "       Q.\n",
        &line("DISPLAY XML-NTEXT."),
        "       Z.\n",
        &line("GOBACK."),
    ]
    .concat();
    let p = lowered(&program("", data, &procedure));
    let [Markup::XmlParse(x)] = markup(&p) else { panic!() };
    let range = p.ranges[x.procedure as usize];
    assert_eq!((range.first, range.last, range.kind), (1, 2, RangeKind::Processing));
    assert!(p.paragraphs[2].abandoned.is_some() && p.paragraphs[1].abandoned.is_none());
    assert_eq!((symbol(&p, p.places[x.event as usize].name), x.on_exception, x.national), ("XML-EVENT", true, false));
    let text: Vec<&Place> = place_named(&p, "XML-TEXT");
    assert!(text.iter().all(|q| q.base == Base::Xml(XmlRegister::Text) && q.kind == Kind::Alnum { justified: false }));
    assert!(text.iter().any(|q| q.refmod.is_some()));
    assert!(place_named(&p, "XML-NTEXT").iter().all(|q| q.base == Base::Xml(XmlRegister::NText) && q.kind == Kind::National));
    assert!(p.blocks.iter().any(|b| b.end == Terminator::GoTo(paragraph(&p, "Z") as u32)), "a GO TO out of the procedure leaves its frame");
}

#[test]
fn json_parse_lays_out_what_names_reach_and_what_each_flag_sets() {
    let data = concat!(
        "       01  T PIC X(80).\n       01 R.\n         02 IND PIC X.\n         02 V PIC X(10).\n         02 F PIC X.\n           88 F-ON VALUE 'T' FALSE 'F'.\n",
        "         02 B PIC X.\n         02 E PIC $$9.99.\n         02 P PIC S9(3) COMP-3.\n         02 S PIC X.\n",
    );
    let parse = "JSON PARSE T INTO R ENCODING 1140\n    NAME OF V IS 'vee'\n    SUPPRESS S\n    CONVERTING F FROM BOOLEAN USING F-ON\n    ALSO B FROM BOOLEAN USING 'a' AND 'z'\n    INDICATING V IS JSON NULL USING 'Y' AND 'N' IN IND\nEND-JSON";
    let body: String = parse.split('\n').map(line).chain([line("GOBACK.")]).collect();
    let p = lowered(&program("", data, &body));
    let [Markup::JsonParse(j)] = markup(&p) else { panic!() };
    assert!(!j.ignore_all && j.encoding != Ccsid::Unnamed);
    let names: Vec<Named> = j.nodes.iter().map(|n| n.name).collect();
    let sym = |s: &str| p.symbols.iter().position(|t| t == s).unwrap() as u32;
    assert_eq!(names, [Named::Folded(sym("R")), Named::Exactly(sym("vee")), Named::Folded(sym("F")), Named::Folded(sym("B")), Named::Folded(sym("E")), Named::Folded(sym("P")), Named::Folded(sym("S"))]);
    assert_eq!(j.nodes[6].value, ParseValue::Suppressed);
    let indicator = j.nodes[1].indicator.as_ref().unwrap();
    assert!(matches!((indicator.place, indicator.flag), (Some(Ok(_)), Flag::Literals { .. })));
    let ParseValue::Leaf(f) = &j.nodes[2].value else { panic!() };
    let Some(Flag::Set { on: SetTo::Move { value: on, .. }, off: SetTo::Move { value: off, .. } }) = f.boolean else { panic!("{:?}", f.boolean) };
    assert_eq!((&p.consts[on as usize], &p.consts[off as usize]), (&Const::Bytes(vec![0xE3]), &Const::Bytes(vec![0xC6])));
    let ParseValue::Leaf(e) = &j.nodes[4].value else { panic!() };
    assert!(matches!((e.text, e.number), (None, NumberInto::Edited(MovePlan::Numeric { .. }))));
    let ParseValue::Leaf(packed) = &j.nodes[5].value else { panic!() };
    assert!(matches!(packed.number, NumberInto::Store(StorePlan::Packed { digits: 3, .. })));
    let ParseValue::Leaf(v) = &j.nodes[1].value else { panic!() };
    assert!(matches!((v.text, v.number), (Some(MovePlan::Alnum { image: Image::Bytes, .. }), NumberInto::Digits)));
}
