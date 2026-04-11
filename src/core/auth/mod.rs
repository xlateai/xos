//! Offline identity: RSA-2048 for LAN mesh. **v2** derives keys from username + password (Argon2id +
//! ChaCha20-seeded RSA); only username and public PEM are stored. **v1** (legacy) encrypted a random key.
//! Storage: Windows `%LOCALAPPDATA%\\xos\\`, Unix/macOS `~/.xos/`.

mod store;

pub use store::{
    auth_data_dir, auth_json_path, has_identity, login_offline, rsa_sign, rsa_verify,
    unlock_identity, AuthError, StoredIdentityFile, StoredIdentityV2, UnlockedIdentity,
};
