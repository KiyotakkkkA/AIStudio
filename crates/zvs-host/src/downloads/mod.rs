pub mod catalogue;
pub mod curated;
pub mod disk;
pub mod service;

#[cfg(test)]
mod tests;

pub use catalogue::{
    BaseUrlSource, CatalogueItem, CatalogueProvider, CatalogueService, CatalogueServiceOptions,
    CuratedProvider, InstalledItem, OllamaProvider, SharedCatalogueProvider, installed_key,
};
pub use disk::{DiskProbe, DiskService, DiskServiceOptions, SharedDiskProbe, SystemDiskProbe};
pub use service::{
    DownloadExecutor, DownloadInstaller, DownloadProgress, DownloadProgressSink, DownloadRunner,
    DownloadService, DownloadServiceOptions, RefMatcher, SharedDownloadExecutor,
    SharedDownloadInstaller,
};
