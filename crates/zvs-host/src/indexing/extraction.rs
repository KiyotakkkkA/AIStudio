use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use async_trait::async_trait;
use regex::Regex;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_trim;
use zvs_contract::{AppError, AppErrorCode, VectorOcrConfig};

use super::ooxml::{docx_segments, pptx_segments, xlsx_segments};

pub type NoteSink = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone, Default)]
pub struct ExtractionContext {
    pub ocr: Option<VectorOcrConfig>,
    pub cancel: Option<CancellationToken>,
    pub note: Option<NoteSink>,
}

impl ExtractionContext {
    pub fn note(&self, message: String) {
        if let Some(note) = &self.note {
            note(message);
        }
    }

    pub fn cancel(&self) -> CancellationToken {
        self.cancel.clone().unwrap_or_default()
    }
}

#[derive(Clone, Default)]
pub struct ExtractionInput {
    pub path: String,
    pub bytes: Vec<u8>,
    pub context: ExtractionContext,
}

impl ExtractionInput {
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
            context: ExtractionContext::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub section: Vec<String>,
    pub page: Option<u32>,
}

impl Segment {
    pub fn new(text: impl Into<String>, section: &[&str], page: Option<u32>) -> Self {
        Self {
            text: text.into(),
            section: section.iter().map(|part| (*part).to_owned()).collect(),
            page,
        }
    }
}

#[async_trait]
pub trait Extractor: Send + Sync {
    async fn extract(&self, input: &ExtractionInput) -> Result<Vec<Segment>, AppError>;
}

pub type SharedExtractor = Arc<dyn Extractor>;

pub struct FnExtractor<F>(pub F);

#[async_trait]
impl<F> Extractor for FnExtractor<F>
where
    F: Fn(&ExtractionInput) -> Result<Vec<Segment>, AppError> + Send + Sync,
{
    async fn extract(&self, input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
        (self.0)(input)
    }
}

pub fn js_extname(path: &str) -> String {
    let separator = |character: char| character == '/' || (cfg!(windows) && character == '\\');
    let base = path.rsplit(separator).next().unwrap_or(path);
    if base == ".." {
        return String::new();
    }
    match base.rfind('.') {
        Some(index) if index > 0 => base[index..].to_owned(),
        _ => String::new(),
    }
}

pub fn js_basename(path: &str) -> String {
    let separator = |character: char| character == '/' || (cfg!(windows) && character == '\\');
    path.trim_end_matches(separator)
        .rsplit(separator)
        .next()
        .unwrap_or_default()
        .to_owned()
}

pub fn is_blank(text: &str) -> bool {
    js_trim(text).is_empty()
}

pub fn plain_segments(text: String) -> Vec<Segment> {
    if is_blank(&text) {
        Vec::new()
    } else {
        vec![Segment {
            text,
            section: Vec::new(),
            page: None,
        }]
    }
}

pub fn segments_text(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

static ATX_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$").expect("a valid pattern")
});
static FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})").expect("a valid pattern"));

pub fn split_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

pub fn open_section(path: &mut Vec<Option<String>>, level: usize, title: String) -> Vec<String> {
    path.truncate(level - 1);
    path.resize(level - 1, None);
    path.push(Some(title));
    path.iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect()
}

pub fn markdown_segments(source: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut path: Vec<Option<String>> = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut section: Vec<String> = Vec::new();
    let mut fence: Option<char> = None;
    let flush = |lines: &mut Vec<&str>, section: &[String], segments: &mut Vec<Segment>| {
        let joined = lines.join("\n");
        let text = js_trim_end(joined.trim_start_matches('\n'));
        if !is_blank(text) {
            segments.push(Segment {
                text: text.to_owned(),
                section: section.to_vec(),
                page: None,
            });
        }
        lines.clear();
    };
    for line in split_lines(source) {
        if let Some(marker) = FENCE.captures(line).and_then(|captures| captures.get(1)) {
            let first = marker.as_str().chars().next();
            match fence {
                None => fence = first,
                Some(open) if Some(open) == first => fence = None,
                Some(_) => {}
            }
            lines.push(line);
            continue;
        }
        let heading = if fence.is_none() {
            ATX_HEADING.captures(line)
        } else {
            None
        };
        let Some(heading) = heading else {
            lines.push(line);
            continue;
        };
        let title = js_trim(heading.get(2).map_or("", |title| title.as_str()));
        flush(&mut lines, &section, &mut segments);
        if title.is_empty() {
            continue;
        }
        let level = heading.get(1).map_or(1, |marks| marks.as_str().len());
        section = open_section(&mut path, level, title.to_owned());
        lines.push(title);
    }
    flush(&mut lines, &section, &mut segments);
    segments
}

pub fn decode(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
}

fn split_csv_line(line: &str, separator: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            if character != '"' {
                cell.push(character);
            } else if characters.peek() == Some(&'"') {
                cell.push('"');
                characters.next();
            } else {
                quoted = false;
            }
        } else if character == '"' {
            quoted = true;
        } else if character == separator {
            cells.push(std::mem::take(&mut cell));
        } else {
            cell.push(character);
        }
    }
    cells.push(cell);
    cells
        .into_iter()
        .map(|value| js_trim(&value).to_owned())
        .collect()
}

pub fn plain_text(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    Ok(plain_segments(decode(&input.bytes)))
}

pub fn markdown(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    Ok(markdown_segments(&decode(&input.bytes)))
}

pub fn csv(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    let separator = if js_extname(&input.path).to_lowercase() == ".tsv" {
        '\t'
    } else {
        ','
    };
    let text = decode(&input.bytes);
    let mut lines = split_lines(&text).filter(|line| !is_blank(line));
    let Some(header) = lines.next() else {
        return Ok(Vec::new());
    };
    let columns = split_csv_line(header, separator);
    let rows: Vec<&str> = lines.collect();
    if rows.is_empty() {
        return Ok(plain_segments(columns.join(", ")));
    }
    Ok(plain_segments(
        rows.iter()
            .map(|line| {
                split_csv_line(line, separator)
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let column = columns
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("column {}", index + 1));
                        format!("{column}: {value}")
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

pub const TEXT_EXTENSIONS: &[&str] = &[
    ".txt", ".text", ".log", ".rst", ".adoc", ".json", ".jsonl", ".yaml", ".yml", ".toml", ".ini",
    ".cfg", ".conf", ".env", ".xml", ".html", ".htm", ".css", ".scss", ".ts", ".tsx", ".js",
    ".jsx", ".mjs", ".cjs", ".py", ".rs", ".go", ".java", ".kt", ".swift", ".c", ".h", ".cc",
    ".cpp", ".hpp", ".cs", ".rb", ".php", ".lua", ".sh", ".bash", ".ps1", ".sql", ".graphql",
    ".proto",
];

pub const MARKDOWN_EXTENSIONS: &[&str] = &[".md", ".markdown", ".mdx"];

pub const DEFERRED_EXTENSIONS: &[&str] = &[".doc", ".odt", ".rtf", ".ppt", ".xls", ".ods", ".epub"];

#[derive(Default)]
struct Entries {
    extractors: HashMap<String, SharedExtractor>,
    deferred: HashSet<String>,
}

#[derive(Default)]
pub struct ExtractorRegistry {
    entries: RwLock<Entries>,
}

impl ExtractorRegistry {
    fn read(&self) -> std::sync::RwLockReadGuard<'_, Entries> {
        self.entries.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Entries> {
        self.entries.write().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn register(&self, extensions: &[&str], extractor: SharedExtractor) -> &Self {
        let mut entries = self.write();
        for extension in extensions {
            let key = extension.to_lowercase();
            entries.deferred.remove(&key);
            entries.extractors.insert(key, Arc::clone(&extractor));
        }
        drop(entries);
        self
    }

    pub fn defer(&self, extensions: &[&str]) -> &Self {
        let mut entries = self.write();
        for extension in extensions {
            let key = extension.to_lowercase();
            if !entries.extractors.contains_key(&key) {
                entries.deferred.insert(key);
            }
        }
        drop(entries);
        self
    }

    pub fn supports(&self, path: &str) -> bool {
        self.read()
            .extractors
            .contains_key(&js_extname(path).to_lowercase())
    }

    pub fn reason(&self, path: &str) -> String {
        let extension = js_extname(path).to_lowercase();
        if self.read().deferred.contains(&extension) {
            return format!("формат {extension} пока не поддерживается");
        }
        if extension.is_empty() {
            "файл без расширения".to_owned()
        } else {
            format!("неизвестный формат {extension}")
        }
    }

    pub async fn extract(&self, input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
        let extractor = self
            .read()
            .extractors
            .get(&js_extname(&input.path).to_lowercase())
            .cloned();
        match extractor {
            Some(extractor) => extractor.extract(input).await,
            None => Err(AppError::new(
                AppErrorCode::UnsupportedFormat,
                self.reason(&input.path),
            )),
        }
    }
}

pub fn create_extractor_registry() -> ExtractorRegistry {
    let registry = ExtractorRegistry::default();
    registry
        .register(TEXT_EXTENSIONS, Arc::new(FnExtractor(plain_text)))
        .register(MARKDOWN_EXTENSIONS, Arc::new(FnExtractor(markdown)))
        .register(&[".csv", ".tsv"], Arc::new(FnExtractor(csv)))
        .register(&[".docx"], Arc::new(FnExtractor(docx_segments)))
        .register(&[".xlsx"], Arc::new(FnExtractor(xlsx_segments)))
        .register(&[".pptx"], Arc::new(FnExtractor(pptx_segments)))
        .defer(DEFERRED_EXTENSIONS);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, section: &[&str]) -> Segment {
        Segment::new(text, section, None)
    }

    #[tokio::test]
    async fn the_extractor_registry_accepts_new_formats_without_editing_the_pipeline() {
        let registry = create_extractor_registry();
        assert!(!registry.supports("a.pdf"));
        assert!(registry.reason("a.pdf").contains(".pdf"));
        registry.register(
            &[".pdf"],
            Arc::new(FnExtractor(|input: &ExtractionInput| {
                Ok(vec![Segment::new(
                    format!("extracted {}", input.bytes.len()),
                    &[],
                    None,
                )])
            })),
        );
        assert!(registry.supports("a.pdf"));
        assert_eq!(
            registry
                .extract(&ExtractionInput::new("a.pdf", vec![1, 2, 3]))
                .await
                .expect("extracts"),
            [segment("extracted 3", &[])]
        );
    }

    #[tokio::test]
    async fn csv_rows_are_extracted_as_labelled_lines() {
        let registry = create_extractor_registry();
        let segments = registry
            .extract(&ExtractionInput::new(
                "people.csv",
                b"name,role\n\"ana, b\",support\nbo,design".to_vec(),
            ))
            .await
            .expect("extracts");
        assert_eq!(
            segments,
            [segment(
                "name: ana, b; role: support\nname: bo; role: design",
                &[]
            )]
        );
    }

    #[tokio::test]
    async fn a_tsv_splits_on_tabs_and_a_header_alone_is_one_line() {
        let registry = create_extractor_registry();
        let segments = registry
            .extract(&ExtractionInput::new(
                "a.tsv",
                "a\tb\r\n1\t2\t3\r\n".as_bytes().to_vec(),
            ))
            .await
            .expect("extracts");
        assert_eq!(segments, [segment("a: 1; b: 2; column 3: 3", &[])]);
        let header = registry
            .extract(&ExtractionInput::new("a.csv", b"x, y\n\n".to_vec()))
            .await
            .expect("extracts");
        assert_eq!(header, [segment("x, y", &[])]);
        let empty = registry
            .extract(&ExtractionInput::new("a.csv", b"\n  \n".to_vec()))
            .await
            .expect("extracts");
        assert!(empty.is_empty());
    }

    #[test]
    fn markdown_headings_open_sections_and_a_fenced_block_cannot_fake_one() {
        let source = [
            "Preface line.",
            "",
            "# Guide",
            "Intro.",
            "## Install ##",
            "```",
            "# not a heading",
            "```",
            "### Windows",
            "Run it.",
            "## Use",
            "Go.",
            "#",
            "# Appendix",
        ]
        .join("\n");
        assert_eq!(
            markdown_segments(&source),
            [
                segment("Preface line.", &[]),
                segment("Guide\nIntro.", &["Guide"]),
                segment("Install\n```\n# not a heading\n```", &["Guide", "Install"]),
                segment("Windows\nRun it.", &["Guide", "Install", "Windows"]),
                segment("Use\nGo.", &["Guide", "Use"]),
                segment("Appendix", &["Appendix"]),
            ]
        );
    }

    #[test]
    fn a_skipped_heading_level_leaves_no_empty_section_part() {
        assert_eq!(
            markdown_segments("### Deep\ntext\n# Top\nmore"),
            [
                segment("Deep\ntext", &["Deep"]),
                segment("Top\nmore", &["Top"])
            ]
        );
    }

    #[tokio::test]
    async fn text_is_decoded_leniently_and_loses_its_byte_order_mark() {
        let registry = create_extractor_registry();
        let mut bytes = "\u{feff}привет".as_bytes().to_vec();
        bytes.push(0xff);
        let segments = registry
            .extract(&ExtractionInput::new("a.TXT", bytes))
            .await
            .expect("extracts");
        assert_eq!(segments, [segment("привет\u{fffd}", &[])]);
        let blank = registry
            .extract(&ExtractionInput::new("a.txt", b" \n\t".to_vec()))
            .await
            .expect("extracts");
        assert!(blank.is_empty());
    }

    #[tokio::test]
    async fn the_office_formats_are_registered_and_the_legacy_binary_ones_stay_deferred() {
        let registry = create_extractor_registry();
        assert!(registry.supports("a.docx"));
        assert!(registry.supports("a.xlsx"));
        assert!(registry.supports("a.pptx"));
        assert!(!registry.supports("a.doc"));
        assert!(registry.reason("a.doc").contains("пока не поддерживается"));
        assert_eq!(registry.reason("README"), "файл без расширения");
        assert_eq!(registry.reason("a.xyz"), "неизвестный формат .xyz");
        let error = registry
            .extract(&ExtractionInput::new("a.doc", Vec::new()))
            .await
            .expect_err("declines");
        assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    }

    #[test]
    fn extensions_follow_node_rules() {
        assert_eq!(js_extname("a/b.MD"), ".MD");
        assert_eq!(js_extname("a/.env"), "");
        assert_eq!(js_extname("a/x.env"), ".env");
        assert_eq!(js_extname("a/README"), "");
        assert_eq!(js_extname("a.b/c"), "");
        assert_eq!(js_basename("C:/docs/handbook.md"), "handbook.md");
        assert_eq!(js_basename("/notes/readme/"), "readme");
    }
}
