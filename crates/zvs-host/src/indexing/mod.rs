pub mod documents;
pub mod extraction;
pub mod ooxml;
pub mod patterns;
pub mod service;
pub mod walk;

#[cfg(test)]
mod tests;

use zvs_contract::{AppError, AppErrorCode};

pub use documents::{
    DOCUMENT_JOB, DocumentExtractorOptions, ImageExtractor, OcrPort, PdfExtractor, SharedOcr,
    image_extensions,
};
pub use extraction::{
    ExtractionContext, ExtractionInput, Extractor, ExtractorRegistry, FnExtractor, Segment,
    SharedExtractor, create_extractor_registry,
};
pub use service::{
    IndexContext, IndexProgress, IndexingService, IndexingServiceOptions, ResourceSampler,
    SamplingGuard, SharedResourceSampler, embedding_input, estimate_chunks,
};
pub use walk::{DiscoveredFile, WalkLimits, WalkOutcome, walk_sources};

pub const NOTES_MAX: usize = zvs_contract::vector_stores::INDEX_NOTES_MAX;
pub const CANCELLED_MESSAGE: &str = "Индексация отменена";

pub fn cancelled() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE)
}

pub fn worker_failed() -> AppError {
    AppError::new(
        AppErrorCode::NativeError,
        "Фоновая задача индексации прервалась",
    )
}
