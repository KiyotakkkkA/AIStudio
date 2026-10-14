#![allow(unsafe_code)]

use std::slice;

use ::windows::Win32::Foundation::{HLOCAL, LocalFree};
use ::windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use ::windows::core::PCWSTR;
use zeroize::Zeroize;
use zvs_host::services::Plaintext;
use zvs_host::services::crypto::{BackendFailure, CryptoBackend};

pub const CIPHER_PREFIX: u8 = 0x01;

const PROBE: &str = "zvs-dpapi-probe";

#[derive(Default)]
struct LocalBlob(CRYPT_INTEGER_BLOB);

impl LocalBlob {
    fn as_slice(&self) -> &[u8] {
        if self.0.pbData.is_null() || self.0.cbData == 0 {
            return &[];
        }
        unsafe { slice::from_raw_parts(self.0.pbData, self.0.cbData as usize) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        if self.0.pbData.is_null() || self.0.cbData == 0 {
            return &mut [];
        }
        unsafe { slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize) }
    }
}

impl Drop for LocalBlob {
    fn drop(&mut self) {
        if self.0.pbData.is_null() {
            return;
        }
        self.as_mut_slice().zeroize();
        unsafe {
            LocalFree(Some(HLOCAL(self.0.pbData.cast())));
        }
    }
}

fn input_blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, BackendFailure> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len())
            .map_err(|_| BackendFailure::new("the value is too large for DPAPI"))?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

fn failure(error: &::windows::core::Error) -> BackendFailure {
    BackendFailure::new(format!("DPAPI failed: {}", error.code()))
}

fn protect(bytes: &[u8]) -> Result<Vec<u8>, BackendFailure> {
    let input = input_blob(bytes)?;
    let mut output = LocalBlob::default();
    unsafe {
        CryptProtectData(
            &raw const input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output.0,
        )
    }
    .map_err(|error| failure(&error))?;
    let blob = output.as_slice();
    let mut cipher = Vec::with_capacity(blob.len() + 1);
    cipher.push(CIPHER_PREFIX);
    cipher.extend_from_slice(blob);
    Ok(cipher)
}

fn unprotect(cipher: &[u8]) -> Result<Plaintext, BackendFailure> {
    let Some((&CIPHER_PREFIX, blob)) = cipher.split_first() else {
        return Err(BackendFailure::new("unsupported cipher version"));
    };
    let input = input_blob(blob)?;
    let mut output = LocalBlob::default();
    unsafe {
        CryptUnprotectData(
            &raw const input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output.0,
        )
    }
    .map_err(|error| failure(&error))?;
    std::str::from_utf8(output.as_slice())
        .map(Plaintext::new)
        .map_err(|_| BackendFailure::new("the decrypted value is not UTF-8"))
}

#[derive(Debug, Clone, Copy)]
pub struct DpapiBackend {
    available: bool,
}

impl DpapiBackend {
    pub fn probe() -> Self {
        let available = protect(PROBE.as_bytes())
            .and_then(|cipher| unprotect(&cipher))
            .is_ok_and(|plaintext| plaintext.expose() == PROBE);
        Self { available }
    }
}

impl CryptoBackend for DpapiBackend {
    fn is_available(&self) -> bool {
        self.available
    }

    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, BackendFailure> {
        protect(plaintext.as_bytes())
    }

    fn decrypt(&self, cipher: &[u8]) -> Result<Plaintext, BackendFailure> {
        unprotect(cipher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUE: &str = "sk-or-v1-0a1b2c3d4e5f60718293a4b5c6d7e8f9c31";

    #[test]
    fn the_backend_is_available_for_the_current_user() {
        assert!(DpapiBackend::probe().is_available());
    }

    #[test]
    fn a_value_round_trips_and_the_ciphertext_is_opaque_and_versioned() {
        let backend = DpapiBackend::probe();
        for value in [VALUE, "", "пароль — ёжик 🔑"] {
            let cipher = backend.encrypt(value).expect("encrypts");
            assert_eq!(cipher[0], CIPHER_PREFIX);
            assert!(cipher.len() > value.len() + 1);
            if !value.is_empty() {
                assert!(
                    !cipher
                        .windows(value.len())
                        .any(|window| window == value.as_bytes())
                );
            }
            assert_eq!(backend.decrypt(&cipher).expect("decrypts").expose(), value);
        }
        assert_ne!(
            backend.encrypt(VALUE).expect("encrypts"),
            backend.encrypt(VALUE).expect("encrypts")
        );
    }

    #[test]
    fn a_tampered_blob_fails_to_decrypt() {
        let backend = DpapiBackend::probe();
        let cipher = backend.encrypt(VALUE).expect("encrypts");
        for index in [cipher.len() / 2, cipher.len() - 1] {
            let mut tampered = cipher.clone();
            tampered[index] ^= 0xff;
            assert!(backend.decrypt(&tampered).is_err(), "byte {index}");
        }
        assert!(backend.decrypt(&cipher[..cipher.len() - 8]).is_err());
    }

    #[test]
    fn an_unknown_version_prefix_or_an_empty_blob_is_refused() {
        let backend = DpapiBackend::probe();
        let mut cipher = backend.encrypt(VALUE).expect("encrypts");
        cipher[0] = 0x02;
        assert_eq!(
            backend.decrypt(&cipher).expect_err("refuses"),
            BackendFailure::new("unsupported cipher version")
        );
        assert!(backend.decrypt(&[]).is_err());
        assert!(backend.decrypt(&[CIPHER_PREFIX]).is_err());
    }
}
