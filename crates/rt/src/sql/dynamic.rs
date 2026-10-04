//! A dynamic statement string as the runtime reads it before any backend sees it: what kind of
//! statement it is, how many parameter markers it holds, and the text a call sends.

/// What a statement string is, as PREPARE and EXECUTE IMMEDIATE decide what may run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A select-statement: a cursor runs it, EXECUTE and EXECUTE IMMEDIATE refuse it (-518).
    Query,
    Commit,
    Rollback,
    /// INSERT, UPDATE or DELETE: a searched one that changes no row is +100, and a positioned one
    /// names its cursor.
    Change { delete: bool, current_of: Option<String> },
    /// SAVEPOINT, RELEASE SAVEPOINT and ROLLBACK TO SAVEPOINT, which ironwork does not run, named.
    Refused(&'static str),
    /// Any other statement, which the backend answers: one Db2 prepares, or words that are no SQL
    /// statement, which the backend refuses as Db2's parser does.
    Other,
    /// An SQL statement Db2 for z/OS does not prepare (-084), or no statement at all.
    Unacceptable,
}

/// The words of `text` outside its quoted strings, upper-cased, with each quoted string as one `'`.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' || c == '"' {
            while let Some(q) = chars.next() {
                if q == c && chars.next_if_eq(&c).is_none() {
                    break;
                }
            }
            out.push("'".into());
        } else if c.is_alphanumeric() || matches!(c, '_' | '#' | '@' | '$') {
            let mut word = c.to_uppercase().collect::<String>();
            while let Some(n) = chars.next_if(|&n| n.is_alphanumeric() || matches!(n, '_' | '#' | '@' | '$' | '-')) {
                word.extend(n.to_uppercase());
            }
            out.push(word);
        } else if !c.is_whitespace() {
            out.push(c.to_string());
        }
    }
    out
}

/// What a statement string is by its first words, against the statements Db2 13 for z/OS prepares
/// (SQL Reference, PREPARE).
pub(super) fn kind(text: &str) -> Kind {
    let words = words(text);
    let word = |i: usize| words.get(i).map_or("", String::as_str);
    let n = words.len();
    match word(0) {
        "SELECT" | "WITH" | "VALUES" | "(" => Kind::Query,
        "COMMIT" if n == 1 || (n == 2 && word(1) == "WORK") => Kind::Commit,
        "ROLLBACK" if n == 1 || (n == 2 && word(1) == "WORK") => Kind::Rollback,
        "ROLLBACK" => Kind::Refused("ROLLBACK TO SAVEPOINT"),
        "SAVEPOINT" => Kind::Refused("SAVEPOINT"),
        "RELEASE" if word(1) == "SAVEPOINT" || (word(1) == "TO" && word(2) == "SAVEPOINT") => Kind::Refused("RELEASE SAVEPOINT"),
        verb @ ("INSERT" | "UPDATE" | "DELETE") => {
            let positioned = n >= 5 && word(n - 4) == "WHERE" && word(n - 3) == "CURRENT" && word(n - 2) == "OF" && word(n - 1) != "'";
            Kind::Change { delete: verb == "DELETE", current_of: positioned.then(|| word(n - 1).to_owned()) }
        }
        "DECLARE" if (word(1), word(2), word(3)) == ("GLOBAL", "TEMPORARY", "TABLE") => Kind::Other,
        "SET" if !matches!(word(1), "CONNECTION" | ":") => Kind::Other,
        "" | "BEGIN" | "CALL" | "CLOSE" | "CONNECT" | "DECLARE" | "DESCRIBE" | "DISCONNECT" | "END" | "EXECUTE" | "FETCH" | "GET" | "INCLUDE" | "OPEN" | "PREPARE" | "RELEASE" | "SET" | "WHENEVER" => {
            Kind::Unacceptable
        }
        _ => Kind::Other,
    }
}

/// The parameter markers in `text`: each `?` outside a quoted string.
pub(super) fn markers(text: &str) -> usize {
    words(text).iter().filter(|w| *w == "?").count()
}

/// The statement's command word, which its call names.
pub(super) fn verb(text: &str) -> String {
    words(text).into_iter().next().filter(|w| w != "'").unwrap_or_default()
}

/// The text a call sends: each run of white space outside a quoted string one space, with none at
/// either end, so a statement built across lines has one spelling in a recording.
pub(super) fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' || c == '"' {
            out.push(c);
            while let Some(q) = chars.next() {
                out.push(q);
                if q == c {
                    match chars.next_if_eq(&c) {
                        Some(again) => out.push(again),
                        None => break,
                    }
                }
            }
        } else if c.is_whitespace() {
            while chars.next_if(|n| n.is_whitespace()).is_some() {}
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_kinds_by_their_first_words() {
        assert_eq!(kind("select a from t where b = ?"), Kind::Query);
        assert_eq!(kind("WITH X AS (SELECT 1 FROM T) SELECT * FROM X"), Kind::Query);
        assert_eq!(kind("(SELECT A FROM T) UNION (SELECT B FROM U)"), Kind::Query);
        assert_eq!(kind("COMMIT WORK"), Kind::Commit);
        assert_eq!(kind("ROLLBACK"), Kind::Rollback);
        assert_eq!(kind("ROLLBACK TO SAVEPOINT A"), Kind::Refused("ROLLBACK TO SAVEPOINT"));
        assert_eq!(kind("SAVEPOINT A ON ROLLBACK RETAIN CURSORS"), Kind::Refused("SAVEPOINT"));
        assert_eq!(kind("RELEASE SAVEPOINT A"), Kind::Refused("RELEASE SAVEPOINT"));
        assert_eq!(kind("UPDATE D01.MAA AS MAA SET X = 1 WHERE K = ?"), Kind::Change { delete: false, current_of: None });
        assert_eq!(kind("DELETE FROM T WHERE CURRENT OF CSR-1"), Kind::Change { delete: true, current_of: Some("CSR-1".into()) });
        assert_eq!(kind("UPDATE T SET A = 'WHERE CURRENT OF C'"), Kind::Change { delete: false, current_of: None });
        assert_eq!(kind("CREATE TABLE T (A INT)"), Kind::Other);
        assert_eq!(kind("DECLARE GLOBAL TEMPORARY TABLE SESSION.T (A INT)"), Kind::Other);
        assert_eq!(kind("SET CURRENT SQLID = 'X'"), Kind::Other);
        assert_eq!(kind("SET CONNECTION LOC1"), Kind::Unacceptable);
        assert_eq!(kind("CONNECT TO LOC1"), Kind::Unacceptable);
        assert_eq!(kind("DECLARE C1 CURSOR FOR S1"), Kind::Unacceptable);
        assert_eq!(kind("CALL PROC1(1)"), Kind::Unacceptable);
        assert_eq!(kind("RELEASE LOC1"), Kind::Unacceptable);
        assert_eq!(kind(""), Kind::Unacceptable);
        assert_eq!(kind("SELEC A FROM T"), Kind::Other);
    }

    #[test]
    fn markers_count_question_marks_outside_strings() {
        assert_eq!(markers("INSERT INTO T VALUES (?, ?, '?', \"?\")"), 2);
        assert_eq!(markers("UPDATE T SET A = 'IT''S ?' WHERE B = ?"), 1);
        assert_eq!(markers("DELETE FROM T"), 0);
    }

    #[test]
    fn normalising_folds_white_space_outside_strings() {
        assert_eq!(normalise("  UPDATE T\n   SET A = 'X   Y'\t WHERE B = 1   "), "UPDATE T SET A = 'X   Y' WHERE B = 1");
        assert_eq!(normalise("SELECT 'A''  B' FROM T"), "SELECT 'A''  B' FROM T");
        assert_eq!(verb("  insert into t values (1)"), "INSERT");
        assert_eq!(verb("'x'"), "");
    }
}
