//! SCRAM-SHA-256 client authentication (RFC 5802, RFC 7677), on the runtime's SHA-256 and HMAC
//! ([`crate::digest`]), with base64 written here, as the workspace takes no dependencies.

pub use crate::digest::{hmac, sha256};

/// RFC 5802's Hi: PBKDF2 with HMAC-SHA-256, one output block.
pub fn hi(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut u = hmac(password, &[salt, &1u32.to_be_bytes()].concat());
    let mut out = u;
    for _ in 1..iterations {
        u = hmac(password, &u);
        out.iter_mut().zip(u).for_each(|(o, x)| *o ^= x);
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    out
}

pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u32> = text.trim_end_matches('=').bytes().map(|c| ALPHABET.iter().position(|&a| a == c).map(|p| p as u32)).collect::<Option<_>>()?;
    if digits.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::new();
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &d)| n | d << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// A client nonce: 18 bytes from the operating system's random generator where std can read one
/// (/dev/urandom on Unix), and elsewhere from std's OS-seeded hasher keys, the process and the clock
/// (assumption SQ6).
pub fn nonce() -> String {
    let mut bytes = [0u8; 18];
    if system_random(&mut bytes) {
        return base64(&bytes);
    }
    use std::hash::{BuildHasher, Hasher};
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let bytes: Vec<u8> = (0..3u64)
        .flat_map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(now);
            h.write_u32(std::process::id());
            h.finish().to_le_bytes()
        })
        .collect();
    base64(&bytes)
}

#[cfg(unix)]
fn system_random(bytes: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(bytes)).is_ok()
}

#[cfg(not(unix))]
fn system_random(_: &mut [u8]) -> bool {
    false
}

/// One SCRAM-SHA-256 exchange, without channel binding.
pub struct Scram {
    password: Vec<u8>,
    nonce: String,
    first_bare: String,
    server_signature: Option<[u8; 32]>,
}

impl Scram {
    /// The password is used as its bytes, without SASLprep, which changes only non-ASCII passwords.
    pub fn new(password: &str, nonce: String) -> Self {
        Self { password: password.as_bytes().to_vec(), nonce, first_bare: String::new(), server_signature: None }
    }

    /// PostgreSQL ignores the user named here and takes the startup message's.
    pub fn client_first(&mut self, user: &str) -> String {
        self.first_bare = format!("n={user},r={}", self.nonce);
        format!("n,,{}", self.first_bare)
    }

    pub fn client_final(&mut self, server_first: &str) -> Result<String, String> {
        let field = |key: &str| server_first.split(',').find_map(|f| f.strip_prefix(key)).ok_or_else(|| format!("the server's SCRAM message has no {key}"));
        let nonce = field("r=")?;
        if !nonce.starts_with(&self.nonce) || nonce.len() == self.nonce.len() {
            return Err("the server's SCRAM nonce does not extend the client's".into());
        }
        let salt = unbase64(field("s=")?).ok_or("the server's SCRAM salt is not base64")?;
        let iterations: u32 = field("i=")?.parse().map_err(|_| "the server's SCRAM iteration count is not a number")?;
        let salted = hi(&self.password, &salt, iterations);
        let client_key = hmac(&salted, b"Client Key");
        let without_proof = format!("c=biws,r={nonce}");
        let auth_message = format!("{},{server_first},{without_proof}", self.first_bare);
        let signature = hmac(&sha256(&client_key), auth_message.as_bytes());
        let proof: Vec<u8> = client_key.iter().zip(signature).map(|(k, s)| k ^ s).collect();
        self.server_signature = Some(hmac(&hmac(&salted, b"Server Key"), auth_message.as_bytes()));
        Ok(format!("{without_proof},p={}", base64(&proof)))
    }

    pub fn verify(&self, server_final: &str) -> Result<(), String> {
        if let Some(e) = server_final.strip_prefix("e=") {
            return Err(format!("the server refused the SCRAM proof: {e}"));
        }
        let claimed = server_final.strip_prefix("v=").and_then(unbase64);
        match (claimed, self.server_signature) {
            (Some(v), Some(expected)) if v == expected => Ok(()),
            _ => Err("the server's SCRAM signature is wrong, so it does not know the password".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha256_as_fips_180_gives_it() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let two_blocks = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(hex(&sha256(two_blocks)), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn hmac_as_rfc_4231_gives_it() {
        assert_eq!(hex(&hmac(&[0x0b; 20], b"Hi There")), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        let long_key = hmac(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First");
        assert_eq!(hex(&long_key), "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
    }

    #[test]
    fn hi_is_pbkdf2() {
        assert_eq!(hex(&hi(b"password", b"salt", 1)), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");
        assert_eq!(hex(&hi(b"password", b"salt", 4096)), "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a");
    }

    #[test]
    fn base64_round_trips() {
        for (bytes, text) in [(&b""[..], ""), (b"f", "Zg=="), (b"fo", "Zm8="), (b"foo", "Zm9v"), (b"foobar", "Zm9vYmFy")] {
            assert_eq!(base64(bytes), text);
            assert_eq!(unbase64(text).as_deref(), Some(bytes));
        }
        assert_eq!(unbase64("Zm9v!"), None);
    }

    #[test]
    fn the_rfc_7677_exchange() {
        let mut scram = Scram::new("pencil", "rOprNGfwEbeRWgbNEkqO".into());
        assert_eq!(scram.client_first("user"), "n,,n=user,r=rOprNGfwEbeRWgbNEkqO");
        let server_first = "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        assert_eq!(
            scram.client_final(server_first).unwrap(),
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
        );
        assert_eq!(scram.verify("v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="), Ok(()));
        assert!(scram.verify("v=AAAA").is_err());
    }

    #[test]
    fn nonces_differ() {
        assert_ne!(nonce(), nonce());
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_nonce_is_the_systems_randomness() {
        let mut bytes = [0u8; 18];
        assert!(system_random(&mut bytes));
        assert_eq!(nonce().len(), 24);
    }
}
