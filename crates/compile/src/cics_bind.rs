//! An EXEC CICS block as the `CicsCommand` `rt::cics` runs: which command it is, found from the
//! command words and options as the translator gives them, and each option it reads, over the
//! walker's references. HANDLE CONDITION and HANDLE ABEND labels are resolved here.

use rt::abend::Abend;
use rt::cics::{Assign, Cics, CicsCommand, Condition, Control, Datum, FileControl, FileOptions, Opt, Record, Resp, Sink, Transfer};
use rt::lir::ParaId;
use syntax::ast::{ExecArg, ExecBlock, Operand, ProcName, Ref};

type R<T> = Result<T, Abend>;

pub type Command<'b> = CicsCommand<&'b Ref, &'b Operand, &'b str>;
type Arg<'b> = Opt<&'b Ref, &'b Operand, &'b str>;

const FILE_CONTROL: &[(&str, FileControl)] = &[
    ("READ", FileControl::Read),
    ("WRITE", FileControl::Write),
    ("REWRITE", FileControl::Rewrite),
    ("DELETE", FileControl::Delete),
    ("UNLOCK", FileControl::Unlock),
    ("STARTBR", FileControl::Startbr),
    ("READNEXT", FileControl::Readnext),
    ("READPREV", FileControl::Readprev),
    ("RESETBR", FileControl::Resetbr),
    ("ENDBR", FileControl::Endbr),
];

/// Options that are not conditions in HANDLE CONDITION and IGNORE CONDITION.
const NOT_CONDITIONS: &[&str] = &["RESP", "RESP2", "NOHANDLE"];

/// FORMATTIME options that are not outputs.
const FORMAT_CONTROLS: &[&str] = &["ABSTIME", "DATESEP", "TIMESEP", "RESP", "RESP2", "NOHANDLE"];

fn option<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Option<ExecArg>> {
    block.options.iter().find(|(n, _)| n == name).map(|(_, a)| a)
}

fn has(block: &ExecBlock, name: &str) -> bool {
    option(block, name).is_some()
}

fn operand<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Operand> {
    match option(block, name) {
        Some(Some(ExecArg::Operand(op))) => Some(op),
        _ => None,
    }
}

fn datum(arg: &Option<ExecArg>) -> Datum<&Ref, &Operand, &str> {
    match arg {
        None => Datum::Bare,
        Some(ExecArg::Operand(Operand::Ref(r))) => Datum::Place(r),
        Some(ExecArg::Operand(op)) => Datum::Value(op),
        Some(ExecArg::Text(t)) => Datum::Text(t),
    }
}

/// The first option of the name, as written.
fn arg<'b>(block: &'b ExecBlock, name: &str) -> Arg<'b> {
    option(block, name).map(datum)
}

/// The first of alternative options (FILE or DATASET, QUEUE or QNAME) that has an argument.
fn first<'b>(block: &'b ExecBlock, names: &[&str]) -> Arg<'b> {
    names.iter().map(|n| arg(block, n)).find(|d| matches!(d, Some(Datum::Place(_) | Datum::Value(_) | Datum::Text(_)))).flatten()
}

fn resp(block: &ExecBlock) -> Resp<&Ref, &Operand, &str> {
    Resp { resp: arg(block, "RESP"), resp2: arg(block, "RESP2"), nohandle: has(block, "NOHANDLE") }
}

fn record(block: &ExecBlock) -> Record<&Ref, &Operand, &str> {
    Record { into: arg(block, "INTO"), set: arg(block, "SET"), length: arg(block, "LENGTH") }
}

fn transfer(block: &ExecBlock) -> Transfer<&Ref, &Operand, &str> {
    Transfer { program: arg(block, "PROGRAM"), commarea: arg(block, "COMMAREA"), length: arg(block, "LENGTH") }
}

fn control(block: &ExecBlock) -> Control {
    Control { erase: has(block, "ERASE"), freekb: has(block, "FREEKB"), alarm: has(block, "ALARM"), frset: has(block, "FRSET") }
}

fn file_options(block: &ExecBlock) -> FileOptions<&Ref, &Operand, &str> {
    FileOptions {
        ridfld: arg(block, "RIDFLD"),
        keylength: arg(block, "KEYLENGTH"),
        reqid: arg(block, "REQID"),
        from: arg(block, "FROM"),
        numrec: arg(block, "NUMREC"),
        record: record(block),
        generic: has(block, "GENERIC"),
        rrn: has(block, "RRN"),
        gteq: has(block, "GTEQ"),
        equal: has(block, "EQUAL"),
        update: has(block, "UPDATE"),
    }
}

/// The options of HANDLE CONDITION and IGNORE CONDITION that may name conditions.
fn condition_options(block: &ExecBlock) -> impl Iterator<Item = &(String, Option<ExecArg>)> {
    block.options.iter().filter(|(n, _)| !NOT_CONDITIONS.contains(&n.as_str()))
}

/// The command an EXEC CICS block writes, dispatched as the translator's words and options say.
/// `label` finds the paragraph a HANDLE label names.
pub fn bind<'b>(block: &'b ExecBlock, label: &dyn Fn(&str) -> R<ParaId>) -> R<Command<'b>> {
    let a = |name: &str| arg(block, name);
    let file_verb = FILE_CONTROL.iter().find(|(name, _)| *name == block.command).map(|&(_, verb)| verb);
    let command = match (block.command.as_str(), file_verb) {
        ("WRITE", _) if has(block, "OPERATOR") => Cics::WriteOperator { text: a("TEXT"), textlength: a("TEXTLENGTH") },
        (_, Some(verb)) => Cics::File { verb, file: first(block, &["FILE", "DATASET"]), options: file_options(block) },
        (command, None) => bind_other(block, command, label)?,
    };
    Ok(CicsCommand { name: block.command.as_str(), command, resp: resp(block), sinks: sinks(block) })
}

/// The data items of a block's options an input could steer, as an observer is told them before
/// the command runs, so a command ironwork does not carry out yet is still traced. Only a data
/// item is: a literal operand is the program's own choice.
pub fn sinks(block: &ExecBlock) -> Vec<(&Ref, Sink)> {
    let command = block.command.as_str();
    let queue = matches!(command, "WRITEQ" | "READQ" | "DELETEQ") || command.starts_with("WRITEQ ") || command.starts_with("READQ ") || command.starts_with("DELETEQ ");
    let mut options: Vec<(&str, Sink)> = Vec::new();
    match command {
        "LINK" | "XCTL" => options.push(("PROGRAM", Sink::DynamicTransfer)),
        "START" | "START TRANSID" => options.push(("TRANSID", Sink::DynamicTransfer)),
        "READ" | "STARTBR" | "RESETBR" => options.push(("RIDFLD", Sink::RecordKey)),
        "DELETE" => options.push(("RIDFLD", Sink::RecordUpdate)),
        "WRITEQ TD" => options.push(("FROM", Sink::Log)),
        "WRITE" if has(block, "OPERATOR") => options.push(("TEXT", Sink::Log)),
        "WRITE" if has(block, "JOURNALNAME") || has(block, "JOURNALNUM") => options.push(("FROM", Sink::Log)),
        "SEND TEXT" | "SEND MAP" | "SEND" => options.push(("FROM", Sink::Screen)),
        "WEB SEND" => options.push(("FROM", Sink::WebResponse)),
        "WEB WRITE" => options.push(("VALUE", Sink::HttpHeader)),
        "WEB OPEN" => options.extend([("HOST", Sink::OutboundHost), ("URL", Sink::OutboundHost)]),
        "WEB CONVERSE" => options.extend([("PATH", Sink::OutboundHost), ("FROM", Sink::OutboundHttp)]),
        _ => {}
    }
    if queue {
        options.extend([("QUEUE", Sink::QueueName), ("QNAME", Sink::QueueName)]);
    }
    options.push(("SYSID", Sink::Sysid));
    options
        .into_iter()
        .filter_map(|(option, sink)| match operand(block, option) {
            Some(Operand::Ref(r)) => Some((r, sink)),
            _ => None,
        })
        .collect()
}

fn bind_other<'b>(block: &'b ExecBlock, command: &str, label: &dyn Fn(&str) -> R<ParaId>) -> R<Cics<&'b Ref, &'b Operand, &'b str>> {
    let a = |name: &str| arg(block, name);
    let queue = || first(block, &["QUEUE", "QNAME"]);
    let map_command = |two_word: &str| has(block, "MAP") || block.command == two_word;
    Ok(match command {
        "RETURN" => Cics::Return { transid: a("TRANSID"), commarea: a("COMMAREA"), length: a("LENGTH"), channel: a("CHANNEL"), immediate: has(block, "IMMEDIATE") },
        "LINK" => Cics::Link(transfer(block)),
        "XCTL" => Cics::Xctl(transfer(block)),
        "ABEND" => Cics::Abend { abcode: a("ABCODE"), cancel: has(block, "CANCEL") },
        "HANDLE CONDITION" => {
            let mut labels = Vec::new();
            for (name, arg) in condition_options(block) {
                let label = match arg {
                    Some(ExecArg::Text(t)) => Some(label(t)?),
                    _ => None,
                };
                if let Some(condition) = Condition::from_name(name) {
                    labels.push((condition, label));
                }
            }
            Cics::HandleCondition(labels)
        }
        "IGNORE CONDITION" => Cics::IgnoreCondition(condition_options(block).filter_map(|(name, _)| Condition::from_name(name)).collect()),
        "PUSH HANDLE" => Cics::PushHandle,
        "POP HANDLE" => Cics::PopHandle,
        "HANDLE ABEND" => {
            if ["PROGRAM", "LABEL", "CANCEL", "RESET"].iter().filter(|o| has(block, o)).count() > 1 {
                return Err(Abend::ironwork("EXEC CICS HANDLE ABEND takes one of PROGRAM, LABEL, CANCEL and RESET", block.pos));
            }
            let label = match option(block, "LABEL") {
                Some(Some(ExecArg::Text(t))) => Some(label(t)?),
                _ => None,
            };
            Cics::HandleAbend { program: arg(block, "PROGRAM"), label, reset: has(block, "RESET") }
        }
        "HANDLE AID" => Cics::HandleAid,
        "SEND" | "SEND MAP" if map_command("SEND MAP") => Cics::SendMap {
            map: a("MAP"),
            mapset: a("MAPSET"),
            from: a("FROM"),
            maponly: has(block, "MAPONLY"),
            dataonly: has(block, "DATAONLY"),
            cursor: a("CURSOR"),
            control: control(block),
        },
        "RECEIVE" | "RECEIVE MAP" if map_command("RECEIVE MAP") => Cics::ReceiveMap { map: a("MAP"), mapset: a("MAPSET"), into: a("INTO"), set: a("SET") },
        "SEND CONTROL" => Cics::SendControl { cursor: a("CURSOR"), control: control(block) },
        "RECEIVE" => Cics::Receive(record(block)),
        "ASKTIME" => Cics::Asktime { abstime: a("ABSTIME") },
        "FORMATTIME" => Cics::Formattime {
            abstime: a("ABSTIME"),
            datesep: a("DATESEP"),
            timesep: a("TIMESEP"),
            outputs: block.options.iter().filter(|(n, _)| !FORMAT_CONTROLS.contains(&n.as_str())).map(|(n, _)| (n.as_str(), a(n).unwrap_or(Datum::Bare))).collect(),
        },
        "ASSIGN" => Cics::Assign(Assign {
            applid: a("APPLID"),
            sysid: a("SYSID"),
            userid: a("USERID"),
            netname: a("NETNAME"),
            facility: a("FACILITY"),
            startcode: a("STARTCODE"),
            abcode: a("ABCODE"),
            program: a("PROGRAM"),
            cwaleng: a("CWALENG"),
            twaleng: a("TWALENG"),
        }),
        "GETMAIN" => Cics::Getmain { flength: a("FLENGTH"), length: a("LENGTH"), initimg: a("INITIMG"), set: a("SET") },
        "FREEMAIN" => Cics::Freemain,
        "ENQ" => Cics::Enq,
        "DEQ" => Cics::Deq,
        "DELAY" => Cics::Delay,
        "SYNCPOINT" => Cics::Syncpoint { rollback: has(block, "ROLLBACK") },
        "ADDRESS" => Cics::Address { eib: a("EIB"), commarea: a("COMMAREA"), cwa: a("CWA"), twa: a("TWA") },
        "SEND TEXT" => Cics::SendText { from: a("FROM"), length: a("LENGTH") },
        "WRITEQ TS" => Cics::WriteqTs { queue: queue(), from: a("FROM"), length: a("LENGTH"), rewrite: has(block, "REWRITE"), item: a("ITEM"), numitems: a("NUMITEMS") },
        "READQ TS" => Cics::ReadqTs { queue: queue(), next: has(block, "NEXT"), item: a("ITEM"), numitems: a("NUMITEMS"), record: record(block) },
        "DELETEQ TS" => Cics::DeleteqTs { queue: queue() },
        "WRITEQ TD" => Cics::WriteqTd { queue: queue(), from: a("FROM"), length: a("LENGTH") },
        "READQ TD" => Cics::ReadqTd { queue: queue(), record: record(block) },
        "DELETEQ TD" => Cics::DeleteqTd { queue: queue() },
        _ => Cics::Unsupported,
    })
}

/// A HANDLE label in paragraph `from`: the paragraph or section it names, where control goes.
pub fn label(program: &syntax::ast::Program, block: &ExecBlock, text: &str, from: usize) -> R<ParaId> {
    let p = ProcName { name: text.trim().to_ascii_uppercase(), section: None };
    crate::procedure_from(program, &p, from).map(|(start, _)| start as ParaId).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))
}
