# Rust addon

Run `pnpm build:rust` from the repository root. This invokes napi build with
`--platform --release --target <rustc-host> --locked` through the napi CLI API.
Outputs live in `apps/studio/resources/native/<triple>/`; `zvs-core.node` is the
stable filename consumed by the host. `pnpm build` also runs this step through Turbo.
The current machine needs Rust and its native linker (MSVC build tools on Windows).

`chunkText(text, { size, overlap })` and `hashBytes(buffer)` return promises.
Chunk results own their text and contain `byteStart`, `byteEnd` (exclusive UTF-8 byte
offsets), and `tokenCount`. Counts follow zvs-core's whitespace-based chunking rule.
Services consume `RustCorePort`; `RustCore` validates configuration, copies binary
inputs, and maps errors. Only the driver loads the raw addon, lazily and once per path.
`platform/paths.ts` resolves native paths through the development or packaged resources
directory. A future packager must copy `resources/native/<triple>/zvs-core.node` to
`<resources>/native/<triple>/zvs-core.node`, outside ASAR; debug artifacts must be excluded.
Only the current host target is built. Windows MSVC, macOS and Linux glibc path layouts
are represented; packaging and other target verification remain TASK_056.

Every exported operation runs asynchronously on napi's Tokio pool and calls `guarded`.
That helper catches unwinds around core work and result conversion. Release builds retain
Rust's default unwind strategy. The addon uses `unsafe_code = deny`, allowing napi's
generated registration glue to override the lint; zvs-core retains `forbid`.
There is no handwritten unsafe code.

Rust's orphan rule prevents implementing `From<zvs_core::Error> for napi::Error`
in this crate. A local `CoreFailure` adapter implements the conversion instead.
The napi async API requires its fixed `Status` type, so rejection messages carry a JSON
`{ code, message }` envelope. `RustCore` decodes that envelope into `AppError.code`;
services and renderers never parse error messages. Unknown failures become `NATIVE_ERROR`.

Run `pnpm test:rust` to build both variants and exercise native calls, stable errors,
event-loop responsiveness and recovery after a panic in the same process.
`pnpm build:rust:debug` enables the explicit `debug-panic` feature and writes a separate
`<triple>/debug/zvs-core.node`; the normal artifact never exports `debugPanic`.
The panic hook writes to stderr before the promise rejects, and subsequent calls succeed.
Never expose the debug function through renderer IPC.

With the app running, use renderer DevTools:

```js
await window.zvs.call("system.nativePing", { text: "word ".repeat(600) });
```

Expected response: `{ ok: true, data: { count: 3 } }`.
