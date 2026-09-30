use super::*;

fn displays(options: &str, data: &str, statements: &[&str]) -> String {
    let body: String = statements.iter().flat_map(|s| s.split('\n')).map(line).chain([line("GOBACK.")]).collect();
    let o = Harness::source(&program(options, data, &body)).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

const DOC: &str = "       01  Doc PIC X(512) VALUE SPACES.\n       01  docSize PIC 9(9) BINARY.\n       01  Code-Out PIC 9(3).\n";

fn generated(data: &str, generate: &str) -> String {
    displays("", &format!("{DOC}{data}"), &[generate, "DISPLAY Doc(1:docSize)", "MOVE SPACES TO Doc"])
}

#[test]
fn groups_become_elements_or_with_attributes_attributes() {
    let data = "       01  G.\n           05  A pic x(3) value \"aaa\".\n           05  B.\n               10  C pic x(3) value \"ccc\".\n               10  D pic x(3) value \"ddd\".\n           05  E pic x(3) value \"eee\".\n";
    assert_eq!(generated(data, "XML Generate Doc from G count in docSize"), "<G><A>aaa</A><B><C>ccc</C><D>ddd</D></B><E>eee</E></G>\n");
    assert_eq!(generated(data, "XML Generate Doc from G count in docSize\n    with attributes"), "<G A=\"aaa\" E=\"eee\"><B C=\"ccc\" D=\"ddd\"></B></G>\n");
}

#[test]
fn a_namespace_is_the_default_or_takes_its_prefix_on_every_element() {
    let data = "       01  Greeting.\n           05  msg  pic x(80)  value 'Hello, world!'.\n       01  NS  pic x(20)   value 'http://example'.\n       01  NP  pic x(5)    value 'pre'.\n";
    assert_eq!(
        generated(data, "XML Generate Doc from Greeting count in docSize\n    namespace is NS"),
        "<Greeting xmlns=\"http://example\"><msg>Hello, world!</msg></Greeting>\n"
    );
    assert_eq!(
        generated(data, "XML Generate Doc from Greeting count in docSize\n    namespace is NS\n    namespace-prefix is NP"),
        "<pre:Greeting xmlns:pre=\"http://example\"><pre:msg>Hello, world!</pre:msg></pre:Greeting>\n"
    );
    assert_eq!(
        generated(data, "XML Generate Doc from Greeting count in docSize\n    with XML-declaration"),
        "<?xml version=\"1.0\" encoding=\"IBM-1140\"?><Greeting><msg>Hello, world!</msg></Greeting>\n"
    );
    let utf8 = displays("", &format!("{DOC}{data}"), &["XML Generate Doc from Greeting count in docSize\n    with Encoding 1208\n    with XML-declaration\nEnd-XML", "DISPLAY docSize ' ' FUNCTION HEX-OF(Doc(1:19))"]);
    assert_eq!(utf8, "000000083 3C3F786D6C2076657273696F6E3D22312E3022\n");
}

#[test]
fn name_and_type_choose_names_and_forms() {
    let data = "       01 Msg.\n           02 Msg-Severity pic 9 value 1.\n           02 Msg-Date pic 9999/99/99.\n           02 Msg-Text pic X(50) value \"Sell everything!\".\n";
    let named = "MOVE 20120412 TO Msg-Date\nXML Generate Doc from Msg count in docSize\n    With attributes\n    Name of Msg is \"Message\"\n        Msg-Severity is \"Severity\"\n        Msg-Date is \"Date\"\n        Msg-Text is \"Text\"\nEnd-XML";
    assert_eq!(generated(data, named), "<Message Severity=\"1\" Date=\"2012/04/12\" Text=\"Sell everything!\"></Message>\n");
    let typed = "MOVE 20120412 TO Msg-Date\nXML Generate Doc from Msg count in docSize\n    With attributes\n    Type of Msg-Severity is attribute\n        Msg-Date is attribute\n        Msg-Text is element\nEnd-XML";
    assert_eq!(generated(data, typed), "<Msg Msg-Severity=\"1\" Msg-Date=\"2012/04/12\"><Msg-Text>Sell everything!</Msg-Text></Msg>\n");
    let content = "MOVE 20120412 TO Msg-Date\nXML Generate Doc from Msg count in docSize\n    Type of Msg-Text is content\nEnd-XML";
    assert_eq!(generated(data, content), "<Msg><Msg-Severity>1</Msg-Severity><Msg-Date>2012/04/12</Msg-Date>Sell everything!</Msg>\n");
}

#[test]
fn suppress_leaves_out_items_and_the_groups_it_empties() {
    let data = concat!(
        "       01 G.\n           02 SensitiveInfo.\n           03 SSN pic x(11) value '123-45-6789'.\n",
        "           03 HomeAddress pic x(50) value '123 Main St, Anytown, USA'.\n",
        "           02 Aarray value spaces.\n              03 A pic AAA occurs 5.\n",
        "           02 Barray value spaces.\n              03 B pic XXX occurs 5.\n",
        "           02 Carray value zeros.\n              03 C pic 999 occurs 5.\n",
    );
    let generate = "Move 'abc' to A(1)\nMove 123 to C(3)\nXML Generate Doc from G count in docSize\n    Suppress SensitiveInfo\n        every nonnumeric element when space\n        every numeric element when zero\nEnd-XML";
    assert_eq!(generated(data, generate), "<G><Aarray><A>abc</A></Aarray><Carray><C>123</C></Carray></G>\n");
}

#[test]
fn the_programming_guides_purchase_order() {
    let data = concat!(
        "       01 numItems pic 99.\n       01 purchaseOrder.\n         05 orderDate pic x(10).\n",
        "         05 shipTo.\n           10 country pic xx value 'US'.\n           10 name pic x(30).\n           10 street pic x(30).\n",
        "           10 city pic x(30).\n           10 state pic xx.\n           10 zip pic x(10).\n",
        "         05 billTo.\n           10 country pic xx value 'US'.\n           10 name pic x(30).\n           10 street pic x(30).\n",
        "           10 city pic x(30).\n           10 state pic xx.\n           10 zip pic x(10).\n",
        "         05 orderComment pic x(80).\n         05 items occurs 0 to 20 times depending on numItems.\n",
        "           10 item.\n             15 partNum pic x(6).\n             15 productName pic x(50).\n             15 quantity pic 99.\n",
        "             15 USPrice pic 999v99.\n             15 shipDate pic x(10).\n             15 itemComment pic x(40).\n",
        "       01 numChars comp pic 999.\n       01 xmlPO pic x(999).\n",
    );
    let statements = [
        "Move 20 to numItems",
        "Move spaces to purchaseOrder",
        "Move '1999-10-20' to orderDate",
        "Move 'US' to country of shipTo",
        "Move 'Alice Smith' to name of shipTo",
        "Move '123 Maple Street' to street of shipTo",
        "Move 'Mill Valley' to city of shipTo",
        "Move 'CA' to state of shipTo",
        "Move '90952' to zip of shipTo",
        "Move 'US' to country of billTo",
        "Move 'Robert Smith' to name of billTo",
        "Move '8 Oak Avenue' to street of billTo",
        "Move 'Old Town' to city of billTo",
        "Move 'PA' to state of billTo",
        "Move '95819' to zip of billTo",
        "Move 'Hurry, my lawn is going wild!' to orderComment",
        "Move 2 to numItems",
        "Move '872-AA' to partNum(1)",
        "Move 'Lawnmower' to productName(1)",
        "Move 1 to quantity(1)",
        "Move 148.95 to USPrice(1)",
        "Move 'Confirm this is electric' to itemComment(1)",
        "Move '926-AA' to partNum(2)",
        "Move 'Baby Monitor' to productName(2)",
        "Move 1 to quantity(2)",
        "Move 39.98 to USPrice(2)",
        "Move '1999-05-21' to shipDate(2)",
        "Move space to xmlPO",
        "Xml generate xmlPO from purchaseOrder count in numChars\n    with xml-declaration with attributes\n    namespace 'http://www.example.com' namespace-prefix 'po'",
        "Display xmlPO(1:numChars)",
    ];
    let out = displays("CODEPAGE(37)", data, &statements);
    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"IBM-037\"?>",
        "<po:purchaseOrder xmlns:po=\"http://www.example.com\" orderDate=\"1999-10-20\" orderComment=\"Hurry, my lawn is going wild!\">",
        "<po:shipTo country=\"US\" name=\"Alice Smith\" street=\"123 Maple Street\" city=\"Mill Valley\" state=\"CA\" zip=\"90952\"></po:shipTo>",
        "<po:billTo country=\"US\" name=\"Robert Smith\" street=\"8 Oak Avenue\" city=\"Old Town\" state=\"PA\" zip=\"95819\"></po:billTo>",
        "<po:items><po:item partNum=\"872-AA\" productName=\"Lawnmower\" quantity=\"1\" USPrice=\"148.95\" shipDate=\" \" itemComment=\"Confirm this is electric\"></po:item></po:items>",
        "<po:items><po:item partNum=\"926-AA\" productName=\"Baby Monitor\" quantity=\"1\" USPrice=\"39.98\" shipDate=\"1999-05-21\" itemComment=\" \"></po:item></po:items>",
        "</po:purchaseOrder>\n",
    );
    assert_eq!(out, expected);
}

#[test]
fn names_take_an_underscore_values_are_escaped_and_illegal_ones_go_in_hex() {
    let data = "       01  Xml-Rec.\n           05 3D PIC X(5) VALUE 'a<b&c'.\n           05 Low PIC XX VALUE LOW-VALUES.\n";
    let out = displays(
        "",
        &format!("{DOC}{data}"),
        &["XML GENERATE Doc FROM Xml-Rec COUNT IN docSize\n    ON EXCEPTION MOVE XML-CODE TO Code-Out\n    DISPLAY 'CODE ' Code-Out\nEND-XML", "DISPLAY Doc(1:docSize)"],
    );
    assert_eq!(out, "CODE 417\n<_Xml-Rec><_3D>a&lt;b&amp;c</_3D><hex.Low>0000</hex.Low></_Xml-Rec>\n");
}

#[test]
fn a_receiver_too_small_is_exception_400_and_a_national_receiver_counts_characters() {
    let data = "       01  Small PIC X(10).\n       01  N PIC 9(4).\n       01  Code-Out PIC 9(3).\n       01  NDoc PIC N(80).\n       01  G.\n           05 Long-Name PIC X(20) VALUE 'VALUE'.\n";
    let out = displays(
        "",
        data,
        &[
            "XML GENERATE Small FROM G COUNT N\n    ON EXCEPTION MOVE XML-CODE TO Code-Out\n    DISPLAY 'TOO SMALL ' Code-Out ' ' N\n    NOT ON EXCEPTION DISPLAY 'FITS'\nEND-XML",
            "DISPLAY Small",
            "XML GENERATE NDoc FROM G COUNT N\n    WITH XML-DECLARATION",
            "MOVE XML-CODE TO Code-Out",
            "DISPLAY Code-Out ' ' N ' ' FUNCTION DISPLAY-OF(NDoc(1:N))",
        ],
    );
    assert_eq!(
        out,
        "TOO SMALL 400 0010\n<G><Long-N\n000 0074 <?xml version=\"1.0\" encoding=\"UTF-16\"?><G><Long-Name>VALUE</Long-Name></G>\n"
    );
}

#[test]
fn encoding_errors_leave_the_receiver_alone() {
    let data = "       01  NDoc PIC N(20).\n       01  G.\n           05 A PIC X VALUE 'A'.\n           05 W PIC N(2) VALUE N'AB'.\n";
    let out = displays(
        "",
        &format!("{DOC}{data}"),
        &[
            "XML GENERATE NDoc FROM G ENCODING 1208",
            "MOVE XML-CODE TO Code-Out",
            "DISPLAY Code-Out",
            "XML GENERATE Doc FROM G ENCODING 819",
            "MOVE XML-CODE TO Code-Out",
            "DISPLAY Code-Out",
            "XML GENERATE Doc FROM G COUNT docSize",
            "MOVE XML-CODE TO Code-Out",
            "DISPLAY Code-Out",
            "XML GENERATE Doc FROM G COUNT docSize ENCODING 1208",
            "MOVE XML-CODE TO Code-Out",
            "DISPLAY Code-Out ' ' docSize",
        ],
    );
    assert_eq!(out, "415\n414\n420\n000 000000024\n");
}
