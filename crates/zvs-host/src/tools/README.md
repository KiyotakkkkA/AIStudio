# tools

The tool registry behind the «Инструменты» page: every tool the model can call — built-in
kernel tools, MCP tools, plugin tools — with its tier (`auto`, `ask`, `off`), enabled flag,
availability, usage and where it is referenced. The tier is the permission _ceiling_
enforced by the kernel (`../kernel/README.md`).

## Files

| File              | Contents                                                                                  |
| ----------------- | ----------------------------------------------------------------------------------------- |
| `registry.rs`     | `ToolRegistry`, `ToolDescriptor`, `ToolCeiling`, `default_tool_tier`, `tool_dto`          |
| `kernel_tools.rs` | `register_kernel_tools`, built-in tool names and descriptions, defaults, renames, filters |

## Tiers

`default_tool_tier(read_only, destructive)`: destructive → `off`, read-only → `auto`,
otherwise `ask`. Built-ins can override: `fs.apply` asks by default; browser navigate / click /
type, `shell.output` / `shell.kill` and the `computer.*` tools default to `auto` because they
carry their own grants and approvals. Registering writes the tool row and its `global` grant (`granted_by = registry`);
`set_tier` rewrites the grant, so the page and the permission model never disagree.

**Re-registration** (app start, an MCP server reconnecting) keeps the user's tier, display
name and enabled flag. The one exception: a tool that newly declares itself destructive is
reset to its default tier, so an upgrade of an MCP server cannot keep `auto` on something that
now deletes data.

## Built-in tools (`kernel_tools.rs`)

`register_kernel_tools` registers every tool-gated kernel node with its `schemars` input
schema, except:

- `NOT_TOOLS` — kernel nodes that are never offered to a model (`llm.generate`,
  `vector.index`, `job.run`, `download.fetch`);
- `GRAPH_ONLY_TOOLS` — usable in scenarios but not in chat (`vector.search`, `vector.list`).

`model_tool(key)` applies both filters. Retired built-ins (`code.run`, the old delegate key)
are deregistered from existing installations. `current_tool_keys` maps tool keys stored in
old conversations and scenarios to their current names (`chat.file` → `fs.read`, `fs.list`,
`fs.apply`), so a renamed tool keeps working where it was picked.

Display names here are Russian (they appear on the «Инструменты» page). The model-facing
descriptions are English today, while CLAUDE.md asks for Russian built-in descriptions; that
gap is noted in the audit, not changed here.

## Usage and references

`record_use` is called by the run service after every admitted step, feeding the usage chart.
`record_references` stores which subjects (scenarios and other configurations, see
`ToolReferenceSubject`) reference a tool, so the page can show where it is used. `ceiling(key)` is what the kernel's
admission reads: tier, enabled and available together.
