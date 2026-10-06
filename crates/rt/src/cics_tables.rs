//! IBM's CICS tables, vendored from cobolwork's provenance by tools/sync-cics-tables.sh: each
//! command's options with the direction their data moves, the numbers DFHRESP names, and the
//! numbers DFHVALUE names.

use std::sync::OnceLock;

const COMMANDS: &str = include_str!("../data/cics-commands.tsv");
const DFHRESP: &str = include_str!("../data/dfhresp.tsv");
const DFHVALUE: &str = include_str!("../data/dfhvalue.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Sends,
    Receives,
    Both,
    /// A paragraph or section to branch to, as HANDLE CONDITION takes.
    Label,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandOption {
    /// `*` stands for any condition or AID name, as HANDLE CONDITION and IGNORE CONDITION take.
    pub name: &'static str,
    /// As the reference types it: data-area, data-value, ptr-ref, cvda, name and so on; `-` for none.
    pub argument: &'static str,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    /// The options that tell it from a namesake, as MAP tells SEND MAP from SEND.
    pub identify: Vec<&'static str>,
    /// The namesake meant when none of the identify options is written, as TS is for READQ.
    pub default: bool,
    pub options: Vec<CommandOption>,
    /// The command's CICS TS 6.x topic.
    pub doc: &'static str,
    /// Its page in the CICS TS 5.3 Application Programming Reference (SC34-7402-00).
    pub page: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resp {
    pub condition: &'static str,
    pub value: i32,
    /// The document that gives the value, and its page when it is a book.
    pub source: &'static str,
    pub page: Option<u32>,
}

struct Tables {
    commands: Vec<Command>,
    every: Vec<CommandOption>,
    resp: Vec<Resp>,
    cvda: Vec<(&'static str, i32)>,
}

fn rows(text: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect())
}

fn direction(word: &str) -> Direction {
    match word {
        "sends" => Direction::Sends,
        "receives" => Direction::Receives,
        "both" => Direction::Both,
        "label" => Direction::Label,
        "-" => Direction::None,
        other => panic!("cics-commands.tsv: no direction {other}"),
    }
}

fn number(cell: &str, table: &str) -> i32 {
    cell.parse().unwrap_or_else(|_| panic!("{table}: {cell} is not a number"))
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let (mut commands, mut every) = (Vec::<Command>::new(), Vec::new());
        for row in rows(COMMANDS) {
            let [command, identify, default, option, argument, dir, doc, page] = row[..] else { panic!("cics-commands.tsv: {row:?} is not eight cells") };
            let option = (option != "-").then(|| CommandOption { name: option, argument, direction: direction(dir) });
            if command == "*" {
                every.extend(option);
                continue;
            }
            if commands.last().is_none_or(|c| c.name != command) {
                let identify = if identify == "-" { Vec::new() } else { identify.split(' ').collect() };
                commands.push(Command { name: command, identify, default: default == "yes", options: Vec::new(), doc, page: page.parse().ok() });
            }
            if let Some(c) = commands.last_mut() {
                c.options.extend(option);
            }
        }
        let resp = rows(DFHRESP)
            .map(|row| {
                let [condition, value, source, page] = row[..] else { panic!("dfhresp.tsv: {row:?} is not four cells") };
                Resp { condition, value: number(value, "dfhresp.tsv"), source, page: page.parse().ok() }
            })
            .collect();
        let cvda = rows(DFHVALUE)
            .map(|row| {
                let [name, value, _since, _source] = row[..] else { panic!("dfhvalue.tsv: {row:?} is not four cells") };
                (name, number(value, "dfhvalue.tsv"))
            })
            .collect();
        Tables { commands, every, resp, cvda }
    })
}

/// Every command cobolwork's precompiler translates, in the table's order.
pub fn commands() -> &'static [Command] {
    &tables().commands
}

pub fn command(name: &str) -> Option<&'static Command> {
    commands().iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// The options any command may carry: RESP and RESP2.
pub fn every_command_options() -> &'static [CommandOption] {
    &tables().every
}

/// Every condition DFHRESP names, by value.
pub fn resp_table() -> &'static [Resp] {
    &tables().resp
}

/// The number DFHRESP(condition) stands for.
pub fn resp(condition: &str) -> Option<i32> {
    resp_table().iter().find(|r| r.condition.eq_ignore_ascii_case(condition)).map(|r| r.value)
}

pub fn resp_condition(value: i32) -> Option<&'static str> {
    resp_table().iter().find(|r| r.value == value).map(|r| r.condition)
}

/// The number DFHVALUE(cvda) stands for.
pub fn cvda(name: &str) -> Option<i32> {
    tables().cvda.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_hold_every_row() {
        assert_eq!((commands().len(), resp_table().len(), tables().cvda.len()), (128, 121, 1061));
        assert_eq!(every_command_options().iter().map(|o| o.name).collect::<Vec<_>>(), ["RESP", "RESP2"]);
        assert!(commands().iter().all(|c| c.doc.starts_with("https://www.ibm.com/docs/")), "every command cites IBM");
        // The asynchronous API came after the 5.3 reference the pages are from.
        let unpaged: Vec<&str> = commands().iter().filter(|c| c.page.is_none()).map(|c| c.name).collect();
        assert_eq!(unpaged, ["FETCH ANY", "FETCH CHILD", "FREE CHILD", "RUN TRANSID"]);
    }

    #[test]
    fn conditions_and_cvdas_have_ibms_numbers() {
        let got: Vec<_> = ["NORMAL", "NOTFND", "ROLLEDBACK", "NOTFINISHED", "BUSY"].iter().map(|c| resp(c)).collect();
        assert_eq!(got, [Some(0), Some(13), Some(82), Some(113), Some(128)]);
        assert_eq!((resp("notfnd"), resp("NOSUCH"), resp_condition(82)), (Some(13), None, Some("ROLLEDBACK")));
        let busy = resp_table().last().unwrap();
        assert_eq!((busy.condition, busy.page), ("BUSY", None));
        assert_eq!((cvda("ABEND"), cvda("ALLOCATD"), cvda("ADDRESS")), (Some(900), Some(81), Some(859)), "ADDRESS as CICS TS 5.4 and later give it");
        assert_eq!((cvda("SECERROR"), cvda("NODEJSAPP"), cvda("AWARE"), cvda("NOTAWARE")), (Some(1214), Some(1215), Some(1256), Some(1257)), "5.4, 5.5 and 6.x values; AWARE from 6.x's numeric table");
    }

    #[test]
    fn commands_carry_their_options_and_what_tells_them_apart() {
        let send_map = command("send map").unwrap();
        assert_eq!((send_map.identify.as_slice(), send_map.page), (&["MAP"][..], Some(605)));
        let option = |c: &Command, name: &str| *c.options.iter().find(|o| o.name == name).unwrap();
        assert_eq!(option(send_map, "FROM"), CommandOption { name: "FROM", argument: "data-area", direction: Direction::Sends });
        assert_eq!(option(send_map, "SET").direction, Direction::Receives);
        assert_eq!(command("HANDLE CONDITION").unwrap().options, [CommandOption { name: "*", argument: "label", direction: Direction::Label }]);
        assert_eq!(command("IGNORE CONDITION").unwrap().options[0].direction, Direction::None);
        assert!(command("SYNCPOINT").unwrap().options.is_empty());
        assert!(command("READQ TS").unwrap().default && !command("READQ TD").unwrap().default);
    }

    /// Run with IRONWORK_COBOLWORK_DIR naming a cobolwork checkout to check the vendored copies.
    #[test]
    fn the_vendored_tables_are_cobolworks() {
        let Ok(dir) = std::env::var("IRONWORK_COBOLWORK_DIR") else { return };
        for (table, ours) in [("cics-commands", COMMANDS), ("dfhresp", DFHRESP), ("dfhvalue", DFHVALUE)] {
            let theirs = std::fs::read_to_string(std::path::Path::new(&dir).join(format!("provenance/{table}.tsv"))).expect("cobolwork's table");
            assert!(theirs.replace('\r', "") == ours.replace('\r', ""), "crates/rt/data/{table}.tsv differs from cobolwork's: run tools/sync-cics-tables.sh");
        }
    }
}
