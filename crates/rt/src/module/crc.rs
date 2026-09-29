//! CRC-32: the IEEE polynomial, reflected, initial and final XOR `0xFFFFFFFF` (load-module.md §3.3).

const TABLE: [u32; 256] = table();

const fn table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            bit += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

pub fn crc32(bytes: &[u8]) -> u32 {
    extend(0, bytes)
}

/// The checksum of the bytes `crc` was computed over, followed by `bytes`.
pub fn extend(crc: u32, bytes: &[u8]) -> u32 {
    let mut c = !crc;
    for &byte in bytes {
        c = TABLE[usize::from(c as u8 ^ byte)] ^ (c >> 8);
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ieee_check_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
    }

    #[test]
    fn extending_equals_one_pass() {
        let text = b"The quick brown fox jumps over the lazy dog";
        for split in 0..=text.len() {
            assert_eq!(extend(crc32(&text[..split]), &text[split..]), crc32(text));
        }
    }
}
