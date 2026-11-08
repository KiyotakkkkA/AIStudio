# drivers · mcp

The MCP client side: transports, the `rmcp` client wrapper, discovery, tool-safety
heuristics, OAuth sign-in, and Windows command launching. Server supervision (start, restart,
health, registering tools) is `services::mcp`; this module only connects and speaks.

`rmcp` (the official SDK, client features only) handles the protocol: handshake, requests,
timeouts with `notifications/cancelled`, sessions. The transports are ours.

## Files

| File                 | Contents                                                                                      |
| -------------------- | --------------------------------------------------------------------------------------------- |
| `client.rs`          | `McpClient`: connect, handshake, paged discovery, call, close; `McpEndpoint`                  |
| `stdio.rs`           | `StdioTransport`, `LaunchSpec`, `default_spawn`, `kill_tree`                                  |
| `http.rs`            | `McpHttpClient`: `rmcp`'s `StreamableHttpClient` over the shared `reqwest` client             |
| `transport.rs`       | Message decoding shared by both transports, `TransportHooks`, `MAX_MESSAGE_BYTES` (16 MiB)    |
| `protocol.rs`        | Descriptors, `CLIENT_PROTOCOL_VERSION`, `infer_destructive`, `infer_read_only`, `call_result` |
| `oauth.rs`           | OAuth 2.1 discovery, dynamic client registration, PKCE authorisation, token refresh           |
| `windows_command.rs` | Resolving `.cmd` / `.bat` / `.exe` on Windows and escaping arguments for `cmd.exe`            |
| `toolchain_env.rs`   | `isolated_toolchain_env`: keeps `npx` / `uvx` downloads inside the app folder                 |

## Connect and discover

1. Build the transport: stdio (`LaunchSpec`: command, args, env, cwd) or HTTP (URL plus
   static headers; secret header values are resolved at launch and never stored or logged).
2. Handshake: the client asks for `2025-06-18` and accepts any of
   `SUPPORTED_PROTOCOL_VERSIONS`, identifying as `zvs-ai-studio`.
3. Discovery lists tools, resources and prompts, following cursors for at most `MAX_PAGES`
   (50) pages per list; a list still unfinished after that is an error rather than a silently
   truncated catalogue.

## Stdio transport

- `default_spawn` on Windows resolves the command through `PATH` and `PATHEXT`
  (`resolve_windows_command`). A `.cmd` or `.bat` target runs as `cmd.exe /d /s /c` with every
  argument quoted by `escape_argument` (backslashes before a quote doubled, quotes escaped,
  then cmd metacharacters caret-escaped), so `npx`-style shims work without shell injection.
  `CREATE_NO_WINDOW` keeps consoles hidden.
- Stdio servers get `isolated_toolchain_env`: npm cache and prefix, uv cache/tools/Python and
  the pip cache all point under `<app data>/mcp-toolchain`, with
  `UV_PYTHON_PREFERENCE=only-managed`. A server's own env entries override these.
- stderr feeds the health rail; a line that is not valid JSON-RPC is logged host-side only
  (`MALFORMED_LOG`) and never forwarded to the model.
- `kill_tree` stops the whole process tree (`taskkill /T /F`) after a 2 s grace.

## HTTP transport

`McpHttpClient` sends the configured headers verbatim, echoes the server's `mcp-session-id`
on later requests, and names a 401/403 in the error so the UI can offer sign-in.

## Tool safety heuristics (`protocol.rs`)

- `infer_destructive`: the server's `destructiveHint` wins; `readOnlyHint: true` means not
  destructive; otherwise the tool name is matched against verbs at a word boundary
  (start, `.`, `_`, `-`, `/`): delete, destroy, drop, remove, erase, purge, truncate, wipe,
  rm, unlink, revoke, uninstall, reset, prune.
- `infer_read_only`: only an explicit `readOnlyHint: true`.

`services::mcp` stores both flags with each discovered tool; the tool registry uses them for
the tool's default tier.

## OAuth sign-in (`oauth.rs`)

1. **Resource metadata.** Try the `resource_metadata` URL from the server's
   `WWW-Authenticate` challenge, then `/.well-known/oauth-protected-resource` (path-aware and
   root variants).
2. **Authorisation server.** The first server listed in the resource metadata, else the MCP
   server's origin; fetch `oauth-authorization-server`, then `openid-configuration`
   metadata. A server without PKCE `S256` is refused.
3. **Client registration.** Dynamic registration when the server offers it.
4. **Authorise.** PKCE `S256` with a random verifier and `state`; the browser returns to a
   loopback listener on `/callback`. Sign-in is cancelled by the user or times out after
   5 minutes.
5. **Tokens.** `services::mcp` seals the `OAuthGrant` (tokens, expiry, client registration)
   with the crypto service and stores it with the server; an expiring grant is refreshed with
   the `refresh_token` grant before the next connection.
