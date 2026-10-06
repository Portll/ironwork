//! An EXEC CICS block as the `CicsCommand` `rt::cics` runs: which command it is, found from the
//! command words and options as the translator gives them, and each option it reads, over the
//! walker's references. HANDLE CONDITION and HANDLE ABEND labels are resolved here.

use super::*;
use crate::cics::{Assign, Cics, CicsCommand, Condition, Control, Datum, FileControl, FileOptions, Opt, Record, Resp, Sink, Transfer};
use rt::lir::ParaId;

pub(crate) type Command<'b> = CicsCommand<&'b Ref, &'b Operand, &'b str>;
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
pub(crate) fn bind<'b>(block: &'b ExecBlock, label: &dyn Fn(&str) -> R<ParaId>) -> R<Command<'b>> {
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
pub(crate) fn sinks(block: &ExecBlock) -> Vec<(&Ref, Sink)> {
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
pub(crate) fn label(program: &syntax::ast::Program, block: &ExecBlock, text: &str, from: usize) -> R<ParaId> {
    let p = ProcName { name: text.trim().to_ascii_uppercase(), section: None };
    crate::procedure_from(program, &p, from).map(|(start, _)| start as ParaId).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))
}

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn bind_cics(&self, block: &'p ExecBlock) -> R<Command<'p>> {
        bind(block, &|text| label(self.program, block, text, self.returns.running))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Execute;

    /// The commands of IBM's table ironwork refuses, each written with only the words that name
    /// it, and the abend a CICS task gets when it is reached. DELETE COUNTER and WRITE JOURNALNAME
    /// reach file control's DELETE and WRITE, which want a FILE.
    const REFUSED: &[(&str, &str)] = &[
        ("ALLOCATE", "EXEC CICS ALLOCATE is not supported yet"),
        ("BIF DEEDIT", "EXEC CICS BIF is not supported yet"),
        ("CANCEL", "EXEC CICS CANCEL is not supported yet"),
        ("CONNECT PROCESS", "EXEC CICS CONNECT is not supported yet"),
        ("CONVERSE", "EXEC CICS CONVERSE is not supported yet"),
        ("DEFINE COUNTER", "EXEC CICS DEFINE is not supported yet"),
        ("DELETE CONTAINER", "EXEC CICS DELETE CONTAINER is not supported yet"),
        ("DELETE COUNTER", "EXEC CICS DELETE needs FILE"),
        ("DOCUMENT CREATE", "EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT DELETE", "EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT INSERT", "EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT RETRIEVE", "EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT SET", "EXEC CICS DOCUMENT is not supported yet"),
        ("DUMP TRANSACTION", "EXEC CICS DUMP is not supported yet"),
        ("ENDBROWSE CONTAINER", "EXEC CICS ENDBROWSE CONTAINER is not supported yet"),
        ("ENTER TRACENUM", "EXEC CICS ENTER is not supported yet"),
        ("EXTRACT ATTRIBUTES", "EXEC CICS EXTRACT is not supported yet"),
        ("EXTRACT PROCESS", "EXEC CICS EXTRACT is not supported yet"),
        ("EXTRACT WEB", "EXEC CICS EXTRACT is not supported yet"),
        ("FETCH ANY", "EXEC CICS FETCH is not supported yet"),
        ("FETCH CHILD", "EXEC CICS FETCH is not supported yet"),
        ("FREE", "EXEC CICS FREE is not supported yet"),
        ("FREE CHILD", "EXEC CICS FREE is not supported yet"),
        ("GET CONTAINER", "EXEC CICS GET CONTAINER is not supported yet"),
        ("GET COUNTER", "EXEC CICS GET is not supported yet"),
        ("GETNEXT CONTAINER", "EXEC CICS GETNEXT CONTAINER is not supported yet"),
        ("INQUIRE ASSOCIATION", "EXEC CICS INQUIRE ASSOCIATION is not supported yet"),
        ("INQUIRE CONNECTION", "EXEC CICS INQUIRE CONNECTION is not supported yet"),
        ("INQUIRE FILE", "EXEC CICS INQUIRE FILE is not supported yet"),
        ("INQUIRE PROGRAM", "EXEC CICS INQUIRE PROGRAM is not supported yet"),
        ("INQUIRE SYSTEM", "EXEC CICS INQUIRE SYSTEM is not supported yet"),
        ("INQUIRE TERMINAL", "EXEC CICS INQUIRE TERMINAL is not supported yet"),
        ("INQUIRE TRANSACTION", "EXEC CICS INQUIRE TRANSACTION is not supported yet"),
        ("INQUIRE URIMAP", "EXEC CICS INQUIRE URIMAP is not supported yet"),
        ("ISSUE CONFIRMATION", "EXEC CICS ISSUE is not supported yet"),
        ("LOAD", "EXEC CICS LOAD is not supported yet"),
        ("PURGE MESSAGE", "EXEC CICS PURGE is not supported yet"),
        ("PUT CONTAINER", "EXEC CICS PUT CONTAINER is not supported yet"),
        ("QUERY COUNTER", "EXEC CICS QUERY is not supported yet"),
        ("QUERY SECURITY", "EXEC CICS QUERY is not supported yet"),
        ("RELEASE", "EXEC CICS RELEASE is not supported yet"),
        ("RETRIEVE", "EXEC CICS RETRIEVE is not supported yet"),
        ("REWIND COUNTER", "EXEC CICS REWIND is not supported yet"),
        ("RUN TRANSID", "EXEC CICS RUN is not supported yet"),
        ("SEND", "EXEC CICS SEND is not supported yet"),
        ("SEND PAGE", "EXEC CICS SEND PAGE is not supported yet"),
        ("SET FILE", "EXEC CICS SET FILE is not supported yet"),
        ("SET PROGRAM", "EXEC CICS SET PROGRAM is not supported yet"),
        ("SET TERMINAL", "EXEC CICS SET TERMINAL is not supported yet"),
        ("SET TRANSACTION", "EXEC CICS SET TRANSACTION is not supported yet"),
        ("SIGNOFF", "EXEC CICS SIGNOFF is not supported yet"),
        ("SIGNON", "EXEC CICS SIGNON is not supported yet"),
        ("SOAPFAULT ADD", "EXEC CICS SOAPFAULT is not supported yet"),
        ("SOAPFAULT CREATE", "EXEC CICS SOAPFAULT is not supported yet"),
        ("SOAPFAULT DELETE", "EXEC CICS SOAPFAULT is not supported yet"),
        ("START", "EXEC CICS START is not supported yet"),
        ("STARTBROWSE CONTAINER", "EXEC CICS STARTBROWSE is not supported yet"),
        ("TRANSFORM DATATOXML", "EXEC CICS TRANSFORM is not supported yet"),
        ("TRANSFORM XMLTODATA", "EXEC CICS TRANSFORM is not supported yet"),
        ("UPDATE COUNTER", "EXEC CICS UPDATE is not supported yet"),
        ("WAIT CONVID", "EXEC CICS WAIT is not supported yet"),
        ("WAIT TERMINAL", "EXEC CICS WAIT is not supported yet"),
        ("WEB CLOSE", "EXEC CICS WEB CLOSE is not supported yet"),
        ("WEB CONVERSE", "EXEC CICS WEB CONVERSE is not supported yet"),
        ("WEB ENDBROWSE FORMFIELD", "EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB ENDBROWSE HTTPHEADER", "EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB ENDBROWSE QUERYPARM", "EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB EXTRACT", "EXEC CICS WEB EXTRACT is not supported yet"),
        ("WEB OPEN", "EXEC CICS WEB OPEN is not supported yet"),
        ("WEB PARSE URL", "EXEC CICS WEB PARSE is not supported yet"),
        ("WEB READ FORMFIELD", "EXEC CICS WEB READ is not supported yet"),
        ("WEB READ HTTPHEADER", "EXEC CICS WEB READ is not supported yet"),
        ("WEB READ QUERYPARM", "EXEC CICS WEB READ is not supported yet"),
        ("WEB READNEXT FORMFIELD", "EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB READNEXT HTTPHEADER", "EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB READNEXT QUERYPARM", "EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB RECEIVE", "EXEC CICS WEB RECEIVE is not supported yet"),
        ("WEB RETRIEVE", "EXEC CICS WEB RETRIEVE is not supported yet"),
        ("WEB SEND", "EXEC CICS WEB SEND is not supported yet"),
        ("WEB STARTBROWSE FORMFIELD", "EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB STARTBROWSE HTTPHEADER", "EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB STARTBROWSE QUERYPARM", "EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB WRITE HTTPHEADER", "EXEC CICS WEB WRITE is not supported yet"),
        ("WRITE JOURNALNAME", "EXEC CICS WRITE needs FILE"),
    ];

    /// Commands whose identifying option picks no variant of its own.
    const SAME_AS: &[(&str, &str)] = &[("ADDRESS SET", "ADDRESS"), ("SYNCPOINT ROLLBACK", "SYNCPOINT")];

    fn program(command: &str) -> syntax::ast::Program {
        let source = format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CHECKED.\n       PROCEDURE DIVISION.\n           EXEC CICS {command} END-EXEC.\n           GOBACK.\n");
        syntax::parse_all_with(&source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{command}: {e:?}")).remove(0)
    }

    fn exec_block(program: &syntax::ast::Program) -> &ExecBlock {
        let mut statements = program.paragraphs.iter().flat_map(|p| &p.statements);
        statements.find_map(|s| if let Stmt::Exec(block) = s { Some(&**block) } else { None }).expect("the EXEC CICS block")
    }

    /// The abend a CICS task running the command alone gets.
    fn refusal(program: syntax::ast::Program) -> String {
        let compiled = crate::compile(program, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        let task = crate::cics::Task { transid: "T1".into(), ..Default::default() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        match compiled.execute_cics_with(crate::unit::Library::default(), crate::files::Dds::default(), task, crate::unit::Clock::System, None, &mut out, &mut err) {
            Err(abend) => abend.message,
            Ok(_) => "no abend".into(),
        }
    }

    #[test]
    fn every_command_ibm_lists_is_a_variant_or_refused_by_name() {
        let table: Vec<&str> = rt::cics_tables::commands().iter().map(|c| c.name).collect();
        for (name, _) in REFUSED.iter().chain(SAME_AS) {
            assert!(table.contains(name), "{name} is not in IBM's table");
        }
        for name in table {
            let program = program(name);
            let command = bind(exec_block(&program), &|_| Ok(0)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            if let Ok(compiled) = crate::compile(program.clone(), &[]) {
                crate::testing::check_lowering(&compiled, rt::sql::fingerprint(name), None);
            }
            match REFUSED.iter().find(|(refused, _)| *refused == name) {
                Some((_, message)) => {
                    let variant = command.command.name();
                    assert_eq!(refusal(program), *message, "{name} binds to {variant}");
                }
                None => {
                    let expected = SAME_AS.iter().find(|(n, _)| *n == name).map_or(name, |&(_, v)| v);
                    assert_eq!(command.command.name(), expected, "{name}");
                }
            }
        }
    }
}
