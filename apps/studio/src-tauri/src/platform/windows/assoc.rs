#![allow(unsafe_code)]

use std::path::Path;

use ::windows::Win32::UI::Shell::{
    ASSOCF_INIT_IGNOREUNKNOWN, ASSOCF_NOTRUNCATE, ASSOCSTR_FRIENDLYAPPNAME, AssocQueryStringW,
};
use ::windows::core::{HSTRING, PCWSTR, PWSTR};
use zvs_contract::AppError;
use zvs_host::platform::external::{FileLauncher, InstalledBrowser, open_with_default_application};

#[derive(Debug, Clone, Copy, Default)]
pub struct ShellLauncher;

impl FileLauncher for ShellLauncher {
    fn application(&self, extension: &str) -> Option<String> {
        let association = HSTRING::from(format!(".{extension}"));
        let flags = ASSOCF_INIT_IGNOREUNKNOWN | ASSOCF_NOTRUNCATE;
        let mut length = 0_u32;
        let _ = unsafe {
            AssocQueryStringW(
                flags,
                ASSOCSTR_FRIENDLYAPPNAME,
                &association,
                PCWSTR::null(),
                None,
                &raw mut length,
            )
        };
        if length == 0 {
            return None;
        }
        let mut buffer = vec![0_u16; length as usize];
        unsafe {
            AssocQueryStringW(
                flags,
                ASSOCSTR_FRIENDLYAPPNAME,
                &association,
                PCWSTR::null(),
                Some(PWSTR(buffer.as_mut_ptr())),
                &raw mut length,
            )
        }
        .ok()
        .ok()?;
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        let name = String::from_utf16_lossy(&buffer[..end]);
        (!name.trim().is_empty()).then_some(name)
    }

    fn open(&self, path: &Path) -> Result<(), AppError> {
        open_with_default_application(path)
    }

    fn browsers(&self) -> Vec<InstalledBrowser> {
        super::browsers::installed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unregistered_extension_has_no_application() {
        assert_eq!(
            ShellLauncher.application("zvs-unregistered-extension"),
            None
        );
    }

    #[test]
    fn a_shell_handled_extension_resolves_to_a_name() {
        assert!(ShellLauncher.application("txt").is_some());
    }
}
