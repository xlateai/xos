//! Offline identity: account + per-machine LAN keys (`~/.xos/auth/`).

mod store;

pub use store::{
    auth_data_dir, auth_identity_dir, auth_json_path, authentication_json_path, delete_identity,
    has_authentication, has_identity, has_node_identity, is_logged_in, load_identity,
    load_node_identity, login_offline, migrate_legacy_identity_file, node_id_from_public_pem,
    node_identity_json_path, reset_offline_identity, rsa_sign, rsa_verify, unlock_identity,
    whisper_model_backend_cache_dir, whisper_model_cache_dir, AuthError, StoredIdentityFile,
    StoredIdentityV2, StoredIdentityV4, StoredNodeIdentity, UnlockedIdentity, UnlockedNodeIdentity,
};
