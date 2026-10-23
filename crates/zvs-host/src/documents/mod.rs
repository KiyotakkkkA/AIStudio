pub mod cache;
pub mod convert;
pub mod kind;
pub mod service;
pub mod text;

#[cfg(test)]
mod tests;

pub use cache::{CacheEntry, DocumentCache, cache_file_name, eviction_plan};
pub use convert::{
    DOCUMENT_CONVERT_NODE, KernelDocumentConverter, OfficeLocator, SharedOfficeLocator,
    register_document_nodes,
};
pub use kind::{
    Detected, Detection, MISLABELLED_DOCUMENT, OfficeFormat, UNSUPPORTED_DOCUMENT, detect,
};
pub use service::{
    DOCUMENT_CACHE_CEILING, DOCUMENT_NOT_FOUND, DOCUMENT_NOT_OPEN, DOCUMENT_OPEN_MAX,
    DOCUMENT_PAGE_NOT_FOUND, DOCUMENT_SOURCE_CHANGED, DOCUMENT_TEXT_MAX, DocumentConverter,
    DocumentService, DocumentServiceOptions, SharedDocumentConverter,
};
