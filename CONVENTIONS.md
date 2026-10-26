# Conventions

Short and directive, like `CONTRIBUTING.md`. `/docs/PASSPORT.md` stays the binding
architecture reference and §17 there owns naming; this file covers what neither decides —
above all the wording of everything a user reads.

## Locale

**PASSPORT §18.6, confirmed in TASK_003: every label and user-facing message is Russian, with
no `t()` shim.** There is one locale, so Russian strings sit in the code that renders them;
no resource bundles, no translation keys, no fallback chain. Developer communication and code
identifiers stay English.

### What is Russian and what is not

| Russian                                                       | English                                                       |
| ------------------------------------------------------------- | ------------------------------------------------------------- |
| Labels, headings, buttons, placeholders, `title`/`aria-label` | Code identifiers, DTO fields, IPC channels, tool keys         |
| Empty states, hints, confirmation dialogs                     | Log lines and diagnostic payloads (`logger.log`, step `line`) |
| Error copy the renderer maps from `AppErrorCode`              | System prompts and instructions sent to a model               |
| Host `AppError` messages shown verbatim (see below)           | Commit messages, PR descriptions, comments, this file         |
| Built-in tool `description` and `display_name`                | Enum values and tier tokens (`auto`, `ask`, `off`)            |

A host message reaches the user unchanged when its code is in a renderer `HOST_WORDED` set
(`features/tools/toolPresentation.ts`, `features/connections/connectionErrors.ts`) or when a
store shows `error.message` directly (`ChatStore`, `VectorStoreStore`). **Those messages are
user-facing copy and must be Russian.** Everything else a host service logs is diagnostics and
stays English. Never parse an error string in the renderer — map the code (PASSPORT §7).

Built-in tool descriptions are both: the «Инструменты» page shows them and
`chat_tools::offered_tools` sends them to the model. They are Russian, like every other
built-in tool description in `kernel_tools.rs`.

## Tone

Neutral and formal, the register of a professional tool. The user is a competent adult who
asked for a result, not a learner who needs encouragement.

**Do not:**

- speak as «мы» about what the app is doing — `Проверяем…`, `Читаем таблицу связей`
- anthropomorphise the model — `Думает`, `Пишет ответ`, `Модель хочет вызвать X`
- reassure, soften or congratulate — `Скачать его снова можно в любой момент`,
  `Не волнуйтесь`, `Готово!`
- explain the implementation when it changes nothing for the user — `кэшируется локально`,
  `форма собирается по схеме выбранного типа`, `без обновления страницы`
- leak the state of development — `задача в бэклоге не заведена`,
  `ещё не перенесена в новую оболочку приложения`
- use exclamation marks, emoji or diminutives

**Do:**

- name a state with a verbal noun — `Проверка…`, `Загрузка настроек…`, `Обработка запроса`
- say what happened and what the user can do about it, in that order, and stop
- keep a destructive confirmation to the consequence and its irreversibility:
  `Из хранилища будут удалены все векторы (1 200) и документы (48). Действие нельзя отменить.`
- keep an empty state to the fact, plus one action if there is one:
  `Связей нет. Добавьте связь вручную или запустите индексацию.`

Progress text is a state, not a narration: `Индексация продолжается`, not `Индексируем ваши
файлы`. A hint explains a setting, not the user's job.

## Wording and typography

- `«…»` for quoted names and UI references; `—` (em dash) for a break; `·` as a separator in
  compact lines; `…` as one character, never three dots
- `ё` where it is written: `ещё`, `её`, `перенесён`
- Terminology is fixed. One term per concept, in the UI and in user-facing host messages:
  **диалог** (never «разговор» or «беседа»), **хранилище**, **фрагмент** (never «чанк» in
  prose), **инструмент**, **коннектор**, **секрет**, **запуск**, **провайдер**
- Latin technical tokens stay latin and lower case where they name a value the user also sees
  in code or in a payload: `auto`, `ask`, `off`, `stdio`, `http`, `bm25`. Explain them in a
  Russian hint beside the token (`TIER_HINTS`, `DEFAULT_TIER_LABELS`); do not translate the
  token itself, and do not replace it with an English phrase either — `Ask first` is neither.
- Numbers: `toLocaleString("ru-RU")` for counts a user reads; units are Russian abbreviations
  (`мс`, `с`, `мин`, `ч`, `Б`, `КБ`, `МБ`, `ГБ`)
- Plurals go through `plural(count, one, few, many)` from
  `features/connections/connectionPresentation.ts`, which returns the count with the form
  (`5 моделей`). Russian has three forms and an exception for 11–14; hand-rolled branches get
  it wrong. Add a `…Word(count)` wrapper next to the existing ones rather than inlining the
  arithmetic. `src/browser-ui` is a separate Vite entry that imports nothing from the
  renderer, so it keeps its own copy in `browserCopy.ts` — the one place a second
  implementation is allowed, and it stays in step with this one.

## Where copy lives

Copy sits next to what renders it, and a feature's reusable strings sit in that feature's
`*Presentation.ts` or `*Copy.ts` module — `toolPresentation.ts`, `connectionPresentation.ts`,
`providerErrors.ts`, `secrets/errorCopy.ts`. A string used by two components belongs there, as
an exported constant or a `Record` keyed by the DTO enum, so that the mapping is visible in
one place and testable without the markup.

Test the mapping, not the markup (CONTRIBUTING, "Testing policy"): assert against the
exported constant — `expect(error.message).toBe(ACTIVE_TURN)` — never against a substring of
the prose. A test that greps for `"active turn"` breaks on translation and tells you nothing
about behaviour.

## Reviewing a change to user-facing text

- Does every new string follow the locale split above?
- Would the wording survive being read aloud to the user it addresses?
- Did a host message that the renderer shows verbatim stay Russian?
- Does a new count use `plural`, and a new number `toLocaleString("ru-RU")`?
- Did any assertion pin the prose instead of the constant?
