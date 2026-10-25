use super::manifest::{VERSION, parse_manifest};

pub struct BundledFile {
    pub path: &'static str,
    pub content: &'static str,
}

pub struct BundledSkill {
    pub key: &'static str,
    pub manifest: &'static str,
    pub files: &'static [BundledFile],
}

impl BundledSkill {
    pub fn version(&self) -> Option<String> {
        parse_manifest(self.manifest)
            .ok()
            .and_then(|manifest| manifest.meta(VERSION).map(str::to_owned))
    }
}

pub fn version_order(value: &str) -> Option<[u64; 3]> {
    let core = value.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or_default();
    let mut order = [0; 3];
    let mut parts = core.split('.');
    for slot in &mut order {
        match parts.next() {
            Some(part) => *slot = part.parse().ok()?,
            None => break,
        }
    }
    if parts.next().is_some() || core.is_empty() {
        return None;
    }
    Some(order)
}

pub fn is_newer(candidate: &str, installed: Option<&str>) -> bool {
    let Some(candidate) = version_order(candidate) else {
        return false;
    };
    candidate > installed.and_then(version_order).unwrap_or_default()
}

macro_rules! bundled_file {
    ($skill:literal, $path:literal) => {
        BundledFile {
            path: $path,
            content: include_str!(concat!("../../skills/", $skill, "/", $path)),
        }
    };
}

macro_rules! bundled_skill {
    ($skill:literal, [$($path:literal),* $(,)?]) => {
        BundledSkill {
            key: $skill,
            manifest: include_str!(concat!("../../skills/", $skill, "/SKILL.md")),
            files: &[$(bundled_file!($skill, $path)),*],
        }
    };
}

pub const BUNDLED_SKILLS: &[BundledSkill] = &[
    bundled_skill!(
        "pdf",
        [
            "references/forms.md",
            "references/reference.md",
            "scripts/convert_to_pdf.py",
            "scripts/create_pdf.py",
            "scripts/extract_tables.py",
            "scripts/extract_text.py",
            "scripts/fill_form.py",
            "scripts/find_text.py",
            "scripts/form_fields.py",
            "scripts/merge.py",
            "scripts/pdf_info.py",
            "scripts/protect.py",
            "scripts/rotate.py",
            "scripts/select_pages.py",
            "scripts/split.py",
            "scripts/watermark.py",
        ]
    ),
    bundled_skill!(
        "docx",
        [
            "references/ooxml.md",
            "references/reference.md",
            "scripts/_blocks.py",
            "scripts/docx_info.py",
            "scripts/docx_to_markdown.py",
            "scripts/extract_blocks.py",
            "scripts/inspect_blocks.py",
            "scripts/markdown_to_docx.py",
            "scripts/ooxml.py",
            "scripts/replace_text.py",
        ]
    ),
    bundled_skill!(
        "xlsx",
        [
            "references/reference.md",
            "references/spec.md",
            "scripts/check_formulas.py",
            "scripts/read_sheet.py",
            "scripts/write_workbook.py",
            "scripts/xlsx_info.py",
        ]
    ),
];
