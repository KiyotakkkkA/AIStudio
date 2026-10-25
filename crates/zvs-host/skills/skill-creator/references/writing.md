# Writing a skill

## The description

The description is the only part of the skill the model sees before choosing it, and the app also matches it against the user's request. It decides whether the skill is ever used.

- Write it in the user's language. Bundled skills use Russian.
- First the job, then the trigger: «Составляет … по … . Используется, когда …».
- Name the nouns users type: file types (`.pdf`, «таблица Excel»), objects («счёт», «договор», «дашборд»), verbs («сравнить», «проверить», «собрать отчёт»).
- Cover the near-synonyms of the trigger: «отчёт», «сводка», «итоги».
- Say what the skill does not do when a neighbouring skill does it: «Для документов Word — навык docx».
- No marketing, no «мощный», «удобный», no instructions to the model.

Weak: «Помогает с отчётами.»

Strong: «Составляет еженедельный отчёт о продажах по выгрузке CRM в CSV или Excel: итоги по менеджерам и регионам, сравнение с прошлой неделей, таблица для письма. Используется, когда пользователь просит отчёт, сводку или итоги продаж за период.»

## The instructions

Write for a capable colleague who has never seen the task.

- **Imperative and concrete.** «Run `xlsx_info.py` first», not «you may want to inspect the file».
- **Order of work first.** A numbered workflow at the top, details below it.
- **One rule that matters most**, in bold, when there is a failure mode that ruins the result.
- **Explain the why** behind non-obvious rules in one clause; the model then handles cases you did not foresee.
- **Decision tables** for "the user wants X → do Y" choices.
- **Examples** of a good result, short. Long examples go to `references/`.
- **Defaults** for every choice the user may not specify: file name, location, format, language.
- **When to ask** the user, and when to proceed: name the cases explicitly.
- **How to finish**: what to report, which path to give, what to check before answering.

Keep `SKILL.md` under roughly 300 lines. Move specifications, long examples, format references and catalogues to `references/` and say exactly when to read each file.

## Scripts versus instructions

Put a step into a script when it is mechanical and has one right answer: parsing, converting, computing, validating, rendering from a template. A script is faster, cheaper and does not drift. Keep judgement in the instructions: what to include, how to phrase, what to ask.

Give every script a single job and a stable interface: input paths, output path, flags. Document each script in one line in the instructions: what it takes, what it prints.

## Assets

Templates, boilerplate and styles that are copied into results go to `assets/`. Tell the model to start from the asset and fill it in, rather than to write the same thing from scratch each time.

## Testing

Before saving, run the skill in your head on the trigger requests from the interview:

- Would the description be chosen for each of them, and not for the requests that should not trigger it?
- Could someone follow the instructions without asking anything that the skill could have answered?
- Does every path the instructions mention exist in the file list?

After saving, suggest the user try one real request and come back with what went wrong.
