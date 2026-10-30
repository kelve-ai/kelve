//! Reading tokens: how the shared graph is read only by someone signed in, and
//! never as them.
//!
//! A signed-in kelve asks the graph, with its account's key, to sign a batch
//! of tokens it has blinded, so the graph signs them without seeing them.
//! Each read then spends one token. The graph can tell a token is one it
//! signed, and that it hasn't been spent, but not whose it was: what an
//! account was given and what's read with it can't be linked. A token read
//! with once can't be read with again, so sharing one gives away no more
//! than that read, and an account gets so many a day.
//!
//! It's Privacy Pass's privately verifiable token (RFC 9578, type 1) in
//! substance, on the VOPRF of RFC 9497, over ristretto255: the graph proves
//! it signed every token in a batch with the key it shows everyone, so it
//! can't mark one account's tokens with a key of their own.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use voprf::{
    BlindedElement, EvaluationElement, Group, Proof, Ristretto255, VoprfClient, VoprfServer,
};

type Suite = Ristretto255;

/// How a read carries its token: `Authorization: PrivateToken token=…`.
pub const SCHEME: &str = "PrivateToken";

/// The most tokens one batch asks for.
pub const MOST_A_BATCH: usize = 100;

/// What a token's input says before its nonce, so it signs nothing else.
const CONTEXT: &[u8] = b"kelve graph reading token v1";

const NONCE: usize = 32;
const ID: usize = 8;
const OUTPUT: usize = 64;

/// The key the graph signs tokens with, as it shows everyone: its ID, and
/// the public key a batch's proof is checked against.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenKey {
    /// The first eight bytes of the public key's SHA-256, in hex.
    pub id: String,
    /// The public key, base64url.
    pub key: String,
}

/// A batch of blinded tokens to sign, for the key with `id`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAsk {
    pub id: String,
    /// Each blinded token, base64url: at most 100.
    pub blinded: Vec<String>,
}

/// The batch signed: each in the order asked, the proof they were all
/// signed with the key shown, and how many more the account may have today.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenGrant {
    pub signed: Vec<String>,
    pub proof: String,
    pub left: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("The graph's reading key isn't one kelve can use.")]
    Key,
    #[error("The tokens the graph signed don't check out against the key it shows everyone.")]
    Proof,
    #[error("That batch of tokens isn't one the graph can sign.")]
    Ask,
}

/// Blinded tokens waiting for the graph to sign them.
pub struct Pending {
    key: TokenKey,
    nonces: Vec<[u8; NONCE]>,
    clients: Vec<VoprfClient<Suite>>,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending")
            .field("tokens", &self.nonces.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Issuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Issuer")
            .field("key", &self.key.id)
            .finish_non_exhaustive()
    }
}

fn input(nonce: &[u8]) -> Vec<u8> {
    [CONTEXT, nonce].concat()
}

fn public_key(
    key: &TokenKey,
) -> Result<<<Suite as voprf::CipherSuite>::Group as Group>::Elem, TokenError> {
    let bytes = B64.decode(&key.key).map_err(|_| TokenError::Key)?;
    if id_of(&bytes) != key.id {
        return Err(TokenError::Key);
    }
    <Suite as voprf::CipherSuite>::Group::deserialize_elem(&bytes).map_err(|_| TokenError::Key)
}

fn id_of(public: &[u8]) -> String {
    Sha256::digest(public)[..ID]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Blinds `n` new tokens for `key`: what to keep, and what to ask the graph.
pub fn blind(key: &TokenKey, n: usize) -> Result<(Pending, TokenAsk), TokenError> {
    public_key(key)?;
    let n = n.clamp(1, MOST_A_BATCH);
    let mut nonces = Vec::with_capacity(n);
    let mut clients = Vec::with_capacity(n);
    let mut blinded = Vec::with_capacity(n);
    for _ in 0..n {
        let mut nonce = [0u8; NONCE];
        OsRng.fill_bytes(&mut nonce);
        let b =
            VoprfClient::<Suite>::blind(&input(&nonce), &mut OsRng).map_err(|_| TokenError::Ask)?;
        blinded.push(B64.encode(b.message.serialize()));
        nonces.push(nonce);
        clients.push(b.state);
    }
    let ask = TokenAsk {
        id: key.id.clone(),
        blinded,
    };
    Ok((
        Pending {
            key: key.clone(),
            nonces,
            clients,
        },
        ask,
    ))
}

/// The tokens, once the graph has signed them and proved it used the key
/// it shows everyone: each ready to send, as `PrivateToken token=…` says.
pub fn finish(pending: Pending, grant: &TokenGrant) -> Result<Vec<String>, TokenError> {
    let pk = public_key(&pending.key)?;
    if grant.signed.len() != pending.clients.len() {
        return Err(TokenError::Proof);
    }
    let signed = grant
        .signed
        .iter()
        .map(|s| {
            B64.decode(s)
                .ok()
                .and_then(|b| EvaluationElement::<Suite>::deserialize(&b).ok())
                .ok_or(TokenError::Proof)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let proof = B64
        .decode(&grant.proof)
        .ok()
        .and_then(|b| Proof::<Suite>::deserialize(&b).ok())
        .ok_or(TokenError::Proof)?;
    let inputs: Vec<Vec<u8>> = pending.nonces.iter().map(|n| input(n)).collect();
    let outputs = VoprfClient::batch_finalize(&inputs, &pending.clients, &signed, &proof, pk)
        .map_err(|_| TokenError::Proof)?;
    let id = hex_bytes(&pending.key.id).ok_or(TokenError::Key)?;
    pending
        .nonces
        .iter()
        .zip(outputs)
        .map(|(nonce, out)| {
            let out = out.map_err(|_| TokenError::Proof)?;
            Ok(B64.encode([&id[..], &nonce[..], &out[..]].concat()))
        })
        .collect()
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A token as a read carries it, from its `Authorization` header.
pub fn from_header(header: &str) -> Option<&str> {
    let rest = header.strip_prefix(SCHEME)?.trim_start();
    rest.strip_prefix("token=")
        .map(|t| t.trim().trim_matches('"'))
}

/// What the graph signs tokens with: one key, the same for everyone.
pub struct Issuer {
    server: VoprfServer<Suite>,
    key: TokenKey,
}

impl Issuer {
    /// The issuer whose secret comes from `seed`, which the graph keeps.
    pub fn from_seed(seed: &[u8]) -> Result<Self, TokenError> {
        let server =
            VoprfServer::<Suite>::new_from_seed(seed, CONTEXT).map_err(|_| TokenError::Key)?;
        let public = <Suite as voprf::CipherSuite>::Group::serialize_elem(server.get_public_key());
        let key = TokenKey {
            id: id_of(&public),
            key: B64.encode(public),
        };
        Ok(Self { server, key })
    }

    /// The key, as the graph shows everyone.
    pub fn key(&self) -> &TokenKey {
        &self.key
    }

    /// Signs a batch, blind, with a proof it used this key; `left` is how
    /// many more the account may have today.
    pub fn sign(&self, ask: &TokenAsk, left: u32) -> Result<TokenGrant, TokenError> {
        if ask.id != self.key.id || ask.blinded.is_empty() || ask.blinded.len() > MOST_A_BATCH {
            return Err(TokenError::Ask);
        }
        let blinded = ask
            .blinded
            .iter()
            .map(|b| {
                B64.decode(b)
                    .ok()
                    .and_then(|b| BlindedElement::<Suite>::deserialize(&b).ok())
                    .ok_or(TokenError::Ask)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let done = self
            .server
            .batch_blind_evaluate(&mut OsRng, &blinded)
            .map_err(|_| TokenError::Ask)?;
        Ok(TokenGrant {
            signed: done
                .messages
                .iter()
                .map(|m| B64.encode(m.serialize()))
                .collect(),
            proof: B64.encode(done.proof.serialize()),
            left,
        })
    }

    /// The token's nonce, when it's one this key signed: what the graph
    /// keeps once it's spent, so it's never read with again.
    pub fn check(&self, token: &str) -> Option<[u8; NONCE]> {
        let bytes = B64.decode(token).ok()?;
        if bytes.len() != ID + NONCE + OUTPUT || hex_bytes(&self.key.id)? != bytes[..ID] {
            return None;
        }
        let nonce: [u8; NONCE] = bytes[ID..ID + NONCE].try_into().ok()?;
        let want = self.server.evaluate(&input(&nonce)).ok()?;
        // In constant time, so no one learns how close a forged one came.
        let got = &bytes[ID + NONCE..];
        let differs = want.iter().zip(got).fold(0u8, |d, (a, b)| d | (a ^ b));
        (differs == 0).then_some(nonce)
    }

    /// The key a token says it was signed with, to pick the issuer from
    /// those the graph keeps while one gives way to the next.
    pub fn key_of(token: &str) -> Option<String> {
        let bytes = B64.decode(token).ok()?;
        (bytes.len() == ID + NONCE + OUTPUT)
            .then(|| bytes[..ID].iter().map(|b| format!("{b:02x}")).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> Issuer {
        Issuer::from_seed(&[7u8; 32]).unwrap()
    }

    #[test]
    fn a_token_signed_blind_reads_once_and_says_nothing_of_whose() {
        let graph = issuer();
        let (pending, ask) = blind(graph.key(), 3).unwrap();
        let grant = graph.sign(&ask, 97).unwrap();
        let tokens = finish(pending, &grant).unwrap();
        assert_eq!(tokens.len(), 3);
        // The graph never saw them: what it signed isn't what's read with.
        for (t, b) in tokens.iter().zip(&ask.blinded) {
            assert!(!t.contains(b.as_str()) && !b.contains(t.as_str()));
        }
        let nonces: Vec<_> = tokens
            .iter()
            .map(|t| graph.check(t).expect("signed"))
            .collect();
        assert_ne!(nonces[0], nonces[1], "each is its own");
        assert_eq!(
            from_header(&format!("{SCHEME} token={}", tokens[0])),
            Some(tokens[0].as_str())
        );
    }

    #[test]
    fn a_forged_or_other_keys_token_doesnt_read() {
        let graph = issuer();
        let (pending, ask) = blind(graph.key(), 1).unwrap();
        let token = finish(pending, &graph.sign(&ask, 0).unwrap())
            .unwrap()
            .remove(0);
        let mut forged = B64.decode(&token).unwrap();
        let last = forged.len() - 1;
        forged[last] ^= 1;
        assert_eq!(graph.check(&B64.encode(&forged)), None);
        let other = Issuer::from_seed(&[8u8; 32]).unwrap();
        assert_eq!(other.check(&token), None);
        assert_eq!(
            Issuer::key_of(&token).as_deref(),
            Some(graph.key().id.as_str())
        );
    }

    #[test]
    fn tokens_signed_with_a_key_not_shown_are_turned_away() {
        // A graph that signed one account's tokens with a key of their own,
        // to know them again, can't prove it used the key it shows everyone.
        let shown = issuer();
        let secret = Issuer::from_seed(&[9u8; 32]).unwrap();
        let (pending, mut ask) = blind(shown.key(), 2).unwrap();
        ask.id = secret.key().id.clone();
        let grant = secret.sign(&ask, 0).unwrap();
        assert_eq!(finish(pending, &grant).err(), Some(TokenError::Proof));
    }

    #[test]
    fn a_batch_is_one_to_a_hundred() {
        let graph = issuer();
        let (_, mut ask) = blind(graph.key(), 1).unwrap();
        ask.blinded.clear();
        assert_eq!(graph.sign(&ask, 0).err(), Some(TokenError::Ask));
        let (_, big) = blind(graph.key(), 500).unwrap();
        assert_eq!(big.blinded.len(), MOST_A_BATCH);
    }
}
