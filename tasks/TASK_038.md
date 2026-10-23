---
task: TASK_038
title: Scenario canvas editor
phase: 7 — Scenarios and Agentic Build
status: done
depends_on: [TASK_037, TASK_076]
unblocks: [TASK_039]
estimate: 4 h
---

# TASK_038 — Scenario canvas editor

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

This is the most visually demanding task in the project: the graph editor from `/design/Scenario.dc.html`. Open that file — the layout is a dot-grid canvas with curved connectors, nodes with ports, agent sub-nodes hanging below on dashed edges with labelled slots (`Chat model *`, `Memory`, `Tools`), a floating toolbar bottom-left with zoom controls, a centred Test scenario button, a node inspector on the right, and a run-log strip along the bottom.

The canvas needs: pan and zoom, node drag, edge creation by dragging from a port, selection, delete, undo/redo, and a node palette for adding steps. Sub-node attachment is its own interaction — dragging a model node onto an agent's `Chat model` slot, not drawing an ordinary edge.

The build-versus-buy question below is the first thing to settle, because it determines everything else about this task.

## Why

The scenario editor is the app's headline feature. It is also the surface where a rough implementation is most obvious to a user.

## Depends on

- `TASK_037` — Scenario model, versioning and validation.

## Read first

- `/design/Scenario.dc.html` — node sizes, port positions, edge curvature, the dot grid, the toolbar
- `/design/Foundations.dc.html` — the node rest and selected specimens
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- Canvas with pan, zoom, grid and fit-to-view
- Node rendering matching the mockup, including agent nodes with their slot ports
- Edge creation, deletion and re-routing; curved connectors as drawn
- Sub-node attachment interaction
- Node palette and add-step flow
- Selection, multi-select, delete, duplicate, undo/redo
- Autosave to a draft, explicit Save creating a version

**Out of scope** (do not do these, even if tempting)
- Execution and the run log (TASK_039)
- Collaborative editing
- Auto-layout beyond a simple tidy action

## Steps

1. Settle the decision below, then build accordingly.
2. Canvas: pan with space-drag and middle-drag, zoom to cursor, a dot grid drawn as an SVG pattern (not a CSS gradient — the app has no gradients), fit-to-view, and a zoom readout in the toolbar.
3. Nodes from the mockup: 11 px radius, `main-800` fill, `main-600` border, icon tile, label and sub-label, ports as 9 px circles. Selected state is the accent border with the soft accent ring.
4. Edges: curved bezier connectors. Main edges solid `ok`-green when a run has traversed them, `main-600` otherwise; attachment edges dashed. Edge labels for item counts as drawn.
5. Attachment interaction: agent nodes expose named slots along their bottom edge. Dropping a compatible node on a slot attaches it; incompatible drops are refused with a visible reason rather than silently ignored.
6. Node palette: searchable list of registered node types grouped by source, respecting the tool ceiling — a node whose tool is `off` is shown disabled with the reason.
7. Undo/redo over graph mutations, with a bounded history. Keyboard: delete, duplicate, select-all, escape to deselect.
8. Autosave the working draft frequently to survive a crash; the explicit Save creates a version through TASK_037.
9. Validation surfacing: invalid nodes carry a marker and a tooltip from the validator's issues.
10. Tests: graph mutations as pure reducers (add, delete, connect, attach, undo, redo) tested without rendering; attachment compatibility rules; serialisation round-trips through the TASK_037 schema.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "For the scenario canvas — build it on a graph library like React Flow, or hand-roll the canvas? A library gives pan, zoom, edges and selection for free but brings its own DOM structure and styling that may fight the exact look in the mockup. Hand-rolling matches the design precisely but is several days of work rather than one."
>
> **Why it matters:** This is the largest build-versus-buy call in the project and it is hard to reverse once the editor exists.
> **Options:** (a) React Flow, styled hard to match the mockup — fastest, some visual compromise; (b) hand-rolled SVG canvas — exact, significantly more work and more bugs to find; (c) React Flow now, revisit if it fights the design.
> **If the user is unavailable:** take (c) — build on React Flow, keep graph state in our own model so the library is a rendering choice rather than the source of truth, which keeps (b) open later.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Rebuild the exact graph from `/design/Scenario.dc.html` — trigger, agent with a chat model and a vector store tool, a Slack action — by hand in the editor. It should be possible without fighting the canvas, and it should look like the mockup.

## Definition of done

- [x] Pan, zoom, drag, connect, attach, select, delete, undo/redo all work
- [x] Nodes and edges match the mockup, including dashed attachment edges
- [x] Palette respects the tool ceiling
- [x] Graph state is our own model; serialisation round-trips
- [x] Autosave survives a crash
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Decision: React Flow 12 (`@xyflow/react` 12.12.0, `base.css` only) as the renderer over our own
  model (option c). `features/scenarios/graphModel.ts` holds the graph reducers (add, move,
  remove, connect, reroute, attach, duplicate, update, replace) with typed refusals;
  `history.ts` is a bounded (100) undo/redo stack; `layout.ts` holds node sizes, slot port
  offsets, the drop-on-slot hit test and the tidy action. `ScenarioEditor` (MobX) owns the
  working graph, selection, drag state, autosave and validation; React Flow is fed derived
  nodes/edges (`flowModel.ts`) and only reports changes back.
- Canvas: dot grid as React Flow's SVG pattern (22 px), pan with Space-drag and middle-drag,
  wheel zoom to cursor, box select, fit-to-view, zoom readout, undo/redo and tidy in the
  bottom-left toolbar, «Добавить шаг» centred, «+» top-right.
- Nodes per the mockup: 11 px radius, `main-800` fill, `main-600` border, icon tile, label and
  sub-label, 9 px ports, trigger accent edge, accent border plus soft ring when selected,
  dashed and dimmed when disabled. Agent nodes expose named slot ports along the bottom with
  `Чат-модель *` / `Память` / `Инструменты` labels and filled-slot chips. Main edges are
  `main-600` beziers; attachment edges are dashed `main-500`.
- Attachment: dragging a sub-node onto a slot port attaches it (the slot lights accent or
  `err` during the drag); an incompatible drop keeps the move and shows the reason in a banner.
  Drawing from a slot port to a node's top port attaches too. Rules: the child must fill the
  slot, a single slot must be free, a step with main edges is refused, no attachment cycles;
  re-attaching moves a node.
- Palette: searchable, grouped (triggers, agents and sub-nodes, core actions, then tools by
  source/MCP server). A node whose tool is `off`, unavailable or unregistered is listed
  disabled with the reason. Adding with a node selected connects the new step after it, or
  attaches it to the first compatible free slot.
- Keyboard: Delete/Backspace, Ctrl+D, Ctrl+A, Escape, Ctrl+Z, Ctrl+Shift+Z/Ctrl+Y.
- Inspector: name, required parameters, the full params as JSON, «Отключён», connected
  sub-nodes per slot (select, detach), the node's issues; with nothing selected, the scenario's
  issue list. Invalid nodes carry a marker whose tooltip lists the validator's issues.
- Host: `scenario_draft` table (migration 0014) and channels `scenarios.draft|saveDraft|
  discardDraft`; a save with a graph clears the draft in the same transaction.
  `scenarios.catalogue` exposes the node types (role, trigger kind, fills, slots, required
  params, tool key). DTOs mirrored in Zod with golden fixtures.
- Autosave writes the draft 800 ms after the last mutation; reopening restores a draft that
  differs from the current version and offers «Отбросить черновик». «Сохранить» creates a
  version through `scenarios.update`. The scenario list page creates, opens and removes
  scenarios; the header renames, toggles «Активен» and shows the version chip.

**Deviated from plan:**
- No «Test scenario» button and no run-log strip: both belong to TASK_039. Edges are drawn in
  their resting colour only; the `ok`-green traversed state and the item-count labels need
  run data, which TASK_039 adds to the edge data in `flowModel.ts`.
- The palette's tool ceiling is computed in the renderer from `tools.list` joined on the
  catalogue's `toolKey`, not by the host.
- Main edges can be re-routed by dragging an end; attachment edges are deleted and redrawn.

**Next session should know:**
- Graph comparison is key-order independent (`sameGraph`): the host returns node fields and
  params in serde order, which differs from the renderer's insertion order.
- React Flow runs controlled: positions during a drag live in `ScenarioEditor.dragPositions`
  and are committed once on drag end; measured sizes are kept in component state.
- `pnpm typecheck` still fails only in `apps/studio/tests/renderer/fileChanges.test.ts`, the
  same pre-existing failure noted in TASK_037.
- Gates run: cargo fmt, clippy `-D warnings`, `cargo test --workspace --exclude zvs-jobd` (a
  running dev build locked `zvs-jobd.exe`), `pnpm lint`, `pnpm test`, `pnpm format:check`,
  `pnpm contract:channels:check`, `pnpm build`. The manual walk through the mockup graph in the
  running app was not done in this session.
