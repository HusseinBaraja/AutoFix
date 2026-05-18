use std::{error::Error, fmt, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_NOT_FOUND, FILETIME},
    Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    },
};

const TARGET_PREFIX: &str = "AutoFix/provider-profile/";
const MAX_CREDENTIAL_BLOB_BYTES: usize = 5 * 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretStoreError {
    EmptyProfileId,
    InvalidProfileId,
    SecretTooLarge,
    SecretNotUtf8,
    CredentialWriteFailed(u32),
    CredentialReadFailed(u32),
    CredentialDeleteFailed(u32),
}

impl fmt::Display for SecretStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProfileId => write!(formatter, "profile id must not be empty"),
            Self::InvalidProfileId => write!(formatter, "profile id contains invalid characters"),
            Self::SecretTooLarge => write!(formatter, "secret is too large for Credential Manager"),
            Self::SecretNotUtf8 => write!(formatter, "stored secret is not valid UTF-8"),
            Self::CredentialWriteFailed(code) => {
                write!(formatter, "failed to write secret credential: {}", code)
            }
            Self::CredentialReadFailed(code) => {
                write!(formatter, "failed to read secret credential: {}", code)
            }
            Self::CredentialDeleteFailed(code) => {
                write!(formatter, "failed to delete secret credential: {}", code)
            }
        }
    }
}

impl Error for SecretStoreError {}

pub fn set_secret(profile_id: &str, secret: &str) -> Result<(), SecretStoreError> {
    let target_name = credential_target_name(profile_id)?;
    let target_name_wide = wide_null(&target_name);
    let user_name_wide = wide_null("AutoFix");
    let mut secret_bytes = secret.as_bytes().to_vec();

    if secret_bytes.len() > MAX_CREDENTIAL_BLOB_BYTES {
        return Err(SecretStoreError::SecretTooLarge);
    }

    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_name_wide.as_ptr() as *mut u16,
        Comment: null_mut(),
        LastWritten: FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: secret_bytes.len() as u32,
        CredentialBlob: secret_bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: null_mut(),
        TargetAlias: null_mut(),
        UserName: user_name_wide.as_ptr() as *mut u16,
    };

    let ok = unsafe { CredWriteW(&credential, 0) };
    zero_bytes(&mut secret_bytes);

    if ok == 0 {
        return Err(SecretStoreError::CredentialWriteFailed(last_error()));
    }

    Ok(())
}

pub fn get_secret(profile_id: &str) -> Result<Option<String>, SecretStoreError> {
    read_secret_bytes(profile_id)?
        .map(String::from_utf8)
        .transpose()
        .map_err(|_| SecretStoreError::SecretNotUtf8)
}

pub fn delete_secret(profile_id: &str) -> Result<(), SecretStoreError> {
    let target_name = credential_target_name(profile_id)?;
    let target_name_wide = wide_null(&target_name);

    let ok = unsafe { CredDeleteW(target_name_wide.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        let error = last_error();
        if error != ERROR_NOT_FOUND {
            return Err(SecretStoreError::CredentialDeleteFailed(error));
        }
    }

    Ok(())
}

pub fn has_secret(profile_id: &str) -> Result<bool, SecretStoreError> {
    read_secret_bytes(profile_id).map(|secret| secret.is_some())
}

fn read_secret_bytes(profile_id: &str) -> Result<Option<Vec<u8>>, SecretStoreError> {
    let target_name = credential_target_name(profile_id)?;
    let target_name_wide = wide_null(&target_name);
    let mut credential = null_mut();

    let ok = unsafe {
        CredReadW(
            target_name_wide.as_ptr(),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    };

    if ok == 0 {
        let error = last_error();
        if error == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(SecretStoreError::CredentialReadFailed(error));
    }

    let secret = unsafe {
        let credential = &*credential;
        let secret = std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
        .to_vec();
        CredFree(credential as *const CREDENTIALW as _);
        secret
    };

    Ok(Some(secret))
}

fn credential_target_name(profile_id: &str) -> Result<String, SecretStoreError> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(SecretStoreError::EmptyProfileId);
    }
    if profile_id.chars().any(|character| character.is_control()) {
        return Err(SecretStoreError::InvalidProfileId);
    }

    Ok(format!("{}{}", TARGET_PREFIX, profile_id))
}

fn wide_null(input: &str) -> Vec<u16> {
    input.encode_utf16().chain([0]).collect()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

fn zero_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_reads_updates_and_deletes_secret_by_profile_id() {
        let profile_id = format!(
            "credential-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        delete_secret(&profile_id).unwrap();

        assert!(!has_secret(&profile_id).unwrap());
        assert_eq!(get_secret(&profile_id).unwrap(), None);

        set_secret(&profile_id, "first-token").unwrap();
        assert!(has_secret(&profile_id).unwrap());
        assert_eq!(
            get_secret(&profile_id).unwrap(),
            Some("first-token".to_owned())
        );

        set_secret(&profile_id, "updated-token").unwrap();
        assert_eq!(
            get_secret(&profile_id).unwrap(),
            Some("updated-token".to_owned())
        );

        delete_secret(&profile_id).unwrap();
        assert!(!has_secret(&profile_id).unwrap());
        assert_eq!(get_secret(&profile_id).unwrap(), None);
    }

    #[test]
    fn rejects_invalid_profile_ids() {
        assert_eq!(
            set_secret(" ", "secret"),
            Err(SecretStoreError::EmptyProfileId)
        );
        assert_eq!(
            set_secret("bad\nprofile", "secret"),
            Err(SecretStoreError::InvalidProfileId)
        );
        assert_eq!(has_secret(" "), Err(SecretStoreError::EmptyProfileId));
    }

    #[test]
    fn secret_value_does_not_appear_in_errors() {
        let error = set_secret("valid-profile", &"x".repeat(MAX_CREDENTIAL_BLOB_BYTES + 1))
            .unwrap_err()
            .to_string();

        assert!(!error.contains('x'));
    }
}
