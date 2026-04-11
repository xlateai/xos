//! Offline identity: RSA-2048 for LAN mesh. **v4** stores PKCS#8 + public PEM in `identity.json`;
//! password is used only at `xos login --offline`. **v3**/**v2**/**v1** legacy formats still unlock via password.
//! Storage: Windows `%LOCALAPPDATA%\\xos\\`, Unix/macOS `~/.xos/`.

mod store;

pub use store::{
    auth_data_dir, auth_json_path, delete_identity, has_identity, load_identity, login_offline,
    rsa_sign, rsa_verify, unlock_identity, AuthError, StoredIdentityFile, StoredIdentityV2,
    StoredIdentityV4, UnlockedIdentity,
};
