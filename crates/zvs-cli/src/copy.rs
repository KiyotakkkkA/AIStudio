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

pub const ACCESS_TITLE: &str = "Доступ к папке";
pub const ACCESS_BODY: &str =
    "Диалоги этой сессии будут работать с файлами в папке. Разрешить доступ?";
pub const ACCESS_CONVERSATION: &str = "Разрешить для этого диалога";
pub const ACCESS_ALWAYS: &str = "Разрешить всегда";
pub const ACCESS_DENY: &str = "Отказать и выйти";
pub const ACCESS_DENIED: &str = "Доступ к папке не предоставлен. Сессия завершена.";

pub const WELCOME_HINT: &str = "Введите сообщение или / для списка команд";
pub const INPUT_PLACEHOLDER: &str = "Сообщение…  (/ — команды, Shift+Enter — новая строка)";
pub const WORKSPACE: &str = "Рабочая папка";
pub const MODEL: &str = "Модель";
pub const NO_MODEL: &str = "не выбрана";
pub const NO_PROVIDER: &str =
    "Нет включённого провайдера с текстовыми моделями. Добавьте его в приложении.";

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
    "Enter — отправить · Esc Esc — выход · Ctrl+O — подробности · PgUp/PgDn — прокрутка";
pub const HINT_RUNNING: &str = "Esc — остановить · Ctrl+O — подробности · PgUp/PgDn — прокрутка";
pub const HINT_PROMPT: &str = "↑/↓ — выбор · Enter — подтвердить · Esc — отклонить";
pub const HINT_PALETTE: &str = "↑/↓ — выбор · Tab — дополнить · Enter — выполнить · Esc — закрыть";
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
pub const NOT_YET: &str = "Команда станет доступна в следующей версии";
pub const BUSY: &str = "Дождитесь окончания ответа или остановите его клавишей Esc";

pub const CMD_HELP: &str = "Список команд и клавиш";
pub const CMD_NEW: &str = "Начать новый диалог в этой папке";
pub const CMD_CLEAR: &str = "Очистить экран";
pub const CMD_EXIT: &str = "Завершить сессию";
pub const CMD_MODELS: &str = "Провайдеры и модели";
pub const CMD_RESUME: &str = "Продолжить диалог этой папки";
pub const CMD_CONTEXT: &str = "Заполнение контекста";
pub const CMD_SKILLS: &str = "Навыки";
pub const CMD_MCP: &str = "MCP-серверы";
pub const CMD_PERMISSIONS: &str = "Режим выполнения и модель-арбитр";

pub const KEYS_TITLE: &str = "Клавиши";
pub const KEYS: &[(&str, &str)] = &[
    ("Enter", "отправить сообщение"),
    ("Shift+Enter, Ctrl+J", "новая строка"),
    ("↑ / ↓", "история ввода"),
    ("Esc", "остановить ответ"),
    ("Esc Esc", "выйти"),
    ("Ctrl+O", "показать или скрыть подробности инструментов"),
    ("PgUp / PgDn, колесо", "прокрутка"),
    ("Ctrl+C", "выйти"),
];

pub const MODE: &str = "Подтверждение действий";
pub const MODE_ALWAYS_ASK: &str = "Всегда спрашивать";
pub const MODE_SAFETY_ASK: &str = "Только опасные действия";
pub const MODE_AUTO_BYPASS: &str = "Автопроверка";
pub const MODE_NO_RESTRICTIONS: &str = "Без ограничений";
pub const CONTEXT: &str = "Контекст";
pub const RECOMMENDED: &str = "рекомендуется";
pub const QUESTION_PROGRESS: &str = "из";
