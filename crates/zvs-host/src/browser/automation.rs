use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

use super::page::PageElement;

pub const NO_BROWSER_TAB: &str = "Нет открытой вкладки браузера";
pub const UNKNOWN_HANDLE: &str =
    "Элемент не найден: получите его заново через browser.find или browser.read_page";
pub const STALE_HANDLE: &str =
    "Страница изменилась: получите элемент заново через browser.find или browser.read_page";
pub const BLOCKED_ADDRESS: &str = "Адрес заблокирован: разрешены только адреса HTTP и HTTPS";
pub const SUBMITS_FORM: &str = "Элемент отправляет форму: для этого служит browser.submit";
pub const NOT_EDITABLE: &str = "Элемент не принимает ввод текста";
pub const ELEMENT_DISABLED: &str = "Элемент недоступен для действий";
pub const FOCUS_MOVED: &str = "Фокус ушёл с выбранного поля, ввод отменён";
pub const OBSCURED: &str = "Элемент перекрыт другим элементом страницы, действие отменено";
pub const NOT_VISIBLE: &str = "Элемент не отображается на странице";
pub const NO_FORM: &str = "Элемент не относится к форме";
pub const NOT_A_LINK: &str = "Элемент не является ссылкой на файл";
pub const DOWNLOAD_NOT_STARTED: &str = "Загрузка не началась";
pub const DOWNLOAD_FAILED: &str = "Загрузка не завершилась";
pub const NAVIGATION_FAILED: &str = "Страница не открылась";
pub const AUTOMATION_FAILED: &str = "Браузер не выполнил действие";
pub const BROWSER_DENIED: &str = "Действие в браузере отклонено";

pub fn refused(message: &str) -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, message)
}

pub fn conflict(message: &str) -> AppError {
    AppError::new(AppErrorCode::Conflict, message)
}

pub fn failed(message: &str) -> AppError {
    AppError::new(AppErrorCode::NativeError, message)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageState {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSnapshot {
    pub page: PageState,
    pub text: String,
    pub elements: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageMatches {
    pub page: PageState,
    pub matches: Vec<PageElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageScreenshot {
    pub page: PageState,
    pub mime_type: String,
    pub width: u64,
    pub height: u64,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedFile {
    pub file_name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementTarget {
    pub origin: String,
    pub role: String,
    pub name: String,
}

#[async_trait]
pub trait AutomationPort: Send + Sync {
    fn target(&self, handle: &str) -> Option<ElementTarget>;

    fn origin(&self) -> Option<String>;

    async fn highlight(&self, handle: Option<&str>) -> Result<(), AppError>;

    async fn navigate(&self, url: &str) -> Result<PageState, AppError>;

    async fn read_page(&self) -> Result<PageSnapshot, AppError>;

    async fn screenshot(&self) -> Result<PageScreenshot, AppError>;

    async fn find(&self, query: &str, role: Option<&str>) -> Result<PageMatches, AppError>;

    async fn click(&self, handle: &str) -> Result<PageState, AppError>;

    async fn type_text(
        &self,
        handle: &str,
        text: &str,
        replace: bool,
    ) -> Result<PageState, AppError>;

    async fn submit(&self, handle: &str) -> Result<PageState, AppError>;

    async fn download(
        &self,
        handle: &str,
        cancel: &CancellationToken,
    ) -> Result<DownloadedFile, AppError>;
}

pub type SharedAutomation = Arc<dyn AutomationPort>;
