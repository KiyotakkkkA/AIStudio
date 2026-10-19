# Account gateway

`AccountGateway` implements `zvs_host::drivers::ai::SessionGateway` for account-mode
providers (PASSPORT §12, §21). Every vendor request runs as `fetch()` inside the browser
profile, so the profile's cookies are used without ever leaving it. Rust sees the status,
the response headers (never `Set-Cookie`) and the body bytes, and nothing else.

## Pages

- The allow-list is `https://chat.qwen.ai`
  (`bearer::ACCOUNT_ORIGINS`). Builds with `cfg(test)` or the `e2e` feature also allow
  `http://127.0.0.1:<port>` for the fixture server. Any other URL is refused before a page
  exists.
- One gateway page per origin, created on first use: a child webview labelled
  `account-gateway-N` in the hidden window `account-gateway`, with the same `browser/work`
  data directory as the site tabs. It starts on `about:blank`, loses the Tauri scripts and
  web messages (`strip_host_bridge`), then loads the Qwen app at `https://chat.qwen.ai/`. Same-origin navigation is allowed
  while loading; SPA history changes keep the context alive. Fixture pages still use
  `<origin>/robots.txt` with exact URL matching. Cross-origin navigation, and new windows, downloads and permissions are denied.
- Once loaded, the host attaches over CDP (`browser::cdp`): `Runtime.enable`,
  `Page.getFrameTree` and `Runtime.executionContextCreated` identify the main frame's
  default context for Qwen. `Runtime.addBinding` is restricted to that context id; response
  events from other contexts are discarded. Fetch runs in the app's JavaScript world so
  installed request wrappers can run. Fixtures retain `Page.createIsolatedWorld` and the
  named-world binding. A probe verifies the allowed URL and completed document load.
  The binding only routes responses for registered requests; it exposes no host commands.
  Qwen scripts can see the gateway state in their own world, so this is deliberately a
  vendor response trust boundary, not an isolated-world confidentiality boundary.
- A page is replaced before the next request if it crashed, started a navigation (a reload
  included), changed origin (or static fixture URL) or was closed. Its requests in flight fail as unreachable. A request
  the page refuses (a stale context) is retried once on a new page.

## Protocol

`page.js` is one constant function, called with `Runtime.callFunctionOn` and a single JSON
argument. The URL, method, headers and body are never part of the source.

| `op`    | Effect                                                                      |
| ------- | --------------------------------------------------------------------------- |
| `probe` | Returns `location.href` and `document.readyState`                           |
| `start` | Starts `fetch(url, { credentials: "include" })` under an id; returns `true` |

`start` also carries the origin's request plan: `transport` (`fetch`, or `xhr` for Qwen's
`chats/new`, the page API its client uses there) and `challenge`, a pattern for the vendor's
verification dialog. While such a request waits for its head the page posts `challenge` once
if a matching element appears; the host then shows the gateway window (`reveal`) so the user
can pass the check, waits up to `challenge_timeout` (5 min) instead of the head timeout, and
hides it again (`conceal`). A watched request silent for `reveal_after` (10 s) is shown too.
Closing the shown window only hides it.
| `ack` | Gives the request `credit` more chunks |
| `abort` | Aborts the request's `AbortController` |

The page reports through the binding: `head` (status and headers), `challenge`, `chunk` (base64, at most
`chunk_size` bytes), `end`, or `error` (with `aborted`). A request may have `window` chunks
unacknowledged; the host acknowledges `ack_every` chunks as the consumer reads them, so a slow
reader holds the page back. Headers the browser owns (`cookie`, `origin`, `referer`,
`user-agent`, `host`, `sec-*`, …) are dropped before the call; a same-origin `referer` becomes
the fetch `referrer`.

Defaults (`GatewayOptions`): page ready 30 s, response head 120 s, body idle 300 s, 32 KiB
chunks, a window of 8, acknowledged every 4. The provider timeout in `AccountTransport` sits
on top. Cancellation, a timeout or dropping the body aborts the page request.

## Bearer capture

Site tabs register `WebResourceRequested` for the allow-listed origins (document requests).
`bearer::captured` keeps an `Authorization: Bearer <token>` only when the request origin is
allow-listed and equals the tab's top-level origin, as `sessionGateway.ts` did. Gateway pages
are not observed, so the host's own requests never set a token. Tokens live in memory
(`BearerTokens`).

## Tests

- `cargo test -p zvs-studio`: the CDP client against a fake transport, routing, the gateway
  against a fake page that follows `page.js`, the page watch and the bearer rule.
- `apps/studio/tests/accountGatewayPage.test.ts`: `page.js` itself in Node against a local
  HTTP server.
- `cargo test -p zvs-studio --features e2e --test account_gateway`: real WebView2 against a
  local fixture server (cookies from a tab login, bearer capture and a third-party frame,
  a 1 MiB body, an incremental stream, a failing status, cancellation, a lost connection, a
  timeout and page replacement). `ZVS_LIVE_VENDOR=1` also calls the Qwen identity endpoint.
- With the `e2e` feature, `ZVS_E2E_GATEWAY_PROBE=<url>` makes the app call that URL through
  the gateway every 15 s and log `Gateway probe` with the status.

## Qwen limitations

The shared profile preserves login and browser verification cookies. Loading the app and
using its fetch context does not guarantee acceptance by Qwen: deferred vendor initialization,
interactive verification, account limits, and changes to the private API still require live
validation. If verification is requested, open Qwen in the app browser and complete it manually
in this shared profile, then retry. No challenge tokens or browser fingerprints are fabricated.
