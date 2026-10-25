# Interview

Ask before writing. A skill built on a guess triggers on the wrong requests and does the wrong thing quietly. Collect the answers in one message; skip any question the conversation already answered, and do not ask about details you can decide yourself with a sensible default (say which default you chose).

## Questions that always matter

1. **The job.** What should the skill accomplish, in one sentence? What does a finished result look like: a file, an answer in the chat, a change to existing files?
2. **Triggers.** Two or three requests the user would actually type that should use the skill, and one or two similar requests that should not. These become the description and the test.
3. **Inputs.** What the skill starts from: files of a certain type, a folder, a knowledge store, text in the message, data from a connector. Where these usually live.
4. **Output.** Format, language, length, where to save files and how to name them.
5. **Steps and rules.** The order the user follows when doing this by hand, the checks they make, the mistakes to avoid, the house style (templates, terminology, tone).

## Questions that matter only sometimes

- **Tools.** Which tools the skill needs: files (`fs.read`, `fs.apply`), knowledge stores (`knowledge.search`), commands (`shell.run`, `code.run`), connector tools. Use only keys the user names or that are offered in this conversation.
- **Scripts.** Whether a step is mechanical enough to be a script: parsing a format, converting files, computing numbers, rendering a document. Deterministic work belongs in a script; judgement belongs in the instructions.
- **Model.** Whether the skill needs a particular model. Usually it does not; the user can set one later on the «Навыки» page.
- **Reference material.** Examples, templates, glossaries or specifications the user can paste or point to. These go into `references/` or `assets/`.
- **Name and identifier.** Propose them yourself; ask only if the user cares.

## When to stop asking

Stop as soon as you can write the description and list the steps. If an answer stays unclear, choose the safer behaviour (ask the user at run time, do not overwrite files, report instead of acting) and write that choice into the instructions.

## Red flags to clarify

- The skill would act on many files, send messages or delete data: make the instructions confirm with the user first.
- The request is really two jobs: propose two skills.
- The request is a one-off: say a skill is not needed and do the task directly.
