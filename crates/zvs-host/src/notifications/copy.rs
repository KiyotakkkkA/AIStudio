pub const APPROVAL_TITLE: &str = "Требуется подтверждение";
pub const QUESTION_TITLE: &str = "Требуется ответ";
pub const TURN_DONE_TITLE: &str = "Ответ готов";
pub const TURN_FAILED_TITLE: &str = "Ответ не получен";
pub const DOWNLOAD_DONE_TITLE: &str = "Загрузка завершена";
pub const DOWNLOAD_FAILED_TITLE: &str = "Загрузка не удалась";
pub const AUTOSTART_FAILED: &str = "Не удалось изменить автозапуск.";

pub const BODY_MAX_CHARS: usize = 120;

pub fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= BODY_MAX_CHARS {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(BODY_MAX_CHARS - 1).collect();
    clipped.push('…');
    clipped
}

pub fn question_body(question: &str, conversation: &str) -> String {
    clip(&format!("«{question}» в диалоге «{conversation}»"))
}

pub fn approval_body(tool: Option<&str>, conversation: &str) -> String {
    match tool {
        Some(tool) => clip(&format!("«{tool}» в диалоге «{conversation}»")),
        None => clip(&format!("Диалог «{conversation}»")),
    }
}
