---
task: TASK_066
title: Account gateway — vendor requests issued inside the browser profile, CDP plumbing, bearer capture
phase: M — Migration to Tauri
status: done
depends_on: [TASK_065]
unblocks: [TASK_067, TASK_046]
estimate: 5 h
---

# TASK_066 — Account gateway — vendor requests issued inside the browser profile, CDP plumbing, bearer capture

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Account-mode providers (Qwen, DeepSeek) have no API key. The user signs into the vendor's web
app in the integrated browser, and the host then sends that vendor's API calls **with the
browser profile's cookies** (PASSPORT §12). In Electron that is `host/browser/sessionGateway.ts`:
- `session.fromPartition("persist:browser-work").fetch(url, { credentials: "include" })`
  issues the request through Chromium's network stack, inside the partition.
- `webRequest.onSendHeaders` on the two vendor origins captures the `Authorization: Bearer …`
  that the vendor's own page sends, but only when the request came from a tab on that same
  origin. That gives `accountToken(url)`.
- `userAgent()` returns the profile's user agent.

`AccountTransport` consumes this `SessionGateway` port and never sees a cookie.

Tauri has no "fetch through a webview profile" API. **Decided 2026-10-10: the request runs as
`fetch()` inside a hidden webview of the browser profile, so cookies never leave the
profile.** This task builds that gateway:
- A hidden window holds one gateway webview per vendor origin, in the same `browser/work` data
  directory as TASK_065. Each one is navigated to a static, script-free document on the vendor
  origin (for example `/robots.txt`), so the requests are same-origin and need no CORS.
- The host drives these webviews over the **Chrome DevTools Protocol** that WebView2 exposes
  (`CallDevToolsProtocolMethod` and `GetDevToolsProtocolEventReceiver`, reached through
  `with_webview`). `Runtime.addBinding` installs a function that exists only in that hidden
  page and only for the host. `Runtime.evaluate` starts a `fetch` that streams the status,
  headers and body chunks back through the binding. The host reassembles them into a
  `SessionResponse` with a byte stream. Cancellation is an `AbortController` inside the page,
  keyed by request id.
- No Tauri IPC and no capability are granted to any remote origin. The binding exists only in
  the gateway page, which runs no vendor script.

The CDP client built here (`zvs-studio::browser::cdp`) is the same plumbing TASK_046 needs for
automation. Build it once, small and typed.

Bearer capture becomes the WebView2 `WebResourceRequested` event filtered to the two vendor
origins on **site tab** webviews. It reads the `Authorization` request header only when the
tab's top-level origin equals the request origin, exactly as the TS filter does.

## Why

Without this, account-mode providers cannot work under Tauri. Doing it inside the profile is
what keeps PASSPORT's rule that cookies are never copied out of the partition, and with it the
promise that clearing a vendor's cookies really un-links the account.

## Depends on

- `TASK_065` — the browser profile, site webviews and the WebView2 COM module.

## Read first

- `/docs/PASSPORT.md` §12 (account mode), §21 (session and identity), §14
- `apps/studio/src/host/browser/sessionGateway.ts`
- `apps/studio/src/host/drivers/ai/transport/SessionGateway.ts`, `AccountTransport.ts`: the
  consumer contract, including how the body is read and streamed
- `apps/studio/tests/sessionGateway.test.ts`
- `docs/QWEN_WEB.md`

## Scope

**In scope**
- `zvs-host::drivers::ai::transport::SessionGateway` trait, mirroring the TS port:
  `partition()`, `user_agent()`, `account_token(url)`, and
  `fetch(url, SessionRequest, CancellationToken) -> SessionResponse` (status, headers, body
  as an async byte stream)
- `zvs-studio::browser::cdp`: send a command with an id and await the result, subscribe to
  named events, typed helpers for the handful of methods used (`Runtime.enable`,
  `Runtime.addBinding`, `Runtime.evaluate`, `Runtime.bindingCalled`). This is not a general
  CDP library.
- `zvs-studio::browser::AccountGateway`, which implements `SessionGateway`:
  - lazily creates the hidden gateway webview per origin, from a fixed allow-list
    (`https://chat.qwen.ai`, `https://chat.deepseek.com`)
  - checks it navigated to the expected same-origin static URL before every request
  - a request script that is a constant: the URL, method, headers and body are passed as one
    JSON argument, never concatenated into source
  - streams chunks base64-encoded, bounded per message, with backpressure (the page waits for
    an ack every N chunks)
  - timeouts, cancellation, and a gateway webview that crashed or navigated away is recreated
- Bearer capture via `WebResourceRequested` on site tabs, with the origin-match rule, held in
  memory only
- `user_agent()` from the profile webview
- A local test fixture: a tiny HTTPS-less origin is not representative, so the allow-list
  gains `http://127.0.0.1:<port>` **only** under `cfg(test)` or the `e2e` feature, to drive a
  local fixture server

**Out of scope** (do not do these, even if tempting)
- `AccountTransport`, identity probes, adapters and `AccountService` (TASK_067)
- Copying cookies into `reqwest`. That was considered and rejected on 2026-10-10.
- Automation primitives (TASK_046). Only the CDP plumbing is built here.

## Steps

1. The `SessionGateway` trait in `zvs-host`, with a fake implementation for tests.
2. The CDP client over WebView2, with a test against a local fixture page: evaluate,
   bindings, events.
3. The gateway webview lifecycle in a hidden window, using the same data directory as the
   site tabs. Verify that a cookie set by logging in from a tab is sent by a gateway request
   (fixture server echoes the cookie header).
4. The fetch script and stream reassembly: large bodies, SSE streams arriving incrementally,
   non-2xx statuses with bodies, network errors, cancellation mid-stream, timeout.
5. Bearer capture with the origin-match rule. Test that a third-party frame's request does not
   set the token.
6. Security review notes in the handoff: what the binding can be called with, why no remote
   origin holds a capability, and what happens if the vendor's static page ever gains a
   script.

## Verify

```bash
cargo test -p zvs-host -p zvs-studio
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: log into chat.qwen.ai in the integrated browser. A debug-only command, removed
before the task closes or gated behind the `e2e` feature, fetches the Qwen identity endpoint
through the gateway and logs the status. Clear `qwen.ai` cookies in Sites & cookies and the
same request returns the signed-out response.

## Definition of done

- [x] Vendor requests run inside the browser profile, and no cookie value ever reaches Rust or
      a DTO
- [x] Streaming, cancellation, timeouts and gateway recreation work and are tested against a
      fixture server
- [x] Bearer capture matches the TS origin rule
- [x] No remote origin holds any Tauri capability
- [x] The CDP client is reusable by TASK_046
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-host`:
  - `drivers::ai::transport::session`: the `SessionGateway` trait (`partition`, `user_agent`,
    `account_token`, `fetch(url, SessionRequest, &CancellationToken)`), `SessionRequest`,
    `SessionResponse` (status, lower-cased `Headers`, a `ByteStream` body, `bytes`, `text`),
    `AccountToken` (`bearer`, `authorization`) and `SharedSessionGateway`. Debug output never
    prints a token, a header value or a body.
  - `browser::BROWSER_PARTITION` (`persist:browser-work`).
  - `testing::FakeSessionGateway` (a responder closure, recorded requests, tokens per origin,
    honours a fired token) with `FakeReply`.
- `zvs-studio` (`browser` is now `pub` in the lib, so the e2e test and TASK_046 can reach it):
  - `browser::cdp`: `CdpTransport` (a call with a reply, a listener per event), `CdpClient`
    (ids, a pending map, a per-call timeout, `close` fails pending calls, event fan-out with
    one listener per event name), `CdpError`, `RemoteObject`, `BindingCalled`, and typed
    helpers `runtime_enable`, `add_binding`, `binding_calls`, `evaluate`, `call_function_on`,
    `main_frame` and `create_isolated_world`. `WebviewCdp` runs it through `with_webview` over
    WebView2's `CallDevToolsProtocolMethod` and `GetDevToolsProtocolEventReceiver`.
  - `browser::bearer`: the allow-list, `account_origin`, `request_filters`, `Sighting`,
    `captured` (the TS rule) and `BearerTokens`, an in-memory map per origin.
  - `browser::gateway`: `AccountGateway` (implements `SessionGateway`), `GatewayOptions`, the
    `GatewayPage` and `PageFactory` ports, `routes::Routes` (binding messages to requests,
    size-capped, unknown ids dropped), `page.js` (with `page.d.ts`), and `WebviewPages`, the
    WebView2 factory with its `PageWatch` state machine.
  - `platform::windows::webview2`: `call_devtools`, `listen_devtools`, `observe_bearer`
    (`WebResourceRequested` for document requests, with `ICoreWebView2_22` source kinds when
    present) and `user_agent`. `site_stub` has the same four.
  - `BrowserWorkspace::new` takes the shared `BearerTokens`, and every site tab registers
    bearer capture next to `observe_site`.
  - Composition: one `Arc<AccountGateway>` managed as Tauri state, registered as
    `account-gateway` in the `Dispose` stage, and disposed when the studio window is destroyed
    (otherwise the hidden gateway window would keep the app alive).
  - `windows-test-manifest.xml` and `rustc-link-arg-tests` in `build.rs`. Test binaries that
    create windows need the common-controls manifest, or they die with
    `STATUS_ENTRYPOINT_NOT_FOUND`.
- Tests: 5 new in `zvs-host` (275 in total, 4 ignored). 45 new in `zvs-studio` (70 in total):
  the CDP client, routes, the page watch, the bearer rule, and 17 gateway cases against a fake
  page that follows `page.js`. They cover 64 KiB under backpressure, data-only arguments and
  stripped headers, one page per origin, refused origins, an incremental stream, a 503 body,
  network errors before and during the body, cancellation before the head and mid-stream, a
  dropped body, head, idle and ready timeouts, a crashed page, a lost context retried once, and
  forged and oversized chunks. 8 tests in `apps/studio/tests/accountGatewayPage.test.ts` run
  `page.js` itself in Node against a local HTTP server.
- `tests/account_gateway.rs` (`harness = false`, `required-features = ["e2e"]`) drives real
  WebView2 against a local axum fixture. All 17 checks passed on Windows 11 on 2026-10-15:
  - a cookie set by logging in from a tab is sent by a gateway request
  - the tab's bearer is captured, and a third-party frame's is not
  - the gateway page's main world has no binding, no state, and no `__TAURI_INTERNALS__`,
    `ipc` or `chrome.webview`
  - a 1 MiB body arrives byte for byte
  - an SSE event arrives before the server sends the next one
  - cancelling closes the server connection
  - a reload replaces the page

  With `ZVS_LIVE_VENDOR=1`, the real `https://chat.qwen.ai/api/v1/auths/` answered
  `401 application/json` through the gateway from a fresh profile.
- Docs: `apps/studio/src-tauri/src/browser/gateway/README.md` and `crates/zvs-host/README.md`
  (Session gateway, the shutdown list, the module table).

**Deviated from plan:**

- No source comments, at the user's request.
- The request runs in a CDP **isolated world**, not in the page's main world. The binding is
  added with `executionContextName`, so it exists only there, and the page's own scripts can
  neither call it nor patch `fetch`. This needs `Page.getFrameTree` and
  `Page.createIsolatedWorld` on top of the listed methods.
- The request script runs through `Runtime.callFunctionOn`: one constant function, one JSON
  argument. `Runtime.evaluate` would have put the request into the source. `evaluate` is still
  a typed helper, and the e2e test uses it.
- `page.js` is an ES module (`export default function gateway`), so ESLint, Prettier and
  Vitest treat it as code. The Rust constant strips `export default `.
- Page loads are detected through `observe_site`'s signals rather than `on_page_load`, because
  `strip_host_bridge` runs on gateway pages too. The page starts on `about:blank`, like a site
  tab.
- Forbidden request headers (`cookie`, `origin`, `referer`, `user-agent`, `host`, `sec-*`, …)
  are dropped in Rust, because the browser sets its own. A same-origin `referer` is passed as
  the fetch `referrer`. `Set-Cookie` is removed in the page and again in Rust.
- `user_agent()` returns the UA read from the first ready gateway page, and an empty string
  until then. It only feeds the `user-agent` header, which the page fetch ignores.
- A network failure in the page maps to `PROVIDER_UNREACHABLE` «Провайдер недоступен». A
  disallowed origin is `UNKNOWN` with `reason: origin-not-allowed`, and a malformed chunk is
  `UNKNOWN` with `reason: gateway-protocol`.
- The gateway's own timeouts sit above `AccountTransport`'s: 30 s for a page to become ready,
  120 s for the response head, 300 s between body chunks.

**Security review:**

- What the binding accepts: `__zvsGatewaySend` takes one string. The host parses it as
  `{ id, kind: head | chunk | end | error, … }`. It drops anything over 1 MiB, anything
  malformed and anything for an id it did not register, and it ends a request on a chunk that
  is not valid base64 or exceeds `chunk_size`. The most a forged message could do is shape the
  response of a request already in flight to that same vendor origin. It cannot reach Tauri,
  the file system or another origin.
- Why no remote origin holds a capability: capabilities name webviews by label (`studio`,
  `browser-ui`), and neither site tabs nor `account-gateway-*` are in any. Gateway pages lose
  the Tauri scripts and web messages, and the e2e test confirms `__TAURI_INTERNALS__`, `ipc`
  and `chrome.webview` are undefined there. No command was added to the isolation allowlist.
- If the vendor's static page gains a script: it runs in the main world, where the binding and
  the gateway state do not exist, so it cannot post messages or see requests. If it navigates
  or reloads, the page is replaced before the next request, and navigation off the exact
  `robots.txt` URL is blocked. The script would have the vendor's own origin and cookies, which
  any vendor page already has.
- Cookie values never cross into Rust. The host sends no cookie header, the browser attaches
  the profile's cookies itself, and `Set-Cookie` is stripped twice. Bearer tokens are read only
  from site tabs whose top-level origin equals the request origin, and are kept in memory.

**Next session should know:**

- TASK_067 gets the gateway with `app.state::<Arc<AccountGateway>>()` and can hand it out as a
  `SharedSessionGateway`. `FakeSessionGateway` in `zvs_host::testing` is there for the
  `AccountTransport` and identity-probe tests.
- TASK_046 reuses `browser::cdp`: build a `CdpClient` with `WebviewCdp::client(webview,
  timeout)` and add typed helpers next to the existing ones. Do not await CDP calls on the main
  thread; `with_webview` posts to it.
- Curl gets an AWS WAF challenge (`202`, empty HTML) from DeepSeek's `robots.txt`. A profile
  that has passed the challenge should get the file. If it does not, the gateway page loads the
  challenge, which runs in the main world and cannot touch the gateway, but the API calls may
  then be challenged too. Watch for this in TASK_067's manual check.
- Gates run on Windows on 2026-10-15, all exit 0: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` (also with `--features e2e` on
  `zvs-studio`), `cargo test --workspace`,
  `cargo test -p zvs-studio --features e2e --test account_gateway`, `pnpm typecheck`,
  `pnpm lint`, `pnpm format:check`, `pnpm build`, `pnpm build:tauri`, `npx vitest run`
  (1200 passed, 2 skipped) and `node scripts/contract-channels.mjs --check`.
- Not done: the manual check with a signed-in Qwen account. Set
  `ZVS_E2E_GATEWAY_PROBE=https://chat.qwen.ai/api/v1/auths/`, run
  `pnpm dev:tauri --features e2e`, sign into chat.qwen.ai in `/browser`, and watch
  `Gateway probe` in `studio.jsonl` turn `200` with `bearer: true`. Then clear `qwen.ai` in
  Sites & cookies. The captured bearer stays in memory, as in TS, so whether the next probe is
  signed out depends on how Qwen validates the token.
