use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio_rustls::TlsAcceptor;

pub const CA: &[u8] = include_bytes!("fixtures/ca.der");
pub const OTHER_CA: &[u8] = include_bytes!("fixtures/other-ca.der");
const CERT: &[u8] = include_bytes!("fixtures/server.der");
const KEY: &[u8] = include_bytes!("fixtures/server.key.der");

pub const USER: &str = "support@example.org";
pub const PASSWORD: &str = "app-password";
pub const BEARER: &str = "ya29.access";

#[derive(Clone)]
pub struct StoredMessage {
    pub uid: u32,
    pub raw: Vec<u8>,
    pub seen: bool,
    pub internal: String,
}

pub struct Folder {
    pub uid_validity: u32,
    pub next_uid: u32,
    pub messages: Vec<StoredMessage>,
}

#[derive(Default)]
pub struct Store {
    pub folders: BTreeMap<String, Folder>,
    pub sent: Vec<String>,
    pub logins: Vec<String>,
    pub idles: usize,
}

#[derive(Clone)]
pub struct MailServer {
    pub imap: SocketAddr,
    pub smtp: SocketAddr,
    pub store: Arc<Mutex<Store>>,
    pub arrived: Arc<Notify>,
}

fn acceptor() -> TlsAcceptor {
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(CERT.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
            )
            .expect("a server certificate");
    TlsAcceptor::from(Arc::new(config))
}

pub fn roots() -> Vec<CertificateDer<'static>> {
    vec![CertificateDer::from(CA.to_vec())]
}

pub fn other_roots() -> Vec<CertificateDer<'static>> {
    vec![CertificateDer::from(OTHER_CA.to_vec())]
}

impl MailServer {
    pub async fn start() -> Self {
        let store = Arc::new(Mutex::new(Store::default()));
        store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .folders
            .insert(
                "INBOX".to_owned(),
                Folder {
                    uid_validity: 7,
                    next_uid: 1,
                    messages: Vec::new(),
                },
            );
        let arrived = Arc::new(Notify::new());
        let imap = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an imap port");
        let smtp = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an smtp port");
        let server = Self {
            imap: imap.local_addr().expect("an address"),
            smtp: smtp.local_addr().expect("an address"),
            store,
            arrived,
        };
        let tls = acceptor();
        {
            let server = server.clone();
            let tls = tls.clone();
            tokio::spawn(async move {
                while let Ok((stream, _)) = imap.accept().await {
                    let server = server.clone();
                    let tls = tls.clone();
                    tokio::spawn(async move {
                        if let Ok(stream) = tls.accept(stream).await {
                            server.imap_session(stream).await;
                        }
                    });
                }
            });
        }
        {
            let server = server.clone();
            tokio::spawn(async move {
                while let Ok((stream, _)) = smtp.accept().await {
                    let server = server.clone();
                    let tls = tls.clone();
                    tokio::spawn(async move {
                        if let Ok(stream) = tls.accept(stream).await {
                            server.smtp_session(stream).await;
                        }
                    });
                }
            });
        }
        server
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn deliver(&self, folder: &str, raw: &str) -> u32 {
        let uid = {
            let mut store = self.lock();
            let folder = store.folders.get_mut(folder).expect("a folder");
            let uid = folder.next_uid;
            folder.next_uid += 1;
            folder.messages.push(StoredMessage {
                uid,
                raw: raw.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes(),
                seen: false,
                internal: "02-Nov-2026 10:00:00 +0000".to_owned(),
            });
            uid
        };
        self.arrived.notify_waiters();
        uid
    }

    pub fn seen(&self, folder: &str, uid: u32) -> bool {
        self.lock().folders[folder]
            .messages
            .iter()
            .any(|message| message.uid == uid && message.seen)
    }

    pub fn sent(&self) -> Vec<String> {
        self.lock().sent.clone()
    }

    pub fn logins(&self) -> Vec<String> {
        self.lock().logins.clone()
    }

    pub fn idles(&self) -> usize {
        self.lock().idles
    }

    async fn imap_session<S>(&self, stream: S)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (read, mut write) = tokio::io::split(stream);
        let mut reader = BufReader::new(read);
        let _ = write
            .write_all(b"* OK [CAPABILITY IMAP4rev1 IDLE AUTH=PLAIN AUTH=XOAUTH2] ready\r\n")
            .await;
        let mut selected: Option<String> = None;
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }
            let trimmed = line.trim_end().to_owned();
            let (tag, rest) = trimmed.split_once(' ').unwrap_or((&trimmed, ""));
            let (command, args) = rest.split_once(' ').unwrap_or((rest, ""));
            let command = command.to_ascii_uppercase();
            let reply = match command.as_str() {
                "CAPABILITY" => format!(
                    "* CAPABILITY IMAP4rev1 IDLE AUTH=PLAIN AUTH=XOAUTH2\r\n{tag} OK done\r\n"
                ),
                "LOGIN" => {
                    let parts = quoted(args);
                    let ok = parts.first().map(String::as_str) == Some(USER)
                        && parts.get(1).map(String::as_str) == Some(PASSWORD);
                    self.lock().logins.push("password".to_owned());
                    if ok {
                        format!("{tag} OK logged in\r\n")
                    } else {
                        format!("{tag} NO [AUTHENTICATIONFAILED] bad\r\n")
                    }
                }
                "AUTHENTICATE" => {
                    let _ = write.write_all(b"+ \r\n").await;
                    let mut response = String::new();
                    let _ = reader.read_line(&mut response).await;
                    let decoded = STANDARD
                        .decode(response.trim())
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .unwrap_or_default();
                    self.lock().logins.push("xoauth2".to_owned());
                    if decoded == super::oauth::xoauth2(USER, BEARER) {
                        format!("{tag} OK authenticated\r\n")
                    } else {
                        format!("{tag} NO [AUTHENTICATIONFAILED] bad token\r\n")
                    }
                }
                "LIST" => {
                    let names: Vec<String> = self.lock().folders.keys().cloned().collect();
                    let mut out = String::new();
                    for name in names {
                        out.push_str(&format!("* LIST () \"/\" \"{name}\"\r\n"));
                    }
                    out.push_str(&format!("{tag} OK done\r\n"));
                    out
                }
                "SELECT" | "EXAMINE" => {
                    let name = quoted(args).into_iter().next().unwrap_or_default();
                    let store = self.lock();
                    match store.folders.get(&name) {
                        Some(folder) => {
                            selected = Some(name.clone());
                            format!(
                                "* FLAGS (\\Seen)\r\n* {} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY {}] ok\r\n* OK [UIDNEXT {}] ok\r\n{tag} OK [READ-WRITE] done\r\n",
                                folder.messages.len(),
                                folder.uid_validity,
                                folder.next_uid
                            )
                        }
                        None => format!("{tag} NO no such folder\r\n"),
                    }
                }
                "UID" => {
                    let (sub, rest) = args.split_once(' ').unwrap_or((args, ""));
                    let folder = selected.clone().unwrap_or_default();
                    match sub.to_ascii_uppercase().as_str() {
                        "SEARCH" => {
                            let uids = self.search(&folder, rest);
                            let listed: Vec<String> = uids.iter().map(u32::to_string).collect();
                            format!("* SEARCH {}\r\n{tag} OK done\r\n", listed.join(" "))
                        }
                        "FETCH" => self.fetch(&folder, rest, tag),
                        "STORE" => {
                            let uid: u32 = rest
                                .split_whitespace()
                                .next()
                                .and_then(|uid| uid.parse().ok())
                                .unwrap_or(0);
                            let mut store = self.lock();
                            if let Some(folder) = store.folders.get_mut(&folder) {
                                for message in &mut folder.messages {
                                    if message.uid == uid {
                                        message.seen = true;
                                    }
                                }
                            }
                            format!("{tag} OK stored\r\n")
                        }
                        _ => format!("{tag} BAD unknown\r\n"),
                    }
                }
                "IDLE" => {
                    self.lock().idles += 1;
                    let _ = write.write_all(b"+ idling\r\n").await;
                    let mut done = String::new();
                    loop {
                        tokio::select! {
                            read = reader.read_line(&mut done) => {
                                if read.unwrap_or(0) == 0 {
                                    return;
                                }
                                break;
                            }
                            () = self.arrived.notified() => {
                                let count = selected
                                    .as_ref()
                                    .and_then(|name| self.lock().folders.get(name).map(|folder| folder.messages.len()))
                                    .unwrap_or(0);
                                let _ = write.write_all(format!("* {count} EXISTS\r\n").as_bytes()).await;
                            }
                        }
                    }
                    format!("{tag} OK idle done\r\n")
                }
                "NOOP" => format!("{tag} OK done\r\n"),
                "LOGOUT" => {
                    let _ = write
                        .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                        .await;
                    return;
                }
                _ => format!("{tag} BAD unknown\r\n"),
            };
            if write.write_all(reply.as_bytes()).await.is_err() {
                return;
            }
        }
    }

    fn search(&self, folder: &str, query: &str) -> Vec<u32> {
        let store = self.lock();
        let Some(folder) = store.folders.get(folder) else {
            return Vec::new();
        };
        let query = query.strip_prefix("CHARSET UTF-8 ").unwrap_or(query);
        let tokens = quoted_tokens(query);
        let mut index = 0;
        let mut matches: Vec<&StoredMessage> = folder.messages.iter().collect();
        while index < tokens.len() {
            let token = tokens[index].to_ascii_uppercase();
            let value = tokens.get(index + 1).cloned().unwrap_or_default();
            match token.as_str() {
                "UID" => {
                    let from: u32 = value
                        .split(':')
                        .next()
                        .and_then(|from| from.parse().ok())
                        .unwrap_or(1);
                    let highest = folder.messages.last().map(|message| message.uid);
                    matches.retain(|message| message.uid >= from || Some(message.uid) == highest);
                    index += 2;
                }
                "UNSEEN" => {
                    matches.retain(|message| !message.seen);
                    index += 1;
                }
                "FROM" | "TEXT" => {
                    let needle = value.to_lowercase();
                    matches.retain(|message| {
                        let raw = String::from_utf8_lossy(&message.raw).to_lowercase();
                        if token == "FROM" {
                            raw.lines()
                                .find(|line| line.starts_with("from:"))
                                .is_some_and(|line| line.contains(&needle))
                        } else {
                            raw.contains(&needle)
                        }
                    });
                    index += 2;
                }
                "SINCE" | "BEFORE" => index += 2,
                _ => index += 1,
            }
        }
        matches.iter().map(|message| message.uid).collect()
    }

    fn fetch(&self, folder: &str, rest: &str, tag: &str) -> String {
        let (set, items) = rest.split_once(' ').unwrap_or((rest, ""));
        let wanted: Vec<u32> = set.split(',').filter_map(|uid| uid.parse().ok()).collect();
        let store = self.lock();
        let mut out = String::new();
        if let Some(folder) = store.folders.get(folder) {
            for (index, message) in folder.messages.iter().enumerate() {
                if !wanted.contains(&message.uid) {
                    continue;
                }
                let mut parts = vec![
                    format!("UID {}", message.uid),
                    format!("RFC822.SIZE {}", message.raw.len()),
                ];
                if items.contains("FLAGS") {
                    parts.push(if message.seen {
                        "FLAGS (\\Seen)".to_owned()
                    } else {
                        "FLAGS ()".to_owned()
                    });
                }
                if items.contains("INTERNALDATE") {
                    parts.push(format!("INTERNALDATE \"{}\"", message.internal));
                }
                let raw = String::from_utf8_lossy(&message.raw).into_owned();
                if items.contains("BODY.PEEK[]") {
                    parts.push(format!("BODY[] {{{}}}\r\n{raw}", raw.len()));
                } else if items.contains("BODY.PEEK[HEADER]") {
                    let header = raw
                        .split_once("\r\n\r\n")
                        .map_or(raw.clone(), |(head, _)| format!("{head}\r\n\r\n"));
                    parts.push(format!("BODY[HEADER] {{{}}}\r\n{header}", header.len()));
                }
                out.push_str(&format!("* {} FETCH ({})\r\n", index + 1, parts.join(" ")));
            }
        }
        out.push_str(&format!("{tag} OK done\r\n"));
        out
    }

    async fn smtp_session<S>(&self, stream: S)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (read, mut write) = tokio::io::split(stream);
        let mut reader = BufReader::new(read);
        let _ = write.write_all(b"220 localhost ESMTP\r\n").await;
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }
            let upper = line.trim_end().to_ascii_uppercase();
            let reply: String = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                "250-localhost\r\n250-AUTH PLAIN XOAUTH2\r\n250 8BITMIME\r\n".to_owned()
            } else if upper.starts_with("AUTH PLAIN ") {
                let decoded = STANDARD
                    .decode(line.trim_end()[11..].trim())
                    .unwrap_or_default();
                if decoded == format!("\0{USER}\0{PASSWORD}").into_bytes() {
                    "235 ok\r\n".to_owned()
                } else {
                    "535 bad\r\n".to_owned()
                }
            } else if upper.starts_with("AUTH XOAUTH2 ") {
                let decoded = STANDARD
                    .decode(line.trim_end()[13..].trim())
                    .unwrap_or_default();
                if decoded == super::oauth::xoauth2(USER, BEARER).into_bytes() {
                    "235 ok\r\n".to_owned()
                } else {
                    "535 bad\r\n".to_owned()
                }
            } else if upper.starts_with("DATA") {
                let _ = write.write_all(b"354 go\r\n").await;
                let mut data = Vec::new();
                while !data.ends_with(b"\r\n.\r\n") {
                    let mut byte = [0_u8; 1];
                    if reader.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    data.push(byte[0]);
                }
                data.truncate(data.len() - 5);
                self.lock()
                    .sent
                    .push(String::from_utf8_lossy(&data).into_owned());
                "250 queued\r\n".to_owned()
            } else if upper.starts_with("QUIT") {
                let _ = write.write_all(b"221 bye\r\n").await;
                return;
            } else {
                "250 ok\r\n".to_owned()
            };
            if write.write_all(reply.as_bytes()).await.is_err() {
                return;
            }
        }
    }
}

fn quoted(args: &str) -> Vec<String> {
    quoted_tokens(args)
}

fn quoted_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&character) = chars.peek() {
        if character == ' ' {
            chars.next();
            continue;
        }
        let mut token = String::new();
        if character == '"' {
            chars.next();
            while let Some(character) = chars.next() {
                match character {
                    '\\' => {
                        if let Some(next) = chars.next() {
                            token.push(next);
                        }
                    }
                    '"' => break,
                    other => token.push(other),
                }
            }
        } else {
            while let Some(&character) = chars.peek() {
                if character == ' ' {
                    break;
                }
                token.push(character);
                chars.next();
            }
        }
        tokens.push(token);
    }
    tokens
}
