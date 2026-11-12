mod console_text;
#[cfg(windows)]
mod windows;

pub use console_text::{ConsoleText, console_text};

#[cfg(windows)]
pub use windows::{
    allow_foreground, ansi_text, client_process_id, connect, copy_text, create_server,
    current_user_sid, oem_text, process_image,
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

pub fn same_image(image: &std::path::Path, installed: &std::path::Path) -> bool {
    let normal = |path: &std::path::Path| {
        let text = path.to_string_lossy().replace('/', "\\");
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_lowercase()
    };
    normal(image) == normal(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_image_is_recognised_whatever_the_spelling() {
        let installed = std::path::Path::new(r"C:\Program Files\ZVS\cli\aistudio.exe");
        for spelling in [
            r"c:\program files\zvs\CLI\AISTUDIO.EXE",
            r"\\?\C:\Program Files\ZVS\cli\aistudio.exe",
            "C:/Program Files/ZVS/cli/aistudio.exe",
        ] {
            assert!(
                same_image(std::path::Path::new(spelling), installed),
                "{spelling}"
            );
        }
        assert!(!same_image(
            std::path::Path::new(r"C:\Users\me\aistudio.exe"),
            installed
        ));
    }

    #[test]
    fn only_the_owner_is_granted_access() {
        assert_eq!(sddl_for("S-1-5-21-7"), "D:P(A;;GA;;;S-1-5-21-7)");
    }
}
