//! Offline identity: RSA-2048 for LAN mesh. **v3** stores encrypted PKCS#8 (deterministic RSA); password
//! is never persisted. **v2**/**v1** legacy formats still unlock.
//! Storage: Windows `%LOCALAPPDATA%\\xos\\`, Unix/macOS `~/.xos/`.

mod store;

pub use store::{
    auth_data_dir, auth_json_path, delete_identity, has_identity, login_offline, rsa_sign,
    rsa_verify, unlock_identity, AuthError, StoredIdentityFile, StoredIdentityV2, UnlockedIdentity,
};
