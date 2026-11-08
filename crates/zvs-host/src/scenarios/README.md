# scenarios

Scenarios («Сценарии»): user-built automations drawn on a canvas — a trigger, then steps
(model calls, tools, plugin actions, branches, file output) — validated, versioned, compiled
into a kernel run graph, and fired by events. The editor is
`apps/studio/src/renderer/features/scenarios`; execution is the kernel's
(`../kernel/README.md`).

## Files

| File            | Contents                                                                                             |
| --------------- | ---------------------------------------------------------------------------------------------------- |
| `catalogue.rs`  | `ScenarioCatalogue`, `NodeSpec`, `SlotSpec`: every node type the editor offers, its ports and params |
| `validation.rs` | `validate_graph` → `ScenarioIssueDto`s; `trigger_binding`                                            |
| `compile.rs`    | `compile` (whole graph → `RunGraph`) and `compile_node` (one node with pinned input)                 |
| `nodes.rs`      | Scenario-only kernel nodes (output to file, templates, …) and `register_scenario_nodes`              |
| `template.rs`   | `{{result.…}}`, `{{event.…}}`, `{{date}}` placeholders; untrusted-content fencing; `civil_date`      |
| `runner.rs`     | `ScenarioRunner`: trigger matching, firing, overlap policy, test runs, retry from a step             |
| `service.rs`    | `ScenarioService`: CRUD, activation, versions, drafts, validation                                    |
| `diff.rs`       | `diff_graphs`: added / removed / changed nodes and edges between two versions                        |
| `effects.rs`    | `HostScenarioEffects`: where scenario output files go (`SCENARIO_FILES_DIR`), path safety            |

## Lifecycle

1. **Draft.** The editor saves drafts continuously (`save_draft`); a draft does not affect the
   active version.
2. **Validate.** `validate_graph` checks the graph against the catalogue and returns every
   issue at once (rules below); the editor shows them on the nodes.
3. **Save / version.** `update` stores a new version; `versions` and `diff_graphs` show the
   history. An active scenario that no longer validates is refused (`ACTIVE_SCENARIO_INVALID`).
4. **Activate.** `set_active(true)` binds the trigger (`trigger_binding`): kind plus params.
5. **Fire.** Events reach `ScenarioRunner::fire(kind, event)` from the chat
   (`chat_listener`) and from plugins (`integration_sink`). Webhook and schedule triggers
   exist in the catalogue and in `trigger_matches`, but no host component fires them yet.

## Validation rules (`validation.rs`)

| Rule                                                                                 | Meaning                                                                                   |
| ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `NoTrigger`, `MultipleTriggers`                                                      | exactly one trigger node                                                                  |
| `UnknownNodeType`, `UnknownNode`, `DuplicateNode`, `DuplicateKind`                   | catalogue and id integrity                                                                |
| `InvalidEdge`, `DuplicateEdge`, `MultipleParents`, `SlotMismatch`, `UnsupportedSlot` | ports and connections                                                                     |
| `MissingAttachment`, `TooManyAttachments`                                            | attachment slots (e.g. a tool attached to a model step)                                   |
| `Cycle`                                                                              | every node on a cycle of the main flow, found with Tarjan's strongly connected components |
| `Unreachable`                                                                        | a node not reachable from the trigger                                                     |
| `MissingDialog`, `LimitedDialog`                                                     | a step that needs a conversation (approvals, questions) under a non-chat trigger          |

Missing required parameters are reported per field (an empty string, empty array or `null`
counts as missing).

## Compilation (`compile.rs`)

`compile` walks the enabled trunk from the trigger in order and turns each node into a kernel
`GraphNode` with bindings to its predecessors' outputs; attached tools are collected into the
run's tool list. Disabled nodes are skipped; a node marked _continue on error_ does not fail
the run. `compile_node` builds a one-node graph with a pinned input, which powers "run this
step" in the editor.

## Templates (`template.rs`)

Text params may contain `{{result.path}}` (the previous step's output), `{{event.path}}` (the
trigger event) and `{{date}}`. Paths are dot-separated and projected into the JSON
(`kernel::project`); objects with a `text` field render that text, other values render as
JSON; unknown roots are left untouched. Untrusted event content (incoming mail, messages) is
wrapped in `<untrusted_content>`, any such tag inside the content is broken up (`<x` →
`< x`) so it cannot close the fence early, and the
model step gets `UNTRUSTED_INPUT_INSTRUCTION`, so an e-mail cannot instruct the model.
`civil_date` converts a timestamp to a calendar date with the days-from-civil algorithm, no
time zone database needed.

## Firing and overlap (`runner.rs`)

`trigger_matches` filters by trigger kind: chat (optional conversation), mail (`from` and
`subject` contain the configured text), webhook (path), schedule (always), integration
(account and trigger name). Each scenario has a lane; if a run is already going, the
scenario's overlap policy decides: **skip** the event (logged) or **queue** it and start it
when the current run ends. `retry_step` re-runs from a failed step, copying the latest
successful outputs of earlier steps instead of re-executing them.
