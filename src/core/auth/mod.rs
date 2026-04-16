//! Offline identity: **`authentication.json`** (account RSA) + **`node_identity.json`** (per-machine
//! LAN keys + `node_name`). **v4** account format; **v3**/**v2**/**v1** legacy unlock via password.
//! Storage: Windows `%LOCALAPPDATA%\\xos\\auth\\`, Unix/macOS `~/.xos/auth/`.

mod store;

pub use store::{
    auth_data_dir, auth_identity_dir, authentication_json_path, auth_json_path, delete_identity,
    has_authentication, has_identity, has_node_identity, is_logged_in, load_identity, load_node_identity,
    login_offline, migrate_legacy_identity_file, node_identity_json_path, node_id_from_public_pem,
    reset_offline_identity, rsa_sign, rsa_verify, unlock_identity, whisper_model_cache_dir,
    AuthError, StoredIdentityFile, StoredIdentityV2, StoredIdentityV4, StoredNodeIdentity,
    UnlockedIdentity, UnlockedNodeIdentity,
};
