//! The parameter Language Environment gives a COBOL main program that EXEC PGM=,PARM= starts.

use zarch::ebcdic::CodePage;

/// Language Environment's runtime options, as the CEEXOPT sample in the Programming Guide
/// (SA38-0682) spells them, with the NO forms it gives.
const RUNTIME_OPTIONS: &[&str] = &[
    "ABPERC", "ABTERMENC", "AIXBLD", "ALL31", "ANYHEAP", "BELOWHEAP", "CBLOPTS", "CBLPSHPOP", "CBLQDA", "CEEDUMP", "CHECK", "COUNTRY", "DEBUG", "DEPTHCONDLMT", "DYNDUMP", "ENVAR", "ERRCOUNT", "ERRUNIT", "FILEHIST", "FILETAG", "HEAP", "HEAPCHK",
    "HEAPPOOLS", "HEAPZONES", "INFOMSGFILTER", "INQPCOPN", "INTERRUPT", "LIBSTACK", "MSGFILE", "MSGQ", "NATLANG", "NOTEST", "NOUSRHDLR", "OCSTATUS", "PAGEFRAMESIZE", "PC", "PLITASKCOUNT", "POSIX", "PROFILE", "PRTUNIT", "PUNUNIT", "RDRUNIT",
    "RECPAD", "RPTOPTS", "RPTSTG", "RTEREUS", "SIMVRD", "STACK", "STORAGE", "TERMTHDACT", "TEST", "THREADHEAP", "THREADSTACK", "TRACE", "TRAP", "UPSI", "USRHDLR", "VCTRSAVE", "XPLINK", "XUFLOW",
];

/// The longest PARM= value JCL allows, in characters.
pub const PARM_LIMIT: usize = 100;

/// The program arguments in `parm` under CBLOPTS(ON), Language Environment's default outside
/// CICS: what precedes the last slash, unless no word after it is a runtime option, when the
/// whole string is (assumption C250).
pub fn program_arguments(parm: &str) -> &str {
    match parm.rfind('/') {
        Some(at) if names_runtime_options(&parm[at + 1..]) => &parm[..at],
        _ => parm,
    }
}

/// Whether the runtime options in `parm` turn TRAP off, the last TRAP among them deciding
/// ([`numeric::assumptions::TRAP_OFF_LEAVES_FILES_OPEN`]).
pub fn trap_off(parm: &str) -> bool {
    let trap = words(runtime_options(parm)).into_iter().rev().find_map(|w| {
        let (name, value) = w.split_once('(')?;
        name.eq_ignore_ascii_case("TRAP").then(|| value.trim_end_matches(')').split(',').next().unwrap_or("").trim().to_ascii_uppercase())
    });
    trap.as_deref() == Some("OFF")
}

/// The UPSI switches the runtime options in `parm` set, the last UPSI among them deciding: UPSI-0
/// to UPSI-7, from the leftmost of its eight digits, each 1 for on and 0 for off (Language
/// Environment Programming Reference, UPSI). None without one, and the message for one that is
/// not eight such digits, which leaves the switches off (assumption C411).
pub fn upsi(parm: &str) -> Option<Result<[bool; 8], String>> {
    let option = words(runtime_options(parm)).into_iter().rev().find(|w| w.split('(').next().is_some_and(|name| name.trim().eq_ignore_ascii_case("UPSI")))?;
    let digits = option.split_once('(').map(|(_, v)| v.trim_end_matches(')').trim());
    let switches = digits.filter(|d| d.len() == 8 && d.bytes().all(|b| matches!(b, b'0' | b'1')));
    Some(match switches {
        Some(d) => Ok(std::array::from_fn(|n| d.as_bytes()[n] == b'1')),
        None => Err(format!("runtime option {option}: UPSI takes eight digits, each 0 or 1, so the UPSI switches stay off")),
    })
}

/// The runtime options of `parm`: what follows its last slash, if that names any.
fn runtime_options(parm: &str) -> &str {
    match parm.rfind('/') {
        Some(at) if names_runtime_options(&parm[at + 1..]) => &parm[at + 1..],
        _ => "",
    }
}

/// Nothing at all, or at least one runtime option among the words, each NAME or NAME(...),
/// separated by commas or blanks.
fn names_runtime_options(text: &str) -> bool {
    let words = words(text);
    words.is_empty() || words.iter().any(|w| RUNTIME_OPTIONS.iter().any(|o| o.eq_ignore_ascii_case(w.split('(').next().unwrap_or(w))))
}

/// The words of runtime options, each NAME or NAME(...), separated by commas or blanks.
fn words(text: &str) -> Vec<String> {
    let (mut words, mut word, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' | ' ' if depth <= 0 => {
                words.push(std::mem::take(&mut word));
                continue;
            }
            _ => {}
        }
        word.push(c);
    }
    words.push(word);
    words.retain(|w| !w.is_empty());
    words
}

/// The storage the main program's first USING item addresses: a halfword length and the
/// arguments in EBCDIC, then X'00' up to the longest PARM (assumption C251).
pub fn parameter_area(arguments: &str, page: &CodePage) -> Vec<u8> {
    let text = page.encode_lossy(arguments);
    let mut area = (text.len() as u16).to_be_bytes().to_vec();
    area.extend(&text);
    area.resize(2 + PARM_LIMIT.max(text.len()), 0);
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_slash_ends_the_arguments_when_runtime_options_follow_it() {
        assert_eq!(program_arguments("RUN=1,MODE=X"), "RUN=1,MODE=X");
        assert_eq!(program_arguments("A/B/RPTOPTS(ON)"), "A/B");
        assert_eq!(program_arguments("ABC/RPTOPTS(ON),MSGFILE(SYSOUT,FBA,121,0)"), "ABC");
        assert_eq!(program_arguments("ABC/ rptstg(on) NOSUCH"), "ABC");
        assert_eq!(program_arguments("ABC/"), "ABC");
        assert_eq!(program_arguments("/TRAP(OFF)"), "");
        assert_eq!(program_arguments("11/16/1967"), "11/16/1967", "the manual's example: 1967 is no runtime option");
    }

    #[test]
    fn trap_is_off_when_the_last_trap_among_the_runtime_options_says_so() {
        assert!(trap_off("/TRAP(OFF)"));
        assert!(trap_off("ARGS/RPTOPTS(ON) trap(off,nospie)"));
        assert!(!trap_off("/TRAP(OFF),TRAP(ON)"));
        assert!(!trap_off("/TRAP(,NOSPIE)"));
        assert!(!trap_off("TRAP(OFF)"), "with no slash the PARM is all program arguments");
        assert!(!trap_off(""));
    }

    #[test]
    fn the_last_upsi_among_the_runtime_options_sets_the_switches_from_its_leftmost_digit() {
        let on = |digits: &str| -> [bool; 8] { std::array::from_fn(|n| digits.as_bytes()[n] == b'1') };
        assert_eq!(upsi("/UPSI(10000001)"), Some(Ok(on("10000001"))));
        assert_eq!(upsi("ARGS/upsi(00000000) RPTOPTS(ON),UPSI( 01000000 )"), Some(Ok(on("01000000"))));
        assert_eq!(upsi("UPSI(10000000)"), None, "with no slash the PARM is all program arguments");
        assert_eq!(upsi("/TRAP(OFF)"), None);
        for malformed in ["/UPSI(1000000)", "/UPSI(100000002)", "/UPSI", "/UPSI(1000000X)"] {
            assert!(upsi(malformed).is_some_and(|r| r.is_err()), "{malformed}");
        }
    }

    #[test]
    fn the_area_holds_a_halfword_length_and_the_arguments_in_ebcdic() {
        let page = numeric::options::Options::default().code_page();
        let area = parameter_area("AB 1", page);
        assert_eq!(&area[..6], [0, 4, 0xC1, 0xC2, 0x40, 0xF1]);
        assert_eq!(area.len(), 2 + PARM_LIMIT);
        assert!(area[6..].iter().all(|&b| b == 0));
    }
}
