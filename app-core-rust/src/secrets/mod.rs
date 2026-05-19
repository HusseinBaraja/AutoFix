mod credential_manager;
mod dpapi;

pub use credential_manager::{delete_secret, get_secret, has_secret, set_secret, SecretStoreError};
