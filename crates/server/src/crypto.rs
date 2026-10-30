//! Random tokens, hashes, signed values and sealed secrets.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use hmac::{Hmac, KeyInit as MacKey, Mac};
use sha2::{Digest, Sha256};

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    // The operating system's generator only fails when there's no OS to ask.
    #[allow(clippy::expect_used)]
    getrandom::fill(&mut bytes).expect("the operating system gives random bytes");
    bytes
}

/// An unguessable token for a link, a session or the shared secret: 32
/// random bytes.
pub fn token() -> String {
    URL_SAFE_NO_PAD.encode(random::<32>())
}

/// An install's ID, for the counts its person shares: 128 random bits, as
/// 32 hex characters.
pub fn install_id() -> String {
    random::<16>().iter().map(|b| format!("{b:02x}")).collect()
}

/// What's kept of a token: its SHA-256.
pub fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// The install's sealing key: it seals the keys kept in the database.
#[derive(Clone)]
pub struct Key([u8; 32]);

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

impl Key {
    pub fn generate() -> Key {
        Key(random::<32>())
    }

    pub fn from_hex(hex: &str) -> Option<Key> {
        if hex.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Key(out))
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn mac(&self, purpose: &str, data: &str) -> Hmac<Sha256> {
        // Any key length works for HMAC.
        #[allow(clippy::expect_used)]
        let mut mac =
            <Hmac<Sha256> as MacKey>::new_from_slice(&self.0).expect("HMAC takes any key");
        mac.update(purpose.as_bytes());
        mac.update(b"\0");
        mac.update(data.as_bytes());
        mac
    }

    /// `data`, signed for one purpose: `data.signature`.
    pub fn sign(&self, purpose: &str, data: &str) -> String {
        let tag = self.mac(purpose, data).finalize().into_bytes();
        format!("{data}.{}", URL_SAFE_NO_PAD.encode(tag))
    }

    /// The data in a signed value, if the signature is this key's, for this purpose.
    pub fn verify<'a>(&self, purpose: &str, signed: &'a str) -> Option<&'a str> {
        let (data, tag) = signed.rsplit_once('.')?;
        let tag = URL_SAFE_NO_PAD.decode(tag).ok()?;
        self.mac(purpose, data).verify_slice(&tag).ok()?;
        Some(data)
    }

    /// Encrypts a secret for the database: a fresh nonce, then the ciphertext.
    pub fn seal(&self, secret: &[u8]) -> Vec<u8> {
        let nonce = random::<24>();
        let cipher = XChaCha20Poly1305::new((&self.0).into());
        // Encryption only fails for messages far larger than any secret.
        #[allow(clippy::expect_used)]
        let sealed = cipher
            .encrypt(&XNonce::from(nonce), secret)
            .expect("a secret encrypts");
        [nonce.as_slice(), &sealed].concat()
    }

    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 24 {
            return None;
        }
        let (nonce, body) = sealed.split_at(24);
        let nonce: [u8; 24] = nonce.try_into().ok()?;
        XChaCha20Poly1305::new((&self.0).into())
            .decrypt(&XNonce::from(nonce), body)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_values_check_out_only_with_their_key_and_purpose() {
        let key = Key::generate();
        let signed = key.sign("invite", "abc");
        assert_eq!(key.verify("invite", &signed), Some("abc"));
        assert_eq!(key.verify("share", &signed), None);
        assert_eq!(Key::generate().verify("invite", &signed), None);
        assert_eq!(key.verify("invite", "abc.AAAA"), None);
    }

    #[test]
    fn sealed_secrets_open_only_with_their_key() {
        let key = Key::generate();
        let sealed = key.seal(b"-----BEGIN RSA PRIVATE KEY-----");
        assert_eq!(
            key.open(&sealed).as_deref(),
            Some(b"-----BEGIN RSA PRIVATE KEY-----".as_slice())
        );
        assert_eq!(Key::generate().open(&sealed), None);
        assert_ne!(
            key.seal(b"x"),
            key.seal(b"x"),
            "every seal has its own nonce"
        );
    }

    #[test]
    fn keys_round_trip_through_hex() {
        let key = Key::generate();
        assert_eq!(
            Key::from_hex(&key.to_hex()).map(|k| k.to_hex()),
            Some(key.to_hex())
        );
        assert!(Key::from_hex("zz").is_none());
    }
}
