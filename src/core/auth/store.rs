//! Persisted identity: `identity.json` (username, salt, Argon2 params, AES-GCM blob, public PEM).

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng};
use aes_gcm::{Aes256Gcm, Key};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::{
    pkcs1v15::Signature, pkcs1v15::SigningKey, pkcs1v15::VerifyingKey, RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;

const RSA_BITS: usize = 2048;
const AUTH_FORMAT: &str = "xos-auth-v1";

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

/// Create `identity.json` with a new RSA-2048 key; private key encrypted with Argon2id + AES-256-GCM.
pub fn login_offline(username: &str, password: &str) -> Result<(), AuthError> {
    let dir = auth_data_dir()?;
    fs::create_dir_all(&dir).map_err(|e| AuthError::Io(e.to_string()))?;
    let path = dir.join("identity.json");
    if path.exists() {
        return Err(AuthError::AlreadyExists);
    }

    let mut rng = AesOsRng;
    let private = RsaPrivateKey::new(&mut rng, RSA_BITS).map_err(|e| AuthError::Crypto(e.to_string()))?;
    let public = private.to_public_key();
    let public_pem = public
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();

    let pk_der = private
        .to_pkcs8_der()
        .map_err(|e| AuthError::Crypto(e.to_string()))?;
    let pk_bytes = pk_der.as_bytes();

    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| AuthError::Io(format!("{e:?}")))?;
    let key_bytes = derive_aes_key(password.as_bytes(), &salt, ARGON_M, ARGON_T, ARGON_P)?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Aes256Gcm::generate_nonce(&mut AesOsRng);
    let ciphertext = cipher
        .encrypt(&nonce, pk_bytes.as_ref())
        .map_err(|e| AuthError::Crypto(e.to_string()))?;

    let stored = StoredIdentityFile {
        format: AUTH_FORMAT.to_string(),
        username: username.trim().to_string(),
        salt_b64: B64.encode(salt),
        argon_m_cost: ARGON_M,
        argon_t_cost: ARGON_T,
        argon_p_cost: ARGON_P,
        aes_nonce_b64: B64.encode(nonce.as_slice()),
        ciphertext_b64: B64.encode(&ciphertext),
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

/// Decrypt stored private key and load RSA identity.
pub fn unlock_identity(password: &str) -> Result<UnlockedIdentity, AuthError> {
    let path = auth_json_path()?;
    let raw = fs::read_to_string(&path).map_err(|e| AuthError::Io(e.to_string()))?;
    let stored: StoredIdentityFile =
        serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    if stored.format != AUTH_FORMAT {
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
