//! SCRAM-SHA-256 client authentication (RFC 5802, RFC 7677), with SHA-256 (FIPS 180-4), HMAC
//! (RFC 2104) and base64 written here, as the workspace takes no dependencies.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74,
    0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d,
    0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e,
    0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(s0.wrapping_add(maj)));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (bytes, word) in out.chunks_exact_mut(4).zip(h) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner: Vec<u8> = block.iter().map(|b| b ^ 0x36).chain(message.iter().copied()).collect();
    let outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).chain(sha256(&inner)).collect();
    sha256(&outer)
}

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

/// A client nonce from `std`'s OS-seeded hasher keys, the process and the clock (assumption S6).
pub fn nonce() -> String {
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
}
