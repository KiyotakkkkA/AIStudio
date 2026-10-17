use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use regex::{Captures, Regex, RegexBuilder};
use zvs_contract::validate::js_trim;
use zvs_contract::{AppError, AppErrorCode};

use super::extraction::{ExtractionInput, Segment, is_blank, open_section, plain_segments};
use crate::platform::archive::read_zip_entries;

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("a valid pattern")
}

static ENTITY: LazyLock<Regex> = LazyLock::new(|| pattern(r"&(#x?[0-9a-fA-F]+|[a-zA-Z]+);"));
static RUNS: LazyLock<Mutex<HashMap<String, Regex>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static SPACES: LazyLock<Regex> = LazyLock::new(|| pattern(r" {2,}"));
static TRAILING: LazyLock<Regex> = LazyLock::new(|| pattern(r"[ \t]+$"));
static TAB: LazyLock<Regex> = LazyLock::new(|| pattern(r"<w:tab\b[^>]*/?>"));
static OUTLINE: LazyLock<Regex> = LazyLock::new(|| pattern(r#"<w:outlineLvl\b[^>]*w:val="(\d+)""#));
static STYLE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"<w:style\b([^>]*)>([\s\S]*?)</w:style>"));
static STYLE_ID: LazyLock<Regex> = LazyLock::new(|| pattern(r#"w:styleId="([^"]+)""#));
static STYLE_NAME: LazyLock<Regex> = LazyLock::new(|| pattern(r#"<w:name\b[^>]*w:val="([^"]+)""#));
static NAMED_HEADING: LazyLock<Regex> = LazyLock::new(|| pattern(r"^heading (\d)$"));
static PARAGRAPH_PROPERTIES: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"<w:pPr\b[^>]*>([\s\S]*?)</w:pPr>"));
static PARAGRAPH_STYLE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r#"<w:pStyle\b[^>]*w:val="([^"]+)""#));
static FALLBACK_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^(?:heading|заголовок)\s*(\d)$")
        .case_insensitive(true)
        .build()
        .expect("a valid pattern")
});
static SHEET: LazyLock<Regex> = LazyLock::new(|| pattern(r"^xl/worksheets/sheet\d+\.xml$"));
static CELL: LazyLock<Regex> = LazyLock::new(|| pattern(r"<c\b([^>]*)>([\s\S]*?)</c>"));
static CELL_TYPE: LazyLock<Regex> = LazyLock::new(|| pattern(r#"\bt="([^"]+)""#));
static TRAILING_TABS: LazyLock<Regex> = LazyLock::new(|| pattern(r"\t+$"));
static SLIDE: LazyLock<Regex> = LazyLock::new(|| pattern(r"^ppt/slides/slide\d+\.xml$"));
static SLIDE_NUMBER: LazyLock<Regex> = LazyLock::new(|| pattern(r"slide(\d+)\.xml$"));
static TITLE_PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| pattern(r#"<p:ph\b[^>]*type="(?:title|ctrTitle)""#));
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| pattern(r"\s+"));

pub const BODY_OUTLINE_LEVEL: u64 = 9;

pub fn js_parse_int(text: &str, radix: u32) -> Option<u64> {
    let digits: String = js_trim(text)
        .chars()
        .take_while(|character| character.is_digit(radix))
        .collect();
    if digits.is_empty() {
        return None;
    }
    Some(u64::from_str_radix(&digits, radix).unwrap_or(u64::MAX))
}

fn named_entity(name: &str) -> Option<&'static str> {
    match name.to_lowercase().as_str() {
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "nbsp" => Some(" "),
        _ => None,
    }
}

pub fn decode_xml_text(raw: &str) -> String {
    ENTITY
        .replace_all(raw, |captures: &Captures<'_>| {
            let whole = captures[0].to_owned();
            let entity = &captures[1];
            if let Some(number) = entity.strip_prefix('#') {
                let code = match number.strip_prefix('x') {
                    Some(hex) => js_parse_int(hex, 16),
                    None => js_parse_int(number, 10),
                };
                return code
                    .filter(|code| *code > 0)
                    .and_then(|code| u32::try_from(code).ok())
                    .and_then(char::from_u32)
                    .map_or(whole, |character| character.to_string());
            }
            named_entity(entity).map_or(whole, str::to_owned)
        })
        .into_owned()
}

fn run_pattern(tag: &str) -> Regex {
    let mut cache = RUNS.lock().unwrap_or_else(PoisonError::into_inner);
    cache
        .entry(tag.to_owned())
        .or_insert_with(|| {
            let tag = regex::escape(tag);
            pattern(&format!(r"<{tag}(?:\s[^>]*)?>([\s\S]*?)</{tag}>"))
        })
        .clone()
}

pub fn text_runs(xml: &str, tag: &str) -> Vec<String> {
    run_pattern(tag)
        .captures_iter(xml)
        .map(|captures| decode_xml_text(captures.get(1).map_or("", |run| run.as_str())))
        .collect()
}

fn blocks<'a>(xml: &'a str, closing: &str) -> Vec<&'a str> {
    xml.split(closing).collect()
}

pub fn tidy(lines: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let spaced = SPACES.replace_all(line, " ");
        let normalised = TRAILING.replace(&spaced, "").into_owned();
        let blank = is_blank(&normalised);
        if blank && out.last().is_none_or(String::is_empty) {
            continue;
        }
        out.push(if blank { String::new() } else { normalised });
    }
    out.join("\n").trim_matches('\n').to_owned()
}

fn parts(
    bytes: &[u8],
    wanted: impl Fn(&str) -> bool,
    what: &str,
) -> Result<Vec<(String, String)>, AppError> {
    let found = read_zip_entries(bytes, wanted)?;
    if found.is_empty() {
        return Err(AppError::new(
            AppErrorCode::UnsupportedFormat,
            format!("В файле нет части {what}"),
        ));
    }
    Ok(found
        .into_iter()
        .map(|(name, content)| (name, String::from_utf8_lossy(&content).into_owned()))
        .collect())
}

fn part<'a>(found: &'a [(String, String)], name: &str) -> Option<&'a str> {
    found
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, content)| content.as_str())
}

fn outline_level(xml: &str) -> Option<usize> {
    let value = OUTLINE.captures(xml)?.get(1)?.as_str();
    let level = js_parse_int(value, 10)?;
    if level >= BODY_OUTLINE_LEVEL {
        None
    } else {
        usize::try_from(level + 1).ok()
    }
}

pub fn heading_levels(styles: &str) -> Vec<(String, usize)> {
    let mut levels: Vec<(String, usize)> = Vec::new();
    for captures in STYLE.captures_iter(styles) {
        let attributes = captures.get(1).map_or("", |value| value.as_str());
        let body = captures.get(2).map_or("", |value| value.as_str());
        let Some(id) = STYLE_ID
            .captures(attributes)
            .and_then(|found| found.get(1))
            .map(|found| found.as_str().to_owned())
        else {
            continue;
        };
        let name = STYLE_NAME
            .captures(body)
            .and_then(|found| found.get(1))
            .map(|found| found.as_str().to_lowercase())
            .unwrap_or_default();
        let named = NAMED_HEADING
            .captures(&name)
            .and_then(|found| found.get(1))
            .and_then(|found| js_parse_int(found.as_str(), 10))
            .and_then(|level| usize::try_from(level).ok());
        let level = match named {
            Some(level) => Some(level),
            None if name == "title" => Some(1),
            None => outline_level(body),
        };
        if let Some(level) = level {
            match levels.iter_mut().find(|(existing, _)| *existing == id) {
                Some(entry) => entry.1 = level,
                None => levels.push((id, level)),
            }
        }
    }
    levels
}

fn fallback_level(style: &str) -> Option<usize> {
    FALLBACK_HEADING
        .captures(style)
        .and_then(|found| found.get(1))
        .and_then(|found| js_parse_int(found.as_str(), 10))
        .and_then(|level| usize::try_from(level).ok())
}

fn paragraph_level(paragraph: &str, levels: &[(String, usize)]) -> Option<usize> {
    let properties = PARAGRAPH_PROPERTIES
        .captures(paragraph)
        .and_then(|found| found.get(1))
        .map_or("", |found| found.as_str());
    if let Some(direct) = outline_level(properties) {
        return Some(direct);
    }
    let style = PARAGRAPH_STYLE
        .captures(properties)
        .and_then(|found| found.get(1))?
        .as_str();
    levels
        .iter()
        .find(|(id, _)| id == style)
        .map(|(_, level)| *level)
        .or_else(|| fallback_level(style))
}

pub fn docx_segments(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    let found = parts(
        &input.bytes,
        |name| name == "word/document.xml" || name == "word/styles.xml",
        "word/document.xml",
    )?;
    let xml = part(&found, "word/document.xml").ok_or_else(|| {
        AppError::new(
            AppErrorCode::UnsupportedFormat,
            "В файле нет части word/document.xml",
        )
    })?;
    let levels = heading_levels(part(&found, "word/styles.xml").unwrap_or_default());
    let mut segments = Vec::new();
    let mut path: Vec<Option<String>> = Vec::new();
    let mut section: Vec<String> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let flush = |lines: &mut Vec<String>, section: &[String], segments: &mut Vec<Segment>| {
        let text = tidy(lines);
        if !text.is_empty() {
            segments.push(Segment {
                text,
                section: section.to_vec(),
                page: None,
            });
        }
        lines.clear();
    };
    for paragraph in blocks(xml, "</w:p>") {
        let tabbed = TAB.replace_all(paragraph, "<w:t>\t</w:t>");
        let line = text_runs(&tabbed, "w:t").concat();
        let level = paragraph_level(paragraph, &levels);
        if let Some(level) = level
            && !is_blank(&line)
            && level > 0
        {
            flush(&mut lines, &section, &mut segments);
            section = open_section(&mut path, level, js_trim(&line).to_owned());
        }
        lines.push(line);
    }
    flush(&mut lines, &section, &mut segments);
    Ok(segments)
}

pub fn xlsx_segments(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    let found = parts(
        &input.bytes,
        |name| name == "xl/sharedStrings.xml" || SHEET.is_match(name),
        "xl/worksheets",
    )?;
    let shared = text_runs(
        part(&found, "xl/sharedStrings.xml").unwrap_or_default(),
        "t",
    );
    let mut sheets: Vec<&(String, String)> = found
        .iter()
        .filter(|(name, _)| name.starts_with("xl/worksheets/"))
        .collect();
    sheets.sort_by(|left, right| left.0.cmp(&right.0));

    let mut lines = Vec::new();
    for (_, sheet) in sheets {
        for row in blocks(sheet, "</row>") {
            let mut cells: Vec<String> = Vec::new();
            for cell in CELL.captures_iter(row) {
                let attributes = cell.get(1).map_or("", |value| value.as_str());
                let body = cell.get(2).map_or("", |value| value.as_str());
                let kind = CELL_TYPE
                    .captures(attributes)
                    .and_then(|found| found.get(1))
                    .map(|found| found.as_str());
                let value = match kind {
                    Some("s") => text_runs(body, "v")
                        .first()
                        .and_then(|index| js_parse_int(index, 10))
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| shared.get(index).cloned())
                        .unwrap_or_default(),
                    Some("inlineStr") => text_runs(body, "t").concat(),
                    _ => text_runs(body, "v").into_iter().next().unwrap_or_default(),
                };
                cells.push(value);
            }
            let joined = cells.join("\t");
            let line = TRAILING_TABS.replace(&joined, "").into_owned();
            if !is_blank(&line) {
                lines.push(line);
            }
        }
    }
    Ok(plain_segments(tidy(&lines)))
}

fn paragraphs(xml: &str) -> Vec<String> {
    blocks(xml, "</a:p>")
        .into_iter()
        .map(|paragraph| text_runs(paragraph, "a:t").concat())
        .collect()
}

fn slide_title(xml: &str) -> String {
    for shape in xml.split("</p:sp>") {
        if !TITLE_PLACEHOLDER.is_match(shape) {
            continue;
        }
        let joined = paragraphs(shape).join(" ");
        return js_trim(&WHITESPACE.replace_all(&joined, " ")).to_owned();
    }
    String::new()
}

fn slide_number(name: &str) -> u64 {
    SLIDE_NUMBER
        .captures(name)
        .and_then(|found| found.get(1))
        .and_then(|found| js_parse_int(found.as_str(), 10))
        .unwrap_or(0)
}

pub fn pptx_segments(input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
    let mut found = parts(&input.bytes, |name| SLIDE.is_match(name), "ppt/slides")?;
    found.sort_by_key(|(name, _)| slide_number(name));
    let mut segments = Vec::new();
    for (index, (_, xml)) in found.iter().enumerate() {
        let text = tidy(&paragraphs(xml));
        if text.is_empty() {
            continue;
        }
        let title = slide_title(xml);
        segments.push(Segment {
            text,
            section: if title.is_empty() {
                Vec::new()
            } else {
                vec![title]
            },
            page: u32::try_from(index + 1).ok(),
        });
    }
    Ok(segments)
}

#[cfg(test)]
mod tests;
