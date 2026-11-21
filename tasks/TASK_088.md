---
task: TASK_088
title: Build the standalone AI Studio CLI
phase: C — Shared dialogs and standalone CLI
status: todo
depends_on: [TASK_086, TASK_087]
unblocks: []
estimate: 6–10 engineer-weeks
---

# TASK_088 — Standalone AI Studio CLI

## Context

The existing Ratatui client builds as `aistudio` from `crates/zvs-cli` and drives Studio over
a Windows named pipe. Move that client to `apps/cli` and give it a Tauri-free runtime built
from the same Rust execution code as Desktop. OpenAI and Copilot must run with Desktop
closed, unlinked or absent. Users can continue the same saved regular dialog from either
interface when the required model and tools are available.

Desktop owns its embedded browser and vector services. The CLI can link a Desktop
installation and use those capabilities over the local bridge. The CLI persists its own
settings and dialogs and uses the resources established by TASK_087.

## Why

Terminal users need a complete local workflow without starting a GUI for every turn and a
predictable handoff to Desktop when they want its interface or integrated services.

## Depends on

- `TASK_086` — JSONL dialog store, indexes, stable IDs and writer locks.
- `TASK_087` — Desktop identity, shared resources and provider credential access.

## Read first

- `AGENTS.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`, `docs/PASSPORT.md` and `docs/FILES_STRUCTURE.md`.
- `crates/zvs-cli/README.md`, `crates/zvs-host/src/cli/README.md`, `crates/zvs-pipe`,
  Desktop startup, and the shared execution kernel, provider and MCP services.
- Read the Qwen and Z.ai crate READMEs before changes involving those adapters or their fixtures.

## Scope

**In scope**

- Move the package to `apps/cli`, keeping the `zvs-cli` Cargo package and `aistudio`
  executable names. Support building, installing and launching it independently on Windows.
  Keep the current TUI and reuse the shared engine, provider adapters and permission logic.
  Compose the CLI with the capabilities below.
- Give the CLI a private `AI Studio CLI` root and versioned boot initializer. Write global
  settings to `config.toml` and keep per-dialog settings with the dialog. Define precedence
  and ownership for `[globals]`, `[workspaces]`, workspace and dialog entries, `[access]`
  and its workspace and dialog overrides, and `[mcp]` and its server entries. Keep credentials
  and sensitive MCP authentication values in secure storage. Cover copy, result retention,
  autocompaction, theme and Desktop location in global settings, and workspace paths and
  dialog aliases in workspace settings.
- Support standalone OpenAI and Copilot setup through `/connect`, model discovery,
  generation, streaming, cancellation and resume. Use shared provider metadata and secrets
  where supported. Continue to use the installed Copilot CLI for its account and runtime.
- Register the shared definitions and implementations for file read/apply/search/list,
  shell run/output/kill, skill read/save and chat ask. Include notes, configurable subagents
  and MCP with server auth status, auth shortcuts and `/mcp auth` in the OS default browser.
  Omit Desktop-only capabilities and memory. Filter browser-use and computer-use skills
  from the picker, invocation paths and model declarations.
- Implement `/desktop link` and `/desktop dialog`. Link discovery must remember the
  installation. Launch and wait for a linked Desktop when a requested operation needs it.
  Delegate Qwen and Z.ai provider operations and vector services, including `@` storage
  binding. Preserve browser account ownership, approval boundaries and current browser
  verification handoff. Handle unavailable, starting and disconnected states in the TUI.
- In model and permission pickers, mute and disable unavailable choices with an expandable
  reason. For auto-bypass judge selection, an unavailable browser-backed candidate falls
  through in the current priority order. Moving the judge and summary model settings into
  `/config` leaves `/permissions` responsible for permissions.
- Support `/rename`, `/pin`, `/branch` and `/skills inspect` with skill-name autocomplete.
  Preserve `/new`, `/context`, `/effort`, `/models`, `/notes`, `/resume`, MCP add and
  inspect, subagent listing and stopping, and the skills list. Use shared dialog IDs and
  indexes so rename, pin, branch, delete and continuation are visible in both apps.
  Carry dialog-specific settings and access rules across handoff and enforce writer ownership.

**Out of scope**

- Legacy SQLite dialog compatibility, cross-machine sync and shared attachment to a live turn.
  First-release handoff occurs between turns.
- Importing Desktop-only tools, browser profiles or Desktop memory into the CLI.

## Steps

1. Move the package and wire the Tauri-free runtime and filesystem layout. Prove a local
   OpenAI or Copilot turn and resume with Desktop absent before wiring the bridge.
2. Connect configuration, shared credentials, permissions, local tools and terminal MCP auth.
3. Add the versioned bridge capabilities and link commands using the current per-user transport.
   Update the Passport, file-structure docs, contract and packaging for the new composition.
4. Complete model availability, dialog and skill commands, and exercise the full Windows
   handoff with both apps. Add service, repository, contract and permission tests.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check && pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --locked -p zvs-cli --bin aistudio
```

Manual check on Windows: start an OpenAI and a Copilot dialog with Desktop absent. Confirm
streaming, cancellation, resume, tools and MCP auth. Install and link Desktop, open a CLI
dialog there, continue it, then close Desktop and resume it in the CLI under the same ID.
Exercise the reverse handoff with a Desktop-created dialog. Verify Qwen, Z.ai and vector
operations can start a linked Desktop, and that an unlinked browser-backed judge falls
through to the next available model.

## Definition of done

- [ ] The CLI builds and runs standalone with the specified local capabilities.
- [ ] Both apps can open, manage and continue a shared JSONL dialog with consistent settings and history.
- [ ] Desktop-dependent features launch or connect when linked and report typed unavailability
  when unlinked. Availability and judge fallback are covered by tests.
- [ ] Verification passes, Handoff is filled in, status is set to `done` and `INDEX.md` is ticked.

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
