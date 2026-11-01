pub const TITLE: &str = "ZVS AI Studio";
pub const SUBTITLE: &str = "Консольная версия";
pub const CONNECTING: &str = "Подключение к ZVS AI Studio…";
pub const LAUNCHING: &str = "Запуск ZVS AI Studio…";
pub const APP_NOT_FOUND: &str = "Не найдено приложение ZVS AI Studio. Переустановите консольную версию в настройках приложения.";
pub const APP_UNREACHABLE: &str =
    "Приложение ZVS AI Studio не отвечает. Проверьте, что оно запущено, и повторите попытку.";
pub const PROTOCOL_MISMATCH: &str =
    "Версия консольного клиента не совпадает с приложением. Обновите ZVS AI Studio.";
pub const DISCONNECTED: &str = "Соединение с приложением потеряно";
pub const NO_TERMINAL: &str = "Консольная версия работает только в интерактивном терминале";
pub const USAGE_COMMANDS: &str = "Использование";
pub const USAGE_UNKNOWN: &str = "Неизвестный аргумент";
pub const USAGE_MCP_SHORT: &str = "подключить MCP-сервер";
pub const USAGE_MCP_MORE: &str = "Подробнее о подключении MCP-серверов: aistudio mcp --help";
pub const USAGE_MCP: &str =
    "Подключение MCP-сервера (в сессии — /mcp add, сведения о сервере — /mcp inspect <имя>):
  <адрес> вида http:// или https:// подключает http-сервер; если сервер требует вход,
  открывается браузер. Иначе первое слово — команда stdio-сервера, остальные — её аргументы.
  Ключи указываются до имени сервера:
  -e, --env КЛЮЧ=значение        переменная окружения stdio-сервера
  -H, --header \"Имя: значение\"   заголовок http-сервера
  Сервер запускается сразу после добавления. Секреты из хранилища подключаются
  только в приложении.

Примеры:
  aistudio mcp add docs https://mcp.example.com/mcp
  aistudio mcp add -e ROOT=D:\\work files npx -y @modelcontextprotocol/server-filesystem D:\\work";
pub const USAGE_RESUME_MISSING: &str = "После --resume укажите идентификатор диалога";
pub const USAGE_RESUME_INVALID: &str = "Недопустимый идентификатор диалога";
pub const RESUME_HINT: &str = "Продолжить этот диалог";

pub const ACCESS_TITLE: &str = "Доступ к папке";
pub const ACCESS_BODY: &str =
    "Диалоги этой сессии будут работать с файлами в папке. Разрешить доступ?";
pub const ACCESS_CONVERSATION: &str = "Разрешить для этого диалога";
pub const ACCESS_ALWAYS: &str = "Разрешить всегда";
pub const ACCESS_DENY: &str = "Отказать и выйти";
pub const ACCESS_DENIED: &str = "Доступ к папке не предоставлен. Сессия завершена.";

pub const WELCOME_HINT: &str = "Введите сообщение, / для списка команд или ? для сочетаний клавиш";
pub const INPUT_PLACEHOLDER: &str =
    "Сообщение…  (/ — команды, ? — сочетания клавиш, Alt+Enter — новая строка)";
pub const WORKSPACE: &str = "Рабочая папка";
pub const MODEL: &str = "Модель";
pub const NO_MODEL: &str = "не выбрана";
pub const NO_PROVIDER: &str =
    "Нет включённого провайдера с текстовыми моделями. Подключите его командой /connect.";

pub const YOU: &str = "Вы";
pub const ASSISTANT: &str = "Ассистент";
pub const REASONING: &str = "Рассуждение";
pub const WORKING: &str = "Обработка запроса";
pub const CANCELLING: &str = "Остановка…";
pub const CANCELLED: &str = "Запрос остановлен";
pub const FAILED: &str = "Не удалось получить ответ";
pub const NEW_CHAT: &str = "Новый диалог в этой папке";
pub const CHAT_CLEARED: &str = "Экран очищен";

pub const HINT_IDLE: &str =
    "Enter — отправить · ? — сочетания клавиш · Esc Esc — выход · PgUp/PgDn — прокрутка";
pub const HINT_RUNNING: &str = "Esc — остановить · PgUp/PgDn — прокрутка";
pub const HINT_PROMPT: &str = "↑/↓ — выбор · Enter — подтвердить · Esc — отклонить";
pub const HINT_PALETTE: &str = "↑/↓ — выбор · Tab — дополнить · Enter — выполнить · Esc — закрыть";
pub const COPIED: &str = "Скопировано в буфер обмена";
pub const COPY_FAILED: &str = "Не удалось скопировать текст";
pub const EXIT_AGAIN: &str = "Нажмите Esc ещё раз для выхода";

pub const APPROVAL_TITLE: &str = "Требуется подтверждение";
pub const APPROVE: &str = "Разрешить";
pub const APPROVE_ALWAYS_HERE: &str = "Разрешить всегда в этой папке";
pub const APPROVE_ALWAYS_EVERYWHERE: &str = "Разрешить всегда";
pub const APPROVE_FILES_HERE: &str = "Разрешить такие изменения во всей папке";
pub const APPROVE_FILES_EXACT: &str = "Разрешить всегда для этих файлов";
pub const DENY: &str = "Отклонить";
pub const RULE_ADDED: &str = "Добавлено правило";
pub const COMMAND: &str = "Команда";
pub const IN_FOLDER: &str = "Папка";
pub const CHANGES: &str = "Изменения";

pub const FOLDER_REQUEST_TITLE: &str = "Запрос доступа к папке";
pub const FOLDER_ONCE: &str = "Разрешить один раз";
pub const FOLDER_CONVERSATION: &str = "Разрешить для этого диалога";
pub const FOLDER_ALWAYS: &str = "Разрешить всегда";

pub const VERIFY_TITLE: &str = "Проверка браузера";
pub const VERIFY_BODY: &str = "Провайдер запросил проверку браузера. Окно проверки открыто: пройдите проверку, и ответ продолжится.";
pub const VERIFY_SITE: &str = "Сайт";
pub const HINT_VERIFY: &str = "Enter — показать окно проверки · Esc — остановить ответ";

pub const SHELL_MODE: &str = "Командная строка";
pub const SHELL_PLACEHOLDER: &str = "Команда для cmd в рабочей папке";
pub const HINT_SHELL: &str = "Enter — выполнить в рабочей папке · Esc — очистить";
pub const HINT_SHELL_RUNNING: &str = "Esc — остановить команду";
pub const SHELL_RUNNING: &str = "выполняется";
pub const SHELL_DONE: &str = "готово";
pub const SHELL_EXIT: &str = "код выхода";
pub const SHELL_STOPPED: &str = "остановлено";
pub const SHELL_FAILED: &str = "Не удалось запустить команду";
pub const SHELL_BUSY: &str = "Дождитесь окончания команды или остановите её клавишей Esc";

pub const QUESTION_TITLE: &str = "Вопрос";
pub const QUESTION_OTHER: &str = "Другой вариант";
pub const QUESTION_TEXT_HINT: &str = "Введите ответ и нажмите Enter";
pub const QUESTION_MULTI_HINT: &str = "Пробел — отметить · Enter — далее · Esc — пропустить";
pub const QUESTION_SKIPPED: &str = "Вопрос пропущен";

pub const TOOL_RUNNING: &str = "выполняется";
pub const TOOL_AWAITING_APPROVAL: &str = "ждёт подтверждения";
pub const TOOL_AWAITING_ANSWER: &str = "ждёт ответа";
pub const TOOL_SUCCEEDED: &str = "готово";
pub const TOOL_FAILED: &str = "ошибка";
pub const TOOL_REFUSED: &str = "запрещено";
pub const TOOL_DENIED: &str = "отклонено";
pub const TOOL_CANCELLED: &str = "отменено";
pub const ARGUMENTS: &str = "Аргументы";
pub const RESULT: &str = "Результат";
pub const TRUNCATED: &str = "Результат сокращён";
pub const MORE_LINES: &str = "ещё строк:";

pub const COMMANDS_TITLE: &str = "Команды";
pub const HELP_TITLE: &str = "Справка";
pub const UNKNOWN_COMMAND: &str = "Неизвестная команда";
pub const BUSY: &str = "Дождитесь окончания ответа или остановите его клавишей Esc";

pub const CMD_HELP: &str = "Список команд и клавиш";
pub const CMD_NEW: &str = "Начать новый диалог в этой папке";
pub const CMD_CLEAR: &str = "Очистить экран";
pub const CMD_EXIT: &str = "Завершить сессию";
pub const CMD_MODELS: &str = "Провайдеры и модели";
pub const CMD_CONNECT: &str = "Подключение провайдера";
pub const CMD_DESKTOP: &str = "Открыть диалог в приложении";
pub const CONNECT_TITLE: &str = "Подключение провайдера";
pub const CONNECT_LINKING: &str = "Вход в аккаунт";
pub const CONNECT_CANCEL_HINT: &str = "Esc — отменить";
pub const CONNECT_LINKED: &str = "подключён. Модель доступна через /models";
pub const CONNECT_CONNECTED: &str = "подключён";
pub const CONNECT_HINT_BROWSER: &str = "Вход во встроенном браузере приложения";
pub const CONNECT_HINT_OPENAI: &str = "Вход в системном браузере";
pub const CONNECT_HINT_COPILOT: &str = "Требуется GitHub Copilot CLI";
pub const DESKTOP_OPENED: &str = "Диалог открыт в приложении";
pub const DESKTOP_NO_CONVERSATION: &str =
    "Нет открытого диалога. Отправьте сообщение или выберите диалог через /resume";
pub const DESKTOP_FAILED: &str = "Не удалось открыть приложение ZVS AI Studio";
pub const COPILOT_EFFORT_LOW: &str = "Низкое";
pub const COPILOT_EFFORT_HIGH: &str = "Высокое";
pub const COPILOT_EFFORT_LOW_HINT: &str = "Низкий уровень рассуждения";
pub const COPILOT_EFFORT_AUTO_HINT: &str = "Уровень рассуждения по умолчанию для выбранной модели";
pub const COPILOT_EFFORT_HIGH_HINT: &str = "Высокий уровень рассуждения";
pub const CMD_RESUME: &str = "Продолжить диалог этой папки";
pub const CMD_CONTEXT: &str = "Заполнение контекста";
pub const CMD_SKILLS: &str = "Навыки";
pub const CMD_PROCESSES: &str = "Фоновые процессы агента";
pub const CMD_GOAL: &str = "Работа над целью: <цель>";
pub const CMD_GOAL_STOP: &str = "Остановить цель";
pub const CMD_GOAL_CONTINUE: &str = "Продолжить приостановленную цель";
pub const GOAL_HINT: &str = "Укажите цель после команды: /goal <цель>";
pub const GOAL_RUNNING: &str = "Цель выполняется. Остановить её: /goal stop или Ctrl+C";
pub const GOAL_NOT_STOPPABLE: &str = "Нет выполняемой или приостановленной цели";
pub const GOAL_NOT_PAUSED: &str = "Нет приостановленной цели";
pub const GOAL_STOP_CONFIRM: &str = "Нажмите Ctrl+C ещё раз, чтобы остановить цель";
pub const GOAL_STOPPING: &str = "Остановка цели…";
pub const GOAL_STOPPED: &str = "Цель остановлена";
pub const GOAL_PAUSED: &str = "Цель приостановлена";
pub const GOAL_FAILED: &str = "Цель завершилась ошибкой";
pub const GOAL_CONTINUED: &str = "Работа над целью продолжена";
pub const GOAL_DONE: &str = "Цель достигнута";
pub const GOAL_EVIDENCE: &str = "Подтверждение";
pub const GOAL_FOOTER: &str = "Цель";
pub const GOAL_REASON_ROUNDS: &str = "исчерпан лимит раундов";
pub const GOAL_REASON_TOKENS: &str = "исчерпан лимит токенов";
pub const GOAL_REASON_MINUTES: &str = "исчерпан лимит времени";
pub const GOAL_REASON_REPETITION: &str = "повторяющиеся вызовы";
pub const GOAL_REASON_RESTART: &str = "приложение было перезапущено";
pub const GOAL_REASON_ERROR: &str = "ход завершился ошибкой";
pub const CMD_MCP: &str = "MCP-серверы";
pub const CMD_MCP_ADD: &str = "Подключить MCP-сервер: <имя> <адрес | команда [аргументы]>";
pub const CMD_MCP_INSPECT: &str = "Сведения о сервере и его инструменты: <имя>";
pub const CMD_PERMISSIONS: &str = "Режим выполнения и модель-арбитр";
pub const CMD_SHORTCUTS: &str = "Сочетания клавиш и их переназначение";
pub const CMD_STOP_SUBAGENT: &str = "Остановить последний запущенный субагент";
pub const SUBAGENT_NONE: &str = "Нет выполняющихся субагентов";
pub const SUBAGENT_STOPPING: &str = "Остановка субагента…";
pub const SUBAGENT_TITLE: &str = "Субагент";
pub const SUBAGENT_EXPLORE: &str = "Исследование";
pub const SUBAGENT_GENERAL: &str = "Общая задача";
pub const SUBAGENT_RUNNING: &str = "Выполняется";
pub const SUBAGENT_QUEUED: &str = "В очереди";
pub const SUBAGENT_PARTIAL: &str = "Частичный результат";
pub const SUBAGENT_STOPPED: &str = "Остановлен";
pub const SUBAGENT_FAILED: &str = "Ошибка";
pub const SUBAGENT_DONE: &str = "Готово";
pub const SUBAGENT_REPORT: &str = "Отчёт";

pub fn subagent_title(label: &str) -> String {
    format!("{SUBAGENT_TITLE}: {label}")
}
pub const CMD_EFFORT: &str = "Сила размышления модели";
pub const CMD_THEME: &str = "Тема оформления";

pub const THEME_TITLE: &str = "Тема оформления";
pub const THEME_CURRENT: &str = "текущая";
pub const THEME_SET: &str = "Тема изменена";
pub const THEME_SAVE_FAILED: &str = "Не удалось сохранить тему оформления";
pub const THEME_EMBER: &str = "Янтарь";
pub const THEME_GRAPHITE: &str = "Графит";
pub const THEME_MOSS: &str = "Мох";
pub const THEME_DUSK: &str = "Сумерки";
pub const THEME_FROST: &str = "Иней";
pub const THEME_HINT_EMBER: &str = "Тёплая оранжевая гамма. Тема по умолчанию.";
pub const THEME_HINT_GRAPHITE: &str = "Нейтральная серая гамма с синим акцентом.";
pub const THEME_HINT_MOSS: &str = "Приглушённая зелёная гамма.";
pub const THEME_HINT_DUSK: &str = "Фиолетовая гамма.";
pub const THEME_HINT_FROST: &str = "Холодная бирюзовая гамма.";
pub const HINT_THEME: &str = "↑/↓ — предпросмотр · Enter — применить · Esc — отмена";

pub const EFFORT_TITLE: &str = "Размышление";
pub const EFFORT_FAST: &str = "Быстрый";
pub const EFFORT_AUTO: &str = "Авто";
pub const EFFORT_THINKING: &str = "Глубокое";
pub const EFFORT_HINT_FAST: &str = "Отвечает без этапа размышления.";
pub const EFFORT_HINT_AUTO: &str = "Модель сама решает, нужно ли размышление.";
pub const EFFORT_HINT_THINKING: &str = "Всегда размышляет перед ответом.";
pub const EFFORT_SET: &str = "Режим размышления изменён";
pub const EFFORT_CURRENT: &str = "текущий";
pub const EFFORT_UNSUPPORTED: &str = "Модель не поддерживает регулирование размышления";

pub const KEYS_TITLE: &str = "Клавиши";
pub const KEYS: &[(&str, &str)] = &[
    ("Enter", "отправить сообщение"),
    ("Alt+Enter, Shift+Enter", "новая строка"),
    ("↑ / ↓", "история ввода"),
    ("Esc", "остановить ответ"),
    ("Esc Esc", "выйти"),
    ("Колесо, PgUp / PgDn", "прокрутка"),
    ("Выделение мышью", "скопировать текст"),
    ("! в начале строки", "выполнить команду в рабочей папке"),
    ("Ctrl+C", "выйти"),
    ("?", "показать сочетания клавиш"),
];

pub const SHORTCUTS_TITLE: &str = "Сочетания клавиш";
pub const KEYS_COMPOSE: &str = "Ввод";
pub const KEYS_SESSION: &str = "Сеанс";
pub const KEYS_TRANSCRIPT: &str = "Просмотр";
pub const KEY_COMMANDS: &str = "Команды";
pub const KEY_SHELL: &str = "Команда в рабочей папке";
pub const KEY_NEWLINE: &str = "Новая строка";
pub const KEY_HISTORY: &str = "История ввода";
pub const KEY_COMPLETE: &str = "Дополнить команду";
pub const KEY_CLEAR_INPUT: &str = "Очистить строку";
pub const KEY_SEND: &str = "Отправить сообщение";
pub const KEY_STOP: &str = "Остановить ответ";
pub const KEY_NEW_CHAT: &str = "Новый диалог";
pub const KEY_MODELS: &str = "Модели";
pub const KEY_EFFORT: &str = "Размышление";
pub const KEY_RESUME: &str = "Продолжить диалог";
pub const KEY_PERMISSIONS: &str = "Подтверждение действий";
pub const KEY_QUIT: &str = "Выход";
pub const KEY_DETAILS: &str = "Подробности инструментов";
pub const KEY_SCROLL: &str = "Прокрутка";
pub const KEY_CLEAR_SCREEN: &str = "Очистить экран";
pub const KEY_CONTEXT: &str = "Контекст";
pub const KEY_MOUSE: &str = "Мышь";
pub const KEY_COPY: &str = "Выделить и скопировать";
pub const KEYS_CUSTOMIZE: &str = "переназначить";
pub const KEYS_THEME: &str = "тема оформления";
pub const HINT_KEYS: &str = "? / Esc — закрыть";
pub const SHORTCUT_COLUMN_KEYS: &str = "Сочетание";
pub const SHORTCUT_COLUMN_ACTION: &str = "Действие";
pub const SHORTCUT_DETAILS: &str = "Показать или скрыть подробности инструментов";
pub const SHORTCUT_CLEAR_INPUT: &str = "Очистить строку ввода";
pub const SHORTCUT_NONE: &str = "не назначено";
pub const SHORTCUT_PRESS: &str = "нажмите сочетание…";
pub const SHORTCUTS_RESET: &str = "Восстановить сочетания по умолчанию";
pub const SHORTCUT_SET: &str = "Назначено";
pub const SHORTCUT_MOVED: &str = "снято с действия";
pub const SHORTCUT_REMOVED: &str = "Сочетание убрано";
pub const SHORTCUT_INVALID: &str = "Используйте Ctrl или Alt с буквой либо клавиши F1–F12";
pub const SHORTCUT_RESERVED: &str =
    "Это сочетание используется терминалом и не может быть назначено";
pub const SHORTCUTS_RESTORED: &str = "Восстановлены сочетания по умолчанию";
pub const SHORTCUTS_SAVE_FAILED: &str = "Не удалось сохранить сочетания клавиш";
pub const HINT_CAPTURE: &str = "Нажмите новое сочетание · Backspace — убрать · Esc — отмена";

pub const MODE: &str = "Подтверждение действий";
pub const MODE_ALWAYS_ASK: &str = "Всегда спрашивать";
pub const MODE_SAFETY_ASK: &str = "Только опасные действия";
pub const MODE_AUTO_BYPASS: &str = "Автопроверка";
pub const MODE_NO_RESTRICTIONS: &str = "Без ограничений";
pub const CONTEXT: &str = "Контекст";
pub const RECOMMENDED: &str = "рекомендуется";
pub const QUESTION_PROGRESS: &str = "из";

pub const SEARCH_PLACEHOLDER: &str = "Поиск…";
pub const LOADING: &str = "Загрузка…";
pub const NOTHING_FOUND: &str = "Ничего не найдено";
pub const HINT_PANEL: &str =
    "↑/↓ — выбор · ←/→, Tab — вкладки · Пробел — подробности · Enter — выбрать · Esc — закрыть";
pub const TAB_ALL: &str = "Все";

pub const MODELS_TITLE: &str = "Модели";
pub const MODEL_SELECTED: &str = "Выбрана модель";
pub const MODEL_CURRENT: &str = "текущая";
pub const CONTEXT_WINDOW: &str = "Окно контекста";
pub const UNAVAILABLE: &str = "недоступна";

pub const RESUME_TITLE: &str = "Диалоги этой папки";
pub const TAB_RECENT: &str = "Недавние";
pub const TAB_PINNED: &str = "Закреплённые";
pub const RESUMED: &str = "Диалог продолжен";
pub const UPDATED: &str = "Изменён";
pub const NO_CONVERSATIONS: &str = "В этой папке ещё нет диалогов";

pub const CONTEXT_TITLE: &str = "Контекст";
pub const TAB_SUMMARY: &str = "Сводка";
pub const TAB_TOOLS: &str = "Инструменты";
pub const CONTEXT_USED: &str = "Занято";
pub const CONTEXT_OF: &str = "из";
pub const CONTEXT_SYSTEM: &str = "Системный промпт";
pub const CONTEXT_TOOLS: &str = "Описания инструментов";
pub const CONTEXT_HISTORY: &str = "История";
pub const CONTEXT_RESERVED: &str = "Резерв для ответа";
pub const CONTEXT_FREE: &str = "Свободно";
pub const CONTEXT_TRIMMED: &str = "Сокращено сообщений";
pub const LIMIT_USED: &str = "использовано";
pub const LIMIT_RESETS: &str = "Обновится";
pub const CONTEXT_NEEDS_MODEL: &str = "Выберите модель через /models";

pub const SKILLS_TITLE: &str = "Навыки";
pub const PROCESSES_TITLE: &str = "Фоновые процессы";
pub const PROCESS_OUTPUT_TITLE: &str = "Детали процесса";
pub const PROCESS_STATUS: &str = "Статус";
pub const PROCESS_RUNTIME: &str = "Время работы";
pub const PROCESS_COMMAND: &str = "Команда";
pub const PROCESS_FOLDER: &str = "Папка";
pub const PROCESS_EXIT_CODE: &str = "Код выхода";
pub const PROCESS_OUTPUT_HEADING: &str = "Вывод";
pub const TAB_RUNNING: &str = "Работают";
pub const PROCESSES_EMPTY: &str = "Фоновых процессов нет";
pub const PROCESS_OUTPUT_EMPTY: &str = "Вывода нет";
pub const PROCESS_STOP_CONFIRM_KEY: &str = "Нажмите x ещё раз, чтобы остановить процесс";
pub const PROCESSES_NO_CONVERSATION: &str = "Фоновые процессы появляются в диалоге";
pub const PROCESS_STOP_CONFIRM: &str = "Нажмите Delete ещё раз, чтобы остановить процесс";
pub const PROCESS_STOPPED: &str = "Процесс остановлен";
pub const PROCESS_RUNNING: &str = "выполняется";
pub const PROCESS_STOPPED_STATE: &str = "остановлен";
pub const PROCESS_FAILED: &str = "ошибка";
pub const PROCESS_TIMED_OUT: &str = "превышено время";
pub const PROCESS_EXITED: &str = "завершён";
pub const PROCESS_IN_BACKGROUND: &str = "в фоне";
pub const PROCESS_ENDED_EVENT: &str = "Фоновый процесс завершён";
pub const PROCESS_NO_CODE: &str = "нет";

pub fn process_event(event: &zvs_contract::ChatEventDto) -> String {
    let lines: Vec<String> = event
        .shells
        .iter()
        .map(|ended| {
            let code = ended
                .shell
                .exit_code
                .map_or_else(|| PROCESS_NO_CODE.to_owned(), |code| code.to_string());
            format!(
                "{PROCESS_ENDED_EVENT}: {} · код выхода {code}",
                ended.shell.argv.join(" ")
            )
        })
        .collect();
    lines.join("\n")
}
pub const PROCESS_TRUNCATED: &str = "Начало вывода больше не хранится";
pub const HINT_PROCESSES: &str =
    "↑/↓ — выбор · ←/→ — вкладки · Enter — вывод · Delete — остановить · Esc — закрыть";
pub const HINT_PROCESS_OUTPUT: &str =
    "← — назад · Esc/Enter/Space — закрыть · x — остановить · ↑/↓ — прокрутка · End — следить";
pub const TAB_ENABLED: &str = "Включённые";
pub const TAB_DISABLED: &str = "Выключенные";
pub const SKILL_ON: &str = "включён";
pub const SKILL_OFF: &str = "выключен";
pub const SKILL_INVALID: &str = "содержит ошибки";
pub const SKILL_ENABLED: &str = "Навык включён";
pub const SKILL_DISABLED: &str = "Навык выключен";
pub const SOURCE_LOCAL: &str = "локальный";
pub const SOURCE_IMPORTED: &str = "импортированный";
pub const SOURCE_BUNDLED: &str = "встроенный";

pub const MCP_TITLE: &str = "MCP-серверы";
pub const TAB_CONNECTED: &str = "Подключённые";
pub const TAB_STOPPED: &str = "Остановленные";
pub const MCP_DISABLED: &str = "выключен";
pub const MCP_STOPPED: &str = "остановлен";
pub const MCP_STARTING: &str = "запуск";
pub const MCP_CONNECTED: &str = "подключён";
pub const MCP_FAILED: &str = "ошибка";
pub const MCP_STARTED: &str = "Сервер запущен";
pub const MCP_HALTED: &str = "Сервер остановлен";
pub const MCP_SIGN_IN_REQUIRED: &str = "требуется вход";
pub const MCP_SIGN_IN_HINT: &str = "Enter — войти через браузер";
pub const MCP_EMPTY: &str = "Серверов нет. Подключите сервер командой /mcp add";
pub const MCP_SERVER: &str = "Сервер";
pub const MCP_ADDING: &str = "Подключение сервера";
pub const MCP_STARTING_SERVER: &str = "Запуск сервера";
pub const MCP_NOT_STARTED: &str = "добавлен, но не запущен";
pub const MCP_SIGNING_IN: &str = "Вход на сервер";
pub const MCP_IN_BROWSER: &str = "в браузере";
pub const MCP_SIGNED_IN: &str = "Вход выполнен";
pub const MCP_SIGN_IN_CANCEL: &str = "Ctrl+C — отменить";
pub const MCP_SIGN_IN_CANCELLED: &str = "Вход отменён";
pub const MCP_ADD_USAGE: &str = "Формат: /mcp add [-e КЛЮЧ=значение] [-H \"Имя: значение\"] <имя> <адрес | команда [аргументы]>";
pub const MCP_ADD_UNKNOWN: &str = "Неизвестная подкоманда /mcp. Доступно: /mcp, /mcp add";
pub const MCP_ADD_QUOTE: &str = "Не закрыта кавычка";
pub const MCP_INSPECT_USAGE: &str = "Формат: /mcp inspect <имя>";
pub const MCP_NOT_FOUND: &str = "Сервер не найден";
pub const THEME_NOT_FOUND: &str = "Тема не найдена";
pub const RESUME_NOT_FOUND: &str = "Диалог не найден";
pub const MCP_ADDRESS: &str = "Адрес";
pub const MCP_COMMAND: &str = "Команда";
pub const MCP_NAMESPACE: &str = "Пространство имён";
pub const MCP_REPORTED_AS: &str = "Сервер сообщает";
pub const MCP_PROTOCOL: &str = "Протокол";
pub const MCP_STARTUP: &str = "Запуск";
pub const MCP_STARTUP_LAUNCH: &str = "вместе с приложением";
pub const MCP_STARTUP_MANUAL: &str = "вручную";
pub const MCP_SIGN_IN: &str = "Вход";
pub const MCP_SIGN_IN_NONE: &str = "не требуется";
pub const MCP_SIGN_IN_DONE: &str = "выполнен";
pub const MCP_ENV: &str = "Переменные окружения";
pub const MCP_HEADERS: &str = "Заголовки";
pub const MCP_SECRET: &str = "секрет";
pub const MCP_LAST_CONNECTED: &str = "Последнее подключение";
pub const MCP_ERROR: &str = "Ошибка";
pub const MCP_INSTRUCTIONS: &str = "Инструкции сервера";
pub const MCP_TOOLS: &str = "Инструменты";
pub const MCP_RESOURCES: &str = "Ресурсы";
pub const MCP_PROMPTS: &str = "Промпты";
pub const MCP_READ_ONLY: &str = "только чтение";
pub const MCP_DESTRUCTIVE: &str = "изменяет данные";
pub const MCP_EXPAND: &str = "показать";
pub const MCP_COLLAPSE: &str = "скрыть";
pub const MCP_ADD_ENV: &str = "Переменная окружения задаётся как КЛЮЧ=значение";
pub const MCP_ADD_HEADER: &str = "Заголовок задаётся как «Имя: значение»";
pub const MCP_ADD_HTTP_EXTRA: &str =
    "Для http-сервера не указываются аргументы и переменные окружения";
pub const MCP_ADD_STDIO_HEADERS: &str = "Заголовки указываются только для http-сервера";

pub const PERMISSIONS_TITLE: &str = "Подтверждение действий";
pub const TAB_MODE: &str = "Режим";
pub const TAB_JUDGE: &str = "Модель-арбитр";
pub const JUDGE_NONE: &str = "Не выбрана";
pub const MODE_SET: &str = "Режим подтверждения изменён";
pub const JUDGE_SET: &str = "Модель-арбитр изменена";
pub const MODE_CURRENT: &str = "текущий";
pub const NO_RESTRICTIONS_CONFIRM: &str = "Модель сможет удалять и перезаписывать файлы, отправлять формы и запускать программы без подтверждения. Нажмите Enter ещё раз, чтобы отключить подтверждения.";
pub const MODE_HINT_ALWAYS_ASK: &str =
    "Инструменты с уровнем ask запрашивают подтверждение перед каждым вызовом.";
pub const MODE_HINT_SAFETY_ASK: &str = "Подтверждение требуется для удаления и перезаписи файлов, отправки форм, загрузок и запуска кода.";
pub const MODE_HINT_AUTO_BYPASS: &str = "Опасные действия с файлами и запуск кода оценивают правила и модель-арбитр. Если решение не принято, запрашивается подтверждение.";
pub const MODE_HINT_NO_RESTRICTIONS: &str = "Подтверждения не запрашиваются. Отключённые инструменты, запрещённые папки и поля оплаты остаются недоступны.";
pub const JUDGE_HINT: &str = "Оценивает опасные действия в режиме «Автопроверка».";
