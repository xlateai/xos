//! Persisted identity (two files under the xos data dir):
//! - **`authentication.json`** — account auth: RSA-2048 derived from username + password (**v4**).
//! - **`node_identity.json`** — per-machine RSA keypair + **`node_name`**; LAN mesh uses this.
//!   **`node_id`** is **SHA256(SPKI DER of the public key)** as hex — **not** stored (derive anytime).
//!
//! Legacy **`identity.json`** is migrated to `authentication.json` + a generated `node_identity.json`
//! on first load. **`v3`/`v2`/`v1`** still work via **`unlock_identity(password)`** when reading old layouts.

use aes_gcm::aead::{Aead, KeyInit, OsRng as AesOsRng};
use aes_gcm::{Aes256Gcm, Key};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::rand_core::OsRng as RsaOsRng;
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
const AUTH_FORMAT_V3: &str = "xos-auth-v3";
const AUTH_FORMAT_V4: &str = "xos-auth-v4";
const NODE_FORMAT_V1: &str = "xos-node-v1";

/// Lighter Argon2 for **v4 only** (one run at `xos login --offline`) so interactive login is not multi-minute.
const V4_ARGON_M: u32 = 8192;
const V4_ARGON_T: u32 = 2;
const V4_ARGON_P: u32 = 1;

#[derive(Debug)]
pub enum AuthError {
    Io(String),
    AlreadyExists,
    NoIdentity,
    InvalidFile(String),
    WrongPassword,
    Crypto(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Io(s) => write!(f, "{s}"),
            AuthError::AlreadyExists => write!(f, "an identity already exists for this machine"),
            AuthError::NoIdentity => write!(f, "no identity file found (nothing to delete)"),
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

/// Account / authentication file (username + password-derived RSA, v4+).
pub fn authentication_json_path() -> Result<PathBuf, AuthError> {
    Ok(auth_data_dir()?.join("authentication.json"))
}

/// Per-machine node keys + friendly name (LAN mesh identity).
pub fn node_identity_json_path() -> Result<PathBuf, AuthError> {
    Ok(auth_data_dir()?.join("node_identity.json"))
}

/// Legacy path (pre-split). Prefer [`authentication_json_path`].
pub fn auth_json_path() -> Result<PathBuf, AuthError> {
    authentication_json_path()
}

fn legacy_identity_json_path() -> Result<PathBuf, AuthError> {
    Ok(auth_data_dir()?.join("identity.json"))
}

pub fn has_authentication() -> bool {
    authentication_json_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

pub fn has_node_identity() -> bool {
    node_identity_json_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// True when offline login has produced both auth + node files, or a legacy `identity.json` exists.
pub fn has_identity() -> bool {
    (has_authentication() && has_node_identity())
        || legacy_identity_json_path()
            .map(|p| p.exists())
            .unwrap_or(false)
}

/// Remove `authentication.json`, `node_identity.json`, and legacy `identity.json` if present.
pub fn delete_identity() -> Result<(), AuthError> {
    let mut removed = false;
    for p in [
        authentication_json_path()?,
        node_identity_json_path()?,
        legacy_identity_json_path()?,
    ] {
        if p.exists() {
            fs::remove_file(&p).map_err(|e| AuthError::Io(e.to_string()))?;
            removed = true;
        }
    }
    if !removed {
        return Err(AuthError::NoIdentity);
    }
    Ok(())
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

/// Plain PEM on disk (`xos-auth-v4`): mesh loads without password.
#[derive(Serialize, Deserialize)]
pub struct StoredIdentityV4 {
    pub format: String,
    pub username: String,
    pub private_key_pem: String,
    pub public_key_pem: String,
}

/// Per-machine keys (`xos-node-v1`). Do **not** store `node_id` — use [`node_id_from_public_pem`].
#[derive(Serialize, Deserialize)]
pub struct StoredNodeIdentity {
    pub format: String,
    pub node_name: String,
    pub private_key_pem: String,
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

/// LAN mesh identity: unique RSA keypair per machine + display name.
pub struct UnlockedNodeIdentity {
    pub node_name: String,
    rsa_private: RsaPrivateKey,
    pub public_pem: String,
}

impl UnlockedNodeIdentity {
    pub fn private_key(&self) -> &RsaPrivateKey {
        &self.rsa_private
    }

    pub fn public_key(&self) -> Result<RsaPublicKey, AuthError> {
        RsaPublicKey::from_public_key_pem(self.public_pem.as_str())
            .map_err(|e| AuthError::Crypto(e.to_string()))
    }

    /// Stable id: **SHA256(SPKI DER of public key)** as lowercase hex (not persisted).
    pub fn node_id(&self) -> String {
        node_id_from_public_pem(self.public_pem.as_str()).unwrap_or_default()
    }
}

/// Derive the stable node id from a public key PEM (same rule as [`UnlockedNodeIdentity::node_id`]).
pub fn node_id_from_public_pem(public_pem: &str) -> Result<String, AuthError> {
    let pk = RsaPublicKey::from_public_key_pem(public_pem.trim())
        .map_err(|e| AuthError::Crypto(e.to_string()))?;
    let der = pk
        .to_public_key_der()
        .map_err(|e| AuthError::Crypto(e.to_string()))?;
    let hash = Sha256::digest(der.as_bytes());
    Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

fn generate_node_rsa() -> Result<(RsaPrivateKey, String), AuthError> {
    let mut rng = RsaOsRng;
    let private =
        RsaPrivateKey::new(&mut rng, RSA_BITS).map_err(|e| AuthError::Crypto(e.to_string()))?;
    let public_pem = private
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();
    Ok((private, public_pem))
}

fn write_node_identity_file(node_name: &str, private: &RsaPrivateKey, public_pem: &str) -> Result<(), AuthError> {
    let private_pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();
    let stored = StoredNodeIdentity {
        format: NODE_FORMAT_V1.to_string(),
        node_name: node_name.to_string(),
        private_key_pem: private_pem,
        public_key_pem: public_pem.to_string(),
    };
    let path = node_identity_json_path()?;
    let json = serde_json::to_string_pretty(&stored).map_err(|e| AuthError::Io(e.to_string()))?;
    fs::write(&path, json).map_err(|e| AuthError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// If `identity.json` exists, copy to `authentication.json` and create `node_identity.json` if missing; then remove legacy file.
pub fn migrate_legacy_identity_file() -> Result<(), AuthError> {
    let leg = legacy_identity_json_path()?;
    let auth = authentication_json_path()?;
    if leg.exists() && !auth.exists() {
        fs::copy(&leg, &auth).map_err(|e| AuthError::Io(e.to_string()))?;
    }
    if auth.exists() && !has_node_identity() {
        let (priv_k, pub_pem) = generate_node_rsa()?;
        let node_name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "migrated".to_string());
        write_node_identity_file(&node_name, &priv_k, &pub_pem)?;
    }
    if leg.exists() && auth.exists() && has_node_identity() {
        fs::remove_file(&leg).map_err(|e| AuthError::Io(e.to_string()))?;
    }
    Ok(())
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

/// Create `authentication.json` (account RSA) and `node_identity.json` (per-machine RSA + `node_name`).
/// Password is not stored; mesh loads node keys with [`load_node_identity`].
pub fn login_offline(username: &str, password: &str, node_name: &str) -> Result<(), AuthError> {
    let dir = auth_data_dir()?;
    fs::create_dir_all(&dir).map_err(|e| AuthError::Io(e.to_string()))?;
    if authentication_json_path()?.exists()
        || node_identity_json_path()?.exists()
        || legacy_identity_json_path()?.exists()
    {
        return Err(AuthError::AlreadyExists);
    }

    let nn = node_name.trim();
    if nn.is_empty() {
        return Err(AuthError::InvalidFile(
            "machine name (node_name) cannot be empty".to_string(),
        ));
    }

    let u = username.trim();
    let (private, public_pem) = rsa_deterministic_from_password(
        password.as_bytes(),
        u,
        V4_ARGON_M,
        V4_ARGON_T,
        V4_ARGON_P,
    )?;

    let private_pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();

    let stored = StoredIdentityV4 {
        format: AUTH_FORMAT_V4.to_string(),
        username: u.to_string(),
        private_key_pem: private_pem,
        public_key_pem: public_pem,
    };

    let auth_path = authentication_json_path()?;
    let json = serde_json::to_string_pretty(&stored).map_err(|e| AuthError::Io(e.to_string()))?;
    fs::write(&auth_path, json).map_err(|e| AuthError::Io(e.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&auth_path, fs::Permissions::from_mode(0o600));
    }

    let (node_priv, node_pub_pem) = generate_node_rsa()?;
    write_node_identity_file(nn, &node_priv, &node_pub_pem)?;

    Ok(())
}

fn restore_file_from_backup(path: &PathBuf, backup: &Option<Vec<u8>>) {
    match backup {
        Some(bytes) => {
            let _ = fs::write(path, bytes);
        }
        None => {
            let _ = fs::remove_file(path);
        }
    }
}

/// Replace existing `authentication.json` + `node_identity.json` atomically enough for CLI reset.
/// The previous file contents are loaded first and restored on partial failure.
pub fn reset_offline_identity(username: &str, password: &str, node_name: &str) -> Result<(), AuthError> {
    let dir = auth_data_dir()?;
    fs::create_dir_all(&dir).map_err(|e| AuthError::Io(e.to_string()))?;
    migrate_legacy_identity_file()?;

    let auth_path = authentication_json_path()?;
    let node_path = node_identity_json_path()?;

    let auth_backup = if auth_path.exists() {
        Some(fs::read(&auth_path).map_err(|e| AuthError::Io(e.to_string()))?)
    } else {
        None
    };
    let node_backup = if node_path.exists() {
        Some(fs::read(&node_path).map_err(|e| AuthError::Io(e.to_string()))?)
    } else {
        None
    };

    let nn = node_name.trim();
    if nn.is_empty() {
        return Err(AuthError::InvalidFile(
            "machine name (node_name) cannot be empty".to_string(),
        ));
    }
    if password.is_empty() {
        return Err(AuthError::InvalidFile(
            "password cannot be empty".to_string(),
        ));
    }

    let u = username.trim();
    // Reset must always rotate account key material, even if username/password are reused.
    let (private, public_pem) = generate_node_rsa()?;
    let private_pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();
    let stored_auth = StoredIdentityV4 {
        format: AUTH_FORMAT_V4.to_string(),
        username: u.to_string(),
        private_key_pem: private_pem,
        public_key_pem: public_pem,
    };

    let (node_priv, node_pub_pem) = generate_node_rsa()?;
    let node_private_pem = node_priv
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AuthError::Crypto(e.to_string()))?
        .to_string();
    let stored_node = StoredNodeIdentity {
        format: NODE_FORMAT_V1.to_string(),
        node_name: nn.to_string(),
        private_key_pem: node_private_pem,
        public_key_pem: node_pub_pem,
    };

    let auth_json =
        serde_json::to_string_pretty(&stored_auth).map_err(|e| AuthError::Io(e.to_string()))?;
    let node_json =
        serde_json::to_string_pretty(&stored_node).map_err(|e| AuthError::Io(e.to_string()))?;

    if let Err(e) = fs::write(&auth_path, auth_json) {
        restore_file_from_backup(&auth_path, &auth_backup);
        return Err(AuthError::Io(e.to_string()));
    }
    if let Err(e) = fs::write(&node_path, node_json) {
        restore_file_from_backup(&auth_path, &auth_backup);
        restore_file_from_backup(&node_path, &node_backup);
        return Err(AuthError::Io(e.to_string()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&auth_path, fs::Permissions::from_mode(0o600));
        let _ = fs::set_permissions(&node_path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

fn identity_from_v4(stored: StoredIdentityV4) -> Result<UnlockedIdentity, AuthError> {
    if stored.format != AUTH_FORMAT_V4 {
        return Err(AuthError::InvalidFile("not xos-auth-v4".to_string()));
    }
    let rsa_private = RsaPrivateKey::from_pkcs8_pem(stored.private_key_pem.trim())
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    Ok(UnlockedIdentity {
        username: stored.username,
        rsa_private,
        public_pem: stored.public_key_pem,
    })
}

fn read_authentication_json_or_legacy() -> Result<String, AuthError> {
    migrate_legacy_identity_file()?;
    let auth = authentication_json_path()?;
    if auth.exists() {
        return fs::read_to_string(&auth).map_err(|e| AuthError::Io(e.to_string()));
    }
    Err(AuthError::NoIdentity)
}

/// Load account RSA from `authentication.json` (v4). Legacy `identity.json` is migrated first.
pub fn load_identity() -> Result<UnlockedIdentity, AuthError> {
    let raw = read_authentication_json_or_legacy()?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    let fmt = v
        .get("format")
        .and_then(|x| x.as_str())
        .ok_or_else(|| AuthError::InvalidFile("missing format".to_string()))?;
    if fmt != AUTH_FORMAT_V4 {
        return Err(AuthError::InvalidFile(
            "account auth needs xos-auth-v4 in authentication.json. Run:  xos login --delete  then  xos login --offline"
                .to_string(),
        ));
    }
    let stored: StoredIdentityV4 =
        serde_json::from_value(v).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    identity_from_v4(stored)
}

/// Load per-machine LAN identity from `node_identity.json`.
pub fn load_node_identity() -> Result<UnlockedNodeIdentity, AuthError> {
    migrate_legacy_identity_file()?;
    let raw = fs::read_to_string(node_identity_json_path()?).map_err(|e| AuthError::Io(e.to_string()))?;
    let stored: StoredNodeIdentity =
        serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    if stored.format != NODE_FORMAT_V1 {
        return Err(AuthError::InvalidFile("unknown node identity format".to_string()));
    }
    let rsa_private = RsaPrivateKey::from_pkcs8_pem(stored.private_key_pem.trim())
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?;
    Ok(UnlockedNodeIdentity {
        node_name: stored.node_name,
        rsa_private,
        public_pem: stored.public_key_pem,
    })
}

fn unlock_identity_encrypted_pkcs8(stored: StoredIdentityFile, password: &str) -> Result<UnlockedIdentity, AuthError> {
    if stored.format != AUTH_FORMAT_V1 && stored.format != AUTH_FORMAT_V3 {
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

/// Load RSA identity: v1/v3 decrypt PKCS#8; v2 derives keys from password; v4 loads PEM from disk (password ignored).
pub fn unlock_identity(password: &str) -> Result<UnlockedIdentity, AuthError> {
    migrate_legacy_identity_file()?;
    let path = authentication_json_path()?;
    let raw = if path.exists() {
        fs::read_to_string(&path).map_err(|e| AuthError::Io(e.to_string()))?
    } else {
        let leg = legacy_identity_json_path()?;
        fs::read_to_string(&leg).map_err(|e| AuthError::Io(e.to_string()))?
    };
    let format: String = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| AuthError::InvalidFile(e.to_string()))?
        .get("format")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AuthError::InvalidFile("missing format".to_string()))?;

    match format.as_str() {
        AUTH_FORMAT_V4 => {
            let stored: StoredIdentityV4 =
                serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
            identity_from_v4(stored)
        }
        AUTH_FORMAT_V1 | AUTH_FORMAT_V3 => {
            let stored: StoredIdentityFile =
                serde_json::from_str(&raw).map_err(|e| AuthError::InvalidFile(e.to_string()))?;
            unlock_identity_encrypted_pkcs8(stored, password)
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

    /// Same memory cost as legacy v2/v3 defaults (tests only).
    const LEGACY_ARGON_M: u32 = 19456;
    const LEGACY_ARGON_T: u32 = 2;
    const LEGACY_ARGON_P: u32 = 1;

    #[test]
    fn deterministic_rsa_same_inputs_same_public_pem() {
        let (a, pem_a) = rsa_deterministic_from_password(
            b"secret",
            "alice",
            LEGACY_ARGON_M,
            LEGACY_ARGON_T,
            LEGACY_ARGON_P,
        )
        .unwrap();
        let (b, pem_b) = rsa_deterministic_from_password(
            b"secret",
            "alice",
            LEGACY_ARGON_M,
            LEGACY_ARGON_T,
            LEGACY_ARGON_P,
        )
        .unwrap();
        assert_eq!(pem_a, pem_b);
        assert_eq!(
            a.to_public_key().to_public_key_pem(LineEnding::LF).unwrap(),
            b.to_public_key().to_public_key_pem(LineEnding::LF).unwrap()
        );
    }

    #[test]
    fn wrong_password_changes_keys() {
        let (_, pem_ok) = rsa_deterministic_from_password(
            b"secret",
            "alice",
            LEGACY_ARGON_M,
            LEGACY_ARGON_T,
            LEGACY_ARGON_P,
        )
        .unwrap();
        let (_, pem_bad) = rsa_deterministic_from_password(
            b"wrong",
            "alice",
            LEGACY_ARGON_M,
            LEGACY_ARGON_T,
            LEGACY_ARGON_P,
        )
        .unwrap();
        assert_ne!(pem_ok, pem_bad);
    }
}
