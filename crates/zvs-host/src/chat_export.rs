use std::path::PathBuf;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, ChatToolCallStatus, ConversationDetailDto, ConversationExportDto,
    ConversationExportFormat, ExportConversationInput, MessageDto, MessageRole, Timestamp,
};

use crate::platform::clock::{Clock, SystemClock};
use crate::platform::paths::utc_iso;
use crate::services::Services;

pub const EXPORT_FORMAT_ID: &str = "zvs-ai-studio.conversation";
pub const EXPORT_VERSION: u64 = 1;
pub const EXPORT_FAILED: &str = "Не удалось экспортировать диалог";
pub const SAVE_DIALOG_UNAVAILABLE: &str = "Диалог сохранения файла недоступен";
const FILE_STEM_MAX: usize = 80;
const FALLBACK_STEM: &str = "Диалог";

const fn extension(format: ConversationExportFormat) -> &'static str {
    match format {
        ConversationExportFormat::Json => "json",
        ConversationExportFormat::Markdown => "md",
    }
}

pub fn file_name(title: &str, format: ConversationExportFormat) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                ' '
            } else {
                character
            }
        })
        .collect();
    let stem: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(FILE_STEM_MAX)
        .collect();
    let stem = stem.trim_end_matches(['.', ' ']);
    let stem = if stem.is_empty() { FALLBACK_STEM } else { stem };
    format!("{stem}.{}", extension(format))
}

fn moment(at: Timestamp) -> String {
    let iso = utc_iso(at);
    format!("{} {} UTC", &iso[..10], &iso[11..16])
}

pub fn render_json(detail: &ConversationDetailDto, exported_at: Timestamp) -> String {
    let document = json!({
        "format": EXPORT_FORMAT_ID,
        "version": EXPORT_VERSION,
        "exportedAt": utc_iso(exported_at),
        "conversation": detail.conversation,
        "messages": detail.messages,
    });
    let mut text = serde_json::to_string_pretty(&document).unwrap_or_default();
    text.push('\n');
    text
}

const fn tool_status(status: ChatToolCallStatus) -> &'static str {
    match status {
        ChatToolCallStatus::Running => "выполняется",
        ChatToolCallStatus::AwaitingApproval => "ждёт подтверждения",
        ChatToolCallStatus::AwaitingAnswer => "ждёт ответа",
        ChatToolCallStatus::Succeeded => "выполнен",
        ChatToolCallStatus::Failed => "ошибка",
        ChatToolCallStatus::Refused => "запрещён",
        ChatToolCallStatus::Denied => "отклонён",
        ChatToolCallStatus::Cancelled => "отменён",
    }
}

fn quote(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn message_markdown(message: &MessageDto) -> String {
    let mut parts = vec![match message.role {
        MessageRole::User => "## Вы".to_owned(),
        MessageRole::Assistant => "## Ассистент".to_owned(),
    }];
    let reasoning = message.reasoning.trim();
    if !reasoning.is_empty() {
        parts.push(format!(
            "<details>\n<summary>Рассуждение</summary>\n\n{reasoning}\n\n</details>"
        ));
    }
    for selection in &message.selections {
        parts.push(quote(selection.text.trim()));
    }
    let content = message.content.trim();
    if !content.is_empty() {
        parts.push(content.to_owned());
    }
    if !message.tool_calls.is_empty() {
        parts.push(
            message
                .tool_calls
                .iter()
                .map(|call| {
                    format!(
                        "- Инструмент `{}` — {}",
                        call.name,
                        tool_status(call.status)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if !message.web_sources.is_empty() {
        parts.push(
            std::iter::once("Источники:".to_owned())
                .chain(message.web_sources.iter().map(|source| {
                    let title = if source.title.trim().is_empty() {
                        source.url.as_str()
                    } else {
                        source.title.trim()
                    };
                    format!("- [{title}]({})", source.url)
                }))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    parts.join("\n\n")
}

pub fn render_markdown(detail: &ConversationDetailDto, exported_at: Timestamp) -> String {
    let conversation = &detail.conversation;
    let mut facts = vec![
        format!("- Модель: {}", conversation.model_id),
        format!("- Создан: {}", moment(conversation.created_at)),
        format!("- Изменён: {}", moment(conversation.updated_at)),
    ];
    if let Some(folder) = &conversation.working_folder {
        facts.push(format!("- Рабочая папка: `{folder}`"));
    }
    facts.push(format!("- Экспортирован: {}", moment(exported_at)));
    let mut sections = vec![format!("# {}", conversation.title.trim()), facts.join("\n")];
    sections.extend(
        detail
            .messages
            .iter()
            .filter(|message| !message.partial)
            .map(message_markdown),
    );
    let mut text = sections.join("\n\n");
    text.push('\n');
    text
}

pub fn render(
    detail: &ConversationDetailDto,
    format: ConversationExportFormat,
    exported_at: Timestamp,
) -> String {
    match format {
        ConversationExportFormat::Json => render_json(detail, exported_at),
        ConversationExportFormat::Markdown => render_markdown(detail, exported_at),
    }
}

fn write_failure(error: &std::io::Error) -> AppError {
    let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
        AppErrorCode::PermissionDenied
    } else {
        AppErrorCode::Unknown
    };
    AppError::new(code, EXPORT_FAILED).with_detail("raw", error.to_string())
}

pub async fn export(
    services: &Services,
    input: &ExportConversationInput,
) -> Result<ConversationExportDto, AppError> {
    let detail = services.chat.get(&input.id)?;
    let picker = services
        .picker
        .clone()
        .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, SAVE_DIALOG_UNAVAILABLE))?;
    let suggested = file_name(&detail.conversation.title, input.format);
    let Some(target) = picker.save_as(&suggested).await? else {
        return Ok(ConversationExportDto::default());
    };
    let text = render(&detail, input.format, SystemClock.now());
    let destination = PathBuf::from(&target);
    tokio::task::spawn_blocking(move || std::fs::write(destination, text))
        .await
        .map_err(|error| {
            AppError::new(AppErrorCode::Unknown, EXPORT_FAILED)
                .with_detail("raw", error.to_string())
        })?
        .map_err(|error| write_failure(&error))?;
    Ok(ConversationExportDto { path: Some(target) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_drops_characters_windows_forbids_and_falls_back_when_empty() {
        assert_eq!(
            file_name(
                "План: релиз 2.0 / итоги?",
                ConversationExportFormat::Markdown
            ),
            "План релиз 2.0 итоги.md"
        );
        assert_eq!(
            file_name("  ...  ", ConversationExportFormat::Json),
            "Диалог.json"
        );
        let long = "а".repeat(200);
        let name = file_name(&long, ConversationExportFormat::Json);
        assert_eq!(name.chars().count(), FILE_STEM_MAX + ".json".len());
    }

    #[test]
    fn a_moment_is_a_readable_utc_minute() {
        assert_eq!(moment(1_700_000_000_000), "2023-11-14 22:13 UTC");
    }
}
