#![allow(unsafe_code)]

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::time::Duration;

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, PipeMode, ServerOptions,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Globalization::{CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
use windows::core::{HSTRING, PWSTR};

use crate::sddl_for;

const BUSY_RETRIES: u32 = 50;
const BUSY_WAIT: Duration = Duration::from_millis(40);

fn native(error: &windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xFFFF)
}

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn token_sid(process: HANDLE) -> io::Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.map_err(|e| native(&e))?;
    let token = Owned(token);
    let mut needed = 0_u32;
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &raw mut needed) };
    if needed == 0 {
        return Err(io::Error::other("the token has no user"));
    }
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8)];
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        )
    }
    .map_err(|e| native(&e))?;
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) }.map_err(|e| native(&e))?;
    let sid = unsafe { text.to_string() }.map_err(|_| io::Error::other("the SID is not UTF-16"));
    let _ = unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    sid
}

pub fn oem_text(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0);
    let needed = unsafe { MultiByteToWideChar(CP_OEMCP, flags, bytes, None) };
    let Ok(size) = usize::try_from(needed) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    if size == 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut wide = vec![0_u16; size];
    let written = unsafe { MultiByteToWideChar(CP_OEMCP, flags, bytes, Some(&mut wide)) };
    let written = usize::try_from(written).unwrap_or(0).min(wide.len());
    String::from_utf16_lossy(&wide[..written])
}

pub fn allow_foreground() {
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}

pub fn current_user_sid() -> io::Result<String> {
    token_sid(unsafe { GetCurrentProcess() })
}

pub fn create_server(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let sid = current_user_sid()?;
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from(sddl_for(&sid)),
            SDDL_REVISION_1,
            &raw mut descriptor,
            None,
        )
    }
    .map_err(|e| native(&e))?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    let created = unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .pipe_mode(PipeMode::Byte)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast::<c_void>())
    };
    let _ = unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    created
}

fn server_sid(client: &NamedPipeClient) -> io::Result<String> {
    let pipe = HANDLE(client.as_raw_handle());
    let mut pid = 0_u32;
    unsafe { GetNamedPipeServerProcessId(pipe, &raw mut pid) }.map_err(|e| native(&e))?;
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(|e| native(&e))?;
    let process = Owned(process);
    token_sid(process.0)
}

pub async fn connect(name: &str) -> io::Result<NamedPipeClient> {
    let mut attempts = 0;
    let client = loop {
        match ClientOptions::new().pipe_mode(PipeMode::Byte).open(name) {
            Ok(client) => break client,
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0.cast_signed())
                    && attempts < BUSY_RETRIES =>
            {
                attempts += 1;
                tokio::time::sleep(BUSY_WAIT).await;
            }
            Err(error) => return Err(error),
        }
    };
    if server_sid(&client)? != current_user_sid()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the pipe is served by another user",
        ));
    }
    Ok(client)
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[test]
    fn the_current_user_has_a_sid() {
        assert!(current_user_sid().expect("a SID").starts_with("S-1-"));
    }

    #[tokio::test]
    async fn a_second_first_instance_is_refused_and_a_client_reaches_the_owner() {
        let name = format!(r"\\.\pipe\zvs-pipe-test-{}", std::process::id());
        let server = create_server(&name, true).expect("creates");
        assert!(create_server(&name, true).is_err());

        let serving = tokio::spawn(async move {
            server.connect().await.expect("accepts");
            let mut server = server;
            let mut buffer = [0_u8; 4];
            server.read_exact(&mut buffer).await.expect("reads");
            server.write_all(&buffer).await.expect("writes");
        });
        let mut client = connect(&name).await.expect("connects");
        client.write_all(b"ping").await.expect("writes");
        let mut echoed = [0_u8; 4];
        client.read_exact(&mut echoed).await.expect("reads");
        assert_eq!(&echoed, b"ping");
        serving.await.expect("serves");
    }
}
