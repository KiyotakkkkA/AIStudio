---
name: skill-key
description: "Что делает навык, над чем и с каким результатом. Используется, когда пользователь просит …"
metadata:
  display-name: Название навыка
  version: "1.0.0"
---

# Skill title

One paragraph: what the skill produces and from what.

## The rule that matters most

The single failure mode that ruins the result, and how to avoid it.

## Workflow

1. Check the inputs: …
2. Do the main work: …
3. Verify: …
4. Report: the path of the result and a short summary.

## Choices

| The user wants | Do  |
| -------------- | --- |
| …              | …   |

## Defaults

- File name: …
- Location: next to the input file, unless the user named one.
- Language: the language of the request.

## When to ask the user

- …

## Files

- `references/…` — read when …
- `scripts/…` — run with `skill.script` to …; prints …
