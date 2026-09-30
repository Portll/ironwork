//! The symbols of an edited PICTURE.

/// One position of an edited PICTURE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym {
    Nine,
    Z,
    Star,
    /// The first symbol of a floating insertion string: a sign or currency position, not a digit.
    FloatLead(char),
    /// A later symbol of a floating insertion string: a digit position.
    Float(char),
    /// A fixed + or -.
    Sign(char),
    Currency,
    Cr,
    Db,
    Point,
    /// V: the decimal point, occupying no position.
    Implied,
    /// B (as a space), 0, / or a comma.
    Insert(char),
    /// X, A or 9 in an alphanumeric-edited PICTURE.
    Char,
}

impl Sym {
    pub fn is_digit(&self) -> bool {
        matches!(self, Sym::Nine | Sym::Z | Sym::Star | Sym::Float(_))
    }

    /// Character positions the symbol occupies in the edited item.
    pub fn width(&self) -> usize {
        match self {
            Sym::Implied => 0,
            Sym::Cr | Sym::Db => 2,
            _ => 1,
        }
    }
}
