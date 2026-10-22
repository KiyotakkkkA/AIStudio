---
task: TASK_NNN
title: <short imperative title>
phase: <n> — <phase name>
status: todo
depends_on: [TASK_NNN]
unblocks: [TASK_NNN]
estimate: <2–4 h>
---

# TASK_NNN — <title>

## Context

What exists right now, what this task adds, and where it sits in the app. Written for someone
who has never seen this project. Name the files, the passport sections and the design artboard
that matter. Two to five paragraphs — enough that no chat history is needed.

## Why

The reason this task exists and what breaks or stays impossible without it. One paragraph.

## Depends on

- `TASK_NNN` — what it left behind that this task builds on.

## Read first

- `/docs/PASSPORT.md` §N — <what to take from it>
- `/design/<File>.dc.html` — <what it specifies>

## Scope

**In scope**
- …

**Out of scope** (do not do these, even if tempting)
- …

## Steps

1. …
2. …

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "<the exact question>"
>
> **Why it matters:** <what changes depending on the answer>
> **Options:** <a / b / c with the trade-off of each>
> **If the user is unavailable:** <the default to take, and note it in Handoff>

## Verify

```bash
<commands that must pass>
```

Manual check: <what to look at, and what correct looks like>

## Definition of done

- [ ] …
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
