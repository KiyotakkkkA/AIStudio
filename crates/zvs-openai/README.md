# zvs-openai

OpenAI ChatGPT-plan OAuth and model-catalog protocol types. This crate performs no I/O.
The Rust host owns browser authorization, token exchange and verification, encrypted storage,
refresh, HTTP streaming, and the Codex child process.

The `openai` account adapter is separate from the API-key `openai-compatible` adapter.
It uses the documented public Responses API and the account-specific `/v1/models` catalog.
Models are discovered after account linking; the client does not hard-code an entitlement list.

The composer chooses direct Responses requests or Codex app-server. Both routes use the
selected registration's OAuth token and ChatGPT plan authorization. The setting is persisted
on the provider and never changes the billing route to an API key automatically.

The OpenAI effort control uses the existing per-dialog composer setting. Low maps to
`reasoning.effort: "low"` for direct requests and `effort: "low"` for Codex turns;
high maps to `"high"`. Default omits the override so the selected model's default applies.
Supported effort values depend on the selected model.

Codex CLI must be installed for app-server mode. The host resolves standalone executables
and npm platform binaries without running a command shell. Each model step starts an
ephemeral thread in an isolated temporary home with the host transcript. Dynamic tool calls
return to the existing ZVS tool loop; Codex environment access and autonomous local tools are
disabled. The host terminates the child on handoff, completion, cancellation, or failure.

`providers.usage` returns provider-reported quota windows or an explicit unavailable state.
The direct OAuth route does not document a remaining-percentage endpoint. App-server's
`account/rateLimits/read` can also be unavailable for a Responses-provider OAuth token.
The UI never estimates subscription quotas from local token counts. The usage button opens
the fixed ChatGPT usage-settings page in the system browser.

References checked on 2026-10-30:

- https://developers.openai.com/siwc/token-sharing-open-source
- https://developers.openai.com/siwc/token-sharing-open-source/sign-in
- https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions
- https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
- https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server
- https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
- https://learn.chatgpt.com/docs/app-server
- https://developers.openai.com/api/docs/guides/reasoning
