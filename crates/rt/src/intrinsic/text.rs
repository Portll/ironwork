//! HEX-OF, HEX-TO-CHAR, BIT-OF, BIT-TO-CHAR and UUID4 (Language Reference SC27-8713-03,
//! pp. 527-529, 569-571, 669). Their characters are returned as text, for the caller to encode in
//! the program's code page.

pub fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

pub fn bit_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:08b}")).collect()
}

/// The bytes the hexadecimal digits spell, or the 1-based position of the first character that
/// is not one, or 0 for an odd count.
pub fn hex_to_char(text: &str) -> Result<Vec<u8>, usize> {
    let digits: Vec<char> = text.chars().collect();
    if let Some(bad) = digits.iter().position(|c| !c.is_ascii_hexdigit()) {
        return Err(bad + 1);
    }
    if !digits.len().is_multiple_of(2) {
        return Err(0);
    }
    Ok(digits.chunks(2).map(|p| (p[0].to_digit(16).unwrap() * 16 + p[1].to_digit(16).unwrap()) as u8).collect())
}

/// The bytes the 0s and 1s spell, or the 1-based position of the first other character, or 0
/// when the count is not a multiple of eight.
pub fn bit_to_char(text: &str) -> Result<Vec<u8>, usize> {
    let bits: Vec<char> = text.chars().collect();
    if let Some(bad) = bits.iter().position(|c| !matches!(c, '0' | '1')) {
        return Err(bad + 1);
    }
    if !bits.len().is_multiple_of(8) {
        return Err(0);
    }
    Ok(bits.chunks(8).map(|byte| byte.iter().fold(0u8, |acc, &c| acc << 1 | (c == '1') as u8)).collect())
}

/// A version 4, variant 1 UUID from 128 random bits, in lowercase as IBM's example shows it.
pub fn uuid4(random: u128) -> String {
    let v = (random & !(0xF000u128 << 64) | (0x4000u128 << 64)) & !(0xC000u128 << 48) | (0x8000u128 << 48);
    let h = format!("{v:032x}");
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_bits_follow_the_language_references_examples() {
        let hello = [0xC8, 0x85, 0x93, 0x93, 0x96, 0x6B, 0x40, 0xA6, 0x96, 0x99, 0x93, 0x84, 0x5A];
        assert_eq!(hex_of(&hello), "C8859393966B40A6969993845A");
        assert_eq!(hex_of(&[0, 0, 0, 12]), "0000000C");
        assert_eq!(bit_of(&[0x12, 0x34, 0x5F]), "000100100011010001011111");
        assert_eq!(bit_of(&[0x00, 0x20]), "0000000000100000");
        assert_eq!(hex_to_char("FFAABB"), Ok(vec![0xFF, 0xAA, 0xBB]));
        assert_eq!(hex_to_char("ffaa"), Ok(vec![0xFF, 0xAA]));
        assert_eq!(hex_to_char("FFG0"), Err(3));
        assert_eq!(hex_to_char("FFA"), Err(0));
        assert_eq!(bit_to_char("1111110010001000"), Ok(vec![0xFC, 0x88]));
        assert_eq!(bit_to_char("1111110"), Err(0));
        assert_eq!(bit_to_char("11112"), Err(5));
    }

    #[test]
    fn uuid4_sets_the_version_and_variant() {
        let u = uuid4(u128::MAX);
        assert_eq!(u, "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let u = uuid4(0);
        assert_eq!(u, "00000000-0000-4000-8000-000000000000");
        assert_eq!(u.len(), 36);
    }
}
