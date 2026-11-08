# browser (shell)

The built-in browser: real WebView2 tabs inside the studio window, the browser chrome UI, the
CDP automation the agent uses, network capture, cookies, downloads, and the account gateway
that runs vendor requests inside the browser profile. Policy (which URLs, which actions, which
tier) lives in `zvs_host::browser`; this module is the WebView2/CDP mechanics behind it.

## Files

| File                     | Contents                                                                                          |
| ------------------------ | ------------------------------------------------------------------------------------------------- |
| `workspace.rs`           | `BrowserWorkspace`: tabs as child webviews, layout, chrome UI channel, downloads, HAR export      |
| `model.rs`               | `Model`, `Tab`, `PageSignal`, `LoadFailure`: the pure tab state machine and its Russian messages  |
| `automation.rs`          | `Session` per tab: the host `AutomationPort` over CDP (read, find, click, type, submit, download) |
| `cdp.rs`                 | `CdpClient`, `CdpTransport`, typed `Runtime.evaluate` / `callFunctionOn` helpers, events          |
| `network.rs`             | `NetworkRecorder`: `Network.*` events folded into request/response entries for the network panel  |
| `bearer.rs`              | `BearerTokens`: `Authorization` headers seen on account origins, `ACCOUNT_ORIGINS`                |
| `cookies.rs`             | Cookie identity and the domains a clear touched                                                   |
| `accounts.rs`            | Adapters from the workspace and gateway to host ports (URL opener, pages, profile, sessions)      |
| `gateway/`               | `AccountGateway` (see `gateway/README.md`)                                                        |
| `automation/presence.js` | The in-page cursor and highlight overlay shown while the agent acts                               |

## Pipeline: tabs (`workspace.rs`)

- Every tab is a child webview labelled `browser-site-N` in the studio window, all sharing one
  profile folder. The chrome (tab strip, address bar, agent panel) is a separate webview,
  `browser-ui`, which talks to the workspace through `browser_command` and receives
  `BrowserState` over `browser_subscribe`.
- Webview callbacks never mutate state directly. They post a `Work` item (`Signal`, `Adopt`,
  `Layout`, `Publish`, `Agent`, `Downloads`, `Closed`) to one worker, which applies it to the
  `Model` and then re-lays out and publishes. All tab state changes are therefore serialised.
- `Model` is pure: it tracks order, the active tab, titles, URLs, history flags, load errors
  and the tab limit (`TAB_LIMIT`, 30). `load_error` maps WebView2 failures to copy: an aborted
  load is silent, a cache miss after a POST explains form resubmission.
- `layout` computes each webview's rectangle from the mounted bounds minus chrome, status,
  approval, devtools and agent-panel heights (constants from `zvs_host::browser::policy`) and
  hides inactive tabs.
- A page's `window.open` is adopted as a new tab (`adopt_popup`): the popup is created in the
  opener's WebView2 environment so `opener` keeps working, then moved into the strip. It is
  denied when the URL is not an allowed main-frame URL or the tab limit is reached. The chrome
  webview itself can never open windows.
- Address-bar input goes through `navigation_url`, which refuses non-web schemes and blocked
  addresses.

## Pipeline: an agent action (`automation.rs`)

1. `Session::ensure_enabled` attaches CDP to the tab and enables `Page`, `DOM`,
   `Accessibility` and `Overlay`; automation scripts run in their own isolated world
   (`WORLD`).
2. `read_page` fetches the accessibility tree and turns it into an outline with
   `zvs_host::browser::page::outline`: text plus numbered element handles, truncated to a
   budget. Handles are stored per origin, so a handle from another site is unknown.
3. `click(handle)` resolves the handle back to a DOM node, reads its facts (visible, enabled,
   obscured, submits a form, editable, secret field) and refuses rather than acts when the
   facts say so: a click that would submit a form must go through `submit`, which has its own
   approval.
4. `type_text` additionally refuses password and other secret fields (checked before and
   after focusing, since focus can move), focuses via `DOM.focus`, verifies focus landed, and
   inserts text with `Input.insertText` (an empty string sends Backspace to clear).
5. While the user watches (`present`), `presence.js` draws the agent's cursor and highlights
   the target before each action.
6. Downloads are armed per tab: the next download within `DOWNLOAD_START` (15 s) is captured,
   moved to the download folder under a unique name and must finish within
   `DOWNLOAD_FINISH` (10 min).

## Network capture (`network.rs`)

`NetworkRecorder` subscribes to six `Network.*` events and merges them by request id into one
entry: request line and headers (plus extra-info headers), response status and headers, cache
hits, size and timing, and failure text. Request bodies and response bodies are capped
(`POST_DATA_LIMIT`, `BODY_LIMIT`). The panel can export the session as a HAR file
(`har_file_name`).

## Bearer tokens (`bearer.rs`)

On account origins only (`chat.qwen.ai`, `chat.z.ai`, plus `127.0.0.1` fixtures in test and
e2e builds), WebView2 request filters report the `Authorization: Bearer …` header of the
page's own requests. `BearerTokens` keeps the latest token per origin for the account
adapters, which need it alongside the cookies the gateway already uses.
