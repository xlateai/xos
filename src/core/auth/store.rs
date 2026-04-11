//! Persisted identity: `identity.json`
//!
//! - **v2 (current):** RSA-2048 is generated deterministically from username + password (Argon2id
//!   stretches the password; ChaCha20 is seeded for RSA keygen). Only **username**, KDF params,
//!   and **public** PEM are stored — no ciphertext, no saved password. The private key is
//!   recomputed on each unlock.
//! - **v1 (legacy):** random RSA key with private key encrypted (Argon2id + AES-GCM); still supported for unlock.

use aes_gcm::aead::{Aead, KeyInit, OsRng as AesOsRng};
use aes_gcm::{Aes256Gcm, Key};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey, LineEnding};
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::{
    pkcs1v15::Signature, pkcs1v15::SigningKey, pkcs1v15::VerifyingKey, RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const RSA_BITS: usize = 2048;
const AUTH_FORMAT_V1: &str = "xos-auth-v1";
const AUTH_FORMAT_V2: &str = "xos-auth-v2";

const ARGON_M: u32 = 19456;
const ARGON_T: u32 = 2;
const ARGON_P: u32 = 1;

#[derive(Debug)]
pub enum AuthError {
    Io(String),
    AlreadyExists,
    InvalidFile(String),
    WrongPassword,
    Crypto(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Io(s) => write!(f, "{s}"),
            AuthError::AlreadyExists => write!(f, "an identity already exists for this machine"),
            AuthError::InvalidFile(s) => write!(f, "{s}"),
            AuthError::WrongPassword => write!(f, "incorrect password"),
            AuthError::Crypto(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Standard per-OS directory for xos app data (machine-local).
pub fn auth_data_dir() -> Result<PathBuf, AuthError> {
    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .ok_or_else(|| AuthError::Io("could not resolve LocalAppData".to_string()))
            .map(|p| p.join("xos"))
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .ok_or_else(|| AuthError::Io("could not resolve home directory".to_string()))
            .map(|p| p.join(".xos"))
    }
}

pub fn auth_json_path() -> Result<PathBuf, AuthError> {
    Ok(auth_data_dir()?.join("identity.json"))
}

pub fn has_identity() -> bool {
    auth_json_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Legacy on-disk layout (`xos-auth-v1`): encrypted PKCS#8 private key.
#[derive(Serialize, Deserialize)]
pub struct StoredIdentityFile {
    pub format: String,
    pub username: String,
    pub salt_b64: String,
    pub argon_m_cost: u32,
    pub argon_t_cost: u32,
    pub argon_p_cost: u32,
    pub aes_nonce_b64: String,
    pub ciphertext_b64: String,
    pub public_key_pem: String,
}

/// Deterministic identity (`xos-auth-v2`): only username, KDF params, and public key on disk.
#[derive(Serialize, Deserialize)]
pub struct StoredIdentityV2 {
    pub format: String,
    pub username: String,
    pub argon_m_cost: u32,
    pub argon_t_cost: u32,
    pub argon_p_cost: u32,
    /// PEM of the public key derived from (username, password); used to detect wrong password.
    pub public_key_pem: String,
}

pub struct UnlockedIdentity {
    pub username: String,
    rsa_private: RsaPrivateKey,
    pub public_pem: String,
}

impl UnlockedIdentity {
    pub fn private_key(&self) -> &RsaPrivateKey {
        &self.rsa_private
    }

    pub fn public_key(&self) -> Result<RsaPublicKey, AuthError> {
        RsaPublicKey::from_public_key_pem(self.public_pem.as_str())
            .map_err(|e| AuthError::Crypto(e.to_string()))
    }
}

fn derive_aes_key(password: &[u8], salt: &[u8], m: u32, t: u32, p: u32) -> Result<[u8; 32], AuthError> {
    let params = Params::new(m, t, p, None).map_err(|e| AuthError::Crypto(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut out)
        .map_err(|e| AuthError::Crypto(e.to_string()))?;
    Ok(out)
}

/// Deterministic 16-byte salt from username (no random salt stored for v2).
fn salt_for_username(username: &str) -> [u8; 16] {
    let h = Sha256::digest(format!("xos-auth-v2|{}", username.trim()).as_bytes());
    h[..16]
        .try_into()
        .expect("sha256 first 16 bytes")
}

/// 32-byte seed for ChaCha20 (`rand_core` 0.6 / compatible with `rsa` keygen): Argon2id(password, salt(username)).
fn derive_rsa_seed(password: &[u8], username: &str, m: u32, t: u32, p: u32) -> Result<[u8; 32], AuthError> {
    let salt = salt_for_username(username);
    derive_aes_key(password, &salt, m, t, p)
}

fn rsa_deterministic_from_password(
    password: &[u8],
    username: &str,
    m: u32,
    t: u32,
    p: u32,
) -> Result<(RsaPrivateKey, String), AuthError> {
    let seed = derive_rsa_seed(password, username, m, t, p)?;
    let mut rng = ChaCha20Rng::from_seed(seed);
    let private = RsaPrivateKey::new(&mut rng, RSA_BITS).map_err(|e| AuthError::Crypto(e.to_string()))?;
    let public_pem = private
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();
    Ok((private, public_pem))
}

/// Create `identity.json` with RSA-2048 derived from username + password (v2). Nothing secret except
/// what the user types; disk holds username, KDF params, and public PEM only.
pub fn login_offline(username: &str, password: &str) -> Result<(), AuthError> {
    let dir = auth_data_dir()?;
    fs::create_dir_all(&dir).map_err(|e| AuthError::Io(e.to_string()))?;
    let path = dir.join("identity.json");
    if path.exists() {
        return Err(AuthError::AlreadyExists);
    }

    let u = username.trim();
    let (_, public_pem) = rsa_deterministic_from_password(password.as_bytes(), u, ARGON_M, ARGON_T, ARGON_P)?;

    let stored = StoredIdentityV2 {
        format: AUTH_FORMAT_V2.to_string(),
        username: u.to_string(),
        argon_m_cost: ARGON_M,
        argon_t_cost: ARGON_T,
        argon_p_cost: ARGON_P,
        public_key_pem: public_pem,
    };

    let json = serde_json::to_string_pretty(&stored).map_err(|e| AuthError::Io(e.to_string()))?;
    fs::write(&path, json).map_err(|e| AuthError::Io(e.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn unlock_identity_v1(stored: StoredIdentityFile, password: &str) -> Result<UnlockedIdentity, AuthError> {
    if stored.format != AUTH_FORMAT_V1 {
        return Err(AuthError::InvalidFile("unknown identity format".to_string()));
    }

    let salt = B64
        .decode(&stored.salt_b64)
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    let key_bytes = derive_aes_key(
        password.as_bytes(),
        &salt,
        stored.argon_m_cost,
        stored.argon_t_cost,
        stored.argon_p_cost,
    )?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce_bytes = B64
        .decode(&stored.aes_nonce_b64)
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    if nonce_bytes.len() != 12 {
        return Err(AuthError::InvalidFile("bad AES nonce".to_string()));
    }
    let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
    let ciphertext = B64
        .decode(&stored.ciphertext_b64)
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    let plain = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| AuthError::WrongPassword)?;

    let rsa_private =
        RsaPrivateKey::from_pkcs8_der(&plain).map_err(|e| AuthError::InvalidFile(e.to_string()))?;

    Ok(UnlockedIdentity {
        username: stored.username,
        rsa_private,
        public_pem: stored.public_key_pem,
    })
}

fn unlock_identity_v2(stored: StoredIdentityV2, password: &str) -> Result<UnlockedIdentity, AuthError> {
    if stored.format != AUTH_FORMAT_V2 {
        return Err(AuthError::InvalidFile("unknown identity format".to_string()));
    }

    let (rsa_private, public_pem) = rsa_deterministic_from_password(
        password.as_bytes(),
        &stored.username,
        stored.argon_m_cost,
        stored.argon_t_cost,
        stored.argon_p_cost,
    )?;

    if public_pem.trim() != stored.public_key_pem.trim() {
        return Err(AuthError::WrongPassword);
    }

    Ok(UnlockedIdentity {
        username: stored.username,
        rsa_private,
        public_pem,
    })
}

/// Load RSA identity: v2 recomputes the private key from username + password; v1 decrypts stored PKCS#8.
pub fn unlock_identity(password: &str) -> Result<UnlockedIdentity, AuthError> {
    let path = auth_json_path()?;
    let raw = fs::read_to_string(&path).map_err(|e| AuthError::Io(e.to_string()))?;
    let format: String = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?
        .get("format")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AuthError::InvalidFile("missing format".to_string()))?;

    match format.as_str() {
        AUTH_FORMAT_V1 => {
            let stored: StoredIdentityFile =
                serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
            unlock_identity_v1(stored, password)
        }
        AUTH_FORMAT_V2 => {
            let stored: StoredIdentityV2 =
                serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
            unlock_identity_v2(stored, password)
        }
        _ => Err(AuthError::InvalidFile("unknown identity format".to_string())),
    }
}

/// RSA-PKCS1-v1.5 + SHA256 sign (LAN handshake).
pub fn rsa_sign(private: &RsaPrivateKey, msg: &[u8]) -> Result<Vec<u8>, AuthError> {
    let mut rng = AesOsRng;
    let signing_key = SigningKey::<Sha256>::new(private.clone());
    let sig = signing_key.sign_with_rng(&mut rng, msg);
    Ok(sig.to_bytes().to_vec())
}

pub fn rsa_verify(public: &RsaPublicKey, msg: &[u8], sig_bytes: &[u8]) -> Result<(), AuthError> {
    let vk = VerifyingKey::<Sha256>::new(public.clone());
    let sig = Signature::try_from(sig_bytes).map_err(|e| AuthError::Crypto(e.to_string()))?;
    vk.verify(msg, &sig)
        .map_err(|_| AuthError::Crypto("RSA verify failed".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_rsa_same_inputs_same_public_pem() {
        let (a, pem_a) =
            rsa_deterministic_from_password(b"secret", "alice", ARGON_M, ARGON_T, ARGON_P).unwrap();
        let (b, pem_b) =
            rsa_deterministic_from_password(b"secret", "alice", ARGON_M, ARGON_T, ARGON_P).unwrap();
        assert_eq!(pem_a, pem_b);
        assert_eq!(
            a.to_public_key().to_public_key_pem(LineEnding::LF).unwrap(),
            b.to_public_key().to_public_key_pem(LineEnding::LF).unwrap()
        );
    }

    #[test]
    fn wrong_password_changes_keys() {
        let (_, pem_ok) =
            rsa_deterministic_from_password(b"secret", "alice", ARGON_M, ARGON_T, ARGON_P).unwrap();
        let (_, pem_bad) =
            rsa_deterministic_from_password(b"wrong", "alice", ARGON_M, ARGON_T, ARGON_P).unwrap();
        assert_ne!(pem_ok, pem_bad);
    }
}
