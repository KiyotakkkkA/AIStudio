use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zvs_contract::{TransferCategory, TransferCategoryDto, TransferPreviewDto, TransferWarning};

use super::{failure, io_error};
use crate::data::repositories::data_transfer::{Record, TABLES, invalid_package, table, text};

pub const MAX_JSON: u64 = 64 * 1024 * 1024;
pub const MAX_FILES: usize = 100_000;
pub const MAX_BYTES: u64 = zvs_contract::data_transfer::TRANSFER_MAX_BYTES;
pub const VERSION: u32 = 2;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member {
    pub category: TransferCategory,
    pub owner: String,
    pub path: String,
    pub original: String,
    pub stored_original: String,
    pub hash: String,
    pub bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Package {
    pub format: String,
    pub version: u32,
    pub producer_version: String,
    pub created_at: i64,
    pub categories: Vec<TransferCategory>,
    pub records: Vec<Record>,
    pub files: Vec<Member>,
    #[serde(skip)]
    pub missing_files: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    hash: String,
    bytes: u64,
}

pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && path.split('/').all(|part| {
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
                && !matches!(
                    stem.as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn preview_name(name: &str) -> String {
    let mut length = 0;
    name.chars()
        .take_while(|character| {
            length += character.len_utf16();
            length <= 120
        })
        .collect()
}

pub fn snapshot(source: &Path, blobs: &Path) -> Result<(String, u64), zvs_contract::AppError> {
    let before = fs::symlink_metadata(source).map_err(io_error)?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > MAX_BYTES {
        return Err(invalid_package());
    }
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = tempfile::NamedTempFile::new_in(blobs).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > MAX_BYTES {
            return Err(invalid_package());
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(io_error)?;
    }
    let after = fs::metadata(source).map_err(io_error)?;
    if bytes != before.len()
        || before.modified().ok() != after.modified().ok()
        || before.len() != after.len()
    {
        return Err(failure(zvs_contract::AppErrorCode::Conflict, "changed"));
    }
    let hash = hex(&hash.finalize());
    if !blobs.join(&hash).exists() {
        output.persist(blobs.join(&hash)).map_err(io_error)?;
    }
    Ok((hash, bytes))
}

impl Package {
    pub fn validate(&self) -> Result<(), zvs_contract::AppError> {
        use zvs_contract::Validate;
        if self.format != "zvs-dbox"
            || !matches!(self.version, 1 | VERSION)
            || self.records.len() > MAX_FILES
            || self.files.len() > MAX_FILES
        {
            return Err(invalid_package());
        }
        zvs_contract::TransferSelection {
            categories: self.categories.clone(),
        }
        .validate()?;
        let mut ids = HashSet::new();
        let mut composite = HashSet::new();
        for record in &self.records {
            crate::data::repositories::data_transfer::validate_fields(record)
                .map_err(|error| error.with_detail("recordKind", record.kind.clone()))?;
            let table = table(&record.kind).ok_or_else(invalid_package)?;
            let columns = table
                .archive_columns(self.version)
                .ok_or_else(invalid_package)?;
            if !self.categories.contains(&table.category)
                || record.fields.len() != columns.split(',').count()
                || columns
                    .split(',')
                    .any(|field| !record.fields.contains_key(field))
                || record
                    .fields
                    .values()
                    .any(|value| !value.is_null() && !value.is_string() && !value.is_number())
            {
                return Err(invalid_package());
            }
            if let Some(id) = record.fields.get("id") {
                let id = id.as_str().ok_or_else(invalid_package)?;
                if uuid::Uuid::parse_str(id).is_err() || !ids.insert(id) {
                    return Err(invalid_package());
                }
            } else {
                let key = serde_json::to_string(&record.fields).map_err(io_error)?;
                if !composite.insert((record.kind.as_str(), key)) {
                    return Err(invalid_package());
                }
            }
            if record.kind == "setting"
                && !crate::data::repositories::data_transfer::portable_setting(text(record, "key")?)
            {
                return Err(invalid_package());
            }
        }
        let owners: HashSet<_> = self
            .records
            .iter()
            .filter_map(|record| {
                record
                    .fields
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(|id| (record.kind.as_str(), id))
            })
            .collect();
        if self.version == VERSION {
            let mut referenced_runs = HashSet::new();
            for record in &self.records {
                match record.kind.as_str() {
                    "message" if !record.fields["run_id"].is_null() => {
                        let id = text(record, "run_id")?;
                        if text(record, "role")? != "assistant" || !owners.contains(&("run", id)) {
                            return Err(invalid_package());
                        }
                        referenced_runs.insert(id);
                    }
                    "run"
                        if !record.fields["subject_id"].is_null()
                            && !owners.contains(&("conversation", text(record, "subject_id")?)) =>
                    {
                        return Err(invalid_package());
                    }
                    _ => {}
                }
            }
            for record in self.records.iter().filter(|record| record.kind == "run") {
                if !referenced_runs.contains(text(record, "id")?) {
                    return Err(invalid_package());
                }
            }
        }
        let mut names = HashSet::new();
        let mut bytes = 0_u64;
        for file in &self.files {
            let kind = match file.category {
                TransferCategory::Conversations => "conversation",
                TransferCategory::Skills => "skill",
                TransferCategory::Stores => "vector_store",
                _ => return Err(invalid_package()),
            };
            if !safe_relative(&file.path)
                || !valid_hash(&file.hash)
                || !ids.contains(file.owner.as_str())
                || !owners.contains(&(kind, file.owner.as_str()))
                || !names.insert((
                    file.category.as_str(),
                    file.owner.as_str(),
                    file.path.to_lowercase(),
                ))
            {
                return Err(invalid_package());
            }
            bytes = bytes.checked_add(file.bytes).ok_or_else(invalid_package)?;
            if bytes > MAX_BYTES {
                return Err(invalid_package());
            }
        }
        self.preview().validate()
    }

    pub fn preview(&self) -> TransferPreviewDto {
        let categories = self
            .categories
            .iter()
            .map(|category| {
                let records: Vec<_> = self
                    .records
                    .iter()
                    .filter(|record| {
                        table(&record.kind).is_some_and(|spec| spec.category == *category)
                    })
                    .collect();
                TransferCategoryDto {
                    category: *category,
                    names: records
                        .iter()
                        .filter(|record| table(&record.kind).is_some_and(|spec| spec.primary))
                        .filter_map(|record| {
                            record
                                .fields
                                .get("title")
                                .or_else(|| record.fields.get("name"))
                                .or_else(|| record.fields.get("content"))
                        })
                        .filter_map(serde_json::Value::as_str)
                        .take(20)
                        .map(preview_name)
                        .collect(),
                    requires: match category {
                        TransferCategory::Conversations | TransferCategory::Stores
                            if !records.is_empty()
                                && self.records.iter().any(|record| record.kind == "provider") =>
                        {
                            vec![TransferCategory::Connections]
                        }
                        TransferCategory::Memory
                            if records.iter().any(|record| {
                                record
                                    .fields
                                    .get("scope")
                                    .and_then(serde_json::Value::as_str)
                                    == Some("conversation")
                            }) =>
                        {
                            vec![TransferCategory::Conversations]
                        }
                        _ => Vec::new(),
                    },
                    count: records
                        .iter()
                        .filter(|record| table(&record.kind).is_some_and(|spec| spec.primary))
                        .count() as u64,
                    bytes: records
                        .iter()
                        .map(|record| {
                            serde_json::to_vec(record).map_or(0, |bytes| bytes.len() as u64)
                        })
                        .sum::<u64>()
                        + self
                            .files
                            .iter()
                            .filter(|file| file.category == *category)
                            .map(|file| file.bytes)
                            .sum::<u64>(),
                }
            })
            .collect();
        let mut warnings = Vec::new();
        if self.missing_files {
            warnings.push(TransferWarning::MissingFiles);
        }
        if self.categories.contains(&TransferCategory::Connections) {
            warnings.push(TransferWarning::Credentials);
        }
        if self.categories.contains(&TransferCategory::Stores) {
            warnings.push(TransferWarning::Indexes);
        }
        if self.categories.iter().any(|category| {
            matches!(
                category,
                TransferCategory::Skills
                    | TransferCategory::Scenarios
                    | TransferCategory::Connections
            )
        }) {
            warnings.push(TransferWarning::Inactive);
        }
        if self.categories.contains(&TransferCategory::Settings) {
            warnings.push(TransferWarning::Settings);
        }
        TransferPreviewDto {
            created_at: self.created_at,
            categories,
            warnings,
            blocked_categories: Vec::new(),
            imported_categories: Vec::new(),
        }
    }
}

fn append<W: Write>(
    archive: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, name, bytes)
}

pub fn write(
    package: &Package,
    blobs: &Path,
    destination: &Path,
) -> Result<(), zvs_contract::AppError> {
    package.validate()?;
    let data = serde_json::to_vec(package).map_err(io_error)?;
    if data.len() as u64 > MAX_JSON {
        return Err(invalid_package());
    }
    let manifest = Manifest {
        format: "zvs-dbox".to_owned(),
        version: package.version,
        hash: digest(&data),
        bytes: data.len() as u64,
    };
    let parent = destination.parent().ok_or_else(invalid_package)?;
    super::require_space(
        parent,
        package.files.iter().map(|member| member.bytes).sum::<u64>() + MAX_JSON + 128 * 1024 * 1024,
    )?;
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    let encoder = GzEncoder::new(output.as_file_mut(), Compression::default());
    let mut archive = tar::Builder::new(encoder);
    append(
        &mut archive,
        "manifest.json",
        &serde_json::to_vec(&manifest).map_err(io_error)?,
    )
    .map_err(io_error)?;
    append(&mut archive, "data.json", &data).map_err(io_error)?;
    let mut seen = HashSet::new();
    for member in &package.files {
        if seen.insert(&member.hash) {
            archive
                .append_path_with_name(blobs.join(&member.hash), format!("blobs/{}", member.hash))
                .map_err(io_error)?;
        }
    }
    let encoder = archive.into_inner().map_err(io_error)?;
    encoder.finish().map_err(io_error)?;
    output.as_file().sync_all().map_err(io_error)?;
    if output.as_file().metadata().map_err(io_error)?.len() > MAX_BYTES {
        return Err(failure(
            zvs_contract::AppErrorCode::ValidationFailed,
            "size",
        ));
    }
    output.persist(destination).map_err(io_error)?;
    Ok(())
}

pub fn read(source: &Path, blobs: &Path) -> Result<(Package, String), zvs_contract::AppError> {
    fs::create_dir_all(blobs).map_err(io_error)?;
    let decoder = GzDecoder::new(File::open(source).map_err(io_error)?);
    let mut archive = tar::Archive::new(decoder.take(MAX_BYTES + MAX_JSON + 128 * 1024 * 1024));
    let mut entries = archive.entries().map_err(|_| invalid_package())?.raw(true);
    let mut json = |expected: &str, max: u64| -> Result<Vec<u8>, zvs_contract::AppError> {
        let mut entry = entries
            .next()
            .ok_or_else(invalid_package)?
            .map_err(|_| invalid_package())?;
        if !entry.header().entry_type().is_file()
            || entry.path().map_err(|_| invalid_package())? != Path::new(expected)
            || entry.size() > max
        {
            return Err(invalid_package());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|_| invalid_package())?;
        Ok(bytes)
    };
    let manifest: Manifest =
        serde_json::from_slice(&json("manifest.json", 4096)?).map_err(|_| invalid_package())?;
    let bytes = json("data.json", MAX_JSON)?;
    let hash = digest(&bytes);
    if manifest.format != "zvs-dbox"
        || !matches!(manifest.version, 1 | VERSION)
        || manifest.hash != hash
        || manifest.bytes != bytes.len() as u64
    {
        return Err(invalid_package());
    }
    let package: Package = serde_json::from_slice(&bytes).map_err(|_| invalid_package())?;
    if package.version != manifest.version {
        return Err(invalid_package());
    }
    package.validate()?;
    let mut expected = BTreeMap::new();
    for member in &package.files {
        if expected
            .insert(member.hash.clone(), member.bytes)
            .is_some_and(|bytes| bytes != member.bytes)
        {
            return Err(invalid_package());
        }
    }
    let mut seen = HashSet::new();
    super::require_space(blobs, expected.values().sum())?;
    for entry in entries {
        let mut entry = entry.map_err(|_| invalid_package())?;
        let name = entry
            .path()
            .map_err(|_| invalid_package())?
            .to_string_lossy()
            .into_owned();
        let hash = name.strip_prefix("blobs/").ok_or_else(invalid_package)?;
        if !entry.header().entry_type().is_file()
            || !valid_hash(hash)
            || expected.get(hash) != Some(&entry.size())
            || !seen.insert(hash.to_owned())
        {
            return Err(invalid_package());
        }
        let target = blobs.join(hash);
        let mut output = File::options()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(io_error)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = entry.read(&mut buffer).map_err(|_| invalid_package())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            output.write_all(&buffer[..count]).map_err(io_error)?;
        }
        if hex(&hasher.finalize()) != hash {
            return Err(invalid_package());
        }
    }
    if seen.len() != expected.len() {
        return Err(invalid_package());
    }
    let mut remaining = archive.into_inner();
    std::io::copy(&mut remaining, &mut std::io::sink()).map_err(|_| invalid_package())?;
    if remaining.limit() == 0 {
        return Err(invalid_package());
    }
    Ok((package, hash))
}

pub fn all_categories() -> Vec<TransferCategory> {
    let mut categories = Vec::new();
    for table in TABLES {
        if !categories.contains(&table.category) {
            categories.push(table.category);
        }
    }
    categories
}

pub fn local_path(root: &Path, relative: &str) -> Result<PathBuf, zvs_contract::AppError> {
    if !safe_relative(relative) {
        return Err(invalid_package());
    }
    Ok(relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part)))
}
