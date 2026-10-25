# Anatomy of a skill

```
release-notes/
  SKILL.md              frontmatter + instructions (required)
  references/*.md       material read on demand: specs, examples, style guides
  scripts/*.py|*.js     programs run with skill.script
  assets/*              templates, snippets, styles copied into results
```

## SKILL.md

```markdown
---
name: release-notes
description: "Составляет заметки о выпуске по списку изменений … Используется, когда …"
allowed-tools: "fs.read fs.apply"
metadata:
  display-name: Заметки о выпуске
  version: "1.0.0"
  author: Имя автора
---

# Release notes

Instructions in Markdown.
```

| Field                   | Rules                                                                                              |
| ----------------------- | -------------------------------------------------------------------------------------------------- |
| `name`                  | The identifier. Equals the folder name. Lowercase Latin letters, digits and single hyphens, starts with a letter, at most 64 characters: `release-notes`, `pdf`, `invoice-check-2`. |
| `description`           | Required, at most 1024 characters, one line. Says what the skill does and when it is used.         |
| `allowed-tools`         | Optional. Space-separated tool keys the skill needs. Every key must exist in the app, otherwise the skill is invalid. A tool that is turned off makes the skill unusable, so list only tools the skill cannot work without. |
| `metadata.display-name` | The name shown to the user, at most 120 characters.                                                |
| `metadata.version`      | `major.minor.patch`. Start at `1.0.0`.                                                              |
| `metadata.author`       | Optional.                                                                                           |

With `skill.save` you pass these as arguments and the app writes the frontmatter itself; `instructions` is only the Markdown body.

Values are plain one-line strings. Quote a value that contains `:` or `#`. Multi-line values (`|`, `>`) are not supported.

## Files

- Every file lives under `references/`, `scripts/` or `assets/`; nested folders are fine (`references/sources/csv.md`). A file anywhere else makes the skill invalid.
- No segment may start with a dot, end with a dot or a space, or contain `\ : * ? " < > |`.
- At most 200 files, 1 MiB per file, 10 MiB in total.
- `skill.save` accepts text files only. Binary assets (images, fonts) are added by the user on the «Навыки» page or through «Импорт папки».

## Scripts

- `.py` files run through `uv`; `.js`, `.mjs` and `.cjs` through Node.js.
- Declare Python dependencies inside the script, with exact versions, so they install on first run:

  ```python
  # /// script
  # requires-python = ">=3.10"
  # dependencies = ["openpyxl==3.1.5"]
  # ///
  ```

- In a chat or an agentic session the working directory is the chat's working folder (`ZVS_CHAT_DIR`); it lives as long as the chat, so files written in one turn are there in the next. Elsewhere it is a temporary folder of the run. The skill folder is in `ZVS_SKILL_DIR` and is read-only: read templates from there, never write there.
- Names starting with `_` (`_spec.json`, `_work/…`) are working files and stay hidden from the user. Documents a script creates in the working folder under other names are shown to the user as results. Tell the model to use `_` names for everything intermediate.
- The model can write and read files in the working folder with the `chat.file` tool, without asking for permission: inputs for a script (a spec, a config) are written there first and passed by relative path.
- Take input and output paths as arguments. Relative paths resolve against the working folder; paths to the user's own files are absolute.
- stdout is capped at 64 KiB. Print compact JSON or short text, and let large results go to files.
- Exit with a non-zero code and a one-line message on stderr when something fails, so the model can react.
- Use `argparse` with a `description`, so `--help` explains the script.

## How the skill is used

1. The model sees `name: description` for the skills that match the request.
2. It calls `skill.load` and receives the instructions and the file list.
3. It reads files with `skill.read` and runs scripts with `skill.script` when the instructions say so.

Point to files by their exact path in the instructions (`Read references/style.md before writing`), and say when each one is needed.
