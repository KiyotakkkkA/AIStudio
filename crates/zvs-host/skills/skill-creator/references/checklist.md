# Checklist before saving

## Identity

- [ ] `key` matches `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`, at most 64 characters, and is not taken (unless the user asked to rewrite their own skill).
- [ ] `name` is short and readable, at most 120 characters.
- [ ] `version` is `1.0.0` for a new skill, raised for a rewrite.

## Description

- [ ] One line, at most 1024 characters, in the user's language.
- [ ] States the job and the trigger («Используется, когда …»).
- [ ] Every trigger request from the interview would match it; the non-trigger requests would not.

## Instructions

- [ ] Start with a numbered workflow.
- [ ] Every choice has a default; every risky action has a confirmation step.
- [ ] Every file the instructions mention exists, with the exact path.
- [ ] Every script is described in one line: arguments and output.
- [ ] They end with what to report to the user.
- [ ] No secrets, personal data or instructions to bypass confirmations.

## Files

- [ ] Every path starts with `references/`, `scripts/` or `assets/`.
- [ ] Text only when saving with `skill.save`.
- [ ] Python scripts declare their dependencies with exact versions in the `# /// script` header.
- [ ] Scripts take paths as arguments, write nothing into the skill folder and print compact output.
- [ ] At most 200 files, 1 MiB each, 10 MiB in total.
- [ ] Every reference starts with a `#` heading and every script with a docstring that says what it does.
- [ ] `read-first` lists only the references every run needs, at most about 16 000 characters together.
- [ ] `requires` names, for each script that fails without it, the reference the model must read first.
