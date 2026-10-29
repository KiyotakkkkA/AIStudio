#![allow(unsafe_code)]

use std::path::PathBuf;

use ::windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, LPARAM, WPARAM};
use ::windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_EXPAND_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use ::windows::core::{HSTRING, PCWSTR};
use zvs_host::cli::{CliInstallState, CliInstaller, path_contains, path_with, path_without};

const ENVIRONMENT: &str = "Environment";
const PATH: &str = "Path";
const BROADCAST_TIMEOUT_MS: u32 = 5000;

pub struct UserPathInstaller {
    binary: PathBuf,
}

impl UserPathInstaller {
    pub const fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    fn folder(&self) -> Option<String> {
        self.binary
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned())
    }

    fn change(&self, edit: impl Fn(&str, &str) -> Option<String>) -> Result<(), String> {
        let folder = self.folder().ok_or("the CLI has no folder")?;
        let current = read_user_path()?;
        if let Some(updated) = edit(&current, &folder) {
            write_user_path(&updated)?;
            broadcast_environment();
        }
        Ok(())
    }
}

impl CliInstaller for UserPathInstaller {
    fn state(&self) -> CliInstallState {
        let available = self.binary.is_file();
        let installed = self
            .folder()
            .zip(read_user_path().ok())
            .is_some_and(|(folder, list)| path_contains(&list, &folder));
        CliInstallState {
            available,
            installed,
        }
    }

    fn install(&self) -> Result<(), String> {
        if !self.binary.is_file() {
            return Err("the CLI binary is missing".to_owned());
        }
        self.change(path_with)
    }

    fn uninstall(&self) -> Result<(), String> {
        self.change(path_without)
    }
}

fn read_user_path() -> Result<String, String> {
    let subkey = HSTRING::from(ENVIRONMENT);
    let value = HSTRING::from(PATH);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let mut bytes = 0_u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &value,
            flags,
            None,
            None,
            Some(&raw mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(String::new());
    }
    if status != ERROR_SUCCESS {
        return Err(format!("reading the user Path failed: {}", status.0));
    }
    let mut buffer = vec![0_u16; (bytes as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &value,
            flags,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("reading the user Path failed: {}", status.0));
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..end]))
}

fn write_user_path(list: &str) -> Result<(), String> {
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(ENVIRONMENT),
            None,
            KEY_SET_VALUE,
            &raw mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("opening the user environment failed: {}", status.0));
    }
    let bytes: Vec<u8> = list
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let status =
        unsafe { RegSetValueExW(key, &HSTRING::from(PATH), None, REG_EXPAND_SZ, Some(&bytes)) };
    let _ = unsafe { RegCloseKey(key) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("writing the user Path failed: {}", status.0))
    }
}

fn broadcast_environment() {
    let area = HSTRING::from(ENVIRONMENT);
    let _ = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(PCWSTR(area.as_ptr()).0 as isize),
            SMTO_ABORTIFHUNG,
            BROADCAST_TIMEOUT_MS,
            None,
        )
    };
}
