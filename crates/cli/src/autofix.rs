//! `--autofix DIR`, and `--remediate DIR` under `--compliance loose`: the source and its COPY members
//! repaired where a message has exactly one sensible fix, compiled again until none is left. DIR
//! receives each repaired file under its own name, `autofix.diff` and `autofix.json`; a fix that
//! would guess at meaning is listed, not made (docs/autofix.md).

use exec::evidence::{Value, canonical, fields};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Rounds of compiling and fixing before autofix stops, whatever is left.
const ROUNDS: usize = 25;
const AREA_A: usize = 7;
const TEXT_END: usize = 72;
const PERIOD_LINE: &str = "           .";

/// One repair, at the position the message gave in the round that made it.
pub struct Fix {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub id: &'static str,
    pub what: String,
}

pub struct Repaired {
    pub text: String,
    pub fixes: Vec<Fix>,
}

enum Edit {
    ExpandTabs,
    InsertLine { before: usize, text: &'static str },
    Blank { line: usize, col: usize },
    ToAreaA { line: usize, col: usize },
    Replace { line: usize, col: usize, len: usize, with: String },
}

/// Repairs `text`, the program `path` names, and its members, compiling under `flags` with the
/// libraries in `dirs`; writes DIR's files and gives the repaired text.
pub fn repair(path: &str, text: &str, dirs: &[PathBuf], flags: &[String], out: &Path) -> Result<Repaired, String> {
    fs::create_dir_all(out).map_err(|e| format!("--autofix {}: {e}", out.display()))?;
    let mut main: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut members: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    let mut fixes: Vec<Fix> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let libraries = || syntax::copy::Libraries::new(std::iter::once(out.to_path_buf()).chain(dirs.iter().cloned()).collect()).with_program(Path::new(path)).with_flags(flags);
    for _ in 0..ROUNDS {
        let current = main.join("\n") + "\n";
        let messages = compile(&current, &libraries(), flags);
        let mut edits: BTreeMap<Option<String>, Vec<(Edit, Fix)>> = BTreeMap::new();
        for m in &messages {
            let lines = match &m.file {
                None => &main,
                Some(f) => &members.entry(f.clone()).or_insert_with(|| original(f)).1,
            };
            let Some((edit, what)) = fix_for(m, lines) else { continue };
            let key = (m.file.clone(), m.pos.line, m.pos.col, m.id);
            if !seen.insert(key) {
                continue;
            }
            let fix = Fix { file: m.file.clone().unwrap_or_else(|| path.to_owned()), line: m.pos.line, col: m.pos.col, id: m.id.unwrap_or_default(), what };
            edits.entry(m.file.clone()).or_default().push((edit, fix));
        }
        if edits.is_empty() {
            break;
        }
        for (file, mut list) in edits {
            // Tabs first and alone: the other positions in the file are counted after them.
            if list.iter().any(|(e, _)| matches!(e, Edit::ExpandTabs)) {
                list.retain(|(e, _)| matches!(e, Edit::ExpandTabs));
                list.truncate(1);
            }
            list.sort_by_key(|(e, _)| std::cmp::Reverse(at(e)));
            let lines = match &file {
                None => &mut main,
                Some(f) => &mut members.get_mut(f).expect("a member read this round").1,
            };
            for (edit, fix) in list {
                if apply(&edit, lines) {
                    fixes.push(fix);
                }
            }
            if let Some(f) = &file {
                write(&out.join(file_name(f)), &members[f].1)?;
            }
        }
    }
    let original_main: Vec<String> = text.lines().map(str::to_owned).collect();
    write(&out.join(file_name(path)), &main)?;
    let mut diff = unified(path, &original_main, &main);
    for (f, (before, after)) in &members {
        diff.push_str(&unified(f, before, after));
    }
    fs::write(out.join("autofix.diff"), diff).map_err(|e| format!("--autofix {}: {e}", out.display()))?;
    fixes.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));
    let text = main.join("\n") + "\n";
    let left = compile(&text, &libraries(), flags);
    fs::write(out.join("autofix.json"), report(path, &fixes, &left) + "\n").map_err(|e| format!("--autofix {}: {e}", out.display()))?;
    Ok(Repaired { text, fixes })
}

/// The messages compiling `text` gives: a refusal the parser stops at, or each program's.
fn compile(text: &str, libraries: &syntax::copy::Libraries, flags: &[String]) -> Vec<syntax::Error> {
    match syntax::parse_all_with(text, libraries) {
        Err(e) => vec![e],
        Ok(mut programs) => {
            let first = programs.remove(0);
            programs.into_iter().filter(|p| p.function.is_some()).chain([first]).flat_map(|p| exec::compile(p, flags).map_or_else(|m| m, |c| c.diagnostics)).collect()
        }
    }
}

fn original(member: &str) -> (Vec<String>, Vec<String>) {
    let lines: Vec<String> = fs::read(member).map(|b| syntax::copy::decode(&b)).unwrap_or_default().lines().map(str::to_owned).collect();
    (lines.clone(), lines)
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_owned())
}

fn write(to: &Path, lines: &[String]) -> Result<(), String> {
    fs::write(to, lines.join("\n") + "\n").map_err(|e| format!("--autofix {}: {e}", to.display()))
}

/// The edit a message calls for and what it does, where there is exactly one.
fn fix_for(m: &syntax::Error, lines: &[String]) -> Option<(Edit, String)> {
    let line = (m.pos.line as usize).checked_sub(1)?;
    let col = (m.pos.col as usize).checked_sub(1)?;
    let text: Vec<char> = lines.get(line)?.chars().collect();
    let starts_line = text.iter().take(col).all(|c| *c == ' ');
    Some(match m.id? {
        "IWX0058" => (Edit::ExpandTabs, "expanded each tab to the next column after a multiple of 8".into()),
        "IWX0001" if ["and the file reads in free form", "column 7 holds", "a literal runs past column 72"].iter().any(|w| m.message.contains(w)) => {
            (Edit::InsertLine { before: 0, text: ">>SOURCE FORMAT FREE" }, "added >>SOURCE FORMAT FREE as the first line".into())
        }
        "IWS0105" if starts_line => (Edit::InsertLine { before: line, text: PERIOD_LINE }, "put the period the compiler assumed on a line of its own".into()),
        "IWS0001" if m.message.starts_with("expected a period after the PROCEDURE DIVISION header") && starts_line => {
            (Edit::InsertLine { before: line, text: PERIOD_LINE }, "ended the PROCEDURE DIVISION header with a period".into())
        }
        "IWS0104" => (Edit::Blank { line, col }, format!("removed the scope terminator {}, which no verb took", word_at(&text, col))),
        "IWX0061" if col > AREA_A && text.iter().skip(AREA_A).take(col - AREA_A).all(|c| *c == ' ') => (Edit::ToAreaA { line, col }, format!("moved the paragraph header {} into Area A", word_at(&text, col))),
        "IWX0063" | "IWS0106" => {
            let quote = *text.get(col)?;
            let room = text.len() < TEXT_END || text.iter().skip(TEXT_END - 1).all(|c| *c == ' ');
            if !room {
                return None;
            }
            (Edit::Replace { line, col, len: 2, with: format!("{quote} {quote}") }, format!("wrote the zero-length literal as {quote} {quote}, the space read for it"))
        }
        _ => return None,
    })
}

fn word_at(text: &[char], col: usize) -> String {
    text.iter().skip(col).take_while(|c| c.is_alphanumeric() || **c == '-').collect()
}

/// Where an edit falls, so a file's edits apply from its end and leave earlier positions in place.
fn at(edit: &Edit) -> (usize, usize) {
    match edit {
        Edit::ExpandTabs => (usize::MAX, 0),
        Edit::InsertLine { before, .. } => (*before, 0),
        Edit::Blank { line, col } | Edit::ToAreaA { line, col } | Edit::Replace { line, col, .. } => (*line, *col),
    }
}

fn apply(edit: &Edit, lines: &mut Vec<String>) -> bool {
    match edit {
        Edit::ExpandTabs => {
            for l in lines.iter_mut() {
                *l = syntax::source::expand_tabs(l).into_iter().collect();
            }
        }
        Edit::InsertLine { before, text } => lines.insert((*before).min(lines.len()), (*text).to_owned()),
        Edit::Blank { line, col } => {
            let mut chars: Vec<char> = lines[*line].chars().collect();
            let end = *col + word_at(&chars, *col).chars().count();
            chars[*col..end].iter_mut().for_each(|c| *c = ' ');
            lines[*line] = chars.into_iter().collect::<String>().trim_end().to_owned();
        }
        Edit::ToAreaA { line, col } => {
            let chars: Vec<char> = lines[*line].chars().collect();
            lines[*line] = chars[..AREA_A].iter().chain(&chars[*col..]).collect();
        }
        Edit::Replace { line, col, len, with } => {
            let chars: Vec<char> = lines[*line].chars().collect();
            lines[*line] = chars[..*col].iter().copied().chain(with.chars()).chain(chars[col + len..].iter().copied()).collect::<String>().trim_end().to_owned();
        }
    }
    true
}

/// The fixes made, the holes and left-out constructs relaxed and loose compiled around, and the
/// messages left at severity E or above, as JSON.
fn report(path: &str, fixes: &[Fix], left: &[syntax::Error]) -> String {
    let fixed = fixes.iter().map(|f| Value::Obj(fields([("file", f.file.as_str().into()), ("line", Value::Int(f.line.into())), ("col", Value::Int(f.col.into())), ("id", f.id.into()), ("fix", f.what.as_str().into())]))).collect();
    let listed = |keep: &dyn Fn(&syntax::Error) -> bool| {
        let entry = |m: &syntax::Error| {
            Value::Obj(fields([
                ("file", m.file.clone().unwrap_or_else(|| path.to_owned()).into()),
                ("line", Value::Int(m.pos.line.into())),
                ("col", Value::Int(m.pos.col.into())),
                ("id", m.id.map_or(Value::Null, Value::from)),
                ("message", m.message.as_str().into()),
            ]))
        };
        Value::Arr(left.iter().filter(|m| keep(m)).map(entry).collect())
    };
    let holes = listed(&|m| m.id == Some("IWX0059"));
    let left_out = listed(&|m| matches!(m.id, Some("IWX0064" | "IWX0065" | "IWX0075")));
    let remaining = listed(&|m| m.severity >= syntax::Severity::Error);
    canonical(&Value::Obj(fields([("fixes", Value::Arr(fixed)), ("holes", holes), ("left_out", left_out), ("remaining", remaining)])))
}

/// A unified diff of `before` and `after`, three lines of context, empty when they are the same.
fn unified(path: &str, before: &[String], after: &[String]) -> String {
    let ops = diff(before, after);
    if ops.iter().all(|op| matches!(op, Op::Same(..))) {
        return String::new();
    }
    let name = path.trim_start_matches('/');
    let mut out = format!("--- a/{name}\n+++ b/{name}\n");
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, op)| !matches!(op, Op::Same(..))).map(|(i, _)| i).collect();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(3);
        let mut end = changed[k] + 3;
        while k + 1 < changed.len() && changed[k + 1] <= end + 3 {
            k += 1;
            end = changed[k] + 3;
        }
        let end = end.min(ops.len() - 1);
        let hunk = &ops[start..=end];
        let first_old = ops[..start].iter().filter(|op| !matches!(op, Op::Add(_))).count() + 1;
        let first_new = ops[..start].iter().filter(|op| !matches!(op, Op::Remove(_))).count() + 1;
        let old_len = hunk.iter().filter(|op| !matches!(op, Op::Add(_))).count();
        let new_len = hunk.iter().filter(|op| !matches!(op, Op::Remove(_))).count();
        out.push_str(&format!("@@ -{first_old},{old_len} +{first_new},{new_len} @@\n"));
        for op in hunk {
            match op {
                Op::Same(l) => out.push_str(&format!(" {}\n", before[*l])),
                Op::Remove(l) => out.push_str(&format!("-{}\n", before[*l])),
                Op::Add(l) => out.push_str(&format!("+{}\n", after[*l])),
            }
        }
        k += 1;
    }
    out
}

enum Op {
    Same(usize),
    Remove(usize),
    Add(usize),
}

/// Myers's shortest edit script, as line operations in order.
fn diff(a: &[String], b: &[String]) -> Vec<Op> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let offset = n + m + 1;
    let index = |k: isize| (k + offset) as usize;
    let mut v = vec![0isize; (2 * offset + 1) as usize];
    // The state at the start of each round of edits.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    'search: for d in 0..=n + m {
        trace.push(v.clone());
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && v[index(k - 1)] < v[index(k + 1)]) { v[index(k + 1)] } else { v[index(k - 1)] + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index(k)] = x;
            if x >= n && y >= m {
                break 'search;
            }
        }
    }
    let mut ops = Vec::new();
    let (mut x, mut y) = (n, m);
    for (d, v) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let k = x - y;
        let prev_k = if k == -d || (k != d && v[index(k - 1)] < v[index(k + 1)]) { k + 1 } else { k - 1 };
        let prev_x = v[index(prev_k)];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push(Op::Same(x as usize));
        }
        if d > 0 {
            if x == prev_x {
                y -= 1;
                ops.push(Op::Add(y as usize));
            } else {
                x -= 1;
                ops.push(Op::Remove(x as usize));
            }
        }
    }
    ops.reverse();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_owned).collect()
    }

    #[test]
    fn a_unified_diff_shows_each_change_with_its_context() {
        let before = lines("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\n");
        let after = lines("a\nb\nc\nd\nE\nf\ng\nh\ni\nj\nX\nk\n");
        assert_eq!(unified("p.cbl", &before, &after), "--- a/p.cbl\n+++ b/p.cbl\n@@ -2,10 +2,11 @@\n b\n c\n d\n-e\n+E\n f\n g\n h\n i\n j\n+X\n k\n");
        assert_eq!(unified("p.cbl", &before, &before), "");
        assert_eq!(unified("p.cbl", &[], &lines("x\n")), "--- a/p.cbl\n+++ b/p.cbl\n@@ -1,0 +1,1 @@\n+x\n");
    }
}
