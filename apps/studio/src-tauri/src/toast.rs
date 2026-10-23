use std::sync::{Arc, OnceLock};

use zvs_contract::{NOTIFICATION_SCHEME, NOTIFICATION_URL_MAX, NotificationTargetDto};
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

pub fn launch_url<S: AsRef<str>>(args: &[S]) -> Option<&str> {
    args.iter().skip(1).map(AsRef::as_ref).find(|arg| {
        arg.strip_prefix(NOTIFICATION_SCHEME)
            .is_some_and(|rest| rest.starts_with("://"))
    })
}

pub fn launch_target(url: &str) -> Option<NotificationTargetDto> {
    if url.len() > NOTIFICATION_URL_MAX {
        return None;
    }
    NotificationTargetDto::from_url(url)
}

pub fn toast_xml(draft: &NotificationDraft) -> String {
    format!(
        "<toast launch=\"{}\" activationType=\"protocol\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
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
    fn the_target_travels_as_a_protocol_launch_url() {
        assert_eq!(
            toast_xml(&draft("Ответ готов", "Диалог")),
            "<toast launch=\"zvs-studio://conversation/0199aa11-1111-7111-8111-111111111111\" activationType=\"protocol\"><visual><binding template=\"ToastGeneric\"><text>Ответ готов</text><text>Диалог</text></binding></visual></toast>"
        );
    }

    #[test]
    fn only_the_first_scheme_argument_is_considered() {
        let first = "zvs-studio://conversation/0199aa11-1111-7111-8111-111111111111";
        let args = [
            "zvs-studio.exe",
            "--autostart",
            "https://zvs-studio://downloads",
            first,
            "zvs-studio://downloads",
        ];
        assert_eq!(launch_url(&args), Some(first));
        assert_eq!(launch_url(&["zvs-studio://downloads"]), None);
        assert_eq!(launch_url(&["zvs-studio.exe", "--autostart"]), None);
        assert_eq!(
            launch_url(&args).and_then(launch_target),
            Some(draft("", "").target)
        );
    }

    #[test]
    fn long_or_malformed_launch_urls_are_dropped() {
        let long = format!("zvs-studio://downloads{}", "/".repeat(300));
        for url in [
            long.as_str(),
            "zvs-studio://evil/x",
            "zvs-studio://conversation/does-not-exist",
            "zvs-studio://downloads?run=1",
        ] {
            let args = ["zvs-studio.exe", url, "zvs-studio://downloads"];
            assert_eq!(launch_url(&args), Some(url));
            assert_eq!(launch_url(&args).and_then(launch_target), None);
        }
    }

    #[test]
    fn markup_in_titles_and_bodies_is_escaped_as_text() {
        let xml = toast_xml(&draft("<b>&\"x\"</b>", "a'b\u{0007}c</text><text>"));
        assert!(xml.contains("<text>&lt;b&gt;&amp;&quot;x&quot;&lt;/b&gt;</text>"));
        assert!(xml.contains("<text>a&apos;b c&lt;/text&gt;&lt;text&gt;</text>"));
        assert_eq!(xml.matches("<text>").count(), 2);
    }
}
