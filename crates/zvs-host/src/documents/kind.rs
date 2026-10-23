use std::path::Path;
use std::sync::LazyLock;

use regex::bytes::Regex;
use zvs_contract::{AppError, AppErrorCode, DocumentKind, DocumentOfficeFormat};

use super::text::looks_textual;

pub const UNSUPPORTED_DOCUMENT: &str = "Формат файла не поддерживается";
pub const MISLABELLED_DOCUMENT: &str = "Содержимое файла не соответствует его расширению";

const PDF_HEADER_WINDOW: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    Docx,
    Xlsx,
    Pptx,
}

impl OfficeFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Docx => "DOCX",
            Self::Xlsx => "XLSX",
            Self::Pptx => "PPTX",
        }
    }

    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
        }
    }

    pub const fn wire(self) -> DocumentOfficeFormat {
        match self {
            Self::Docx => DocumentOfficeFormat::Docx,
            Self::Xlsx => DocumentOfficeFormat::Xlsx,
            Self::Pptx => DocumentOfficeFormat::Pptx,
        }
    }

    const fn marker(self) -> &'static [u8] {
        match self {
            Self::Docx => b"word/",
            Self::Xlsx => b"xl/",
            Self::Pptx => b"ppt/",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub kind: DocumentKind,
    pub mime_type: &'static str,
    pub extension: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    Native(Detected),
    Office(OfficeFormat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signature {
    Pdf,
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
    Zip,
    Ole,
}

impl Signature {
    const fn image(self) -> Option<Detected> {
        let (mime_type, extension) = match self {
            Self::Png => ("image/png", "png"),
            Self::Jpeg => ("image/jpeg", "jpg"),
            Self::Gif => ("image/gif", "gif"),
            Self::Webp => ("image/webp", "webp"),
            Self::Bmp => ("image/bmp", "bmp"),
            Self::Pdf | Self::Zip | Self::Ole => return None,
        };
        Some(Detected {
            kind: DocumentKind::Image,
            mime_type,
            extension,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    Pdf,
    Image,
    Office(OfficeFormat),
    Text(DocumentKind, &'static str),
    Unknown,
}

const PDF: Detected = Detected {
    kind: DocumentKind::Pdf,
    mime_type: "application/pdf",
    extension: "pdf",
};

const CODE_LANGUAGES: &[(&str, &str)] = &[
    ("rs", "rust"),
    ("ts", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("tsx", "tsx"),
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("jsx", "jsx"),
    ("json", "json"),
    ("jsonc", "jsonc"),
    ("toml", "toml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("xml", "xml"),
    ("svg", "xml"),
    ("html", "html"),
    ("htm", "html"),
    ("css", "css"),
    ("scss", "scss"),
    ("less", "less"),
    ("py", "python"),
    ("rb", "ruby"),
    ("go", "go"),
    ("java", "java"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("c", "c"),
    ("h", "c"),
    ("cpp", "cpp"),
    ("cc", "cpp"),
    ("cxx", "cpp"),
    ("hpp", "cpp"),
    ("cs", "csharp"),
    ("php", "php"),
    ("sh", "shellscript"),
    ("bash", "shellscript"),
    ("zsh", "shellscript"),
    ("ps1", "powershell"),
    ("psm1", "powershell"),
    ("bat", "bat"),
    ("cmd", "bat"),
    ("sql", "sql"),
    ("swift", "swift"),
    ("lua", "lua"),
    ("r", "r"),
    ("dart", "dart"),
    ("vue", "vue"),
    ("svelte", "svelte"),
    ("ini", "ini"),
    ("cfg", "ini"),
    ("conf", "ini"),
    ("gradle", "groovy"),
    ("groovy", "groovy"),
    ("proto", "proto"),
    ("graphql", "graphql"),
    ("gql", "graphql"),
];

const CODE_FILE_NAMES: &[(&str, &str)] = &[("dockerfile", "dockerfile"), ("makefile", "makefile")];

fn lowercase_extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
}

fn lowercase_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn code_language(path: &Path) -> Option<&'static str> {
    let name = lowercase_name(path);
    if let Some((_, language)) = CODE_FILE_NAMES.iter().find(|(known, _)| *known == name) {
        return Some(language);
    }
    let extension = lowercase_extension(path)?;
    CODE_LANGUAGES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, language)| *language)
}

fn claim(path: &Path) -> Claim {
    if code_language(path).is_some() {
        return Claim::Text(DocumentKind::Code, "text/plain");
    }
    match lowercase_extension(path).as_deref() {
        Some("pdf") => Claim::Pdf,
        Some("png" | "jpg" | "jpeg" | "jfif" | "gif" | "webp" | "bmp") => Claim::Image,
        Some("docx") => Claim::Office(OfficeFormat::Docx),
        Some("xlsx") => Claim::Office(OfficeFormat::Xlsx),
        Some("pptx") => Claim::Office(OfficeFormat::Pptx),
        Some("md" | "markdown" | "mdx") => Claim::Text(DocumentKind::Markdown, "text/markdown"),
        Some("csv") => Claim::Text(DocumentKind::Csv, "text/csv"),
        Some("tsv") => Claim::Text(DocumentKind::Csv, "text/tab-separated-values"),
        Some("txt" | "text" | "log") => Claim::Text(DocumentKind::Text, "text/plain"),
        _ => Claim::Unknown,
    }
}

fn is_bmp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BM")
        && bytes.get(14..18).is_some_and(|header| {
            matches!(
                u32::from_le_bytes([header[0], header[1], header[2], header[3]]),
                12 | 40 | 52 | 56 | 64 | 108 | 124
            )
        })
}

fn signature(bytes: &[u8]) -> Option<Signature> {
    let head = &bytes[..bytes.len().min(PDF_HEADER_WINDOW)];
    if head.windows(5).any(|window| window == b"%PDF-") {
        return Some(Signature::Pdf);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Signature::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Signature::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(Signature::Gif);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(Signature::Webp);
    }
    if is_bmp(bytes) {
        return Some(Signature::Bmp);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(Signature::Zip);
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Some(Signature::Ole);
    }
    None
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn extension_detail(path: &Path) -> String {
    lowercase_extension(path).unwrap_or_default()
}

pub fn unsupported(path: &Path) -> AppError {
    AppError::new(AppErrorCode::UnsupportedFormat, UNSUPPORTED_DOCUMENT)
        .with_detail("extension", extension_detail(path))
}

fn mislabelled(path: &Path) -> AppError {
    AppError::new(AppErrorCode::UnsupportedFormat, MISLABELLED_DOCUMENT)
        .with_detail("extension", extension_detail(path))
        .with_detail("reason", "mislabelled")
}

const fn text(kind: DocumentKind, mime_type: &'static str) -> Detected {
    let extension = match kind {
        DocumentKind::Markdown => "md",
        DocumentKind::Csv => "csv",
        _ => "txt",
    };
    Detected {
        kind,
        mime_type,
        extension,
    }
}

pub fn detect(path: &Path, bytes: &[u8]) -> Result<Detection, AppError> {
    let found = signature(bytes);
    match claim(path) {
        Claim::Pdf if found == Some(Signature::Pdf) => Ok(Detection::Native(PDF)),
        Claim::Image => found
            .and_then(Signature::image)
            .map(Detection::Native)
            .ok_or_else(|| mislabelled(path)),
        Claim::Office(format)
            if found == Some(Signature::Zip) && contains(bytes, format.marker()) =>
        {
            Ok(Detection::Office(format))
        }
        Claim::Text(kind, mime_type) if found.is_none() && looks_textual(bytes) => {
            Ok(Detection::Native(text(kind, mime_type)))
        }
        Claim::Pdf | Claim::Office(_) | Claim::Text(..) => Err(mislabelled(path)),
        Claim::Unknown => match found {
            Some(Signature::Pdf) => Ok(Detection::Native(PDF)),
            Some(signature) => signature
                .image()
                .map(Detection::Native)
                .ok_or_else(|| unsupported(path)),
            None if looks_textual(bytes) => {
                Ok(Detection::Native(text(DocumentKind::Text, "text/plain")))
            }
            None => Err(unsupported(path)),
        },
    }
}

static PAGE_OBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/Type\s*/Page(?:[^A-Za-z]|$)").expect("a static pattern"));

pub fn pdf_page_estimate(bytes: &[u8]) -> Option<u64> {
    let pages = PAGE_OBJECT.find_iter(bytes).count();
    u64::try_from(pages).ok().filter(|pages| *pages > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDF_BYTES: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Pages /Kids [2 0 R 3 0 R] /Count 2 >> endobj\n\
        2 0 obj << /Type /Page /Parent 1 0 R >> endobj\n3 0 obj << /Type/Page/Parent 1 0 R >> endobj\n%%EOF";
    const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn native(path: &str, bytes: &[u8]) -> Detected {
        match detect(Path::new(path), bytes).expect("detects") {
            Detection::Native(detected) => detected,
            Detection::Office(format) => panic!("unexpected office format {format:?}"),
        }
    }

    fn refusal(path: &str, bytes: &[u8]) -> AppError {
        detect(Path::new(path), bytes).expect_err("refuses")
    }

    #[test]
    fn content_confirms_what_the_extension_claims() {
        assert_eq!(native("C:/a/report.pdf", PDF_BYTES).kind, DocumentKind::Pdf);
        assert_eq!(native("C:/a/logo.PNG", PNG_BYTES).mime_type, "image/png");
        assert_eq!(
            native("C:/a/notes.md", b"# Plan\n").kind,
            DocumentKind::Markdown
        );
        assert_eq!(
            native("C:/a/data.csv", b"a,b\n1,2\n").kind,
            DocumentKind::Csv
        );
        assert_eq!(
            native("C:/a/main.rs", b"fn main() {}\n").kind,
            DocumentKind::Code
        );
        assert_eq!(
            native("C:/a/Dockerfile", b"FROM rust\n").kind,
            DocumentKind::Code
        );
    }

    #[test]
    fn a_wrong_extension_fails_cleanly() {
        for (path, bytes) in [
            ("C:/a/report.pdf", PNG_BYTES),
            ("C:/a/logo.png", PDF_BYTES),
            ("C:/a/notes.md", PDF_BYTES),
            ("C:/a/notes.txt", PNG_BYTES),
            ("C:/a/data.csv", b"\0\x01\x02binary".as_slice()),
            ("C:/a/letter.docx", b"plain text".as_slice()),
        ] {
            let error = refusal(path, bytes);
            assert_eq!(error.code, AppErrorCode::UnsupportedFormat, "{path}");
            assert_eq!(error.message, MISLABELLED_DOCUMENT, "{path}");
        }
    }

    #[test]
    fn an_image_keeps_the_type_its_content_names() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
        let detected = native("C:/a/photo.png", &jpeg);
        assert_eq!(detected.kind, DocumentKind::Image);
        assert_eq!(detected.mime_type, "image/jpeg");
        assert_eq!(detected.extension, "jpg");
    }

    #[test]
    fn an_unknown_extension_is_read_by_its_content() {
        assert_eq!(native("C:/a/scan.bin", PDF_BYTES).kind, DocumentKind::Pdf);
        assert_eq!(native("C:/a/README", b"hello\n").kind, DocumentKind::Text);
        let error = refusal("C:/a/archive.bin", b"\0\0\0\x01");
        assert_eq!(error.message, UNSUPPORTED_DOCUMENT);
        assert_eq!(
            refusal(
                "C:/a/legacy.doc",
                &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]
            )
            .message,
            UNSUPPORTED_DOCUMENT
        );
    }

    #[test]
    fn a_bitmap_needs_more_than_two_letters() {
        assert_eq!(
            native("C:/a/notes.md", b"BMW notes\n").kind,
            DocumentKind::Markdown
        );
    }

    #[test]
    fn office_files_are_recognised_by_their_package_parts() {
        let docx = b"PK\x03\x04....[Content_Types].xml....PK\x03\x04word/document.xml";
        assert_eq!(
            detect(Path::new("C:/a/letter.docx"), docx).expect("detects"),
            Detection::Office(OfficeFormat::Docx)
        );
        assert_eq!(
            refusal("C:/a/sheet.xlsx", docx).message,
            MISLABELLED_DOCUMENT
        );
    }

    #[test]
    fn the_page_estimate_counts_page_objects_not_the_tree() {
        assert_eq!(pdf_page_estimate(PDF_BYTES), Some(2));
        assert_eq!(pdf_page_estimate(b"%PDF-1.7\n%%EOF"), None);
    }
}
