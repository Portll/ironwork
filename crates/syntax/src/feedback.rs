//! CEEIGZCT, ironwork's own declaration of Language Environment's feedback codes for a COPY
//! CEEIGZCT that no library answers. The conditions and their tokens are `rt::feedback`.

pub use rt::feedback::{conditions, symbol, token};

/// CEEIGZCT: a condition name per condition, for the COPY that follows the 8-byte group the
/// condition token begins with (SA38-0682-60, Figure 76).
pub fn member() -> String {
    conditions()
        .map(|(number, severity)| {
            let hex: String = token(severity, number).iter().map(|b| format!("{b:02X}")).collect();
            format!("           88 {} VALUE X'{hex}'.\n", symbol(number))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_member_names_each_condition_in_area_b() {
        let member = member();
        let lines: Vec<&str> = member.lines().collect();
        assert_eq!(lines.len(), 723);
        assert_eq!(lines[0], "           88 CEE000 VALUE X'0000000000000000'.");
        assert!(lines.contains(&"           88 CEE2EB VALUE X'000309CB59C3C5C5'."));
        assert!(lines.contains(&"           88 CEE5KB VALUE X'0003168B59C3C5C5'."));
        assert!(lines.iter().all(|l| l.len() <= 72));
    }
}
