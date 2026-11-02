use mail_parser::{MessageParser, TextList};
use zvs_contract::validate::js_slice;
use zvs_contract::{
    MailAttachmentDto, MailBodyDto, MailMessageDto, MailRefDto, Timestamp, UntrustedTextDto,
};

pub const SNIPPET_UNITS: usize = 300;
pub const HEADER_UNITS: usize = 2000;
pub const ADDRESS_UNITS: usize = 320;
pub const BODY_UNITS: usize = 100_000;
pub const EVENT_ID_UNITS: usize = 512;
const RECIPIENTS: usize = 50;
const REFERENCES: usize = 100;
const ATTACHMENTS: usize = 100;

pub struct Fetched {
    pub reference: MailRefDto,
    pub raw: Vec<u8>,
    pub size: u64,
    pub seen: bool,
    pub internal_date: Option<Timestamp>,
}

pub fn clip(text: &str, units: usize) -> (String, bool) {
    let clipped = js_slice(text, units);
    (clipped.to_owned(), clipped.len() < text.len())
}

fn clipped(text: &str, units: usize) -> String {
    clip(text, units).0
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn bracketed(id: &str) -> Option<String> {
    let trimmed = id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(clipped(&format!("<{trimmed}>"), HEADER_UNITS))
    }
}

fn ids(list: Option<TextList<'_>>, keep: usize) -> Vec<String> {
    let Some(list) = list else {
        return Vec::new();
    };
    let all: Vec<String> = (0..list.len())
        .filter_map(|index| list.get(index))
        .filter_map(bracketed)
        .collect();
    let skip = all.len().saturating_sub(keep);
    all.into_iter().skip(skip).collect()
}

fn address(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| clipped(value, ADDRESS_UNITS))
}

fn timestamp(seconds: i64) -> Option<Timestamp> {
    seconds.checked_mul(1000).filter(|millis| *millis >= 0)
}

fn untrusted(text: &str, units: usize) -> UntrustedTextDto {
    UntrustedTextDto::new(clipped(text, units))
}

pub fn summary(fetched: &Fetched) -> MailMessageDto {
    let parsed = MessageParser::default().parse(&fetched.raw);
    let Some(message) = parsed.as_ref() else {
        return MailMessageDto {
            untrusted: true,
            reference: fetched.reference.clone(),
            message_id: None,
            from: UntrustedTextDto::new(""),
            from_address: None,
            to: Vec::new(),
            subject: UntrustedTextDto::new(""),
            date: fetched.internal_date,
            in_reply_to: None,
            references: Vec::new(),
            snippet: UntrustedTextDto::new(""),
            attachments: Vec::new(),
            size: fetched.size,
            seen: fetched.seen,
        };
    };
    let sender = message.from().and_then(|from| from.first());
    let from_address = sender
        .as_ref()
        .and_then(|mailbox| address(mailbox.address()));
    let from = match (
        sender.as_ref().and_then(|mailbox| mailbox.name()),
        &from_address,
    ) {
        (Some(name), Some(address)) => format!("{name} <{address}>"),
        (Some(name), None) => name.to_owned(),
        (None, Some(address)) => address.clone(),
        (None, None) => String::new(),
    };
    let to = message
        .to()
        .map(|list| {
            list.mailboxes()
                .filter_map(|mailbox| address(mailbox.address()))
                .take(RECIPIENTS)
                .collect()
        })
        .unwrap_or_default();
    let snippet = message
        .body_preview(SNIPPET_UNITS * 2)
        .map(|preview| collapse(&preview))
        .unwrap_or_default();
    let attachments = message
        .attachments()
        .take(ATTACHMENTS)
        .map(|part| MailAttachmentDto {
            name: untrusted(part.attachment_name().unwrap_or_default(), HEADER_UNITS),
            content_type: part
                .content_type()
                .map(|kind| match kind.subtype() {
                    Some(subtype) => format!("{}/{subtype}", kind.ctype()),
                    None => kind.ctype().to_owned(),
                })
                .map_or_else(
                    || "application/octet-stream".to_owned(),
                    |kind| clipped(&kind, HEADER_UNITS),
                ),
            size: part.decoded_len() as u64,
        })
        .collect();
    MailMessageDto {
        untrusted: true,
        reference: fetched.reference.clone(),
        message_id: message.message_id().and_then(bracketed),
        from: untrusted(&from, HEADER_UNITS),
        from_address,
        to,
        subject: untrusted(
            &collapse(message.subject().unwrap_or_default()),
            HEADER_UNITS,
        ),
        date: message
            .date()
            .and_then(|date| timestamp(date.to_timestamp()))
            .or(fetched.internal_date),
        in_reply_to: ids(message.in_reply_to(), 1).into_iter().next_back(),
        references: ids(message.references(), REFERENCES),
        snippet: untrusted(&snippet, SNIPPET_UNITS),
        attachments,
        size: fetched.size,
        seen: fetched.seen,
    }
}

pub fn body(fetched: &Fetched) -> MailBodyDto {
    let text = MessageParser::default()
        .parse(&fetched.raw)
        .and_then(|message| message.body_text(0).map(|text| text.into_owned()))
        .unwrap_or_default();
    let (text, truncated) = clip(&text, BODY_UNITS);
    MailBodyDto {
        untrusted: true,
        message: summary(fetched),
        text: UntrustedTextDto::new(text),
        truncated,
    }
}

pub fn event_id(message: &MailMessageDto) -> String {
    let reference = &message.reference;
    let id = match &message.message_id {
        Some(id) => format!("{}:{id}", reference.folder),
        None => format!(
            "{}:uid:{}:{}",
            reference.folder, reference.uid_validity, reference.uid
        ),
    };
    clipped(&id, EVENT_ID_UNITS)
}

pub fn reply_subject(original: &str) -> String {
    let trimmed = original.trim();
    let lowered = trimmed.to_lowercase();
    if lowered.starts_with("re:") || lowered.starts_with("ответ:") {
        trimmed.to_owned()
    } else if trimmed.is_empty() {
        "Re:".to_owned()
    } else {
        format!("Re: {trimmed}")
    }
}

pub fn thread_references(original: &MailMessageDto) -> Vec<String> {
    let mut references = original.references.clone();
    if references.is_empty()
        && let Some(parent) = &original.in_reply_to
    {
        references.push(parent.clone());
    }
    if let Some(id) = &original.message_id
        && !references.contains(id)
    {
        references.push(id.clone());
    }
    if references.len() > REFERENCES {
        let first = references[0].clone();
        let tail = references.split_off(references.len() - (REFERENCES - 1));
        references = std::iter::once(first).chain(tail).collect();
    }
    references
}
