# zvs-copilot

Credential-free protocol types for GitHub Copilot CLI. No I/O or vendor SDK.
The Rust host launches an installed CLI over Content-Length framed JSON-RPC stdio;
no Node runtime is added to the app. VS Code's bootstrap shim is not a CLI server.

Sign-in reuses the CLI's active github.com OAuth identity, or opens GitHub authorization
in the system browser through `session.accounts.login.begin` / `advance`. Credentials
remain in the CLI's credential store. Plaintext persistence consent is never granted.
ZVS stores only the identity, verifies it before discovery and every turn, and refuses
ambient token/BYOK overrides. Unlinking removes the ZVS association and preserves CLI login.
One active Copilot identity is supported; GitHub Enterprise identities are not offered.

`models.list` supplies model names, enabled policy state, context limits and supported
reasoning efforts. The existing low / auto / high control maps to `low` / omitted / `high`.
The explicit control is available when the model advertises both low and high; otherwise
the model default is used. The host revalidates effort against the live catalog.

Auto model selection is always offered as the model id `auto` («Автовыбор модели»), after the
enabled catalog models. Plans that route every turn (Copilot Free, Student) may list no models or
only non-`enabled` policies; `auto` still works for them because the CLI resolves the sentinel to a
concrete model per turn. An `auto` turn skips the catalog check and keeps the model default effort.
A catalog entry named `auto` is not listed twice. Disabled and unconfigured models stay hidden.

Each model step runs in a fresh session with an empty working directory, built-in tools
disabled, configuration discovery disabled, no skills, plugins, custom instructions or
repository hooks, and remote mode off. ZVS tool calls use the existing injected dialect
and host permission gate. Unexpected server requests are refused. Children are killed on
completion, failure, cancellation and stream drop.

Usage comes from the turn's own events. `assistant.usage` (one per model call) supplies input
and output tokens; they add up across calls and replace the local estimate on the message.
`session.usage_info.tokenLimit` is the window of the model that served the turn; it is saved on
that model row (and kept across model refreshes that report none), so `auto` shows a real window
in the context meter from the second turn on.

Limits use the provider usage hook (`providers.usage`, the «Лимиты провайдера» block and the CLI
`/context` panel). The adapter calls `account.getQuota` (name inferred from the SDK's
`AccountGetQuotaRequest`/`AccountGetQuotaResult` types; an unknown method is ignored) and otherwise
shows the last `quotaSnapshots` seen in an `assistant.usage` event, kept in memory per identity.
`premium_interactions` comes first, then `chat`; `hasQuota: false` is skipped and
`entitlementRequests: -1` or `isUnlimitedEntitlement` means unlimited. Before the first turn after
a restart, and without the account call, the block says the remainder arrives with the answer.
`session.quota.*` is not called: its result type was not published at the time of writing.

Desktop: choose GitHub Copilot in the account sign-in menu, then select its models in the
composer. CLI: `/copilot` signs in and discovers models; `/models` selects a model and
`/effort` configures supported reasoning. Esc cancels sign-in; `/context` also shows the limits. The pipe
exposes only the Copilot-specific login/cancel channels and the read-only `providers.usage`, not
general account or provider editing.

Protocol observations checked 2026-10-31 against Copilot CLI 1.0.105 (protocol 3).
Usage field names (`AssistantUsageData`, `AssistantUsageQuotaSnapshot`, `UsageInfoData`) were
read from copilot-sdk `generated/session-events.ts`, and `AccountQuotaSnapshot` from
`generated/rpc.ts`, on 2026-10-31. No real quota response has been captured yet.
The `auto` sentinel and the Free/Student routing were taken from the copilot-sdk generated RPC
types and GitHub's auto model selection docs on 2026-10-31; a real Free-plan turn was not run.
Unauthenticated status and isolated session creation were verified with the real executable;
authentication, cancellation, stream framing and permission refusal are covered by fakes.
The installed CLI must support protocol >= 3
and the account login RPCs; unsupported versions fail with an update message.

- https://github.com/github/copilot-sdk
- https://github.com/github/copilot-sdk/blob/main/nodejs/src/client.ts
- https://github.com/github/copilot-sdk/blob/main/nodejs/src/generated/rpc.ts
- https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli
- https://docs.github.com/en/copilot/concepts/auto-model-selection
