//! An EXEC CICS block as the `CicsCommand` `rt::cics` runs: which command it is, found from the
//! command words and options as the translator gives them, and each option it reads, over the
//! walker's references. HANDLE CONDITION and HANDLE ABEND labels are resolved here.

use super::*;
use crate::cics::{Assign, Cics, CicsCommand, Condition, Control, Datum, FileControl, FileOptions, Opt, Record, Resp, Transfer};
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

pub(super) fn option<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Option<ExecArg>> {
    block.options.iter().find(|(n, _)| n == name).map(|(_, a)| a)
}

pub(super) fn has(block: &ExecBlock, name: &str) -> bool {
    option(block, name).is_some()
}

pub(super) fn operand<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Operand> {
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
    Ok(CicsCommand { name: block.command.as_str(), command, resp: resp(block) })
}

fn bind_other<'b>(block: &'b ExecBlock, command: &str, label: &dyn Fn(&str) -> R<ParaId>) -> R<Cics<&'b Ref, &'b Operand, &'b str>> {
    let a = |name: &str| arg(block, name);
    let queue = || first(block, &["QUEUE", "QNAME"]);
    let map_command = |two_word: &str| has(block, "MAP") || block.command == two_word;
    Ok(match command {
        "RETURN" => Cics::Return { transid: a("TRANSID"), commarea: a("COMMAREA"), length: a("LENGTH") },
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
            let program = has(block, "PROGRAM");
            let label = match option(block, "LABEL") {
                Some(Some(ExecArg::Text(t))) if !program => Some(label(t)?),
                _ => None,
            };
            Cics::HandleAbend { program, label, reset: has(block, "CANCEL") || has(block, "RESET") }
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

/// A HANDLE label: the paragraph or section it names, where control goes.
pub(crate) fn label(program: &syntax::ast::Program, block: &ExecBlock, text: &str) -> R<ParaId> {
    let p = ProcName { name: text.trim().to_ascii_uppercase(), section: None };
    crate::procedure(program, &p).map(|(start, _)| start as ParaId).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))
}

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn bind_cics(&self, block: &'p ExecBlock) -> R<Command<'p>> {
        bind(block, &|text| label(self.program, block, text))
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
        ("BIF DEEDIT", "EXEC CICS BIF is not supported yet"),
        ("CANCEL", "EXEC CICS CANCEL is not supported yet"),
        ("DEFINE COUNTER", "EXEC CICS DEFINE is not supported yet"),
        ("DELETE CONTAINER", "EXEC CICS DELETE CONTAINER is not supported yet"),
        ("DELETE COUNTER", "EXEC CICS DELETE needs FILE"),
        ("DUMP TRANSACTION", "EXEC CICS DUMP is not supported yet"),
        ("ENTER TRACENUM", "EXEC CICS ENTER is not supported yet"),
        ("GET CONTAINER", "EXEC CICS GET CONTAINER is not supported yet"),
        ("GET COUNTER", "EXEC CICS GET is not supported yet"),
        ("LOAD", "EXEC CICS LOAD is not supported yet"),
        ("PURGE MESSAGE", "EXEC CICS PURGE is not supported yet"),
        ("PUT CONTAINER", "EXEC CICS PUT CONTAINER is not supported yet"),
        ("QUERY COUNTER", "EXEC CICS QUERY is not supported yet"),
        ("RELEASE", "EXEC CICS RELEASE is not supported yet"),
        ("RETRIEVE", "EXEC CICS RETRIEVE is not supported yet"),
        ("REWIND COUNTER", "EXEC CICS REWIND is not supported yet"),
        ("SEND", "EXEC CICS SEND is not supported yet"),
        ("SEND PAGE", "EXEC CICS SEND PAGE is not supported yet"),
        ("SIGNOFF", "EXEC CICS SIGNOFF is not supported yet"),
        ("SIGNON", "EXEC CICS SIGNON is not supported yet"),
        ("START", "EXEC CICS START is not supported yet"),
        ("UPDATE COUNTER", "EXEC CICS UPDATE is not supported yet"),
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
