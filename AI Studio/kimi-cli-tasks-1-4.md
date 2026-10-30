# Задачи для Kimi CLI: ZVS AI Studio — CLI и описания инструментов (пункты 1–4)

## Контекст

Репозиторий: `C:\Users\kirza\Desktop\ZVS\ZVSAIStudio` (монорепо: Rust workspace `crates/*` + фронт `apps/studio`).

Перед началом прочти `AGENTS.md`, `CONVENTIONS.md`, `CLAUDE.md` в корне — там правила кодстайла и коммитов.

Ключевые места:

* CLI (крейт `zvs-cli`, бинарь `aistudio`): `crates/zvs-cli/src/` — Elm-подобная архитектура: `state/mod.rs` (App, Msg, Request, update), `runtime.rs` (цикл событий, dispatch), `ui/` (ratatui-рендер), `state/palette.rs` (slash-команды), `state/shortcuts.rs` (кеймап), `state/overlay.rs` (интерактивные панели).
* Контракты: `crates/zvs-contract/src/chat.rs` (`ChatSendInput.vendor.effort: Option<ThinkingEffort>`, `ThinkingEffort { Auto, Thinking, Fast }`), `crates/zvs-contract/src/providers.rs` (`ModelDto.capabilities: Vec<ModelCapability>`, варианты `reasoning` / `adaptive-reasoning`).
* Описания инструментов для модели: `crates/zvs-host/src/` — `services/chat_tools.rs`, `tools/kernel_tools.rs`, `browser/nodes.rs`, `files/nodes.rs`, `commands/nodes.rs`, `skills/nodes.rs`, `kernel/knowledge_nodes.rs`, `services/chat_memory.rs`, `services/chat_ask.rs`, `agentic/node.rs`.
* `app.env` в корне: `ZVS_APP_VERSION`, `ZVS_APP_CHANNEL`, `ZVS_CLI_VERSION=1.0.0`, `ZVS_CLI_CHANNEL=BETA`.
* Пример проброса app.env в Rust: `apps/studio/src-tauri/build.rs` (читает `../../../app.env`, прокидывает через `cargo:rustc-env`).
* Проброс во фронт: `scripts/app-env.mjs` (`appEnvDefine()` кладёт ВЕСЬ app.env в `__ZVS_APP_ENV__`), типизация в `apps/studio/src/renderer/app/appEnv.ts`.

Проверка после каждого пункта: `cargo test -p zvs-cli` (для CLI-части), `cargo test -p zvs-host` (для описаний), `cargo clippy --workspace` без новых предупреждений; фронт — `pnpm --filter studio test` и typecheck по скриптам из package.json.

***

## Пункт 1. `aistudio --resume <id>` и подсказка при выходе

### Что уже есть

* Команда `/resume` в CLI: `state/mod.rs` — `Msg::Loaded` → `Request::Attach(id)` → `chat.attach`; панель `Panel::Resume` в `state/overlay.rs`.
* Выход: `App.exit: Option<Exit>`, `Exit::{Quiet, Message}`; `runtime.rs::run()` после `ratatui::restore()` печатает сообщение и возвращает код.
* `main.rs` — 4 строки, аргументы не разбираются.

### Что сделать

1. **Разбор аргументов** в `zvs-cli` (без clap и новых зависимостей, ручной разбор `std::env::args`):
   * `aistudio` — текущее поведение.
   * `aistudio --resume <conversation_id>` — после подключения к хосту загрузить и приаттачить существующую беседу вместо создания новой.
   * `aistudio --version` — печатает версию (см. пункт 3) и выходит с кодом 0.
   * `aistudio --help` — usage и выход с кодом 0.
   * Неизвестный аргумент или `--resume` без значения — usage в stderr и код 2.
     Разбор вынести в отдельный модуль `args.rs` с чистой функцией `parse(args) -> Result<Args, UsageError>`, покрытой юнит-тестами. `Args { resume: Option<String> }`.
2. **Resume при старте.** Сейчас после `Msg::Connected` отправляются `[Request::Open, Request::Providers, Request::Policy]`, а новая беседа создаётся позже. Нужно: если задан `--resume <id>`, после открытия сессии (и выдачи доступа к папке, фаза Access → Chat — поведение как у обычного старта) выполнить тот же конвейер, что у `/resume`: `Request::Load(id)` (`chat.conversations.get`) → `Msg::Loaded` → `Request::Attach(id)`. Ошибка загрузки (беседа не найдена) — сообщение об ошибке в транскрипт И продолжение с новой беседой либо выход с кодом 1: выбрать выход с кодом 1 и понятным сообщением (`Conversation <id> not found`), это предсказуемее для скриптов. id приходит строкой из argv — парсить в `ConversationId` с обработкой ошибки формата (невалидный id → usage-ошибка, код 2).
3. **Подсказка при выходе.** Изменить `session()`/`run()` в `runtime.rs`: после `ratatui::restore()`, если за время сессии существовала активная беседа (`app.conversation: Option<ConversationDto>`), печатать в stdout:
   ```
   Продолжить эту сессию: aistudio --resume <conversation_id>
   ```
   Печатать при любом выходе после установленной беседы (`/exit`, Ctrl+C/Ctrl+D, `Msg::Closed`), кроме случая, когда беседа так и не была создана/приаттачена. Для этого вернуть id из `session()` наружу (например, изменить тип возврата на `(Option<Exit>, Option<ConversationId>)` или хранить `last_conversation: Option<ConversationId>` в App и отдавать его вместе с Exit). Текст — в `copy.rs` константой, как остальные строки.
4. **Тесты**: парсер аргументов; что `--resume` приводит к `Request::Load` после подключения (по образцу `state/tests.rs::resuming_loads_the_conversation_and_reattaches`); что при отсутствии беседы подсказка не формируется.

***

## Пункт 2. Команда `/effort` — интерактивное переключение силы размышления

### Как устроена интерактивность (справочно)

* Slash-команды: `state/palette.rs` — enum `Command`, `COMMANDS`, `name()`, `description()`, `parse()`.
* Шорткаты: `state/shortcuts.rs` — enum `Shortcut`, `SHORTCUTS`, `default_chord()`, маппинг `command()`; кеймап сериализуется в keymap-файл (`keymap_file.rs`).
* Панели: `state/overlay.rs` — enum `Panel`, `Overlay`, строки `Row` с `action: Option<Action>`; Enter на строке применяет `Action`; `View` передаёт снимок состояния App в построитель строк. Рендер — `ui/overlay.rs`.
* Обработка действий: `state/mod.rs` — применение `Action::*` из оверлея.
* Отправка: `state/mod.rs::send()` строит `ChatSendInput`; у него есть `vendor: VendorTurnOptions { effort: Option<ThinkingEffort>, .. }`.

### Как effort работает в вебе (эталон)

`apps/studio/src/renderer/features/chat/accountSetupCopy.ts`: ступени `["fast", "auto", "thinking"]`; если у модели capability `adaptive-reasoning` — все три; иначе только `["fast", "thinking"]`; если нет capability `reasoning` — переключение недоступно. Уходит в `chat.send` как `vendor.effort`.

### Что сделать

1. **`Command::Effort`** (`/effort`) в `palette.rs` с описанием (константа в `copy.rs`, на русском, в стиле остальных `CMD_*`).
2. **`Shortcut::Effort`** в `shortcuts.rs`: id `"effort"`, `command() -> Command::Effort`, дефолтная привязка `Alt+E` (проверить отсутствие конфликтов с существующими дефолтами: Ctrl+O/U/N/L/P/R/K, Alt+S/M/P/K, F1). Попадёт в `/shortcuts` автоматически.
3. **`Panel::Effort`** в `overlay.rs`: строки — ступени effort, доступные для **текущей выбранной модели** (`App.model` → найти её в `App.providers` → `capabilities`):
   * capability `adaptive-reasoning` → `fast`, `auto`, `thinking`;
   * только `reasoning` → `fast`, `thinking`;
   * нет `reasoning` → одна информационная строка без `action` («модель не поддерживает регулирование размышления»).
     Текущий режим пометить `current: true` (как в `model_rows`). Новый `Action::SetEffort(ThinkingEffort)`.
4. **Состояние**: `App.effort: ThinkingEffort`, дефолт `Auto`. Применение `Action::SetEffort` — обновить поле, закрыть оверлей, показать status-подтверждение (паттерн `Tone::Success`, как при смене модели).
5. **Отправка**: в `send()` проставлять `input.vendor.effort = Some(self.effort)`. Учесть, что в вебе для `accountFamily === "openai"` `webSearch: false` — в CLI web-поиска нет, `web_search` оставить `false` (дефолт).
6. **Отображение**: текущий effort показывать в статусной/промпт-строке рядом с именем модели (см. `ui/prompt.rs` / `ui/mod.rs` — там, где уже выводится модель); короткий ярлык: `fast` / `auto` / `thinking`.
7. **Тесты** (`state/tests.rs`, `state/overlay.rs` тесты): открытие `/effort`; набор ступеней для модели с `adaptive-reasoning` / только `reasoning` / без; применение `SetEffort` меняет `app.effort`; `send()` включает `vendor.effort`.

***

## Пункт 3. Версия CLI: в PreferencesDialog, на welcome-экране и в `--version`

### Что сделать

1. **`build.rs` для `zvs-cli`** по образцу `apps/studio/src-tauri/build.rs`: читать `../../../app.env` от `CARGO_MANIFEST_DIR`, валидировать формат, требовать `ZVS_CLI_VERSION` и `ZVS_CLI_CHANNEL`, прокидывать через `println!("cargo:rustc-env=...")`, `cargo:rerun-if-changed` на app.env. В коде: `env!("ZVS_CLI_VERSION")`, `env!("ZVS_CLI_CHANNEL")`. Дублирование логики чтения app.env с src-tauri допустимо (10 строк), общий крейт не создавать.
2. **`--version`** (из пункта 1): вывод `aistudio <ZVS_CLI_VERSION> (<ZVS_CLI_CHANNEL>)`, код 0.
3. **Welcome-экран CLI** (`ui/mod.rs::welcome`, рядом `ui/banner.rs`): под артом/рядом с папкой и моделью добавить строку версии: `v1.0.0 · BETA` (канал из env, привести к виду как `APP_VERSION_LABEL` во фронте: `BETA v1.0.0` — выбрать единый формат `<CHANNEL> v<VERSION>` и использовать везде в CLI). Обновить тест `ui/tests.rs::the_welcome_screen_shows_the_art_folder_and_model` — теперь и версию.
4. **PreferencesDialog (фронт)**:
   * В `apps/studio/src/renderer/app/appEnv.ts` расширить тип `__ZVS_APP_ENV__` ключами `ZVS_CLI_VERSION: string; ZVS_CLI_CHANNEL: string` и экспортировать `CLI_VERSION`, `CLI_CHANNEL` (данные туда уже попадают через `appEnvDefine()`, менять vite-конфиг не нужно — проверить).
   * В `PreferencesDialog.tsx` секция «О программе» (`AboutSection`): **удалить** секцию «Компьютер» (`deviceRows`, загрузку `about.load()`/`system.device`). **Добавить** секцию «CLI» со строками: Версия — `<ZVS_CLI_VERSION>`, Канал — `<ZVS_CLI_CHANNEL>`. Строки — через `InfoTableRows`, подписи в `preferencesPresentation.ts` (добавить `cliRows()` по образцу `appRows()`).
   * Если `AboutStore` после этого нигде не используется — удалить его и регистрацию в `RootStore.ts`; если используется — оставить, но из диалога убрать. Проверить `settingsIndex.ts`/`aboutSettingId`: удалить записи, относящиеся к «Компьютер», добавить записи для CLI, чтобы поиск по настройкам не вёл на несуществующие строки.
   * Обновить/удалить соответствующие тесты рендерера (`apps/studio/tests/renderer/`).

***

## Пункт 4. Описания инструментов — на английском

Перевести на английский **только model-facing** тексты (то, что уходит модели в описаниях инструментов и system-инструкциях). User-facing русский текст (CLI `copy.rs`, `TOOL_NOT_FOUND` в `tools/registry.rs`, `ENGINE_DESCRIPTION` в `voice/catalogue.rs`) **не трогать**.

Файлы и константы:

1. `crates/zvs-host/src/services/chat_tools.rs` — `TOOL_SEARCH_DESCRIPTION`.
2. `crates/zvs-host/src/tools/kernel_tools.rs` — описания `vector.search` и `vector.list` (inline в `BUILTIN`).
3. `crates/zvs-host/src/kernel/knowledge_nodes.rs` — `SEARCH_DESCRIPTION`.
4. `crates/zvs-host/src/browser/nodes.rs` — `NAVIGATE/READ_PAGE/SCREENSHOT/FIND/CLICK/TYPE/SUBMIT/DOWNLOAD_DESCRIPTION` (8 шт).
5. `crates/zvs-host/src/files/nodes.rs` — `LIST/READ/SEARCH/APPLY_DESCRIPTION` (4 шт).
6. `crates/zvs-host/src/commands/nodes.rs` — `CODE_DESCRIPTION`, `SHELL_DESCRIPTION`.
7. `crates/zvs-host/src/skills/nodes.rs` — `LOAD_DESCRIPTION`, `SAVE_DESCRIPTION`.
8. `crates/zvs-host/src/services/chat_memory.rs` — `MEMORY_DESCRIPTION`.
9. `crates/zvs-host/src/services/chat_ask.rs` — `ASK_DESCRIPTION`.
10. `crates/zvs-host/src/agentic/node.rs` — `PLAN_DESCRIPTION`, `COMPLETE_DESCRIPTION`.

Требования к переводу:

* Сохранить смысл дословно и стиль «плотной инструкции» (без воды), термины не переводить: имена параметров (`query`, `documentId`, `fromChunk–toChunk`, `argv`, `cwd`, `stdin`), схемы путей (`chat:`, `skill:<ключ>/` → `skill:<key>/`), имена инструментов (`fs.list`, `browser.click`), `ZVS_WORKSPACE_DIR`, `ZVS_SKILL_DIR`, PEP 723 и т.п.
* Сохранить форматирование ссылок на другие инструменты и переносы смысловых частей.
* После перевода прогнать `cargo test -p zvs-host`: тесты, проверяющие тексты описаний (если есть), поправить; snapshot-тесты system-промптов обновить.
* Дополнительно проверить, что в тестах и fixtures нет ассертов на русские фрагменты этих описаний (`rg "Загрузить параметры|Поиск по базам" crates/`).

***

## Критерии приёмки

1. `aistudio --help`, `--version`, `--resume <id>`, `--resume` без id, неизвестный флаг — все ведут себя как описано; при выходе из сессии с беседой печатается подсказка с правильным id.
2. `/effort` открывает панель с корректным набором ступеней для текущей модели; выбор меняет режим; шорткат Alt+E работает и переназначается через `/shortcuts`; `chat.send` уходит с `vendor.effort`.
3. В PreferencesDialog нет секции «Компьютер», есть секция «CLI» с версией/каналом из app.env; welcome-экран CLI и `--version` показывают ту же версию; смена `ZVS_CLI_VERSION` в app.env меняет вывод после пересборки (build.rs триггерится по rerun-if-changed).
4. Все перечисленные описания на английском; user-facing строки не изменились.
5. `cargo test -p zvs-cli`, `cargo test -p zvs-host`, `cargo clippy --workspace` — чисто; тесты фронта зелёные.

