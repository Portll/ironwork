//! The walker binds an EXEC CICS block by `compile::cics_bind`, from the paragraph running.

use super::*;
use compile::cics_bind::{Command, bind, label};
pub(crate) use compile::cics_bind::sinks;

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
        ("ALLOCATE", "IWR0058-S EXEC CICS ALLOCATE is not supported yet"),
        ("BIF DEEDIT", "IWR0058-S EXEC CICS BIF is not supported yet"),
        ("CANCEL", "IWR0058-S EXEC CICS CANCEL is not supported yet"),
        ("CONNECT PROCESS", "IWR0058-S EXEC CICS CONNECT is not supported yet"),
        ("CONVERSE", "IWR0058-S EXEC CICS CONVERSE is not supported yet"),
        ("DEFINE COUNTER", "IWR0058-S EXEC CICS DEFINE is not supported yet"),
        ("DELETE CONTAINER", "IWR0058-S EXEC CICS DELETE CONTAINER is not supported yet"),
        ("DELETE COUNTER", "EXEC CICS DELETE needs FILE"),
        ("DOCUMENT CREATE", "IWR0058-S EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT DELETE", "IWR0058-S EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT INSERT", "IWR0058-S EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT RETRIEVE", "IWR0058-S EXEC CICS DOCUMENT is not supported yet"),
        ("DOCUMENT SET", "IWR0058-S EXEC CICS DOCUMENT is not supported yet"),
        ("DUMP TRANSACTION", "IWR0058-S EXEC CICS DUMP is not supported yet"),
        ("ENDBROWSE CONTAINER", "IWR0058-S EXEC CICS ENDBROWSE CONTAINER is not supported yet"),
        ("ENTER TRACENUM", "IWR0058-S EXEC CICS ENTER is not supported yet"),
        ("EXTRACT ATTRIBUTES", "IWR0058-S EXEC CICS EXTRACT is not supported yet"),
        ("EXTRACT PROCESS", "IWR0058-S EXEC CICS EXTRACT is not supported yet"),
        ("EXTRACT WEB", "IWR0058-S EXEC CICS EXTRACT is not supported yet"),
        ("FETCH ANY", "IWR0058-S EXEC CICS FETCH is not supported yet"),
        ("FETCH CHILD", "IWR0058-S EXEC CICS FETCH is not supported yet"),
        ("FREE", "IWR0058-S EXEC CICS FREE is not supported yet"),
        ("FREE CHILD", "IWR0058-S EXEC CICS FREE is not supported yet"),
        ("GET CONTAINER", "IWR0058-S EXEC CICS GET CONTAINER is not supported yet"),
        ("GET COUNTER", "IWR0058-S EXEC CICS GET is not supported yet"),
        ("GETNEXT CONTAINER", "IWR0058-S EXEC CICS GETNEXT CONTAINER is not supported yet"),
        ("INQUIRE ASSOCIATION", "IWR0058-S EXEC CICS INQUIRE ASSOCIATION is not supported yet"),
        ("INQUIRE CONNECTION", "IWR0058-S EXEC CICS INQUIRE CONNECTION is not supported yet"),
        ("INQUIRE FILE", "IWR0058-S EXEC CICS INQUIRE FILE is not supported yet"),
        ("INQUIRE PROGRAM", "IWR0058-S EXEC CICS INQUIRE PROGRAM is not supported yet"),
        ("INQUIRE SYSTEM", "IWR0058-S EXEC CICS INQUIRE SYSTEM is not supported yet"),
        ("INQUIRE TERMINAL", "IWR0058-S EXEC CICS INQUIRE TERMINAL is not supported yet"),
        ("INQUIRE TRANSACTION", "IWR0058-S EXEC CICS INQUIRE TRANSACTION is not supported yet"),
        ("INQUIRE URIMAP", "IWR0058-S EXEC CICS INQUIRE URIMAP is not supported yet"),
        ("ISSUE CONFIRMATION", "IWR0058-S EXEC CICS ISSUE is not supported yet"),
        ("LOAD", "IWR0058-S EXEC CICS LOAD is not supported yet"),
        ("PURGE MESSAGE", "IWR0058-S EXEC CICS PURGE is not supported yet"),
        ("PUT CONTAINER", "IWR0058-S EXEC CICS PUT CONTAINER is not supported yet"),
        ("QUERY COUNTER", "IWR0058-S EXEC CICS QUERY is not supported yet"),
        ("QUERY SECURITY", "IWR0058-S EXEC CICS QUERY is not supported yet"),
        ("RELEASE", "IWR0058-S EXEC CICS RELEASE is not supported yet"),
        ("RETRIEVE", "IWR0058-S EXEC CICS RETRIEVE is not supported yet"),
        ("REWIND COUNTER", "IWR0058-S EXEC CICS REWIND is not supported yet"),
        ("RUN TRANSID", "IWR0058-S EXEC CICS RUN is not supported yet"),
        ("SEND", "IWR0058-S EXEC CICS SEND is not supported yet"),
        ("SEND PAGE", "IWR0058-S EXEC CICS SEND PAGE is not supported yet"),
        ("SET FILE", "IWR0058-S EXEC CICS SET FILE is not supported yet"),
        ("SET PROGRAM", "IWR0058-S EXEC CICS SET PROGRAM is not supported yet"),
        ("SET TERMINAL", "IWR0058-S EXEC CICS SET TERMINAL is not supported yet"),
        ("SET TRANSACTION", "IWR0058-S EXEC CICS SET TRANSACTION is not supported yet"),
        ("SIGNOFF", "IWR0058-S EXEC CICS SIGNOFF is not supported yet"),
        ("SIGNON", "IWR0058-S EXEC CICS SIGNON is not supported yet"),
        ("SOAPFAULT ADD", "IWR0058-S EXEC CICS SOAPFAULT is not supported yet"),
        ("SOAPFAULT CREATE", "IWR0058-S EXEC CICS SOAPFAULT is not supported yet"),
        ("SOAPFAULT DELETE", "IWR0058-S EXEC CICS SOAPFAULT is not supported yet"),
        ("START", "IWR0058-S EXEC CICS START is not supported yet"),
        ("STARTBROWSE CONTAINER", "IWR0058-S EXEC CICS STARTBROWSE is not supported yet"),
        ("TRANSFORM DATATOXML", "IWR0058-S EXEC CICS TRANSFORM is not supported yet"),
        ("TRANSFORM XMLTODATA", "IWR0058-S EXEC CICS TRANSFORM is not supported yet"),
        ("UPDATE COUNTER", "IWR0058-S EXEC CICS UPDATE is not supported yet"),
        ("WAIT CONVID", "IWR0058-S EXEC CICS WAIT is not supported yet"),
        ("WAIT TERMINAL", "IWR0058-S EXEC CICS WAIT is not supported yet"),
        ("WEB CLOSE", "IWR0058-S EXEC CICS WEB CLOSE is not supported yet"),
        ("WEB CONVERSE", "IWR0058-S EXEC CICS WEB CONVERSE is not supported yet"),
        ("WEB ENDBROWSE FORMFIELD", "IWR0058-S EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB ENDBROWSE HTTPHEADER", "IWR0058-S EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB ENDBROWSE QUERYPARM", "IWR0058-S EXEC CICS WEB ENDBROWSE is not supported yet"),
        ("WEB EXTRACT", "IWR0058-S EXEC CICS WEB EXTRACT is not supported yet"),
        ("WEB OPEN", "IWR0058-S EXEC CICS WEB OPEN is not supported yet"),
        ("WEB PARSE URL", "IWR0058-S EXEC CICS WEB PARSE is not supported yet"),
        ("WEB READ FORMFIELD", "IWR0058-S EXEC CICS WEB READ is not supported yet"),
        ("WEB READ HTTPHEADER", "IWR0058-S EXEC CICS WEB READ is not supported yet"),
        ("WEB READ QUERYPARM", "IWR0058-S EXEC CICS WEB READ is not supported yet"),
        ("WEB READNEXT FORMFIELD", "IWR0058-S EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB READNEXT HTTPHEADER", "IWR0058-S EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB READNEXT QUERYPARM", "IWR0058-S EXEC CICS WEB READNEXT is not supported yet"),
        ("WEB RECEIVE", "IWR0058-S EXEC CICS WEB RECEIVE is not supported yet"),
        ("WEB RETRIEVE", "IWR0058-S EXEC CICS WEB RETRIEVE is not supported yet"),
        ("WEB SEND", "IWR0058-S EXEC CICS WEB SEND is not supported yet"),
        ("WEB STARTBROWSE FORMFIELD", "IWR0058-S EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB STARTBROWSE HTTPHEADER", "IWR0058-S EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB STARTBROWSE QUERYPARM", "IWR0058-S EXEC CICS WEB STARTBROWSE is not supported yet"),
        ("WEB WRITE HTTPHEADER", "IWR0058-S EXEC CICS WEB WRITE is not supported yet"),
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
