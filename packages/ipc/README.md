# IPC client

The renderer's end of the process boundary, derived from the one contract in `@zvs/shared`
(PASSPORT §7). The host end is `zvs-host::ipc::IpcServer` in Rust, reached through the Tauri
command `ipc_call`; the renderer's `tauriBridge` turns that command into the `IpcBridge` this
package wraps.

`createIpcClient(contract, bridge, { validateResponses })` validates input before it reaches
the bridge, validates the response envelope by default, resolves with the channel's output and
rejects with `IpcError`. Switch on `IpcError.code`; never parse its `message`. Turning
`validateResponses` off trusts the envelope as is; the studio does that only when built with
`ZVS_VALIDATE_IPC=false`.

Nothing here knows about a feature: add a channel to the contract and a handler in
`zvs-host`, done.
