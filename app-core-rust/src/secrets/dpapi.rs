use std::ptr::{null, null_mut};

use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};

pub(crate) fn dpapi_round_trip_available() -> bool {
    protect_secret(b"autofix-secret-check")
        .and_then(|protected| unprotect_secret(&protected))
        .is_ok_and(|secret| secret == b"autofix-secret-check")
}

fn protect_secret(secret: &[u8]) -> Result<Vec<u8>, &'static str> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err("CryptProtectData failed");
    }

    copy_blob(output)
}

fn unprotect_secret(secret: &[u8]) -> Result<Vec<u8>, &'static str> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };

    if ok == 0 {
        return Err("CryptUnprotectData failed");
    }

    copy_blob(output)
}

fn copy_blob(blob: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, &'static str> {
    if blob.pbData.is_null() {
        return Err("DPAPI returned no data");
    }

    let bytes = unsafe {
        let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
        LocalFree(blob.pbData as _);
        bytes
    };

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_and_unprotects_secret_with_dpapi() {
        assert!(dpapi_round_trip_available());
    }
}
