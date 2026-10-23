#![allow(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

use ::windows::Data::Xml::Dom::XmlDocument;
use ::windows::Foundation::TypedEventHandler;
use ::windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastDismissedEventArgs, ToastFailedEventArgs, ToastNotification,
    ToastNotificationManager, ToastNotifier,
};
use ::windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};
use ::windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use ::windows::core::{HSTRING, IInspectable, Interface, PCWSTR};
use serde_json::json;
use zvs_contract::NotificationTargetDto;
use zvs_host::platform::logger::{LogLevel, SharedLogger};
use zvs_host::platform::shell::{NotificationDraft, NotificationKind};

use crate::toast::{ActivationSlot, Toaster, toast_xml};

type Live = Arc<Mutex<HashMap<(NotificationKind, String), ToastNotification>>>;

enum Command {
    Show(NotificationDraft),
    Clear(NotificationTargetDto),
}

pub struct WindowsToaster {
    commands: Mutex<Sender<Command>>,
}

impl WindowsToaster {
    pub fn start(
        app_id: &str,
        display_name: &str,
        icon: Option<&Path>,
        activation: ActivationSlot,
        logger: SharedLogger,
    ) -> Self {
        if let Err(error) = register_app_id(app_id, display_name, icon) {
            logger.log(
                LogLevel::Warn,
                "shell",
                "Could not register the notification identity",
                &[("error", json!(error.to_string()))],
            );
        }
        let (commands, receiver) = channel();
        let worker = Worker {
            app_id: HSTRING::from(app_id),
            notifier: None,
            live: Arc::new(Mutex::new(HashMap::new())),
            activation,
            logger,
        };
        let _ = std::thread::Builder::new()
            .name("toasts".to_owned())
            .spawn(move || worker.run(&receiver));
        Self {
            commands: Mutex::new(commands),
        }
    }

    fn send(&self, command: Command) {
        let _ = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(command);
    }
}

impl Toaster for WindowsToaster {
    fn show(&self, draft: NotificationDraft) {
        self.send(Command::Show(draft));
    }

    fn clear(&self, target: NotificationTargetDto) {
        self.send(Command::Clear(target));
    }
}

struct Worker {
    app_id: HSTRING,
    notifier: Option<ToastNotifier>,
    live: Live,
    activation: ActivationSlot,
    logger: SharedLogger,
}

impl Worker {
    fn run(mut self, receiver: &Receiver<Command>) {
        while let Ok(command) = receiver.recv() {
            let outcome = match command {
                Command::Show(draft) => self.show(draft),
                Command::Clear(target) => self.clear(target),
            };
            if let Err(error) = outcome {
                self.logger.log(
                    LogLevel::Warn,
                    "shell",
                    "A notification could not be shown",
                    &[("error", json!(error.to_string()))],
                );
            }
        }
    }

    fn notifier(&mut self) -> ::windows::core::Result<&ToastNotifier> {
        if self.notifier.is_none() {
            self.notifier = Some(ToastNotificationManager::CreateToastNotifierWithId(
                &self.app_id,
            )?);
        }
        self.notifier
            .as_ref()
            .ok_or_else(::windows::core::Error::empty)
    }

    fn show(&mut self, draft: NotificationDraft) -> ::windows::core::Result<()> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(toast_xml(&draft)))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        toast.SetTag(&HSTRING::from(draft.tag.as_str()))?;
        toast.SetGroup(&HSTRING::from(draft.kind.group()))?;
        toast.SetSuppressPopup(draft.silent)?;
        let key = (draft.kind, draft.tag.clone());

        let live = Arc::clone(&self.live);
        let activation = Arc::clone(&self.activation);
        let activated_key = key.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, args| {
                forget(&live, &activated_key);
                let target = args
                    .as_ref()
                    .and_then(|args| args.cast::<ToastActivatedEventArgs>().ok())
                    .and_then(|args| args.Arguments().ok())
                    .and_then(|launch| NotificationTargetDto::from_url(&launch.to_string()));
                if let (Some(target), Some(activate)) = (target, activation.get()) {
                    activate(target);
                }
                Ok(())
            },
        ))?;

        let live = Arc::clone(&self.live);
        let dismissed_key = key.clone();
        toast.Dismissed(&TypedEventHandler::<
            ToastNotification,
            ToastDismissedEventArgs,
        >::new(move |_, _| {
            forget(&live, &dismissed_key);
            Ok(())
        }))?;

        let live = Arc::clone(&self.live);
        let logger = self.logger.clone();
        let failed_key = key.clone();
        toast.Failed(
            &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, args| {
                forget(&live, &failed_key);
                let code = args
                    .as_ref()
                    .and_then(|args| args.ErrorCode().ok())
                    .map(|code| code.0);
                logger.log(
                    LogLevel::Warn,
                    "shell",
                    "Windows refused a notification",
                    &[("code", json!(code))],
                );
                Ok(())
            }),
        )?;

        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), toast.clone());
        let shown = self.notifier().and_then(|notifier| notifier.Show(&toast));
        if shown.is_err() {
            forget(&self.live, &key);
        }
        shown
    }

    fn clear(&self, target: NotificationTargetDto) -> ::windows::core::Result<()> {
        let history = ToastNotificationManager::History()?;
        match target {
            NotificationTargetDto::Conversation { conversation_id } => {
                let tag = conversation_id.to_string();
                for kind in [NotificationKind::Approval, NotificationKind::Turn] {
                    forget(&self.live, &(kind, tag.clone()));
                    history.RemoveGroupedTagWithId(
                        &HSTRING::from(tag.as_str()),
                        &HSTRING::from(kind.group()),
                        &self.app_id,
                    )?;
                }
            }
            NotificationTargetDto::Downloads => {
                self.live
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retain(|(kind, _), _| *kind != NotificationKind::Download);
                history.RemoveGroupWithId(
                    &HSTRING::from(NotificationKind::Download.group()),
                    &self.app_id,
                )?;
            }
            NotificationTargetDto::Scenarios => {}
        }
        Ok(())
    }
}

fn forget(live: &Live, key: &(NotificationKind, String)) {
    live.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(key);
}

fn register_app_id(
    app_id: &str,
    display_name: &str,
    icon: Option<&Path>,
) -> ::windows::core::Result<()> {
    let subkey = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{app_id}"));
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &raw mut key,
            None,
        )
    }
    .ok()?;
    let mut written = write_string(key, "DisplayName", display_name);
    if let Some(icon) = icon {
        written = written.and(write_string(key, "IconUri", &icon.to_string_lossy()));
    }
    let _ = unsafe { RegCloseKey(key) };
    written?;
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id)) }
}

fn write_string(key: HKEY, name: &str, value: &str) -> ::windows::core::Result<()> {
    let bytes: Vec<u8> = value
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    unsafe { RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(&bytes)) }.ok()
}
