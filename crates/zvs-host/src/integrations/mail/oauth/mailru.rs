use super::MailPreset;

pub const PRESET: MailPreset = MailPreset {
    key: "mailru",
    label: "Mail.ru (OAuth)",
    imap_host: "imap.mail.ru",
    imap_port: 993,
    smtp_host: "smtp.mail.ru",
    smtp_port: 465,
    authorize_endpoint: "https://o2.mail.ru/login",
    token_endpoint: "https://o2.mail.ru/token",
    scope: "userinfo mail.imap",
    authorize_params: &[],
};
