//! Offline identity: **`authentication.json`** (account RSA) + **`node_identity.json`** (per-machine
//! LAN keys + `node_name`). **v4** account format; **v3**/**v2**/**v1** legacy unlock via password.
//! Storage: Windows `%LOCALAPPDATA%\\xos\\`, Unix/macOS `~/.xos/`.

mod store;

pub use store::{
    auth_data_dir, authentication_json_path, auth_json_path, delete_identity, has_authentication,
    has_identity, has_node_identity, load_identity, load_node_identity, login_offline,
    reset_offline_identity,
    migrate_legacy_identity_file, node_identity_json_path, node_id_from_public_pem, rsa_sign,
    rsa_verify, unlock_identity, AuthError, StoredIdentityFile, StoredIdentityV2, StoredIdentityV4,
    StoredNodeIdentity, UnlockedIdentity, UnlockedNodeIdentity,
};
