# Qwen web adapter

The adapter implements text generation and streaming over the linked Electron
account session. It is an unofficial web protocol integration, not the Qwen OAuth
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

The shared account transport currently treats HTML as a signed-out response, so
this live browser-verification response surfaces as `PROVIDER_SESSION_EXPIRED`.
Distinguishing verification challenges from login pages is a remaining transport
limitation; a successful identity probe alone does not establish generation access.

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
It uses direct fetch without Electron's cookie jar, so browser verification can
block it even with a valid token. Without the environment variable it is skipped.
