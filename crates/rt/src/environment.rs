//! The environment variables Micro Focus's and GnuCOBOL's ACCEPT ... FROM ENVIRONMENT, SET
//! ENVIRONMENT and DISPLAY UPON ENVIRONMENT-NAME and ENVIRONMENT-VALUE read and set under
//! `--compliance extended` (docs/compliance.md IWX0021, assumption C464): those `--env` gives the
//! run, never the process's own.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    pub variables: BTreeMap<String, String>,
    /// The variable DISPLAY UPON ENVIRONMENT-NAME last named.
    pub name: Option<String>,
}

impl Environment {
    pub fn of(variables: BTreeMap<String, String>) -> Self {
        Self { variables, name: None }
    }

    /// DISPLAY UPON ENVIRONMENT-NAME: the variable the next ENVIRONMENT-VALUE reads or sets, the
    /// text without its trailing spaces.
    pub fn name(&mut self, text: &str) {
        self.name = Some(text.trim_end().to_owned());
    }

    /// DISPLAY UPON ENVIRONMENT-VALUE: the named variable takes the text without its trailing
    /// spaces; with no variable named, nothing changes.
    pub fn set(&mut self, text: &str) {
        if let Some(name) = &self.name {
            self.variables.insert(name.clone(), text.trim_end().to_owned());
        }
    }

    /// ACCEPT ... FROM ENVIRONMENT-VALUE: the named variable's value, None when it is not set.
    pub fn value(&self) -> Option<&str> {
        self.variables.get(self.name.as_ref()?).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_selects_the_variable_its_value_reads_and_sets() {
        let mut e = Environment::of(BTreeMap::from([("IW_ONE".to_owned(), "hello".to_owned())]));
        assert_eq!(e.value(), None);
        e.name("IW_ONE    ");
        assert_eq!(e.value(), Some("hello"));
        e.name("IW_NEW");
        assert_eq!(e.value(), None);
        e.set("NEWVAL    ");
        assert_eq!(e.value(), Some("NEWVAL"));
    }
}
