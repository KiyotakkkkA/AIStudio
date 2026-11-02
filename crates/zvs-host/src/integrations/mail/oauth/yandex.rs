use super::MailPreset;

pub const PRESET: MailPreset = MailPreset {
    key: "yandex",
    label: "Яндекс (OAuth)",
    imap_host: "imap.yandex.com",
    imap_port: 993,
    smtp_host: "smtp.yandex.com",
    smtp_port: 465,
    authorize_endpoint: "https://oauth.yandex.com/authorize",
    token_endpoint: "https://oauth.yandex.com/token",
    scope: "mail:imap_full mail:smtp",
    authorize_params: &[("force_confirm", "yes")],
};
