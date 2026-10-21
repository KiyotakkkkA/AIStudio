use zvs_contract::{ProviderId, SkillModelDto};

pub const SKILL_FILE: &str = "SKILL.md";

const FENCE: &str = "---";
const NAME: &str = "name";
const DESCRIPTION: &str = "description";
const ALLOWED_TOOLS: &str = "allowed-tools";
const METADATA: &str = "metadata";
const DISPLAY_NAME: &str = "display-name";
const MODEL_PROVIDER: &str = "model-provider";
const MODEL: &str = "model";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub display_name: Option<String>,
    pub model: Option<SkillModelDto>,
    pub extra: Vec<(String, String)>,
    pub metadata: Vec<(String, String)>,
    pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    pub line: usize,
}

fn unquote(raw: &str, line: usize) -> Result<String, ManifestError> {
    let value = raw.trim();
    if value.starts_with('"') {
        return serde_json::from_str::<String>(value).map_err(|_| ManifestError { line });
    }
    if let Some(inner) = value.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'').ok_or(ManifestError { line })?;
        return Ok(inner.replace("''", "'"));
    }
    if matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
        return Err(ManifestError { line });
    }
    let plain = value
        .split_once(" #")
        .map_or(value, |(before, _)| before)
        .trim_end();
    Ok(plain.to_owned())
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

fn split_entry(line: &str, number: usize) -> Result<(String, String), ManifestError> {
    let (key, value) = line.split_once(':').ok_or(ManifestError { line: number })?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return Err(ManifestError { line: number });
    }
    Ok((key.to_owned(), value.to_owned()))
}

fn tool_list(value: &str) -> Vec<String> {
    value
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|tool| !tool.is_empty())
        .map(str::to_owned)
        .collect()
}

fn indented(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

pub fn parse_manifest(text: &str) -> Result<Manifest, ManifestError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|line| line.trim_end()) != Some(FENCE) {
        return Err(ManifestError { line: 1 });
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim_end() == FENCE)
        .map(|(index, _)| index)
        .ok_or(ManifestError { line: lines.len() })?;
    let mut manifest = Manifest::default();
    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut index = 1;
    while index < close {
        let number = index + 1;
        let line = lines[index];
        index += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if indented(line) {
            return Err(ManifestError { line: number });
        }
        let (key, value) = split_entry(line, number)?;
        let nested_lines = || {
            let mut nested = Vec::new();
            let mut cursor = index;
            while cursor < close && (indented(lines[cursor]) || lines[cursor].trim().is_empty()) {
                if !lines[cursor].trim().is_empty() {
                    nested.push((cursor + 1, lines[cursor].trim()));
                }
                cursor += 1;
            }
            (nested, cursor)
        };
        match key.as_str() {
            METADATA => {
                if !value.trim().is_empty() {
                    return Err(ManifestError { line: number });
                }
                let (nested, next) = nested_lines();
                index = next;
                for (number, entry) in nested {
                    let (name, value) = split_entry(entry, number)?;
                    let value = unquote(&value, number)?;
                    match name.as_str() {
                        DISPLAY_NAME => manifest.display_name = Some(value),
                        MODEL_PROVIDER => provider = Some(value),
                        MODEL => model = Some(value),
                        _ => manifest.metadata.push((name, value)),
                    }
                }
            }
            ALLOWED_TOOLS if value.trim().is_empty() => {
                let (nested, next) = nested_lines();
                index = next;
                for (number, entry) in nested {
                    let item = entry
                        .strip_prefix('-')
                        .ok_or(ManifestError { line: number })?;
                    manifest.allowed_tools.push(unquote(item, number)?);
                }
            }
            ALLOWED_TOOLS => {
                let value = value.trim();
                if let Some(inner) = value
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
                {
                    for item in inner.split(',').filter(|item| !item.trim().is_empty()) {
                        manifest.allowed_tools.push(unquote(item, number)?);
                    }
                } else {
                    manifest.allowed_tools = tool_list(&unquote(value, number)?);
                }
            }
            NAME => manifest.name = unquote(&value, number)?,
            DESCRIPTION => manifest.description = unquote(&value, number)?,
            _ => manifest.extra.push((key, unquote(&value, number)?)),
        }
    }
    manifest.model = match (provider, model) {
        (Some(provider), Some(model)) => Some(SkillModelDto {
            provider_id: ProviderId::parse(provider.trim()).ok_or(ManifestError { line: 1 })?,
            model_id: model,
        }),
        (None, None) => None,
        _ => return Err(ManifestError { line: 1 }),
    };
    manifest.instructions = lines[close + 1..].join("\n").trim().to_owned();
    Ok(manifest)
}

pub fn render_manifest(manifest: &Manifest) -> String {
    let mut out = String::new();
    out.push_str(FENCE);
    out.push('\n');
    out.push_str(&format!("{NAME}: {}\n", quote(&manifest.name)));
    out.push_str(&format!(
        "{DESCRIPTION}: {}\n",
        quote(&manifest.description)
    ));
    if !manifest.allowed_tools.is_empty() {
        out.push_str(&format!(
            "{ALLOWED_TOOLS}: {}\n",
            quote(&manifest.allowed_tools.join(" "))
        ));
    }
    for (key, value) in &manifest.extra {
        out.push_str(&format!("{key}: {}\n", quote(value)));
    }
    let mut metadata: Vec<(&str, String)> = Vec::new();
    if let Some(display_name) = &manifest.display_name {
        metadata.push((DISPLAY_NAME, display_name.clone()));
    }
    if let Some(model) = &manifest.model {
        metadata.push((MODEL_PROVIDER, model.provider_id.to_string()));
        metadata.push((MODEL, model.model_id.clone()));
    }
    for (key, value) in &manifest.metadata {
        metadata.push((key.as_str(), value.clone()));
    }
    if !metadata.is_empty() {
        out.push_str(&format!("{METADATA}:\n"));
        for (key, value) in metadata {
            out.push_str(&format!("  {key}: {}\n", quote(&value)));
        }
    }
    out.push_str(FENCE);
    out.push_str("\n\n");
    out.push_str(manifest.instructions.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        Manifest {
            name: "release-notes".to_owned(),
            description: "Составляет заметки: \"кратко\" и по делу".to_owned(),
            allowed_tools: vec!["vector.search".to_owned(), "skill.read".to_owned()],
            display_name: Some("Заметки о выпуске".to_owned()),
            model: Some(SkillModelDto {
                provider_id: ProviderId::parse("0199aa11-2222-7222-8222-222222222222")
                    .expect("an id"),
                model_id: "gpt-4o-mini".to_owned(),
            }),
            extra: vec![("license".to_owned(), "MIT".to_owned())],
            metadata: vec![("author".to_owned(), "team".to_owned())],
            instructions: "# Шаги\n\n1. Сгруппируйте изменения.".to_owned(),
        }
    }

    #[test]
    fn a_rendered_manifest_parses_back_to_the_same_skill() {
        let manifest = sample();
        assert_eq!(parse_manifest(&render_manifest(&manifest)), Ok(manifest));
    }

    #[test]
    fn hand_written_frontmatter_in_the_common_shapes_is_understood() {
        let text = "\u{feff}---\nname: pdf\ndescription: Extract text from PDF files # inline\n\
            allowed-tools:\n  - vector.search\n  - \"skill.read\"\nlicense: 'it''s fine'\n\
            metadata:\n  display-name: PDF\n---\n\nUse the tools.\n";
        let manifest = parse_manifest(text).expect("parses");
        assert_eq!(manifest.name, "pdf");
        assert_eq!(manifest.description, "Extract text from PDF files");
        assert_eq!(manifest.allowed_tools, ["vector.search", "skill.read"]);
        assert_eq!(
            manifest.extra,
            [("license".to_owned(), "it's fine".to_owned())]
        );
        assert_eq!(manifest.display_name.as_deref(), Some("PDF"));
        assert_eq!(manifest.instructions, "Use the tools.");
        let inline =
            parse_manifest("---\nname: a\nallowed-tools: [a.b, c.d]\n---\n").expect("parses");
        assert_eq!(inline.allowed_tools, ["a.b", "c.d"]);
        let spaced =
            parse_manifest("---\nname: a\nallowed-tools: a.b c.d, e.f\n---\n").expect("parses");
        assert_eq!(spaced.allowed_tools, ["a.b", "c.d", "e.f"]);
    }

    #[test]
    fn malformed_frontmatter_is_reported_with_its_line() {
        assert_eq!(parse_manifest("no fence"), Err(ManifestError { line: 1 }));
        assert!(parse_manifest("---\nname: a\n").is_err());
        assert_eq!(
            parse_manifest("---\nname: a\n  stray: b\n---\n"),
            Err(ManifestError { line: 3 })
        );
        assert_eq!(
            parse_manifest("---\ndescription: >\n  folded\n---\n"),
            Err(ManifestError { line: 2 })
        );
        assert!(parse_manifest("---\nmetadata:\n  model: m\n---\n").is_err());
    }
}
