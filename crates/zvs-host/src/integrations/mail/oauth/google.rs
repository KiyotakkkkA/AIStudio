use super::MailPreset;

pub const PRESET: MailPreset = MailPreset {
    key: "google",
    label: "Google (OAuth)",
    imap_host: "imap.gmail.com",
    imap_port: 993,
    smtp_host: "smtp.gmail.com",
    smtp_port: 465,
    authorize_endpoint: "https://accounts.google.com/o/oauth2/v2/auth",
    token_endpoint: "https://oauth2.googleapis.com/token",
    scope: "https://mail.google.com/",
    authorize_params: &[("access_type", "offline"), ("prompt", "consent")],
};
