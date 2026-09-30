//! The mapset model BMS map source describes: each map's fields with their positions, attributes
//! and initial data, as SEND MAP and RECEIVE MAP use it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Initial {
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    Askip,
    Prot,
    Unprot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intensity {
    Norm,
    Brt,
    Drk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attrb {
    pub protection: Protection,
    pub numeric: bool,
    pub intensity: Intensity,
    pub detectable: bool,
    pub cursor: bool,
    pub fset: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Upper-cased label; None for an unlabelled field.
    pub name: Option<String>,
    /// 1-based position of the attribute byte within the map.
    pub line: u16,
    pub column: u16,
    pub length: u16,
    pub attrb: Attrb,
    /// INITIAL or GINIT text, or XINIT's bytes.
    pub initial: Option<Initial>,
    pub picin: Option<String>,
    pub picout: Option<String>,
    pub occurs: u16,
    pub group: Option<String>,
    pub justify_right: bool,
    pub fill_zero: bool,
    pub color: Option<String>,
    pub hilight: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Map {
    pub name: String,
    pub lines: u16,
    pub columns: u16,
    pub line: u16,
    pub column: u16,
    pub ctrl: Vec<String>,
    pub tioapfx: bool,
    pub dsatts: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapset {
    pub name: String,
    pub mode: Mode,
    pub ctrl: Vec<String>,
    pub maps: Vec<Map>,
}
