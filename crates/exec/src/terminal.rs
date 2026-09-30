//! rt's 3270 terminal, with its AID bytes checked against the DFHAID copybook `syntax` serves.

pub use rt::terminal::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aid_bytes_match_the_dfhaid_copybook() {
        let copybook = syntax::system::member("DFHAID").unwrap();
        let dfh = |name: &str| -> u8 {
            let line = copybook.lines().find(|l| l.split_whitespace().nth(1) == Some(name)).unwrap();
            let hex = line.split("X'").nth(1).unwrap().split('\'').next().unwrap();
            u8::from_str_radix(hex, 16).unwrap()
        };
        for (name, aid) in [("DFHENTER", AID_ENTER), ("DFHCLEAR", AID_CLEAR), ("DFHPA1", AID_PA1), ("DFHPA2", AID_PA2), ("DFHPA3", AID_PA3)] {
            assert_eq!(dfh(name), aid, "{name}");
        }
        for n in 1..=24 {
            assert_eq!(aid_of(&format!("PF{n}")), Some(dfh(&format!("DFHPF{n}"))), "PF{n}");
        }
    }
}
