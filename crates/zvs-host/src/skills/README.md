# skills

Skills («Навыки»): folders with a `SKILL.md` (front matter + instructions) and optional
scripts and reference files that teach the model a task. Bundled skills ship with the app;
users can create, import, edit, export and generate their own. The model sees a catalogue of
skill descriptions and loads a skill on demand with `skill.load`.

## Files

| File          | Contents                                                                                                                                    |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `manifest.rs` | `SKILL.md` front matter: `name`, `description`, `allowed-tools`, `metadata` (`display-name`, `version`, `author`, `read-first`, `requires`) |
| `library.rs`  | `SkillLibrary`: the skills folder on disk, load, find, author, import/export, script lookup                                                 |
| `bundled.rs`  | `BUNDLED_SKILLS`: `browser-use`, `computer-use`, `pdf`, `docx`, `xlsx`, `skill-creator`, `dashboard` (embedded at compile time)             |
| `service.rs`  | `SkillService`: CRUD, enable/disable, bundled updates, catalogue for a turn, `command_skill`                                                |
| `matching.rs` | `lexical_score`, `cosine`: ranking skills against the user's message                                                                        |
| `nodes.rs`    | Kernel tools `skill.load`, `skill.generate`, `skill.save`                                                                                   |
| `active.rs`   | Skills loaded earlier in the dialog that stay in force (up to 3, 24 000 chars)                                                              |
| `turn.rs`     | `SkillTurn`: "read the skill files before running its scripts"; result size for `skill.load`                                                |
| `purpose.rs`  | `purpose_of`: a one-line purpose for each skill file, shown in listings                                                                     |
| `script.rs`   | `ScriptRuntime`: `.py` → `uv run`, `.js/.mjs/.cjs` → Node.js; `SKILL_DIR_ENV`                                                               |

## Catalogue for a turn (`SkillService::select`)

The system prompt lists at most `catalogue_limit` enabled skills. When there are more:

1. **Embedding ranking**, if a skill-trigger embedding model is configured: the message and
   every skill's description are embedded (descriptions cached by text), scored by `cosine`.
2. **Lexical fallback** (no model, or embedding failed — logged): `lexical_score` reduces
   words of 3+ characters to 5-character stems, lowercased, and returns the share of the
   message's stems that appear in the description. Crude stemming works across Russian
   inflections ("договор", "договора", "договору" share `догов`).
3. Sort by score, take the limit.

## Loading and active skills

`skill.load(key)` returns the instructions plus the files listed in `read-first`, inlined. Its
result may be up to 6× the normal tool-result limit, capped at 40 000 characters
(`result_limit`). A loaded skill stays _active_ for later turns of the dialog: its
instructions are re-sent in the system prompt (`active_skills_text`), at most 3 skills and
24 000 characters, the oldest replaced first.

`requires` in the manifest maps a script to the files that must be read first. `SkillTurn`
tracks which skill files were inlined or read with `fs.read` during the turn and answers a
`shell.run` of such a script with a hint (`READ_BEFORE_RUN`) until they are.

## Bundled skills

Bundled skills are compiled into the binary (`include_str!` / `include_bytes!` of
`crates/zvs-host/skills/<name>/…`). `install_bundled` seeds each one once: a marker file
records which keys were seeded, with the shipped version and a digest of the installed folder
(relative path, size and mtime of every file, hashed with SHA-256). A skill the user already
has under that key is never overwritten, and a skill the user deleted is not re-seeded.
`skills.updateBundled` / `updateAllBundled` replace a seeded skill with the shipped version on
request; the digest tells whether the user edited it since seeding.
