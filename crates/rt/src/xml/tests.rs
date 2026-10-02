use super::*;

fn ascii(c: char) -> bool {
    c.is_ascii()
}

/// Each event as "NAME text", with the namespace and prefix when set, and END-OF-INPUT between
/// segments; `Err` names the first error.
fn run(segments: &[&str]) -> Result<Vec<String>, (Vec<String>, Malformed)> {
    let mut s = Scanner::new(segments[0], &ascii);
    let mut rest = segments[1..].iter();
    let mut out = Vec::new();
    loop {
        match s.advance() {
            Step::Event(e) => {
                let mut line = [e.kind.name(), &e.text].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" ");
                if !e.namespace.is_empty() || !e.prefix.is_empty() {
                    line.push_str(&format!(" [{}|{}]", e.prefix, e.namespace));
                }
                if e.information == 2 {
                    line.push_str(" (more)");
                }
                if e.code != 0 {
                    line.push_str(&format!(" {}", e.code));
                }
                out.push(line.trim_end().to_owned());
                if e.kind == EventKind::EndOfDocument {
                    return Ok(out);
                }
            }
            Step::EndOfInput => {
                out.push("END-OF-INPUT".into());
                match rest.next() {
                    Some(segment) => s.feed(segment),
                    None => s.finish(),
                }
            }
            Step::Error(m) => return Err((out, m)),
            Step::Done => return Ok(out),
        }
    }
}

#[test]
fn a_simple_document_gives_the_programming_guides_events() {
    let events = run(&[r#"<?xml version="1.0"?><msg type="short">Hello, World!</msg>"#]).unwrap();
    assert_eq!(
        events,
        ["START-OF-DOCUMENT", "VERSION-INFORMATION 1.0", "START-OF-ELEMENT msg", "ATTRIBUTE-NAME type", "ATTRIBUTE-CHARACTERS short", "CONTENT-CHARACTERS Hello, World!", "END-OF-ELEMENT msg", "END-OF-DOCUMENT"]
    );
}

#[test]
fn segments_give_the_programming_guides_xmlsss_output() {
    let segments = [
        r#"<?xml version="1.0" encoding="ibm-1047" "#,
        r#"standalone="yes"?><!--This document is j"#,
        r#"ust an example--><sandwich><bread type=""#,
        r#"baker&apos;s best"/><?spread We'll use r"#,
        "eal mayonnaise?><meat>Ham &amp; turkey</",
        "meat><filling>Cheese, lettuce, tomato, a",
        "nd that's all, Folks!</filling><![CDATA[",
        "We should add a <relish> element!]]><lis",
        "tprice>$4.99</listprice><discount>0.10</",
        "discount></sandwich>                    ",
    ];
    let expected = [
        "START-OF-DOCUMENT",
        "END-OF-INPUT",
        "VERSION-INFORMATION 1.0",
        "ENCODING-DECLARATION ibm-1047",
        "STANDALONE-DECLARATION yes",
        "COMMENT This document is j",
        "END-OF-INPUT",
        "COMMENT ust an example",
        "START-OF-ELEMENT sandwich",
        "END-OF-INPUT",
        "START-OF-ELEMENT bread",
        "ATTRIBUTE-NAME type",
        "ATTRIBUTE-CHARACTERS baker's best",
        "END-OF-ELEMENT bread",
        "PROCESSING-INSTRUCTION-TARGET spread",
        "PROCESSING-INSTRUCTION-DATA We'll use r",
        "END-OF-INPUT",
        "PROCESSING-INSTRUCTION-TARGET spread",
        "PROCESSING-INSTRUCTION-DATA eal mayonnaise",
        "START-OF-ELEMENT meat",
        "CONTENT-CHARACTERS Ham & turkey",
        "END-OF-INPUT",
        "END-OF-ELEMENT meat",
        "START-OF-ELEMENT filling",
        "CONTENT-CHARACTERS Cheese, lettuce, tomato, a (more)",
        "END-OF-INPUT",
        "CONTENT-CHARACTERS nd that's all, Folks!",
        "END-OF-ELEMENT filling",
        "END-OF-INPUT",
        "START-OF-CDATA-SECTION",
        "CONTENT-CHARACTERS We should add a <relish> element!",
        "END-OF-CDATA-SECTION",
        "END-OF-INPUT",
        "START-OF-ELEMENT listprice",
        "CONTENT-CHARACTERS $4.99",
        "END-OF-ELEMENT listprice",
        "START-OF-ELEMENT discount",
        "CONTENT-CHARACTERS 0.10",
        "END-OF-INPUT",
        "END-OF-ELEMENT discount",
        "END-OF-ELEMENT sandwich",
        "END-OF-DOCUMENT",
    ];
    assert_eq!(run(&segments).unwrap(), expected);
}

#[test]
fn namespaces_bind_element_and_attribute_names() {
    let events = run(&[r#"<p:a xmlns:p="urn:p" xmlns="urn:d"><b p:c="1" d="2"/></p:a>"#]).unwrap();
    assert_eq!(
        events,
        [
            "START-OF-DOCUMENT",
            "START-OF-ELEMENT a [p|urn:p]",
            "NAMESPACE-DECLARATION [p|urn:p]",
            "NAMESPACE-DECLARATION [|urn:d]",
            "START-OF-ELEMENT b [|urn:d]",
            "ATTRIBUTE-NAME c [p|urn:p]",
            "ATTRIBUTE-CHARACTERS 1",
            "ATTRIBUTE-NAME d",
            "ATTRIBUTE-CHARACTERS 2",
            "END-OF-ELEMENT b [|urn:d]",
            "END-OF-ELEMENT a [p|urn:p]",
            "END-OF-DOCUMENT",
        ]
    );
}

#[test]
fn a_document_that_is_not_well_formed_stops_where_it_goes_wrong() {
    let error = |doc: &str| run(&[doc]).unwrap_err().1;
    assert_eq!(error("<a></b>"), Malformed { offset: 3, why: Why::MismatchedEndTag });
    assert_eq!(error(r#"<a x="1" x="2"/>"#), Malformed { offset: 14, why: Why::DuplicateAttribute });
    assert_eq!(error("<a>&foo;</a>"), Malformed { offset: 3, why: Why::UndeclaredEntity });
    assert_eq!(error("<a>"), Malformed { offset: 3, why: Why::UnexpectedEnd });
    assert_eq!(error("<a/>x"), Malformed { offset: 4, why: Why::ContentAfterRoot });
    assert_eq!(error("<a/><b/>"), Malformed { offset: 4, why: Why::SecondRoot });
    assert_eq!(Why::MismatchedEndTag.code(), 0x000C_3035);
    assert_eq!(error("   "), Malformed { offset: 3, why: Why::NoRoot });
    assert_eq!(error("<a><!-- x -- y --></a>"), Malformed { offset: 18, why: Why::BadComment });
    assert_eq!(error(r#"<a b="<"/>"#), Malformed { offset: 6, why: Why::LessThanInAttribute });
    assert_eq!(error(" <?xml version=\"1.0\"?><a/>"), Malformed { offset: 3, why: Why::BadDeclaration });
    let (before, _) = run(&["<a>x</b>"]).unwrap_err();
    assert_eq!(before, ["START-OF-DOCUMENT", "START-OF-ELEMENT a", "CONTENT-CHARACTERS x"]);
}

#[test]
fn a_character_the_code_page_lacks_is_a_national_character() {
    let events = run(&["<a t='&#233;!'>x&#233;y&#65;</a>"]).unwrap();
    assert_eq!(
        events,
        [
            "START-OF-DOCUMENT",
            "START-OF-ELEMENT a",
            "ATTRIBUTE-NAME t",
            "ATTRIBUTE-NATIONAL-CHARACTER é",
            "ATTRIBUTE-CHARACTERS !",
            "CONTENT-CHARACTERS x",
            "CONTENT-NATIONAL-CHARACTER é",
            "CONTENT-CHARACTERS yA",
            "END-OF-ELEMENT a",
            "END-OF-DOCUMENT",
        ]
    );
}

#[test]
fn a_doctype_gives_its_root_and_standalone_no_lets_an_entity_stay_unresolved() {
    let events = run(&[r#"<?xml version="1.0" standalone="no"?><!DOCTYPE a [<!ENTITY e "x">]><a>&e;</a>"#]).unwrap();
    assert_eq!(
        events,
        ["START-OF-DOCUMENT", "VERSION-INFORMATION 1.0", "STANDALONE-DECLARATION no", "DOCUMENT-TYPE-DECLARATION a", "START-OF-ELEMENT a", "UNRESOLVED-REFERENCE e", "END-OF-ELEMENT a", "END-OF-DOCUMENT"]
    );
}

#[test]
fn an_undeclared_prefix_is_a_warning_before_its_name_as_the_programming_guides_table_83_shows() {
    let document = concat!(
        r#"<pfx0:root xmlns:pfx1="http://whatever">"#,
        "<pfx1:localElName1>",
        "<pfx2:localElName2/>",
        r#"<pfx3:localElName3 pfx4:localAtName4="">"#,
        "c1",
        r#"<pfx5:localElName5 pfx6:localAtName6=""/>"#,
        "c2</pfx3:localElName3>c3",
        "</pfx1:localElName1></pfx0:root>",
    );
    assert_eq!(
        run(&[document]).unwrap(),
        [
            "START-OF-DOCUMENT",
            "EXCEPTION pfx0:root 264193",
            "START-OF-ELEMENT root [pfx0|]",
            "NAMESPACE-DECLARATION [pfx1|http://whatever]",
            "START-OF-ELEMENT localElName1 [pfx1|http://whatever]",
            "EXCEPTION pfx2:localElName2 264193",
            "START-OF-ELEMENT localElName2 [pfx2|]",
            "END-OF-ELEMENT localElName2 [pfx2|]",
            "EXCEPTION pfx3:localElName3 264193",
            "START-OF-ELEMENT localElName3 [pfx3|]",
            "EXCEPTION pfx4:localAtName4 264192",
            "ATTRIBUTE-NAME localAtName4 [pfx4|]",
            "ATTRIBUTE-CHARACTERS",
            "CONTENT-CHARACTERS c1",
            "EXCEPTION pfx5:localElName5 264193",
            "START-OF-ELEMENT localElName5 [pfx5|]",
            "EXCEPTION pfx6:localAtName6 264192",
            "ATTRIBUTE-NAME localAtName6 [pfx6|]",
            "ATTRIBUTE-CHARACTERS",
            "END-OF-ELEMENT localElName5 [pfx5|]",
            "CONTENT-CHARACTERS c2",
            "END-OF-ELEMENT localElName3 [pfx3|]",
            "CONTENT-CHARACTERS c3",
            "END-OF-ELEMENT localElName1 [pfx1|http://whatever]",
            "END-OF-ELEMENT root [pfx0|]",
            "END-OF-DOCUMENT",
        ]
    );
}
