//! The codec traits, the writer and reader they use, the §4.1 and §4.4 impls, and the §4.6 macros.

use std::collections::BTreeMap;

use super::ModuleError;
use super::leb::{self, LebError};
use super::strings::{Interner, StringTable};

pub trait Encode {
    fn encode(&self, w: &mut Writer);
}

pub trait Decode: Sized {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError>;
}

/// Section bytes being built, and the string table they intern into.
#[derive(Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
    strings: Interner,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn byte(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn leb(&mut self, value: u64) {
        leb::write(&mut self.bytes, value);
    }

    pub fn zigzag(&mut self, value: i64) {
        self.leb(leb::zigzag(value));
    }

    pub fn count(&mut self, count: usize) {
        self.leb(count as u64);
    }

    /// Writes the string's index in the table, adding it on first use.
    pub fn string(&mut self, text: &str) {
        let index = self.strings.intern(text);
        self.count(index);
    }

    /// The bytes written since the last `take`. The string table carries on.
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    pub fn strings(&self) -> &StringTable {
        self.strings.table()
    }
}

/// A bounds-checked cursor over one section's bytes.
#[derive(Clone, Debug)]
pub struct Reader<'r> {
    bytes: &'r [u8],
    at: usize,
    section: &'static str,
    strings: &'r StringTable,
}

impl<'r> Reader<'r> {
    pub fn new(section: &'static str, bytes: &'r [u8], strings: &'r StringTable) -> Self {
        Self { bytes, at: 0, section, strings }
    }

    pub fn position(&self) -> usize {
        self.at
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    pub fn malformed(&self, at: usize, reason: impl Into<String>) -> ModuleError {
        ModuleError::Malformed { section: self.section, offset: at, reason: reason.into() }
    }

    pub fn byte(&mut self) -> Result<u8, ModuleError> {
        Ok(self.bytes(1)?[0])
    }

    pub fn bytes(&mut self, len: usize) -> Result<&'r [u8], ModuleError> {
        let taken = self.at.checked_add(len).and_then(|end| self.bytes.get(self.at..end));
        let taken = taken.ok_or_else(|| {
            self.malformed(self.at, format!("reads past the end: {len} wanted, {} left", self.remaining()))
        })?;
        self.at += len;
        Ok(taken)
    }

    pub fn leb(&mut self) -> Result<u64, ModuleError> {
        let (value, len) = leb::read(&self.bytes[self.at..]).map_err(|e| {
            self.malformed(
                self.at,
                match e {
                    LebError::End => "the bytes end inside an integer",
                    LebError::OverLong => "an integer is not in its shortest form",
                    LebError::Overflow => "an integer overflows 64 bits",
                },
            )
        })?;
        self.at += len;
        Ok(value)
    }

    pub fn zigzag(&mut self) -> Result<i64, ModuleError> {
        Ok(leb::unzigzag(self.leb()?))
    }

    /// A count of elements, each of which takes at least one byte, so no more than remain.
    pub fn count(&mut self) -> Result<usize, ModuleError> {
        let at = self.at;
        let count = self.leb()?;
        let remaining = self.remaining();
        usize::try_from(count)
            .ok()
            .filter(|&n| n <= remaining)
            .ok_or_else(|| self.malformed(at, format!("a count of {count} with {remaining} bytes left")))
    }

    pub fn string(&mut self) -> Result<&'r str, ModuleError> {
        let at = self.at;
        let index = self.leb()?;
        let strings = self.strings;
        usize::try_from(index)
            .ok()
            .and_then(|i| strings.get(i))
            .ok_or_else(|| self.malformed(at, format!("string {index} of a table of {}", strings.len())))
    }

    /// Refuses bytes left after the last value.
    pub fn finish(self) -> Result<(), ModuleError> {
        match self.remaining() {
            0 => Ok(()),
            left => Err(self.malformed(self.at, format!("bytes left after the last value: {left}"))),
        }
    }
}

/// One `T` that fills `bytes` exactly.
pub fn decode_all<T: Decode>(section: &'static str, bytes: &[u8], strings: &StringTable) -> Result<T, ModuleError> {
    let mut r = Reader::new(section, bytes, strings);
    let value = T::decode(&mut r)?;
    r.finish()?;
    Ok(value)
}

/// The `check` of a `codec_struct!` that names none.
pub fn unchecked<T>(_: &T) -> Result<(), String> {
    Ok(())
}

impl Encode for u8 {
    fn encode(&self, w: &mut Writer) {
        w.byte(*self);
    }
}

impl Decode for u8 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        r.byte()
    }
}

impl Encode for bool {
    fn encode(&self, w: &mut Writer) {
        w.byte(u8::from(*self));
    }
}

impl Decode for bool {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        match r.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(r.malformed(at, format!("bool {other}"))),
        }
    }
}

macro_rules! unsigned {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            fn encode(&self, w: &mut Writer) {
                w.leb(*self as u64);
            }
        }

        impl Decode for $t {
            fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
                let at = r.position();
                let value = r.leb()?;
                <$t>::try_from(value).map_err(|_| r.malformed(at, format!("{value} overflows {}", stringify!($t))))
            }
        }
    )*};
}

unsigned!(u16, u32, u64, usize);

macro_rules! signed {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            fn encode(&self, w: &mut Writer) {
                w.zigzag(i64::from(*self));
            }
        }

        impl Decode for $t {
            fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
                let at = r.position();
                let value = r.zigzag()?;
                <$t>::try_from(value).map_err(|_| r.malformed(at, format!("{value} overflows {}", stringify!($t))))
            }
        }
    )*};
}

signed!(i16, i32, i64);

impl Encode for char {
    fn encode(&self, w: &mut Writer) {
        w.leb(u64::from(*self));
    }
}

impl Decode for char {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        let value = r.leb()?;
        u32::try_from(value)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| r.malformed(at, format!("{value:#x} is not a Unicode scalar value")))
    }
}

const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

impl Encode for f64 {
    fn encode(&self, w: &mut Writer) {
        let bits = if self.is_nan() { CANONICAL_NAN } else { self.to_bits() };
        w.bytes(&bits.to_le_bytes());
    }
}

impl Decode for f64 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        let mut bits = [0u8; 8];
        bits.copy_from_slice(r.bytes(8)?);
        let value = f64::from_le_bytes(bits);
        if value.is_nan() && value.to_bits() != CANONICAL_NAN {
            return Err(r.malformed(at, "a NaN other than the canonical one"));
        }
        Ok(value)
    }
}

impl Encode for String {
    fn encode(&self, w: &mut Writer) {
        w.string(self);
    }
}

impl Decode for String {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        r.string().map(str::to_owned)
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, w: &mut Writer) {
        w.count(self.len());
        for item in self {
            item.encode(w);
        }
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let count = r.count()?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            items.push(T::decode(r)?);
        }
        Ok(items)
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, w: &mut Writer) {
        for item in self {
            item.encode(w);
        }
    }
}

impl<T: Decode, const N: usize> Decode for [T; N] {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        if N > r.remaining() {
            return Err(r.malformed(at, format!("an array of {N} with {} bytes left", r.remaining())));
        }
        let mut items = Vec::with_capacity(N);
        for _ in 0..N {
            items.push(T::decode(r)?);
        }
        items.try_into().map_err(|_| r.malformed(at, format!("an array of {N}")))
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, w: &mut Writer) {
        match self {
            None => w.byte(0),
            Some(value) => {
                w.byte(1);
                value.encode(w);
            }
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        match r.byte()? {
            0 => Ok(None),
            1 => T::decode(r).map(Some),
            other => Err(r.malformed(at, format!("Option tag {other}"))),
        }
    }
}

impl<T: Encode, E: Encode> Encode for Result<T, E> {
    fn encode(&self, w: &mut Writer) {
        match self {
            Ok(value) => {
                w.byte(0);
                value.encode(w);
            }
            Err(error) => {
                w.byte(1);
                error.encode(w);
            }
        }
    }
}

impl<T: Decode, E: Decode> Decode for Result<T, E> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        match r.byte()? {
            0 => T::decode(r).map(Ok),
            1 => E::decode(r).map(Err),
            other => Err(r.malformed(at, format!("Result tag {other}"))),
        }
    }
}

impl<T: Encode> Encode for Box<T> {
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}

impl<T: Decode> Decode for Box<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        T::decode(r).map(Box::new)
    }
}

macro_rules! tuple {
    ($($name:ident $index:tt),+) => {
        impl<$($name: Encode),+> Encode for ($($name,)+) {
            fn encode(&self, w: &mut Writer) {
                $(self.$index.encode(w);)+
            }
        }

        impl<$($name: Decode),+> Decode for ($($name,)+) {
            fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
                Ok(($($name::decode(r)?,)+))
            }
        }
    };
}

tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);

impl<K: Encode, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, w: &mut Writer) {
        w.count(self.len());
        for (key, value) in self {
            key.encode(w);
            value.encode(w);
        }
    }
}

impl<K: Decode + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let count = r.count()?;
        let mut map = BTreeMap::new();
        for _ in 0..count {
            let at = r.position();
            let key = K::decode(r)?;
            if map.last_key_value().is_some_and(|(last, _)| *last >= key) {
                return Err(r.malformed(at, "map keys that do not strictly ascend"));
            }
            map.insert(key, V::decode(r)?);
        }
        Ok(map)
    }
}

/// `Encode` and `Decode` from one field list; `check` names a `fn(&T) -> Result<(), String>`.
#[macro_export]
macro_rules! codec_struct {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        $crate::codec_struct!($ty { $($field),* } check $crate::module::codec::unchecked);
    };
    ($ty:ident { $($field:ident),* $(,)? } check $check:path) => {
        impl $crate::module::codec::Encode for $ty {
            fn encode(&self, w: &mut $crate::module::codec::Writer) {
                let $ty { $($field),* } = self;
                $($crate::module::codec::Encode::encode($field, w);)*
            }
        }

        impl $crate::module::codec::Decode for $ty {
            fn decode(
                r: &mut $crate::module::codec::Reader<'_>,
            ) -> ::core::result::Result<Self, $crate::module::ModuleError> {
                let at = r.position();
                $(let $field = $crate::module::codec::Decode::decode(r)?;)*
                let value = $ty { $($field),* };
                $check(&value).map_err(|reason| r.malformed(at, reason))?;
                ::core::result::Result::Ok(value)
            }
        }
    };
}

/// `Encode` and `Decode` from the variants and their tags: `Unit = 0`, `Named { a } = 1`, `Tuple(a) = 2`.
#[macro_export]
macro_rules! codec_enum {
    ($ty:ident {
        $($variant:ident $({ $($field:ident),* $(,)? })? $(( $($elem:ident),* $(,)? ))? = $tag:literal),* $(,)?
    }) => {
        impl $crate::module::codec::Encode for $ty {
            fn encode(&self, w: &mut $crate::module::codec::Writer) {
                match self {
                    $($ty::$variant $({ $($field),* })? $(( $($elem),* ))? => {
                        w.leb($tag);
                        $($($crate::module::codec::Encode::encode($field, w);)*)?
                        $($($crate::module::codec::Encode::encode($elem, w);)*)?
                    })*
                }
            }
        }

        impl $crate::module::codec::Decode for $ty {
            fn decode(
                r: &mut $crate::module::codec::Reader<'_>,
            ) -> ::core::result::Result<Self, $crate::module::ModuleError> {
                let at = r.position();
                match r.leb()? {
                    $($tag => {
                        $($(let $field = $crate::module::codec::Decode::decode(r)?;)*)?
                        $($(let $elem = $crate::module::codec::Decode::decode(r)?;)*)?
                        ::core::result::Result::Ok($ty::$variant $({ $($field),* })? $(( $($elem),* ))?)
                    })*
                    tag => ::core::result::Result::Err(r.malformed(at, format!("{} has no tag {tag}", stringify!($ty)))),
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded<T: Encode>(value: &T) -> (Vec<u8>, StringTable) {
        let mut w = Writer::new();
        value.encode(&mut w);
        (w.take(), w.strings().clone())
    }

    fn round_trip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let (bytes, strings) = encoded(&value);
        assert_eq!(decode_all::<T>("TEST", &bytes, &strings), Ok(value));
    }

    fn refused<T: Decode + std::fmt::Debug>(bytes: &[u8]) -> String {
        match decode_all::<T>("TEST", bytes, &StringTable::default()) {
            Err(ModuleError::Malformed { section: "TEST", reason, .. }) => reason,
            other => panic!("{bytes:02X?} decoded as {other:?}"),
        }
    }

    #[test]
    fn integers_round_trip_at_their_limits() {
        for v in [0, 1, 0x7F, 0x80, u8::MAX] {
            round_trip(v);
        }
        for v in [0, 0x80, u16::MAX] {
            round_trip(v);
        }
        for v in [0, 0x3FFF, 0x4000, u32::MAX] {
            round_trip(v);
        }
        for v in [0, u64::from(u32::MAX) + 1, u64::MAX] {
            round_trip(v);
        }
        for v in [0, usize::MAX] {
            round_trip(v);
        }
        for v in [i16::MIN, -1, 0, 1, i16::MAX] {
            round_trip(v);
        }
        for v in [i32::MIN, -64, 63, i32::MAX] {
            round_trip(v);
        }
        for v in [i64::MIN, -1, 0, i64::MAX] {
            round_trip(v);
        }
        round_trip(false);
        round_trip(true);
        for c in ['\0', 'A', 'é', '€', '\u{10FFFF}'] {
            round_trip(c);
        }
    }

    #[test]
    fn integers_have_the_documented_bytes() {
        assert_eq!(encoded(&1140u16).0, [0xF4, 0x08]);
        assert_eq!(encoded(&-1i32).0, [0x01]);
        assert_eq!(encoded(&-65i64).0, [0x81, 0x01]);
        assert_eq!(encoded(&200u8).0, [200]);
        assert_eq!(encoded(&true).0, [1]);
        assert_eq!(encoded(&'€').0, [0xAC, 0x41]);
    }

    #[test]
    fn a_value_that_overflows_its_type_is_malformed() {
        assert_eq!(refused::<u16>(&[0x80, 0x80, 0x04]), "65536 overflows u16");
        assert_eq!(refused::<u32>(&[0x80, 0x80, 0x80, 0x80, 0x10]), "4294967296 overflows u32");
        assert_eq!(refused::<i16>(&[0x80, 0x80, 0x04]), "32768 overflows i16");
        assert_eq!(refused::<i32>(&[0x81, 0x80, 0x80, 0x80, 0x10]), "-2147483649 overflows i32");
        assert_eq!(refused::<bool>(&[2]), "bool 2");
        assert_eq!(refused::<char>(&[0x80, 0xB0, 0x03]), "0xd800 is not a Unicode scalar value");
        assert_eq!(refused::<char>(&[0x80, 0x80, 0x44]), "0x110000 is not a Unicode scalar value");
    }

    #[test]
    fn integers_are_read_in_their_shortest_form_only() {
        assert_eq!(refused::<u64>(&[0x80, 0x00]), "an integer is not in its shortest form");
        assert_eq!(refused::<i64>(&[0x81, 0x00]), "an integer is not in its shortest form");
        assert_eq!(refused::<u64>(&[0xFF; 10]), "an integer overflows 64 bits");
        assert_eq!(refused::<u32>(&[0x80]), "the bytes end inside an integer");
    }

    #[test]
    fn any_nan_is_written_as_the_canonical_nan() {
        let odd = f64::from_bits(0xFFF0_0000_0000_0001);
        assert!(odd.is_nan());
        assert_eq!(encoded(&odd).0, CANONICAL_NAN.to_le_bytes());
        assert_eq!(refused::<f64>(&0xFFF0_0000_0000_0001u64.to_le_bytes()), "a NaN other than the canonical one");
        for v in [0.0, -0.0, 1.5, f64::MIN_POSITIVE, f64::INFINITY, f64::NEG_INFINITY] {
            let (bytes, strings) = encoded(&v);
            assert_eq!(decode_all::<f64>("TEST", &bytes, &strings).map(f64::to_bits), Ok(v.to_bits()));
        }
        assert_eq!(refused::<f64>(&[0; 7]), "reads past the end: 8 wanted, 7 left");
    }

    #[test]
    fn strings_are_indices_into_the_table() {
        let value = vec!["B".to_owned(), "A".to_owned(), "B".to_owned(), String::new()];
        let (bytes, strings) = encoded(&value);
        assert_eq!(bytes, [4, 0, 1, 0, 2]);
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["B", "A", ""]);
        round_trip(value);
        assert_eq!(refused::<String>(&[0]), "string 0 of a table of 0");
    }

    #[test]
    fn containers_round_trip() {
        round_trip(Vec::<u32>::new());
        round_trip(vec![1u8, 2, 3]);
        round_trip(vec![Some(-5i64), None]);
        round_trip([7u16, 300, 65_535]);
        round_trip(Some(Some(false)));
        round_trip(Ok::<u8, String>(4));
        round_trip(Err::<u8, String>("bad".to_owned()));
        round_trip(Box::new(9u32));
        round_trip((true, 5u32));
        round_trip(("X".to_owned(), 1u8, -1i32, None::<u8>));
        round_trip(BTreeMap::from([(3u32, "c".to_owned()), (1, "a".to_owned())]));
        assert_eq!(encoded(&vec![1u8, 2]).0, [2, 1, 2]);
        assert_eq!(encoded(&[1u8, 2]).0, [1, 2]);
        assert_eq!(encoded(&Some(7u8)).0, [1, 7]);
        assert_eq!(encoded(&None::<u8>).0, [0]);
        assert_eq!(encoded(&Err::<u8, u8>(3)).0, [1, 3]);
        assert_eq!(encoded(&BTreeMap::from([(2u8, 0u8), (1, 9)])).0, [2, 1, 9, 2, 0]);
    }

    #[test]
    fn a_bad_container_is_malformed() {
        assert_eq!(refused::<Vec<u8>>(&[5, 1, 2]), "a count of 5 with 2 bytes left");
        assert_eq!(refused::<Vec<u8>>(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]), "a count of 4294967295 with 0 bytes left");
        assert_eq!(refused::<Option<u8>>(&[2, 0]), "Option tag 2");
        assert_eq!(refused::<Result<u8, u8>>(&[2, 0]), "Result tag 2");
        assert_eq!(refused::<[u8; 3]>(&[1, 2]), "an array of 3 with 2 bytes left");
        assert_eq!(refused::<BTreeMap<u8, u8>>(&[2, 1, 0, 1, 0]), "map keys that do not strictly ascend");
        assert_eq!(refused::<BTreeMap<u8, u8>>(&[2, 2, 0, 1, 0]), "map keys that do not strictly ascend");
        assert_eq!(refused::<u8>(&[1, 2]), "bytes left after the last value: 1");
        assert_eq!(refused::<u8>(&[]), "reads past the end: 1 wanted, 0 left");
    }

    #[test]
    fn an_error_names_the_offset_it_found() {
        let err = decode_all::<(u8, u8, bool)>("LAYOUT", &[0, 0, 7], &StringTable::default());
        assert_eq!(err, Err(ModuleError::Malformed { section: "LAYOUT", offset: 2, reason: "bool 7".into() }));
        assert_eq!(err.unwrap_err().to_string(), "LAYOUT is malformed at byte 2: bool 7");
    }

    #[test]
    fn the_same_value_encodes_to_the_same_bytes() {
        let value = (vec!["Z".to_owned(), "A".to_owned()], BTreeMap::from([(2i32, 'x'), (-1, 'y')]), f64::NAN);
        assert_eq!(encoded(&value), encoded(&value));
    }
}
