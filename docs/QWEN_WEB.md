# Qwen web adapter

The adapter implements text generation and streaming over the linked browser
account session. Under Electron that session was the `persist:browser-work` partition. After
the Tauri migration (PASSPORT D10, TASK_066–TASK_067), requests run as `fetch()` inside a
hidden webview of the browser's work profile, so the vendor's cookies are never copied out. It is an unofficial web protocol integration, not the Qwen OAuth
or Qwen Code API.

## Model capabilities and thinking

Discovery advertises only `streaming` and, when supported, `reasoning`. Vendor
vision, file, audio, video, search and image-generation abilities are not exposed:
the adapter currently sends text with an empty `files` array. The shared adapter
capability `image` means image generation through `ImageDriver`, not vision input.
Refresh the provider's model list to update previously stored capability badges.

Generation obtains the model list on first use and reuses it for that adapter
instance. An explicit list refresh replaces the cache. A model with reasoning
enabled uses `Thinking`; other models, including unknown models, use `Fast`.
An explicit `capabilities.thinking` value takes precedence over legacy
`abilities.thinking`, so `false` or `0` cannot be overridden by an ability value.
Missing capability metadata falls back to legacy abilities; missing both disables
thinking. Discovery failures propagate before a vendor chat is created.

## Conversation history

Every generation creates a new Qwen chat. A single user message is sent directly;
system instructions and conversation history are serialized as role-labelled text.
This preserves the supplied context as text but does not provide native vendor
conversation continuity or enforce native system/assistant roles. Vendor chat,
parent and response identifiers are not reused across generations.

## Validation on 2026-09-10

Fixture tests cover normal generation, reasoning and usage, cancellation,
truncation, malformed replies, and session expiration preserving partial text.
Standalone SSE `status: 401` and `status: 403`, authentication codes, and string
codes are recognized without requiring an `error` or `success: false` field.

Authenticated live checks accepted the provided credential at `/api/v1/auths/`
and returned six models from `/api/v2/models/`. Chat creation returned HTTP 200
with HTML containing browser-verification/WAF markers instead of JSON. Adding
the public implementation's request headers did not change this. Headless browser
attempts did not produce a usable completion capture. Consequently, live
generation and a fresh browser payload comparison remain unverified. The existing
`local` chat mode, `2.1` version and phase output schema have been retained;
third-party differences alone are insufficient evidence to change them.

Since TASK_074_EXTRA_1 the account transport tells the two apart: a redirect is still
`PROVIDER_SESSION_EXPIRED`, but a 2xx HTML page answering an API call is
`PROVIDER_UNREACHABLE` with `reason: "browser-verification"` and «Вендор вернул страницу
проверки браузера (антибот-защита)…», and it no longer marks the account for re-linking. A
successful identity probe alone does not establish generation access. We do not try to pass
the verification (ADR 0001); when it blocks generation, Qwen is used through its official
API instead.

The reviewed [public Qwen implementation](https://github.com/xtekky/gpt4free/blob/main/g4f/Provider/Qwen.py)
also uses dynamic browser state. Its OAuth/device flow is a separate integration;
it is not evidence that the current web identity endpoint should be replaced.

## Repeatable live test

Set `LIVE_QWEN_TOKEN` in the process environment, then run:

```sh
pnpm exec vitest run apps/studio/tests/liveQwen.test.ts
```

This opt-in test checks identity and discovery, then requests one short generation
for each of the Thinking and Fast model categories. It can create two vendor
chats. It prints endpoint status and counts, never credentials or response bodies.
It uses direct fetch without the browser profile's cookies, so browser verification
can block it even with a valid token. After TASK_067 this test is an `#[ignore]`d Rust test
(`cargo test -p zvs-host -- --ignored live_qwen`) with the same behaviour. Without the environment variable it is skipped.
