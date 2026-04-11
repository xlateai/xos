//! Offline identity: one RSA-2048 key pair per machine, private key encrypted at rest (Argon2id + AES-256-GCM).
//! Storage: Windows `%LOCALAPPDATA%\\xos\\`, Unix/macOS `~/.xos/`. iOS/Android: same relative layout under the app sandbox; production apps often move secrets to Keychain / Keystore.

mod store;

pub use store::{
    auth_data_dir, auth_json_path, has_identity, login_offline, rsa_sign, rsa_verify,
    unlock_identity, AuthError, StoredIdentityFile, UnlockedIdentity,
};
