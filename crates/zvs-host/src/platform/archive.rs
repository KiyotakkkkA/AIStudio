use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use flate2::read::{DeflateDecoder, GzDecoder};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

pub const UNKNOWN_FORMAT: &str = "Неизвестный формат архива";
pub const NO_DIRECTORY: &str = "Архив повреждён: нет оглавления";
pub const TRUNCATED: &str = "Архив повреждён: данные обрываются";
pub const ZIP64_REFUSED: &str = "Архивы ZIP64 не поддерживаются";
pub const UNSUPPORTED_METHOD: &str = "Неподдерживаемый метод сжатия";
pub const EXTRACTION_CANCELLED: &str = "Распаковка отменена";

pub type EntrySink = Arc<dyn Fn(&str, u64, u64) + Send + Sync>;

#[derive(Clone, Default)]
pub struct ExtractOptions {
    pub cancel: Option<CancellationToken>,
    pub on_entry: Option<EntrySink>,
}

impl ExtractOptions {
    fn check_cancelled(&self) -> Result<(), AppError> {
        if self
            .cancel
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            Err(AppError::new(
                AppErrorCode::RunCancelled,
                EXTRACTION_CANCELLED,
            ))
        } else {
            Ok(())
        }
    }

    fn entry(&self, name: &str, done: u64, total: u64) {
        if let Some(sink) = &self.on_entry {
            sink(name, done, total);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractReport {
    pub files: u64,
    pub bytes: u64,
}

fn io_failure(error: &io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, error.to_string())
}

fn corrupt(message: &str) -> AppError {
    AppError::new(AppErrorCode::UnsupportedFormat, message)
}

pub async fn extract_archive(
    archive: &Path,
    target: &Path,
    options: ExtractOptions,
) -> Result<ExtractReport, AppError> {
    let archive = archive.to_path_buf();
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || extract_archive_blocking(&archive, &target, &options))
        .await
        .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))?
}

pub fn extract_archive_blocking(
    archive: &Path,
    target: &Path,
    options: &ExtractOptions,
) -> Result<ExtractReport, AppError> {
    let lower = archive.to_string_lossy().to_lowercase();
    if lower.ends_with(".zip") {
        return extract_zip(archive, target, options);
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return extract_tar_gz(archive, target, options);
    }
    Err(
        AppError::new(AppErrorCode::UnsupportedFormat, UNKNOWN_FORMAT)
            .with_detail("archivePath", archive.to_string_lossy().into_owned()),
    )
}

fn is_drive_prefixed(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

pub fn safe_member_path(target: &Path, name: &str) -> Option<PathBuf> {
    let replaced = name.replace('\\', "/");
    let cleaned = replaced.trim_start_matches('/');
    if cleaned.is_empty() || cleaned.ends_with('/') || is_drive_prefixed(cleaned) {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in cleaned.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut resolved = target.to_path_buf();
    for part in parts {
        let component = Path::new(part);
        if component
            .components()
            .any(|piece| !matches!(piece, Component::Normal(_)))
        {
            return None;
        }
        resolved.push(component);
    }
    Some(resolved)
}

fn write_member(destination: &Path, content: &mut dyn Read, mode: u32) -> Result<u64, AppError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io_failure(&error))?;
    }
    let mut file = File::create(destination).map_err(|error| io_failure(&error))?;
    let written = io::copy(content, &mut file).map_err(|error| io_failure(&error))?;
    apply_mode(destination, mode);
    Ok(written)
}

#[cfg(unix)]
fn apply_mode(destination: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if mode != 0 {
        let _ = std::fs::set_permissions(destination, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(not(unix))]
const fn apply_mode(_destination: &Path, _mode: u32) {}

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const MAX_COMMENT: u64 = 0xffff;
const UNIX_HOST: u8 = 3;

#[derive(Debug, Clone)]
struct ZipEntry {
    name: String,
    method: u16,
    compressed_size: u64,
    local_offset: u64,
    mode: u32,
}

trait ByteSource {
    fn size(&self) -> u64;
    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, AppError>;
}

struct SliceSource<'a>(&'a [u8]);

impl ByteSource for SliceSource<'_> {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, AppError> {
        let start = usize::try_from(offset).map_err(|_| corrupt(TRUNCATED))?;
        let end = usize::try_from(offset.saturating_add(length)).map_err(|_| corrupt(TRUNCATED))?;
        self.0
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| corrupt(TRUNCATED))
    }
}

struct FileSource {
    file: File,
    size: u64,
}

impl ByteSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, AppError> {
        if offset.saturating_add(length) > self.size {
            return Err(corrupt(TRUNCATED));
        }
        let mut buffer = vec![0; usize::try_from(length).map_err(|_| corrupt(TRUNCATED))?];
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.read_exact(&mut buffer))
            .map_err(|error| io_failure(&error))?;
        Ok(buffer)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .map_or(0, |slice| u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes.get(at..at + 4).map_or(0, |slice| {
        u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
    })
}

fn read_directory(source: &mut dyn ByteSource) -> Result<Vec<ZipEntry>, AppError> {
    let tail_length = source.size().min(MAX_COMMENT + 22);
    let tail = source.read_at(source.size() - tail_length, tail_length)?;
    let eocd = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|at| tail.len() >= 22 && u32_at(&tail, *at) == EOCD_SIGNATURE)
        .ok_or_else(|| corrupt(NO_DIRECTORY))?;
    if eocd >= 20 && u32_at(&tail, eocd - 20) == ZIP64_LOCATOR_SIGNATURE {
        return Err(corrupt(ZIP64_REFUSED));
    }
    let entry_count = u16_at(&tail, eocd + 10);
    let directory = source.read_at(
        u64::from(u32_at(&tail, eocd + 16)),
        u64::from(u32_at(&tail, eocd + 12)),
    )?;

    let mut entries = Vec::new();
    let mut cursor = 0;
    for _ in 0..entry_count {
        if cursor + 46 > directory.len() || u32_at(&directory, cursor) != CENTRAL_SIGNATURE {
            break;
        }
        let name_length = usize::from(u16_at(&directory, cursor + 28));
        let extra_length = usize::from(u16_at(&directory, cursor + 30));
        let comment_length = usize::from(u16_at(&directory, cursor + 32));
        let made_by_unix = directory.get(cursor + 5) == Some(&UNIX_HOST);
        let name_bytes = directory
            .get(cursor + 46..cursor + 46 + name_length)
            .unwrap_or_default();
        entries.push(ZipEntry {
            name: String::from_utf8_lossy(name_bytes).into_owned(),
            method: u16_at(&directory, cursor + 10),
            compressed_size: u64::from(u32_at(&directory, cursor + 20)),
            local_offset: u64::from(u32_at(&directory, cursor + 42)),
            mode: if made_by_unix {
                (u32_at(&directory, cursor + 38) >> 16) & 0o7777
            } else {
                0
            },
        });
        cursor += 46 + name_length + extra_length + comment_length;
    }
    Ok(entries)
}

fn read_member(source: &mut dyn ByteSource, entry: &ZipEntry) -> Result<Vec<u8>, AppError> {
    if entry.method != 0 && entry.method != 8 {
        return Err(corrupt(UNSUPPORTED_METHOD)
            .with_detail("name", entry.name.clone())
            .with_detail("method", entry.method));
    }
    let header = source.read_at(entry.local_offset, 30)?;
    let data_start =
        entry.local_offset + 30 + u64::from(u16_at(&header, 26)) + u64::from(u16_at(&header, 28));
    let compressed = if entry.compressed_size > 0 {
        source.read_at(data_start, entry.compressed_size)?
    } else {
        Vec::new()
    };
    if entry.method == 0 {
        return Ok(compressed);
    }
    let mut inflated = Vec::new();
    DeflateDecoder::new(compressed.as_slice())
        .read_to_end(&mut inflated)
        .map_err(|error| io_failure(&error))?;
    Ok(inflated)
}

pub fn read_zip_entries(
    bytes: &[u8],
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let mut source = SliceSource(bytes);
    let entries = read_directory(&mut source)?;
    let mut found: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in entries {
        if entry.name.ends_with('/') || !wanted(&entry.name) {
            continue;
        }
        let content = read_member(&mut source, &entry)?;
        match found.iter_mut().find(|(name, _)| *name == entry.name) {
            Some(existing) => existing.1 = content,
            None => found.push((entry.name, content)),
        }
    }
    Ok(found)
}

fn extract_zip(
    archive: &Path,
    target: &Path,
    options: &ExtractOptions,
) -> Result<ExtractReport, AppError> {
    let file = File::open(archive).map_err(|error| io_failure(&error))?;
    let size = file.metadata().map_err(|error| io_failure(&error))?.len();
    let mut source = FileSource { file, size };
    let entries = read_directory(&mut source)?;
    let total = entries.len() as u64;
    let mut report = ExtractReport::default();
    for entry in &entries {
        options.check_cancelled()?;
        let Some(destination) = safe_member_path(target, &entry.name) else {
            continue;
        };
        let content = read_member(&mut source, entry)?;
        report.bytes += write_member(&destination, &mut content.as_slice(), entry.mode)?;
        report.files += 1;
        options.entry(&entry.name, report.files, total);
    }
    Ok(report)
}

fn extract_tar_gz(
    archive: &Path,
    target: &Path,
    options: &ExtractOptions,
) -> Result<ExtractReport, AppError> {
    let file = File::open(archive).map_err(|error| io_failure(&error))?;
    let mut reader = tar::Archive::new(GzDecoder::new(io::BufReader::new(file)));
    let mut report = ExtractReport::default();
    for entry in reader.entries().map_err(|error| io_failure(&error))? {
        options.check_cancelled()?;
        let mut entry = entry.map_err(|error| io_failure(&error))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let Some(destination) = safe_member_path(target, &name) else {
            continue;
        };
        let mode = entry.header().mode().unwrap_or(0) & 0o7777;
        report.bytes += write_member(&destination, &mut entry, mode)?;
        report.files += 1;
        options.entry(&destination.to_string_lossy(), report.files, 0);
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
