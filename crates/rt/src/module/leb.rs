//! Unsigned LEB128 and zigzag (load-module.md §4.1), with only the canonical form accepted.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LebError {
    /// The bytes end before the last group.
    End,
    /// A zero final group after a continuation, as `80 00`.
    OverLong,
    /// More than 64 bits.
    Overflow,
}

pub fn write(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let group = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(group);
            return;
        }
        out.push(group | 0x80);
    }
}

/// The value at the front of `bytes`, and how many bytes it takes.
pub fn read(bytes: &[u8]) -> Result<(u64, usize), LebError> {
    let mut value = 0u64;
    for (i, &byte) in bytes.iter().enumerate() {
        if i == 9 && byte > 1 {
            return Err(LebError::Overflow);
        }
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return if byte == 0 && i > 0 { Err(LebError::OverLong) } else { Ok((value, i + 1)) };
        }
    }
    Err(LebError::End)
}

pub const fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

pub const fn unzigzag(z: u64) -> i64 {
    (z >> 1) as i64 ^ -((z & 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        write(&mut out, value);
        out
    }

    #[test]
    fn seven_bits_per_byte_low_group_first() {
        assert_eq!(bytes(0), [0x00]);
        assert_eq!(bytes(127), [0x7F]);
        assert_eq!(bytes(128), [0x80, 0x01]);
        assert_eq!(bytes(1140), [0xF4, 0x08]);
        assert_eq!(bytes(16_384), [0x80, 0x80, 0x01]);
        assert_eq!(bytes(u64::MAX), [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
    }

    #[test]
    fn every_width_reads_back_and_reports_its_length() {
        for shift in 0..64 {
            for value in [1u64 << shift, (1u64 << shift) - 1, (1u64 << shift) + 1] {
                let encoded = bytes(value);
                assert_eq!(read(&encoded), Ok((value, encoded.len())), "{value}");
            }
        }
        assert_eq!(read(&[0x05, 0xFF]), Ok((5, 1)));
    }

    #[test]
    fn only_the_canonical_form_is_read() {
        assert_eq!(read(&[0x80, 0x00]), Err(LebError::OverLong));
        assert_eq!(read(&[0xFF, 0x80, 0x00]), Err(LebError::OverLong));
        assert_eq!(read(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00]), Err(LebError::OverLong));
        assert_eq!(read(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02]), Err(LebError::Overflow));
        assert_eq!(read(&[0x80; 11]), Err(LebError::Overflow));
        assert_eq!(read(&[]), Err(LebError::End));
        assert_eq!(read(&[0x80]), Err(LebError::End));
        assert_eq!(read(&[0xFF, 0xFF]), Err(LebError::End));
    }

    #[test]
    fn zigzag_interleaves_the_signs() {
        let pairs = [(0, 0), (-1, 1), (1, 2), (-2, 3), (2, 4), (i64::MAX, u64::MAX - 1), (i64::MIN, u64::MAX)];
        for (n, z) in pairs {
            assert_eq!(zigzag(n), z, "{n}");
            assert_eq!(unzigzag(z), n, "{z}");
        }
        assert_eq!(zigzag(i64::from(i32::MIN)), u64::from(u32::MAX));
    }
}
