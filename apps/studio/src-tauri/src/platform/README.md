# platform (shell)

Windows implementations of the host's platform ports. `mod.rs` exposes one factory per port
(`crypto_backend`, `gpu_probe`, `audio_input`, `file_launcher`, `cli_installer`, `toaster`)
and picks the Windows type under `cfg(windows)` or a no-op host type otherwise. `site` and
`overlay` are re-exported the same way, with `site_stub.rs` and `overlay_stub.rs` as the
non-Windows stand-ins. The Win32 and COM calls need `unsafe`; each `windows/*.rs` file opts in
with `#![allow(unsafe_code)]` (the only other opt-in in the shell is one function in `e2e.rs`).

## Ports and their Windows implementations

| Port (`zvs-host`)    | Implementation                | How                                                                                             |
| -------------------- | ----------------------------- | ----------------------------------------------------------------------------------------------- |
| `CryptoBackend`      | `dpapi::DpapiBackend`         | `CryptProtectData` / `CryptUnprotectData` bound to the user account, no UI                      |
| `GpuProbe`           | `dxgi::DxgiProbe`             | DXGI adapter enumeration: name, vendor, dedicated video memory                                  |
| `AudioInput`         | `audio::WasapiInput`          | WASAPI shared-mode capture from the default (or chosen) microphone, PCM                         |
| `FileLauncher`       | `assoc::ShellLauncher`        | Default application lookup per extension, open via the shell, installed browsers                |
| `CliInstaller`       | `cli_path::UserPathInstaller` | Edits `HKCU\Environment\Path` and broadcasts `WM_SETTINGCHANGE`                                 |
| `Toaster` (shell)    | `toast::WindowsToaster`       | WinRT toast notifications with an app id, icon and activation callback                          |
| browser site hooks   | `webview2.rs`                 | WebView2 COM: navigation filtering, load failures, cookies, history, request sightings, corners |
| computer-use overlay | `overlay.rs`                  | Capture-excluded, non-activating overlay windows; pointer state; Shift+Esc stop key             |
| installed browsers   | `browsers.rs`                 | `SOFTWARE\Clients\StartMenuInternet` under HKCU and HKLM                                        |

## Notes per adapter

**DPAPI.** Ciphertext is `0x01` (`CIPHER_PREFIX`) followed by the DPAPI blob, so a future
scheme can be told apart. `DpapiBackend::probe` round-trips a fixed string at start-up; if
that fails the backend reports `is_available() == false` and `CryptoService` refuses to store
secrets up front instead of failing later. Non-Windows builds use the host's
`UnavailableBackend`. Decrypted buffers are zeroed before `LocalFree`.

**User `PATH`.** The value is read and written as `REG_EXPAND_SZ` without expansion
(`RRF_NOEXPAND`), so `%USERPROFILE%`-style entries survive. Membership and edits go through
`zvs_host::cli::path_list` (case-, quote- and slash-insensitive). After a write the shell
broadcasts `WM_SETTINGCHANGE` with `SMTO_ABORTIFHUNG`, so new terminals see the change.

**Overlay.** The computer-use panel and glow are excluded from screen capture
(`WDA_EXCLUDEFROMCAPTURE`) so the agent's screenshots never contain them, and use
`WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` so they never steal focus. A low-level keyboard hook
watches for Shift+Esc and ignores injected keystrokes, so only the physical user can stop
the agent, never the agent's own input.

**WebView2.** `allowed_main` / `allowed_frame` decide which top-level and frame navigations
may proceed; `strip_host_bridge` removes the Tauri IPC from site webviews; load errors map
to `model::LoadFailure`. Cookie and browsing-data calls are asynchronous COM operations
completed through reply callbacks.

**Browsers.** `program_of` extracts the executable from a registry command line (quoted or
not), and `distinct` removes duplicates registered under both hives.
