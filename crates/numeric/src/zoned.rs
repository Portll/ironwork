//! Zoned decimal into packed, as the generated code does it. PACK takes at most 16 bytes in each
//! operand, so an item of more than 16 digits is packed in parts; see
//! [`crate::assumptions::LONG_ZONED_BY_PACKS`].

use zarch::check::ProgramCheck;
use zarch::decimal;

/// PACK's longest second operand, and the zoned bytes each lower part takes: 15 digits and the
/// sign fill 8 packed bytes.
const PACK_MAX: usize = 16;
const PART: usize = 15;

/// `zoned` packed into `zoned.len() / 2 + 1` bytes: every zone but the last discarded, the last
/// zone the sign. Nothing is validated, as PACK validates nothing.
pub fn pack(zoned: &[u8]) -> Result<Vec<u8>, ProgramCheck> {
    let mut packed = vec![0u8; zoned.len() / 2 + 1];
    pack_into(&mut packed, zoned)?;
    Ok(packed)
}

/// The high-order part is packed first, into one byte more than it owns: that byte's low nibble
/// is the zone of the part's last digit, and the PACK of the part below overwrites it.
fn pack_into(packed: &mut [u8], zoned: &[u8]) -> Result<(), ProgramCheck> {
    if zoned.len() <= PACK_MAX {
        return decimal::pack(packed, zoned);
    }
    let (high, low) = (zoned.len() - PART, packed.len() - PART.div_ceil(2));
    pack_into(&mut packed[..=low], &zoned[..=high])?;
    decimal::pack(&mut packed[low..], &zoned[high..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What one PACK would give if it took any length: each digit nibble, then the last zone.
    fn one_pack(zoned: &[u8]) -> Vec<u8> {
        let mut nibbles: Vec<u8> = zoned.iter().map(|b| b & 0x0F).collect();
        nibbles.push(zoned.last().unwrap() >> 4);
        if nibbles.len() % 2 == 1 {
            nibbles.insert(0, 0);
        }
        nibbles.chunks(2).map(|p| p[0] << 4 | p[1]).collect()
    }

    fn zoned(digits: &str, sign_zone: u8) -> Vec<u8> {
        let mut z: Vec<u8> = digits.bytes().map(|d| 0xF0 | (d - b'0')).collect();
        let last = z.len() - 1;
        z[last] = sign_zone << 4 | (z[last] & 0x0F);
        z
    }

    #[test]
    fn eighteen_digits_pack_into_ten_bytes() {
        let z = zoned("999999999999999999", 0xF);
        assert_eq!(pack(&z).unwrap(), [0x09, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9F]);
        let n = zoned("123456789012345678", 0xD);
        let p = pack(&n).unwrap();
        assert_eq!(p, [0x01, 0x23, 0x45, 0x67, 0x89, 0x01, 0x23, 0x45, 0x67, 0x8D]);
        assert_eq!(decimal::decode(&p).unwrap(), decimal::Decimal { negative: true, magnitude: 123_456_789_012_345_678 });
    }

    #[test]
    fn every_length_packs_as_one_pack_would() {
        let digits = "9876543210123456789012345678901";
        for len in 1..=31 {
            for sign in [0xC, 0xD, 0xF, 0x4] {
                let mut z = zoned(&digits[..len], sign);
                if len > 2 {
                    z[1] = 0x41;
                }
                assert_eq!(pack(&z).unwrap(), one_pack(&z), "{len} digits, sign {sign:X}");
            }
        }
    }

    #[test]
    fn an_invalid_digit_survives_to_the_packed_form() {
        let mut z = zoned("12345678901234567", 0xC);
        z[3] = 0xFA;
        let p = pack(&z).unwrap();
        assert_eq!(decimal::decode(&p), Err(ProgramCheck::Data));
    }

    #[test]
    fn nothing_is_there_to_pack_in_an_empty_operand() {
        assert_eq!(pack(&[]), Err(ProgramCheck::Specification));
    }
}
