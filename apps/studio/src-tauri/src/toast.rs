use std::sync::{Arc, OnceLock};

use zvs_contract::NotificationTargetDto;
use zvs_host::platform::shell::NotificationDraft;

pub type Activation = Arc<dyn Fn(NotificationTargetDto) + Send + Sync>;
pub type ActivationSlot = Arc<OnceLock<Activation>>;

pub trait Toaster: Send + Sync {
    fn show(&self, draft: NotificationDraft);

    fn clear(&self, target: NotificationTargetDto);
}

#[cfg(not(windows))]
pub struct NoToaster;

#[cfg(not(windows))]
impl Toaster for NoToaster {
    fn show(&self, _draft: NotificationDraft) {}

    fn clear(&self, _target: NotificationTargetDto) {}
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            character if character.is_control() => escaped.push(' '),
            character => escaped.push(character),
        }
    }
    escaped
}

pub fn toast_xml(draft: &NotificationDraft) -> String {
    format!(
        "<toast launch=\"{}\" activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(&draft.target.to_url()),
        escape(&draft.title),
        escape(&draft.body),
    )
}

#[cfg(test)]
mod tests {
    use zvs_contract::ConversationId;
    use zvs_host::platform::shell::NotificationKind;

    use super::*;

    fn draft(title: &str, body: &str) -> NotificationDraft {
        NotificationDraft {
            kind: NotificationKind::Turn,
            tag: "0199aa11-1111-7111-8111-111111111111".to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            target: NotificationTargetDto::Conversation {
                conversation_id: ConversationId::parse("0199aa11-1111-7111-8111-111111111111")
                    .expect("uuid"),
            },
            silent: false,
        }
    }

    #[test]
    fn the_target_travels_as_the_launch_url() {
        assert_eq!(
            toast_xml(&draft("Ответ готов", "Диалог")),
            "<toast launch=\"zvs-studio://conversation/0199aa11-1111-7111-8111-111111111111\" activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text>Ответ готов</text><text>Диалог</text></binding></visual></toast>"
        );
    }

    #[test]
    fn markup_in_titles_and_bodies_is_escaped_as_text() {
        let xml = toast_xml(&draft("<b>&\"x\"</b>", "a'b\u{0007}c</text><text>"));
        assert!(xml.contains("<text>&lt;b&gt;&amp;&quot;x&quot;&lt;/b&gt;</text>"));
        assert!(xml.contains("<text>a&apos;b c&lt;/text&gt;&lt;text&gt;</text>"));
        assert_eq!(xml.matches("<text>").count(), 2);
    }
}
