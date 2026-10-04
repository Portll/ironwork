//! UPSI switches (Language Reference SC27-8713-03, pp. 125-127, 283, 442-443). Each switch a
//! program's SPECIAL-NAMES names is declared as the run unit's one-byte EXTERNAL record for it
//! ([`rt::unit::switch_record`]), holding a child for each entry, named by its mnemonic-name, with
//! the entry's condition-names; SET mnemonic-name TO ON or OFF becomes SET TO TRUE of a
//! condition-name of the switch's status that no source can spell. Assumption C410.

use crate::layout::Layout;
use crate::oo;
use crate::report::entry;
use syntax::ast::{Literal, Program, Ref, SetStmt, Stmt, Switch};
use syntax::Error;

/// The condition-name of switch `n` being on, or off, that SET ... TO ON or OFF sets.
fn status(n: u8, on: bool) -> String {
    format!("UPSI-{n} {}", if on { "ON" } else { "OFF" })
}

/// Declares the program's switches after its WORKING-STORAGE and rewrites its SET statements of
/// them.
pub(crate) fn declare(program: &mut Program, errors: &mut Vec<Error>) {
    let switches = program.environment.switches.clone();
    let mut numbers: Vec<u8> = switches.iter().map(|s| s.number).collect();
    numbers.sort_unstable();
    numbers.dedup();
    for n in numbers {
        let entries: Vec<&Switch> = switches.iter().filter(|s| s.number == n).collect();
        let mut record = entry(1, Some(rt::unit::switch_record(n)), None, None, entries[0].pos);
        record.external = true;
        program.working_storage.push(record);
        let mut first: Option<String> = None;
        for (k, s) in entries.into_iter().enumerate() {
            let name = s.mnemonic.clone().unwrap_or_else(|| format!("UPSI-{n} ENTRY {k}"));
            let mut byte = entry(5, Some(name.clone()), Some("X".into()), None, s.pos);
            byte.redefines = first.clone();
            program.working_storage.push(byte);
            let own = first.is_none().then(|| [(status(n, true), true), (status(n, false), false)]).into_iter().flatten();
            let named = s.on.iter().map(|c| (c.clone(), true)).chain(s.off.iter().map(|c| (c.clone(), false)));
            for (condition, on) in own.chain(named) {
                let mut e = entry(88, Some(condition), None, None, s.pos);
                e.condition_values = vec![(Literal::Hex(vec![u8::from(on)]), None)];
                program.working_storage.push(e);
            }
            first.get_or_insert(name);
        }
    }
    for p in &mut program.paragraphs {
        oo::each_mut(&mut p.statements, &mut |s| {
            if let Stmt::Set { set, .. } = s
                && let SetStmt::Switches(groups) = set
            {
                *set = SetStmt::ConditionTrue(set_to(&switches, groups, errors));
            }
        });
    }
}

/// The condition-names SET ... TO ON or OFF sets, in order: a mnemonic-name of an UPSI switch
/// in each target (p. 443).
fn set_to(switches: &[Switch], groups: &[(Vec<Ref>, bool)], errors: &mut Vec<Error>) -> Vec<Ref> {
    let mut conditions = Vec::new();
    for (targets, on) in groups {
        for r in targets {
            let switch = switches.iter().find(|s| s.mnemonic.as_deref() == Some(r.name.as_str()));
            match switch {
                Some(s) if r.qualifiers.is_empty() && r.subscripts.is_empty() && r.refmod.is_none() => {
                    conditions.push(Ref { name: status(s.number, *on), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: r.pos });
                }
                _ => errors.push(Error::at(r.pos, format!("SET {} TO {}: {} is not the mnemonic-name of an UPSI switch", r.name, if *on { "ON" } else { "OFF" }, r.name))),
            }
        }
    }
    conditions
}

/// The UPSI switch whose record holds item `i`, a mnemonic-name's child or a condition-name's
/// item among them.
pub(crate) fn switch_of(layout: &Layout, mut i: usize) -> Option<u8> {
    while let Some(parent) = layout.items[i].parent {
        i = parent;
    }
    rt::unit::switch_of_record(layout.items[i].name.as_deref()?)
}

/// A reference to a switch's mnemonic-name as data: the Language Reference lets SET name it, and a
/// condition-name be qualified by it, and nothing else (p. 127).
pub(crate) fn mnemonic_as_data(r: &Ref, layout: &Layout, i: usize) -> Error {
    let n = switch_of(layout, i).unwrap_or_default();
    Error::at(r.pos, format!("{} is the mnemonic-name of UPSI-{n}: only SET ... TO ON or OFF and a condition-name's qualifier can name it", r.name))
}

/// SET TO TRUE of condition-name `c` of a switch, as `r` names it: its conditional variable is the
/// entry's mnemonic-name (p. 127), so an entry written without one gives SET nothing to set
/// (assumption C412).
pub(crate) fn without_variable(layout: &Layout, c: usize, r: &Ref) -> Option<Error> {
    let item = layout.conditions[c].item;
    let unnamed = layout.items[item].name.as_deref().is_none_or(|name| name.contains(' '));
    (switch_of(layout, item).is_some() && unnamed && !r.name.contains(' '))
        .then(|| Error::at(r.pos, format!("SET {} TO TRUE: the UPSI switch's entry has no mnemonic-name, which would be its conditional variable", r.name)))
}
