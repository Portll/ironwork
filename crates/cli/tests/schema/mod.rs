//! A JSON reader and a validator for a subset of JSON Schema (draft 2020-12), so a test can check
//! that a document ironwork wrote has the shape its committed schema describes. The workspace has no
//! JSON crate a test could use.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// A JSON value, preserving key order in objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

/// Maximum recursion depth for parsing and validation.
const MAX_DEPTH: usize = 128;

/// Parses a JSON document from a string.
///
/// Returns `Ok(Json)` on success, or `Err(String)` with a byte offset on failure.
pub fn parse(text: &str) -> Result<Json, String> {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    let value = parse_value(bytes, &mut pos, 0)?;
    skip_whitespace(bytes, &mut pos);
    if pos < bytes.len() {
        return Err(format!("trailing non-whitespace at byte offset {}", pos));
    }
    Ok(value)
}

/// Validates a document against a schema.
///
/// Returns a `Vec<String>` of violations, each formatted as "PATH: description".
/// An empty vector means the document conforms.
pub fn validate(schema: &Json, doc: &Json) -> Vec<String> {
    let mut errors = Vec::new();
    validate_node(schema, schema, doc, "", &mut errors, 0);
    errors
}

const MANIFEST_SCHEMA: &str = include_str!("../../../../docs/fuzz-manifest.schema.json");
const INTERFACE_SCHEMA: &str = include_str!("../../../../docs/fuzz-interface-manifest.schema.json");

/// The schema each manifest format is described by.
fn schema_of(format: &str) -> Option<&'static str> {
    match format {
        "ironwork-fuzz/v1" => Some(MANIFEST_SCHEMA),
        "ironwork-fuzz-interface/v1" => Some(INTERFACE_SCHEMA),
        _ => None,
    }
}

/// Every way `manifest`, a fuzz run's manifest.json, departs from the schema its `format` names.
pub fn manifest_violations(manifest: &str) -> Vec<String> {
    let doc = match parse(manifest) {
        Ok(doc) => doc,
        Err(e) => return vec![format!("not JSON: {e}")],
    };
    let format = match &doc {
        Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == "format").and_then(|(_, v)| if let Json::Str(f) = v { Some(f.as_str()) } else { None }),
        _ => None,
    };
    // A manifest that names no format is judged as the first one, which requires it.
    match format.map_or(Some(MANIFEST_SCHEMA), schema_of) {
        Some(text) => validate(&parse(text).expect("a committed schema is JSON"), &doc),
        None => vec![format!(": format {format:?} has no schema in docs/")],
    }
}

/// DIR/manifest.json, once it conforms to the schema its format names with every key it writes
/// listed there: the published schema leaves objects open to keys a later release adds.
pub fn read_manifest(dir: &Path) -> String {
    let text = fs::read_to_string(dir.join("manifest.json")).unwrap();
    let mut violations = manifest_violations(&text);
    if violations.is_empty() {
        let doc = parse(&text).unwrap();
        let format = match &doc {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == "format").and_then(|(_, v)| if let Json::Str(f) = v { schema_of(f) } else { None }),
            _ => None,
        };
        violations = validate(&closed(parse(format.unwrap_or(MANIFEST_SCHEMA)).unwrap()), &doc);
    }
    assert!(violations.is_empty(), "{} departs from its schema in docs/:\n{}\n{text}", dir.display(), violations.join("\n"));
    text
}

/// `schema` with every object that lists `properties` refusing the keys it does not list.
pub fn closed(schema: Json) -> Json {
    match schema {
        Json::Obj(pairs) => {
            let lists = pairs.iter().any(|(k, _)| k == "properties") && !pairs.iter().any(|(k, _)| k == "additionalProperties");
            let mut pairs: Vec<(String, Json)> = pairs.into_iter().map(|(k, v)| (k, closed(v))).collect();
            if lists {
                pairs.push(("additionalProperties".to_owned(), Json::Bool(false)));
            }
            Json::Obj(pairs)
        }
        Json::Arr(items) => Json::Arr(items.into_iter().map(closed).collect()),
        other => other,
    }
}

fn parse_value(bytes: &[u8], pos: &mut usize, depth: usize) -> Result<Json, String> {
    if depth > MAX_DEPTH {
        return Err(format!("recursion depth exceeded at byte offset {}", *pos));
    }
    skip_whitespace(bytes, pos);
    if *pos >= bytes.len() {
        return Err(format!("unexpected end of input at byte offset {}", *pos));
    }
    let c = bytes[*pos];
    match c {
        b'{' => parse_object(bytes, pos, depth),
        b'[' => parse_array(bytes, pos, depth),
        b'"' => parse_string(bytes, pos).map(Json::Str),
        b't' => parse_literal(bytes, pos, "true", Json::Bool(true)),
        b'f' => parse_literal(bytes, pos, "false", Json::Bool(false)),
        b'n' => parse_literal(bytes, pos, "null", Json::Null),
        b'-' | b'0'..=b'9' => parse_number(bytes, pos),
        _ => Err(format!("unexpected character '{}' at byte offset {}", c as char, *pos)),
    }
}

fn skip_whitespace(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() {
        match bytes[*pos] {
            b' ' | b'\t' | b'\n' | b'\r' => *pos += 1,
            _ => break,
        }
    }
}

fn parse_object(bytes: &[u8], pos: &mut usize, depth: usize) -> Result<Json, String> {
    *pos += 1;
    let mut members = Vec::new();
    let mut seen_keys = BTreeSet::new();
    skip_whitespace(bytes, pos);
    if *pos < bytes.len() && bytes[*pos] == b'}' {
        *pos += 1;
        return Ok(Json::Obj(members));
    }
    loop {
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() {
            return Err(format!("unexpected end of input in object at byte offset {}", *pos));
        }
        if bytes[*pos] != b'"' {
            return Err(format!("expected string key at byte offset {}", *pos));
        }
        let key = parse_string(bytes, pos)?;
        if !seen_keys.insert(key.clone()) {
            return Err(format!("duplicate key '{}' at byte offset {}", key, *pos));
        }
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() || bytes[*pos] != b':' {
            return Err(format!("expected ':' at byte offset {}", *pos));
        }
        *pos += 1;
        let value = parse_value(bytes, pos, depth + 1)?;
        members.push((key, value));
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() {
            return Err(format!("unexpected end of input in object at byte offset {}", *pos));
        }
        match bytes[*pos] {
            b',' => {
                *pos += 1;
            }
            b'}' => {
                *pos += 1;
                break;
            }
            _ => {
                return Err(format!(
                    "expected ',' or '}}' at byte offset {}",
                    *pos
                ))
            }
        }
    }
    Ok(Json::Obj(members))
}

fn parse_array(bytes: &[u8], pos: &mut usize, depth: usize) -> Result<Json, String> {
    *pos += 1;
    let mut elements = Vec::new();
    skip_whitespace(bytes, pos);
    if *pos < bytes.len() && bytes[*pos] == b']' {
        *pos += 1;
        return Ok(Json::Arr(elements));
    }
    loop {
        let value = parse_value(bytes, pos, depth + 1)?;
        elements.push(value);
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() {
            return Err(format!("unexpected end of input in array at byte offset {}", *pos));
        }
        match bytes[*pos] {
            b',' => {
                *pos += 1;
            }
            b']' => {
                *pos += 1;
                break;
            }
            _ => {
                return Err(format!(
                    "expected ',' or ']' at byte offset {}",
                    *pos
                ))
            }
        }
    }
    Ok(Json::Arr(elements))
}

fn parse_string(bytes: &[u8], pos: &mut usize) -> Result<String, String> {
    *pos += 1;
    let mut result = String::new();
    loop {
        if *pos >= bytes.len() {
            return Err(format!("unexpected end of input in string at byte offset {}", *pos));
        }
        let c = bytes[*pos];
        match c {
            b'"' => {
                *pos += 1;
                return Ok(result);
            }
            b'\\' => {
                *pos += 1;
                if *pos >= bytes.len() {
                    return Err(format!("unexpected end of input in escape at byte offset {}", *pos));
                }
                let esc = bytes[*pos];
                match esc {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'/' => result.push('/'),
                    b'b' => result.push('\u{0008}'),
                    b'f' => result.push('\u{000C}'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'u' => {
                        *pos += 1;
                        let code = parse_unicode_escape(bytes, pos)?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            if *pos + 1 < bytes.len()
                                && bytes[*pos] == b'\\'
                                && bytes[*pos + 1] == b'u'
                            {
                                *pos += 2;
                                let low = parse_unicode_escape(bytes, pos)?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    let combined = 0x10000
                                        + ((code - 0xD800) << 10)
                                        + (low - 0xDC00);
                                    match char::from_u32(combined) {
                                        Some(ch) => result.push(ch),
                                        None => {
                                            return Err(format!(
                                                "invalid surrogate pair at byte offset {}",
                                                *pos
                                            ))
                                        }
                                    }
                                } else {
                                    return Err(format!(
                                        "invalid low surrogate at byte offset {}",
                                        *pos
                                    ));
                                }
                            } else {
                                return Err(format!(
                                    "lone high surrogate at byte offset {}",
                                    *pos
                                ));
                            }
                        } else if (0xDC00..=0xDFFF).contains(&code) {
                            return Err(format!(
                                "lone low surrogate at byte offset {}",
                                *pos
                            ));
                        } else {
                            match char::from_u32(code) {
                                Some(ch) => result.push(ch),
                                None => {
                                    return Err(format!(
                                        "invalid unicode escape at byte offset {}",
                                        *pos
                                    ))
                                }
                            }
                        }
                        continue;
                    }
                    _ => {
                        return Err(format!(
                            "invalid escape character '{}' at byte offset {}",
                            esc as char,
                            *pos
                        ))
                    }
                }
                *pos += 1;
            }
            _ if c < 0x20 => {
                return Err(format!(
                    "unescaped control character at byte offset {}",
                    *pos
                ));
            }
            _ => {
                let start = *pos;
                let len = utf8_len(c);
                if start + len > bytes.len() {
                    return Err(format!(
                        "invalid UTF-8 sequence at byte offset {}",
                        start
                    ));
                }
                let slice = &bytes[start..start + len];
                match std::str::from_utf8(slice) {
                    Ok(s) => {
                        result.push_str(s);
                        *pos = start + len;
                    }
                    Err(_) => {
                        return Err(format!(
                            "invalid UTF-8 sequence at byte offset {}",
                            start
                        ));
                    }
                }
            }
        }
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

fn parse_unicode_escape(bytes: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > bytes.len() {
        return Err(format!(
            "unexpected end of input in unicode escape at byte offset {}",
            *pos
        ));
    }
    let hex = &bytes[*pos..*pos + 4];
    let mut code: u32 = 0;
    for &b in hex {
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => {
                return Err(format!(
                    "invalid hex digit in unicode escape at byte offset {}",
                    *pos
                ))
            }
        };
        code = (code << 4) | digit as u32;
    }
    *pos += 4;
    Ok(code)
}

fn parse_literal(bytes: &[u8], pos: &mut usize, literal: &str, value: Json) -> Result<Json, String> {
    let len = literal.len();
    if *pos + len > bytes.len() {
        return Err(format!(
            "unexpected end of input in literal at byte offset {}",
            *pos
        ));
    }
    if &bytes[*pos..*pos + len] != literal.as_bytes() {
        return Err(format!(
            "invalid literal at byte offset {}",
            *pos
        ));
    }
    *pos += len;
    Ok(value)
}

fn parse_number(bytes: &[u8], pos: &mut usize) -> Result<Json, String> {
    let start = *pos;
    if *pos < bytes.len() && bytes[*pos] == b'-' {
        *pos += 1;
    }
    if *pos >= bytes.len() {
        return Err(format!("unexpected end of input in number at byte offset {}", *pos));
    }
    if bytes[*pos] == b'0' {
        *pos += 1;
    } else if bytes[*pos] >= b'1' && bytes[*pos] <= b'9' {
        *pos += 1;
        while *pos < bytes.len() && bytes[*pos] >= b'0' && bytes[*pos] <= b'9' {
            *pos += 1;
        }
    } else {
        return Err(format!("invalid number at byte offset {}", *pos));
    }
    let mut is_float = false;
    if *pos < bytes.len() && bytes[*pos] == b'.' {
        is_float = true;
        *pos += 1;
        if *pos >= bytes.len() || bytes[*pos] < b'0' || bytes[*pos] > b'9' {
            return Err(format!("invalid number fraction at byte offset {}", *pos));
        }
        while *pos < bytes.len() && bytes[*pos] >= b'0' && bytes[*pos] <= b'9' {
            *pos += 1;
        }
    }
    if *pos < bytes.len() && (bytes[*pos] == b'e' || bytes[*pos] == b'E') {
        is_float = true;
        *pos += 1;
        if *pos < bytes.len() && (bytes[*pos] == b'+' || bytes[*pos] == b'-') {
            *pos += 1;
        }
        if *pos >= bytes.len() || bytes[*pos] < b'0' || bytes[*pos] > b'9' {
            return Err(format!("invalid number exponent at byte offset {}", *pos));
        }
        while *pos < bytes.len() && bytes[*pos] >= b'0' && bytes[*pos] <= b'9' {
            *pos += 1;
        }
    }
    let num_str = std::str::from_utf8(&bytes[start..*pos]).map_err(|_| {
        format!("invalid UTF-8 in number at byte offset {}", start)
    })?;
    if is_float {
        num_str
            .parse::<f64>()
            .map(Json::Float)
            .map_err(|e| format!("invalid float '{}' at byte offset {}: {}", num_str, start, e))
    } else {
        match num_str.parse::<i64>() {
            Ok(n) => Ok(Json::Int(n)),
            Err(_) => num_str.parse::<f64>().map(Json::Float).map_err(|e| format!("invalid number '{num_str}' at byte offset {start}: {e}")),
        }
    }
}

fn validate_node(
    root: &Json,
    schema: &Json,
    doc: &Json,
    path: &str,
    errors: &mut Vec<String>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        errors.push(format!("{}: recursion depth exceeded", path));
        return;
    }
    match schema {
        Json::Bool(true) => {}
        Json::Bool(false) => {
            errors.push(format!("{}: schema is false, rejects everything", path));
        }
        Json::Obj(schema_obj) => {
            for (key, value) in schema_obj {
                match key.as_str() {
                    "$schema" | "$id" | "title" | "description" | "$comment" | "$defs" => {}
                    "type" => {
                        if let Some(type_name) = get_type_name(value) {
                            if !matches_type(type_name, doc) {
                                errors.push(format!(
                                    "{}: expected type '{}' but got '{}'",
                                    path,
                                    type_name,
                                    json_type_name(doc)
                                ));
                            }
                        } else if let Json::Arr(types) = value {
                            let mut any_match = false;
                            let mut failed_types = Vec::new();
                            for t in types {
                                if let Some(type_name) = get_type_name(t) {
                                    if matches_type(type_name, doc) {
                                        any_match = true;
                                        break;
                                    } else {
                                        failed_types.push(type_name);
                                    }
                                }
                            }
                            if !any_match {
                                let type_list = failed_types.join(", ");
                                errors.push(format!(
                                    "{}: expected type one of [{}] but got '{}'",
                                    path,
                                    type_list,
                                    json_type_name(doc)
                                ));
                            }
                        }
                    }
                    "const" => {
                        if !deep_eq(value, doc) {
                            errors.push(format!(
                                "{}: expected const '{}' but got '{}'",
                                path,
                                json_to_string(value),
                                json_to_string(doc)
                            ));
                        }
                    }
                    "enum" => {
                        if let Json::Arr(enum_values) = value {
                            let mut found = false;
                            for v in enum_values {
                                if deep_eq(v, doc) {
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                errors.push(format!(
                                    "{}: value '{}' not in enum",
                                    path,
                                    json_to_string(doc)
                                ));
                            }
                        }
                    }
                    "required" => {
                        if let Json::Arr(required_keys) = value
                            && let Json::Obj(doc_obj) = doc {
                                let doc_keys: BTreeSet<&str> =
                                    doc_obj.iter().map(|(k, _)| k.as_str()).collect();
                                for rk in required_keys {
                                    if let Json::Str(key) = rk
                                        && !doc_keys.contains(key.as_str()) {
                                            errors.push(format!(
                                                "{}: missing required property '{}'",
                                                path, key
                                            ));
                                        }
                                }
                            }
                    }
                    "properties" => {
                        if let Json::Obj(props) = value
                            && let Json::Obj(doc_obj) = doc {
                                for (prop_name, prop_schema) in props {
                                    if let Some((_, prop_value)) =
                                        doc_obj.iter().find(|(k, _)| k == prop_name)
                                    {
                                        let prop_path = format!("{}/{}", path, prop_name);
                                        validate_node(
                                            root,
                                            prop_schema,
                                            prop_value,
                                            &prop_path,
                                            errors,
                                            depth + 1,
                                        );
                                    }
                                }
                            }
                    }
                    "additionalProperties" => {
                        if let Json::Obj(doc_obj) = doc {
                            let prop_names: BTreeSet<&str> = if let Some(Json::Obj(props)) =
                                schema_obj
                                    .iter()
                                    .find(|(k, _)| k == "properties")
                                    .map(|(_, v)| v)
                            {
                                props.iter().map(|(k, _)| k.as_str()).collect()
                            } else {
                                BTreeSet::new()
                            };
                            for (key, val) in doc_obj {
                                if !prop_names.contains(key.as_str()) {
                                    match value {
                                        Json::Bool(false) => {
                                            errors.push(format!(
                                                "{}: additional property '{}' not allowed",
                                                path, key
                                            ));
                                        }
                                        Json::Bool(true) => {}
                                        _ => {
                                            let prop_path = format!("{}/{}", path, key);
                                            validate_node(
                                                root,
                                                value,
                                                val,
                                                &prop_path,
                                                errors,
                                                depth + 1,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "items" => {
                        if let Json::Arr(doc_arr) = doc {
                            for (i, item) in doc_arr.iter().enumerate() {
                                let item_path = format!("{}/{}", path, i);
                                validate_node(root, value, item, &item_path, errors, depth + 1);
                            }
                        }
                    }
                    "minItems" => {
                        if let Json::Int(min) = value
                            && let Json::Arr(doc_arr) = doc
                                && (doc_arr.len() as i64) < *min {
                                    errors.push(format!(
                                        "{}: array has {} items, minimum is {}",
                                        path,
                                        doc_arr.len(),
                                        min
                                    ));
                                }
                    }
                    "minimum" => {
                        match value {
                            Json::Int(min) => {
                                match doc {
                                    Json::Int(v) => {
                                        if *v < *min {
                                            errors.push(format!(
                                                "{}: value {} is less than minimum {}",
                                                path, v, min
                                            ));
                                        }
                                    }
                                    Json::Float(v)
                                        if *v < *min as f64 => {
                                            errors.push(format!(
                                                "{}: value {} is less than minimum {}",
                                                path, v, min
                                            ));
                                        }
                                    _ => {}
                                }
                            }
                            Json::Float(min) => {
                                match doc {
                                    Json::Int(v) => {
                                        if (*v as f64) < *min {
                                            errors.push(format!(
                                                "{}: value {} is less than minimum {}",
                                                path, v, min
                                            ));
                                        }
                                    }
                                    Json::Float(v)
                                        if *v < *min => {
                                            errors.push(format!(
                                                "{}: value {} is less than minimum {}",
                                                path, v, min
                                            ));
                                        }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    "minLength" => {
                        if let Json::Int(min) = value
                            && let Json::Str(s) = doc {
                                let len = s.chars().count() as i64;
                                if len < *min {
                                    errors.push(format!(
                                        "{}: string length {} is less than minimum {}",
                                        path, len, min
                                    ));
                                }
                            }
                    }
                    "$ref" => {
                        if let Json::Str(ref_str) = value {
                            if let Some(name) = ref_str.strip_prefix("#/$defs/") {
                                if let Some(target) = resolve_ref(root, name) {
                                    validate_node(root, target, doc, path, errors, depth + 1);
                                } else {
                                    errors.push(format!(
                                        "{}: $ref '#/$defs/{}' does not resolve",
                                        path, name
                                    ));
                                }
                            } else {
                                errors.push(format!(
                                    "{}: unsupported $ref format '{}'",
                                    path, ref_str
                                ));
                            }
                        }
                    }
                    other => {
                        errors.push(format!(
                            "{}: unknown schema keyword '{}'",
                            path, other
                        ));
                    }
                }
            }

        }
        _ => {
            errors.push(format!("{}: schema must be an object or boolean", path));
        }
    }
}

fn get_type_name(value: &Json) -> Option<&str> {
    if let Json::Str(s) = value {
        Some(s.as_str())
    } else {
        None
    }
}

fn matches_type(type_name: &str, doc: &Json) -> bool {
    match type_name {
        "object" => matches!(doc, Json::Obj(_)),
        "array" => matches!(doc, Json::Arr(_)),
        "string" => matches!(doc, Json::Str(_)),
        "boolean" => matches!(doc, Json::Bool(_)),
        "null" => matches!(doc, Json::Null),
        "integer" => match doc {
            Json::Int(_) => true,
            Json::Float(f) => f.fract() == 0.0,
            _ => false,
        },
        "number" => matches!(doc, Json::Int(_) | Json::Float(_)),
        _ => false,
    }
}

fn json_type_name(doc: &Json) -> &'static str {
    match doc {
        Json::Null => "null",
        Json::Bool(_) => "boolean",
        Json::Int(_) => "integer",
        Json::Float(_) => "number",
        Json::Str(_) => "string",
        Json::Arr(_) => "array",
        Json::Obj(_) => "object",
    }
}

fn deep_eq(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Null, Json::Null) => true,
        (Json::Bool(a), Json::Bool(b)) => a == b,
        (Json::Int(a), Json::Int(b)) => a == b,
        (Json::Int(a), Json::Float(b)) => *a as f64 == *b,
        (Json::Float(a), Json::Int(b)) => *a == *b as f64,
        (Json::Float(a), Json::Float(b)) => a == b,
        (Json::Str(a), Json::Str(b)) => a == b,
        (Json::Arr(a), Json::Arr(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| deep_eq(x, y))
        }
        (Json::Obj(a), Json::Obj(b)) => {
            a.len() == b.len()
                && a.iter().all(|(k, v)| {
                    b.iter()
                        .find(|(bk, _)| bk == k)
                        .map(|(_, bv)| deep_eq(v, bv))
                        .unwrap_or(false)
                })
        }
        _ => false,
    }
}

fn json_to_string(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Int(i) => i.to_string(),
        Json::Float(f) => f.to_string(),
        Json::Str(s) => format!("\"{}\"", s),
        Json::Arr(items) => {
            let inner: Vec<String> = items.iter().map(json_to_string).collect();
            format!("[{}]", inner.join(", "))
        }
        Json::Obj(pairs) => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, json_to_string(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

fn resolve_ref<'a>(root_schema: &'a Json, name: &str) -> Option<&'a Json> {
    if let Json::Obj(obj) = root_schema
        && let Some((_, defs)) = obj.iter().find(|(k, _)| k == "$defs")
            && let Json::Obj(defs_obj) = defs {
                return defs_obj.iter().find(|(k, _)| k == name).map(|(_, v)| v);
            }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_parses() {
        assert_eq!(parse("null").unwrap(), Json::Null);
    }

    #[test]
    fn true_and_false_parse() {
        assert_eq!(parse("true").unwrap(), Json::Bool(true));
        assert_eq!(parse("false").unwrap(), Json::Bool(false));
    }

    #[test]
    fn a_number_without_fraction_or_exponent_is_an_integer() {
        assert_eq!(parse("42").unwrap(), Json::Int(42));
        assert_eq!(parse("-42").unwrap(), Json::Int(-42));
        assert_eq!(parse("0").unwrap(), Json::Int(0));
    }

    #[test]
    fn a_fraction_or_an_exponent_makes_a_float() {
        assert_eq!(parse("42.5").unwrap(), Json::Float(42.5));
        assert_eq!(parse("1e10").unwrap(), Json::Float(1e10));
        assert_eq!(parse("-1.5e-3").unwrap(), Json::Float(-0.0015));
    }

    #[test]
    fn every_string_escape_decodes_surrogate_pairs_included() {
        assert_eq!(parse("\"hello\"").unwrap(), Json::Str("hello".to_string()));
        assert_eq!(parse("\"\\\"\"").unwrap(), Json::Str("\"".to_string()));
        assert_eq!(parse("\"\\\\\"").unwrap(), Json::Str("\\".to_string()));
        assert_eq!(parse("\"\\/\"").unwrap(), Json::Str("/".to_string()));
        assert_eq!(parse("\"\\b\"").unwrap(), Json::Str("\u{0008}".to_string()));
        assert_eq!(parse("\"\\f\"").unwrap(), Json::Str("\u{000C}".to_string()));
        assert_eq!(parse("\"\\n\"").unwrap(), Json::Str("\n".to_string()));
        assert_eq!(parse("\"\\r\"").unwrap(), Json::Str("\r".to_string()));
        assert_eq!(parse("\"\\t\"").unwrap(), Json::Str("\t".to_string()));
        assert_eq!(parse("\"\\u0041\"").unwrap(), Json::Str("A".to_string()));
        assert_eq!(parse("\"\\uD83D\\uDE00\"").unwrap(), Json::Str("😀".to_string()));
    }

    #[test]
    fn an_array_keeps_its_order() {
        assert_eq!(parse("[]").unwrap(), Json::Arr(vec![]));
        assert_eq!(
            parse("[1, \"two\", null]").unwrap(),
            Json::Arr(vec![Json::Int(1), Json::Str("two".to_string()), Json::Null])
        );
    }

    #[test]
    fn an_object_keeps_its_key_order() {
        assert_eq!(parse("{}").unwrap(), Json::Obj(vec![]));
        let obj = parse(r#"{"a": 1, "b": "two"}"#).unwrap();
        assert_eq!(
            obj,
            Json::Obj(vec![
                ("a".to_string(), Json::Int(1)),
                ("b".to_string(), Json::Str("two".to_string()))
            ])
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse("").is_err());
        assert!(parse("tru").is_err());
        assert!(parse("01").is_err());
        assert!(parse("-").is_err());
        assert!(parse(".5").is_err());
        assert!(parse("1.").is_err());
        assert!(parse("1 2").is_err());
        assert!(parse("{\"a\": 1, \"a\": 2}").is_err());
        assert!(parse("\"\\uD800\"").is_err());
        assert!(parse("\"\\uDC00\"").is_err());
    }

    #[test]
    fn a_conforming_document_has_no_violations() {
        let schema = parse(r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#)
            .unwrap();
        let doc = parse(r#"{"name": "test"}"#).unwrap();
        assert!(validate(&schema, &doc).is_empty());
    }

    #[test]
    fn an_undescribed_property_is_refused_where_additional_properties_is_false() {
        let schema = parse(r#"{"type": "object", "additionalProperties": false}"#).unwrap();
        let doc = parse(r#"{"extra": 1}"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("additional property")));
    }

    #[test]
    fn a_missing_required_property_is_named() {
        let schema = parse(r#"{"type": "object", "required": ["name"]}"#).unwrap();
        let doc = parse(r#"{}"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("missing required property 'name'")));
    }

    #[test]
    fn a_value_outside_enum_is_refused() {
        let schema = parse(r#"{"enum": ["a", "b"]}"#).unwrap();
        let doc = parse(r#""c""#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("not in enum")));
    }

    #[test]
    fn a_value_other_than_const_is_refused() {
        let schema = parse(r#"{"const": 42}"#).unwrap();
        let doc = parse(r#"43"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("expected const")));
    }

    #[test]
    fn a_fraction_is_not_an_integer() {
        let schema = parse(r#"{"type": "integer"}"#).unwrap();
        let doc = parse(r#"1.5"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("expected type 'integer'")));
    }

    #[test]
    fn a_value_below_minimum_is_refused() {
        let schema = parse(r#"{"minimum": 10}"#).unwrap();
        let doc = parse(r#"5"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("less than minimum")));
    }

    #[test]
    fn a_nested_ref_resolves_against_the_root_schema_defs() {
        let schema = parse(
            r##"{"$defs": {"name": {"type": "string"}}, "type": "object", "properties": {"name": {"$ref": "#/$defs/name"}}}"##,
        )
        .unwrap();
        let doc = parse(r#"{"name": "test"}"#).unwrap();
        assert!(validate(&schema, &doc).is_empty());

        let bad_doc = parse(r#"{"name": 123}"#).unwrap();
        let errors = validate(&schema, &bad_doc);
        assert!(errors.iter().any(|e| e.contains("/name")));
    }

    #[test]
    fn a_keyword_the_validator_does_not_check_is_reported() {
        let schema = parse(r#"{"unknownKeyword": true}"#).unwrap();
        let doc = parse(r#"{}"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors.iter().any(|e| e.contains("unknown schema keyword 'unknownKeyword'")));
    }

    #[test]
    fn a_violation_is_placed_by_its_json_pointer() {
        let schema = parse(
            r#"{"type": "object", "properties": {"runs": {"type": "array", "items": {"type": "object", "properties": {"abend": {"type": "object", "properties": {"optimized": {"type": "boolean"}}}}}}}}"#,
        )
        .unwrap();
        let doc = parse(r#"{"runs": [{"abend": {"optimized": "not a bool"}}]}"#).unwrap();
        let errors = validate(&schema, &doc);
        assert!(errors
            .iter()
            .any(|e| e.starts_with("/runs/0/abend/optimized")));
    }

    const MANIFEST: &str = r#"{"clock":"2026-01-01T00:00:00","counts":{"abend":1,"clean":0,"refused":0,"runs":1,"timeout":0},"entry":"run","format":"ironwork-fuzz/v1","inputs":[{"bytes":"","id":"r0-INDD","kind":"dd","minimized":true,"name":"INDD"}],"program":{"file":"A.cbl","id":"A"},"roots":[".",null],"runs":[{"abend":{"code":"S0C7","file":"A.cbl","line":3,"message":"Data exception","optimized":false},"coverage":"coverage/0.json","input":["r0-INDD"],"journal":"20261003T000000Z-0000000000000000","outcome":"abend"}],"seed":1,"strategy":"fields","tool":"ironwork-fuzz","version":"0.3.0"}"#;

    fn run_definition(schema: &str) -> Json {
        let Ok(Json::Obj(top)) = parse(schema) else { panic!("a schema is an object") };
        let defs = top.into_iter().find(|(k, _)| k == "$defs").map(|(_, v)| v);
        let Some(Json::Obj(defs)) = defs else { panic!("the schema has $defs") };
        defs.into_iter().find(|(k, _)| k == "run").map(|(_, v)| v).expect("$defs.run")
    }

    #[test]
    fn an_interface_run_s_kept_runs_are_described_as_a_main_program_s_are() {
        assert_eq!(run_definition(INTERFACE_SCHEMA), run_definition(MANIFEST_SCHEMA));
    }

    #[test]
    fn the_manifest_schema_reads_an_added_key_and_refuses_a_missing_one_and_another_format() {
        assert_eq!(manifest_violations(MANIFEST), Vec::<String>::new());
        let added = MANIFEST.replace(r#""optimized":false"#, r#""optimized":false,"limit":10"#);
        assert_eq!(manifest_violations(&added), Vec::<String>::new());
        let strict = closed(parse(MANIFEST_SCHEMA).unwrap());
        assert_eq!(validate(&strict, &parse(&added).unwrap()), ["/runs/0/abend: additional property 'limit' not allowed"]);
        let without = MANIFEST.replace(r#""format":"ironwork-fuzz/v1","#, "");
        assert_eq!(manifest_violations(&without), [": missing required property 'format'"]);
        let other = MANIFEST.replace("ironwork-fuzz/v1", "ironwork-fuzz/v2");
        assert_eq!(manifest_violations(&other).len(), 1);
        let kind = MANIFEST.replace(r#""kind":"dd""#, r#""kind":"socket""#);
        assert_eq!(manifest_violations(&kind), ["/inputs/0/kind: value '\"socket\"' not in enum"]);
    }

    #[test]
    fn true_accepts_and_false_rejects_anything() {
        let true_schema = Json::Bool(true);
        let doc = Json::Int(42);
        assert!(validate(&true_schema, &doc).is_empty());

        let false_schema = Json::Bool(false);
        let errors = validate(&false_schema, &doc);
        assert!(!errors.is_empty());
    }
}
