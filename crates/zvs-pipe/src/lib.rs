#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    allow_foreground, connect, copy_text, create_server, current_user_sid, oem_text,
};

#[cfg(windows)]
pub use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer};

#[cfg(windows)]
pub fn pipe_name() -> std::io::Result<String> {
    current_user_sid().map(|sid| zvs_contract::cli_pipe_name(&sid))
}

pub fn sddl_for(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_owner_is_granted_access() {
        assert_eq!(sddl_for("S-1-5-21-7"), "D:P(A;;GA;;;S-1-5-21-7)");
    }
}
