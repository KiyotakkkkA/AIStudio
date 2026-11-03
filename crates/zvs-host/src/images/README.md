# Reusable image-loading pipeline

`ImageLoader` is a host service independent of the composer and provider protocols. It accepts
trusted paths from a native picker, stores conversation-owned copies, and resolves references
to `ImageInput { attachment, bytes: Arc<[u8]> }` before generation. Tools and scenarios can reuse
these stages without implementing file selection, storage or vendor uploads again.

1. Construct `ImageLoader::new(artifact_store.clone())` with the application's conversation store.
2. Call `import(owner, picked_paths)` from a blocking task. Paths must come from an authorized
   host picker or an existing host permission flow; never accept arbitrary renderer/model paths.
3. Store the returned `ImageAttachmentDto` references with the owning message or pipeline input.
   They contain an opaque UUID, name, MIME and size; no filesystem path or vendor credential.
4. Carry those references through `ChatMessage::with_images` / `GenerateMessage.images`.
5. After context windowing, call `load_messages(owner, &mut request.messages, cancel).await`.
   It reads on blocking tasks, verifies ownership, metadata, size, file signature and SHA-256,
   checks cancellation, and loads only the images retained in context. `validate_inputs` checks
   the complete loaded batch before uploads. `ImageInput` Debug output contains no pixels.
6. Pass the request to a supported driver. Qwen obtains an upload grant and performs an OSS PUT;
   Z.ai sends multipart form data. Pure request builders live in the provider crates, and I/O
   remains behind the host transport. Upload errors fail the turn; there is no text-only fallback.

The first UI consumer is `chat.images.pick` (input `ConversationRef`, output image references).
The native picker filters PNG, JPEG, GIF and WebP. Import validates file signatures rather than
trusting extensions, caps each file at 20 MiB and each message at five images, bounds reads and
rolls back a failed batch. It does not decode or resize images. Unsupported/corrupt image
contents beyond header validation may still be refused by the provider.

Copies and SHA-256 manifests live under `conversations/<owner>/work/_images/`. They survive
restart, are copied by the existing branch operation, and are removed with the conversation.
Removing a draft chip or truncating history retains the local copy until conversation deletion,
so edits and retries can reuse its reference. Imports share the existing 200 MiB / 1,000-file
working-folder quota. Per-owner UUID paths and canonical containment checks prevent traversal
and cross-conversation attachment references. Changed copies fail integrity checks.

`message.images` is a JSON column introduced by migration `0038_message_images`, defaulting to
`[]` for old conversations. Rust and Zod definitions and golden fixtures cover the DTOs and
image-only `ChatSendInput`. `GenerateMessage` serializes references, never image bytes, so run
inputs and diagnostics do not store base64. Current chat admission accepts images only for
`qwen-web` and `zai-web`; a different adapter is rejected before a run is created. Provider
image-generation capabilities remain unchanged.

Context budgeting reserves an estimated 4,096 tokens per image; actual usage comes from the
provider. Windowing drops the oldest history when its retained image batch would exceed five.
The text queue remains text-only, and does not silently strip attached images. Switching to an
unsupported provider blocks sending until the attachments are removed or a supported provider
is selected. Existing image history also requires a supported provider.

Tests: `cargo test -p zvs-host images`, the provider adapter suites, contract golden fixtures,
and `apps/studio/tests/renderer/chatStore.test.ts`. Upload fixtures are synthetic. Provider
protocols were reviewed from the linked reference implementations on 2026-11-03; live Qwen
and Z.ai image-understanding verification is pending and documented in their READMEs.
