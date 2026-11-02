# ADR 0006 — Secrets have no types

- **Status:** accepted, 2026-11-02
- **Task:** TASK_050 (Telegram, GitHub and GitLab plugins)
- **Supersedes:** the secret type registry of TASK_011 (`SecretTypeKey`, `SecretTypeSchema`,
  `secrets.types`) and the per-type secret pickers of the Providers page

## Context

TASK_011 gave every secret a type from a fixed registry: `ollama`, `openrouter`, `mistral` and
`custom` («Иное»). The type chose a schema of extra fields (`baseUrl`, `referer`,
`description`) stored next to the value, and each secret picker showed only the types that
the consumer was expected to use. The Providers page, for example, offered only `ollama` and
`custom` secrets to an Ollama connection.

That registry stopped matching how secrets are used:

- The non-secret fields duplicated configuration that already lives with the consumer. A
  provider has its own base URL; nothing in the host read the secret's `baseUrl` or `referer`.
- Plugins (mail, Telegram, GitHub, GitLab) need bot tokens, personal access tokens, IMAP
  passwords and OAuth client secrets. A type per credential kind would make the registry grow
  with every integration, and `custom` already carried most of them.
- Filtering a picker by type hid perfectly usable secrets (an OpenRouter key stored as
  `custom`) and gave no help once a user has more than a handful of secrets.

## Decision

1. **A secret is a name, a value, a scope and metadata** (tags, note, rotation reminder).
   There is no type and no schema of extra fields. `CreateSecretInput`, `UpdateSecretInput`,
   `SecretSummaryDto`, `SecretDto` and `SecretFilter` lose `type` and `fields`; the
   `secrets.types` channel is removed. The Secrets page form has a name and a value.
2. **Every secret picker lists every secret** and is searchable (`SelectInput searchable`).
   The provider connection token, MCP environment and header values and the plugin forms all
   use the same list.
3. **Scopes stay** (`personal`, `shared`, `public`); only types were removed.
4. **Host-internal secrets stay hidden.** The `secret.type` column remains as a host-side kind:
   every user secret has the kind `user`; account tokens and other host-internal credentials
   keep their own kinds and are never listed, read, changed or removed through `secrets.*`.
   Grants saved by a plugin sign-in are user secrets, visible on the Secrets page.
5. **Migration `0035_secret_kinds`** turns `ollama`, `openrouter`, `mistral` and `custom` into
   `user`, copies a `custom` secret's `description` into an empty note, renames a secret whose
   name would collide with another user secret (`<name> · <old type>`), and drops the `fields`
   column. Values are untouched.

## Consequences

- Choosing the right secret is the user's job; search and the name are the help. A secret
  used by a consumer it does not fit fails at that consumer's check (the provider probe, the
  plugin's «Проверить подключение»), not at the picker.
- A future credential that needs structured parts (a key pair, a client id and secret) is
  stored as separate secrets or as a JSON value the consumer parses, as OAuth grants already
  are. It does not bring the type registry back.
- `FieldDescriptor` and `FieldKind` survive as the plugin form schema (`packages/shared/src/fields.ts`).
