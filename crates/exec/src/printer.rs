//! Which files are print files, and how, is `compile::printer`; the control characters a WRITE
//! puts out are `rt::printer`.

pub use compile::printer::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonic_environment_names() {
        assert_eq!(mnemonic_space("C01"), Some(Space::Channel(1)));
        assert_eq!(mnemonic_space("C12"), Some(Space::Channel(12)));
        assert_eq!(mnemonic_space("CSP"), Some(Space::Lines(0)));
        assert_eq!(mnemonic_space("AFP-5A"), Some(Space::PageMode));
        assert_eq!(mnemonic_space("S01"), None);
        assert_eq!(mnemonic_space("C13"), None);
    }
}
