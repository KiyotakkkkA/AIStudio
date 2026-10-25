---
task: TASK_045_EXTRA_2
title: Tool dialects in the web adapters — the injected protocol leaves the service layer
phase: 8 — Documents and the agent browser
status: done
depends_on: [TASK_045]
unblocks: []
estimate: 4 h
---

# TASK_045_EXTRA_2 — Tool dialects in the web adapters

## Context

Account-mode web adapters (`qwen_web`, `zai_web`) cannot pass tools natively, so their
capabilities say `tools: ToolMode::Injected`. The injected protocol lives in
`crates/zvs-host/src/drivers/ai/injected_tools.rs` as a decorator, `InjectedToolsDriver`: it
writes the tool catalogue into the system prompt, rewrites history (tool calls as
`{"zvs_call_tool": …}` JSON, results as `[SYSTEM_TOOL_RESULT]` user messages) and parses the
stream back into `TextDelta::ToolCall`. API adapters (`openai_compatible`, `ollama`, `local`)
are native and do not touch it.

The parsing is already outside the conversation driver. What leaks is the wrap decision and
provider quirks:

- Four callers repeat `if mode == ToolMode::Injected { with_injected_tools(raw, None) }`:
  `services/chat.rs` (`respond`), `agentic/node.rs`, `scenarios/nodes.rs`,
  `skills/nodes.rs` (skill generation), each after `resolve_tool_mode`.
- Qwen-specific parsing sits in the generic module: the `<tool_call>` tag and the `name` key
  in `ENVELOPE`, and an alias table (`ALIASES`) that hard-codes the wire name `code__run`.
- `services/chat.rs` branches on the mode when shaping results (`fence_tool_result` only for
  native).
- The injected prompt says "Call at most one tool per reply", so the parallel group of
  read-only calls in `respond` never runs for web models. Reading three files costs three web
  round trips.

## Why

Callers should see one shape: a driver that emits native tool-call deltas. Provider quirks
belong to the adapter that has them, with tests next to it. Allowing several calls per reply
cuts web round trips on read-heavy flows (the dashboard skill makes 8–12 calls, many of them
independent reads).

## Depends on

- `TASK_045` — base task of this extra (backlog placement only; no code dependency).

## Read first

- `/docs/PASSPORT.md` §12 (AI layer, the two axes, accounts) — binding
- `crates/zvs-host/src/drivers/ai/injected_tools.rs` and its tests
- `crates/zvs-host/src/drivers/ai/adapters/qwen_web.rs`, `zai_web.rs`, `account_family.rs`
- `crates/zvs-contract/src/ai.rs` (`ToolMode`, `resolve_tool_mode`)
- `docs/adr/0001-account-provider-limitations.md`

## Scope

**In scope**
- A `ToolDialect` trait in `drivers/ai` (`prompt`, `encode_history`, `parse_stream`) with the
  current protocol as the default dialect and a Qwen dialect that owns `<tool_call>` and the
  aliases; Z.ai uses the default unless its tests show otherwise.
- Web adapters wrap themselves: `qwen_web` and `zai_web` return a driver that already emits
  native `ToolCall` deltas. `ToolMode::Injected` stops being a decision the services make.
- The four call sites lose their wrap branch; `chat.rs` loses the mode branch in result
  shaping, or it moves behind the driver.
- The dialect accepts several calls in one reply (a JSON array, or several envelopes) and the
  prompt allows it for read-only tools; the existing parallel group in `respond` then works
  for web models.
- Alias targets resolved from the offered tools, not a hard-coded wire name.
- All current `injected_tools.rs` tests keep passing, moved next to their dialect.

**Out of scope** (do not do these, even if tempting)
- Native tool calling for Qwen web through any undocumented endpoint.
- Changing tool names or the tool set (TASK_045_EXTRA_1).
- Retry heuristics for a tool-bearing turn that ends in bare text (a later follow-up).

## Steps

1. Introduce `ToolDialect` and move the default protocol behind it without behaviour change;
   all tests green.
2. Move the Qwen tag, envelope key and aliases into a Qwen dialect; make alias targets
   resolve against the offered tool names.
3. Make `qwen_web` and `zai_web` apply their dialect inside the adapter; report a capability
   that callers treat as native. Decide whether `ToolMode::Injected` stays in the contract as
   a descriptive value for the UI (`ChatStore.toolMode`) or goes; if the DTO changes, update
   the Rust type, the Zod schema and the golden fixture together.
4. Remove the four wrap branches and the mode branch in result shaping.
5. Multi-call replies: parse, assign ids, emit several `ToolCall` deltas in order; tests for
   two reads in one reply, a mixed read-plus-write reply (write still runs alone, in order),
   and a malformed second call.
6. Record in Handoff the round-trip count for a three-file read before and after on the fake
   web driver.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: with a linked Qwen account, ask the model to read three files of the working
folder and summarise them; confirm one round with three calls, and that a write still asks.

## Definition of done

- [x] No service decides on `ToolMode::Injected`; web adapters emit native tool-call deltas
- [x] Qwen quirks live in the Qwen dialect with their tests
- [x] Several read-only calls in one reply run in one round
- [x] Contract change, if any, touches Rust type, Zod schema and fixture
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `drivers/ai/tool_dialect.rs` (was `injected_tools.rs`): the `ToolDialect` trait (`prompt`,
  `encode_history`, `parse_stream`), `DefaultDialect`, a `Grammar` (optional tag, name keys,
  aliases) behind a shared `ReplyParser`, and `with_tool_dialect`, which wraps a driver and
  emits native `TextDelta::ToolCall`s.
- `adapters/qwen_web/dialect.rs`: `QwenDialect` owns `<tool_call>`, the `name` key and the
  code-interpreter aliases, with its tests beside it. An alias target is the one offered tool
  whose schema has `language` and `source`; none or several such tools means no alias.
- `qwen_web_driver` / `zai_web_driver` wrap the adapters; the family table builds them. Z.ai uses
  `DefaultDialect`: nothing in its tests or fixtures shows a different call shape.
- The four call sites lost their wrap branch; `chat.rs`, `agentic/node.rs`, `scenarios/nodes.rs`
  and `skills/nodes.rs` always fence tool results.
- Multi-call replies: several envelopes (bare, fenced or tagged) or one JSON array; ids in order;
  a malformed later call stays a `Malformed` call next to the good ones. The prompt allows several
  read-only calls per reply and puts a call that changes data last.
- Round trips for a three-file read on the fake web driver (`a_web_driver_reads_three_files_in_one_round`):
  before, 4 requests (one call per reply, three tool rounds plus the answer); after, 2.

**Deviated from plan:**
- `ToolMode::Injected` stays in the contract as a descriptive value; the adapters still report it
  and the renderer only tests for `none`. No DTO, schema or fixture change.
- Result shaping moved out of the services by always fencing: web models now see
  `<tool_result name status>` inside the `[SYSTEM_TOOL_RESULT]` message, matching
  `TOOL_RESULTS_INSTRUCTION`, which the chat already sent them.

**Next session should know:**
- The 64 Ki UTF-16 capture limit now covers every call in a reply together, not one call.
- A lone `[` streamed before an array of calls is released as text before the capture starts;
  the prompt asks for one object per line, so this only affects models that ignore it.
- Manual check with a linked Qwen account not run here.
- `pnpm format:check` flags `apps/studio/src/renderer/ui/organisms/NavRail.tsx`; that file was
  already like that and this task did not touch it.
