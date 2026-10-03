//! The mapset model BMS map source describes: each map's fields with their positions, attributes
//! and initial data, as SEND MAP and RECEIVE MAP use it, and as a load module's `BMS` section holds
//! it (load-module.md §5.3).

use crate::{codec_enum, codec_struct};

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

codec_enum!(Mode { In = 0, Out = 1, InOut = 2 });
codec_enum!(Protection { Askip = 0, Prot = 1, Unprot = 2 });
codec_enum!(Intensity { Norm = 0, Brt = 1, Drk = 2 });
codec_enum!(Initial { Text(text) = 0, Bytes(bytes) = 1 });
codec_struct!(Attrb { protection, numeric, intensity, detectable, cursor, fset });
codec_struct!(Field { name, line, column, length, attrb, initial, picin, picout, occurs, group, justify_right, fill_zero, color, hilight });
codec_struct!(Map { name, lines, columns, line, column, ctrl, tioapfx, dsatts, fields });
codec_struct!(Mapset { name, mode, ctrl, maps });

/// The extended attributes in symbolic-map order: name, field-name suffix, and whether EXTATT=YES implies it.
pub const EXTENDED: &[(&str, char, bool)] = &[
    ("COLOR", 'C', true),
    ("PS", 'P', true),
    ("HILIGHT", 'H', true),
    ("VALIDN", 'V', true),
    ("OUTLINE", 'U', false),
    ("SOSI", 'M', false),
    ("TRANSP", 'T', false),
];

/// The bytes a PICIN or PICOUT picture occupies: S and V take none.
pub fn picture_size(pic: &str) -> u32 {
    let chars: Vec<char> = pic.chars().collect();
    let (mut size, mut i) = (0, 0);
    while i < chars.len() {
        let c = chars[i].to_ascii_uppercase();
        i += 1;
        let mut repeat = 1;
        if chars.get(i) == Some(&'(')
            && let Some(close) = chars[i..].iter().position(|&c| c == ')')
        {
            repeat = chars[i + 1..i + close].iter().collect::<String>().parse().unwrap_or(1);
            i += close + 1;
        }
        if !matches!(c, 'S' | 'V') {
            size += repeat;
        }
    }
    size
}

/// Where one occurrence of a named field lies in a map's symbolic structure: the offsets of its L
/// and F/A bytes and extended attributes (for the first member of a group, or a lone field), and
/// of its data. They are the offsets the symbolic map declares, which SEND MAP and RECEIVE MAP use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub field: usize,
    pub occurrence: u16,
    pub control: Option<usize>,
    pub extended: usize,
    pub data: usize,
    pub size: usize,
}

impl Slot {
    /// The L halfword, then the F/A byte, when the slot has them.
    pub fn length_at(&self) -> Option<usize> {
        self.control
    }

    pub fn attribute_at(&self) -> Option<usize> {
        self.control.map(|c| c + 2)
    }
}

/// The DSATTS attributes a map's symbolic structure carries, in their order.
pub fn extended_attributes(map: &Map) -> Vec<&'static str> {
    EXTENDED.iter().filter(|(a, _, _)| map.dsatts.iter().any(|d| d == a)).map(|(a, _, _)| *a).collect()
}

/// Every named field's slots, in structure order, sized for the input side (PICIN) or the output
/// side (PICOUT).
pub fn slots(map: &Map, input: bool) -> Vec<Slot> {
    let k = extended_attributes(map).len();
    let mut at = if map.tioapfx { 12 } else { 0 };
    let mut out = Vec::new();
    let mut group: Option<&str> = None;
    for (i, f) in map.fields.iter().enumerate().filter(|(_, f)| f.name.is_some()) {
        let lead = match &f.group {
            Some(g) => {
                let first = group != Some(g.as_str());
                group = Some(g);
                first
            }
            None => {
                group = None;
                true
            }
        };
        let picture = if input { &f.picin } else { &f.picout };
        let size = picture.as_deref().map_or(usize::from(f.length), |p| picture_size(p) as usize);
        let copies = if f.group.is_none() { f.occurs.max(1) } else { 1 };
        for occurrence in 0..copies {
            let control = lead.then_some(at);
            if lead {
                at += 3 + k;
            }
            out.push(Slot { field: i, occurrence, control, extended: at - k, data: at, size });
            at += size;
        }
    }
    out
}
