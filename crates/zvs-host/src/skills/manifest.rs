pub const SKILL_FILE: &str = "SKILL.md";

const FENCE: &str = "---";
const NAME: &str = "name";
const DESCRIPTION: &str = "description";
const ALLOWED_TOOLS: &str = "allowed-tools";
const METADATA: &str = "metadata";
const DISPLAY_NAME: &str = "display-name";
pub const VERSION: &str = "version";
pub const AUTHOR: &str = "author";
pub const READ_FIRST: &str = "read-first";
pub const REQUIRES: &str = "requires";

pub type Requires = Vec<(String, Vec<String>)>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub display_name: Option<String>,
    pub extra: Vec<(String, String)>,
    pub metadata: Vec<(String, String)>,
    pub instructions: String,
}

impl Manifest {
    pub fn meta(&self, key: &str) -> Option<&str> {
        meta(&self.metadata, key)
    }
}

pub fn meta<'a>(metadata: &'a [(String, String)], key: &str) -> Option<&'a str> {
    metadata
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

pub fn read_first(metadata: &[(String, String)]) -> Option<Vec<String>> {
    meta(metadata, READ_FIRST).map_or(Some(Vec::new()), |value| match Flow::parse(value)? {
        Flow::List(items) => items.into_iter().map(Flow::scalar).collect(),
        Flow::Scalar(item) => Some(vec![item]),
        Flow::Map(_) => None,
    })
}

pub fn requires(metadata: &[(String, String)]) -> Option<Requires> {
    meta(metadata, REQUIRES).map_or(Some(Vec::new()), |value| {
        let Flow::Map(entries) = Flow::parse(value)? else {
            return None;
        };
        entries
            .into_iter()
            .map(|(script, files)| {
                let files = match files {
                    Flow::List(items) => items.into_iter().map(Flow::scalar).collect(),
                    Flow::Scalar(item) => Some(vec![item]),
                    Flow::Map(_) => None,
                }?;
                Some((script, files))
            })
            .collect()
    })
}

enum Flow {
    Scalar(String),
    List(Vec<Flow>),
    Map(Vec<(String, Flow)>),
}

impl Flow {
    fn parse(text: &str) -> Option<Self> {
        let chars: Vec<char> = text.chars().collect();
        let mut cursor = 0;
        let value = Self::value(&chars, &mut cursor)?;
        Self::skip(&chars, &mut cursor);
        (cursor == chars.len()).then_some(value)
    }

    fn scalar(self) -> Option<String> {
        match self {
            Self::Scalar(value) => Some(value),
            Self::List(_) | Self::Map(_) => None,
        }
    }

    fn skip(chars: &[char], cursor: &mut usize) {
        while chars
            .get(*cursor)
            .is_some_and(|character| character.is_whitespace())
        {
            *cursor += 1;
        }
    }

    fn value(chars: &[char], cursor: &mut usize) -> Option<Self> {
        Self::skip(chars, cursor);
        match chars.get(*cursor)? {
            '[' => {
                *cursor += 1;
                let mut items = Vec::new();
                loop {
                    Self::skip(chars, cursor);
                    if chars.get(*cursor) == Some(&']') {
                        *cursor += 1;
                        return Some(Self::List(items));
                    }
                    items.push(Self::value(chars, cursor)?);
                    Self::skip(chars, cursor);
                    match chars.get(*cursor)? {
                        ',' => *cursor += 1,
                        ']' => {}
                        _ => return None,
                    }
                }
            }
            '{' => {
                *cursor += 1;
                let mut entries = Vec::new();
                loop {
                    Self::skip(chars, cursor);
                    if chars.get(*cursor) == Some(&'}') {
                        *cursor += 1;
                        return Some(Self::Map(entries));
                    }
                    let Self::Scalar(key) = Self::value(chars, cursor)? else {
                        return None;
                    };
                    Self::skip(chars, cursor);
                    if chars.get(*cursor) != Some(&':') {
                        return None;
                    }
                    *cursor += 1;
                    entries.push((key, Self::value(chars, cursor)?));
                    Self::skip(chars, cursor);
                    match chars.get(*cursor)? {
                        ',' => *cursor += 1,
                        '}' => {}
                        _ => return None,
                    }
                }
            }
            '"' => {
                let start = *cursor;
                *cursor += 1;
                while *cursor < chars.len() && chars[*cursor] != '"' {
                    *cursor += if chars[*cursor] == '\\' { 2 } else { 1 };
                }
                *cursor += 1;
                let quoted: String = chars.get(start..*cursor)?.iter().collect();
                serde_json::from_str(&quoted).ok().map(Self::Scalar)
            }
            '\'' => {
                *cursor += 1;
                let mut value = String::new();
                loop {
                    match chars.get(*cursor)? {
                        '\'' if chars.get(*cursor + 1) == Some(&'\'') => {
                            value.push('\'');
                            *cursor += 2;
                        }
                        '\'' => {
                            *cursor += 1;
                            return Some(Self::Scalar(value));
                        }
                        character => {
                            value.push(*character);
                            *cursor += 1;
                        }
                    }
                }
            }
            _ => {
                let start = *cursor;
                while chars.get(*cursor).is_some_and(|character| {
                    !matches!(character, ',' | ':' | ']' | '}' | '[' | '{')
                }) {
                    *cursor += 1;
                }
                let bare: String = chars[start..*cursor].iter().collect();
                let bare = bare.trim();
                (!bare.is_empty()).then(|| Self::Scalar(bare.to_owned()))
            }
        }
    }
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
                        _ => manifest.metadata.push((name, value)),
                    }
                }
            }
            ALLOWED_TOOLS => {
                let mut tools = Vec::new();
                if value.trim().is_empty() {
                    let (nested, next) = nested_lines();
                    index = next;
                    for (number, entry) in nested {
                        let item = entry
                            .strip_prefix('-')
                            .ok_or(ManifestError { line: number })?;
                        tools.push(unquote(item, number)?);
                    }
                } else if let Some(inner) = value
                    .trim()
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
                {
                    for item in inner.split(',').filter(|item| !item.trim().is_empty()) {
                        tools.push(unquote(item, number)?);
                    }
                } else {
                    tools = tool_list(&unquote(&value, number)?);
                }
                manifest
                    .extra
                    .push((ALLOWED_TOOLS.to_owned(), tools.join(" ")));
            }
            NAME => manifest.name = unquote(&value, number)?,
            DESCRIPTION => manifest.description = unquote(&value, number)?,
            _ => manifest.extra.push((key, unquote(&value, number)?)),
        }
    }
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
    for (key, value) in &manifest.extra {
        out.push_str(&format!("{key}: {}\n", quote(value)));
    }
    let mut metadata: Vec<(&str, String)> = Vec::new();
    if let Some(display_name) = &manifest.display_name {
        metadata.push((DISPLAY_NAME, display_name.clone()));
    }
    for (key, value) in &manifest.metadata {
        metadata.push((key.as_str(), value.clone()));
    }
    if !metadata.is_empty() {
        out.push_str(&format!("{METADATA}:\n"));
        for (key, value) in metadata {
            let flow = matches!(key, READ_FIRST | REQUIRES)
                && !value.contains(" #")
                && Flow::parse(&value).is_some_and(|flow| !matches!(flow, Flow::Scalar(_)));
            if flow {
                out.push_str(&format!("  {key}: {}\n", value.trim()));
            } else {
                out.push_str(&format!("  {key}: {}\n", quote(&value)));
            }
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
            display_name: Some("Заметки о выпуске".to_owned()),
            extra: vec![
                ("allowed-tools".to_owned(), "fs.read shell.run".to_owned()),
                ("license".to_owned(), "MIT".to_owned()),
            ],
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
        assert_eq!(
            manifest.extra,
            [
                (
                    "allowed-tools".to_owned(),
                    "vector.search skill.read".to_owned()
                ),
                ("license".to_owned(), "it's fine".to_owned())
            ]
        );
        assert_eq!(manifest.display_name.as_deref(), Some("PDF"));
        assert_eq!(manifest.instructions, "Use the tools.");
        let inline =
            parse_manifest("---\nname: a\nallowed-tools: [a.b, c.d]\n---\n").expect("parses");
        assert_eq!(
            inline.extra,
            [("allowed-tools".to_owned(), "a.b c.d".to_owned())]
        );
        let spaced =
            parse_manifest("---\nname: a\nallowed-tools: a.b c.d, e.f\n---\n").expect("parses");
        assert_eq!(
            spaced.extra,
            [("allowed-tools".to_owned(), "a.b c.d e.f".to_owned())]
        );
    }

    #[test]
    fn read_first_and_requires_are_flow_collections_that_survive_a_render() {
        let text = "---\nname: dashboard\ndescription: d\nmetadata:\n  \
            read-first: [references/spec.md, \"references/exploration.md\"]\n  \
            requires: { \"scripts/build.py\": [references/spec.md], scripts/check.py: 'assets/a b.json' }\n\
            ---\n\nSteps.\n";
        let manifest = parse_manifest(text).expect("parses");
        assert_eq!(
            read_first(&manifest.metadata),
            Some(vec![
                "references/spec.md".to_owned(),
                "references/exploration.md".to_owned()
            ])
        );
        let expected = vec![
            (
                "scripts/build.py".to_owned(),
                vec!["references/spec.md".to_owned()],
            ),
            (
                "scripts/check.py".to_owned(),
                vec!["assets/a b.json".to_owned()],
            ),
        ];
        assert_eq!(requires(&manifest.metadata), Some(expected.clone()));
        let rendered = render_manifest(&manifest);
        assert!(
            rendered
                .contains("  read-first: [references/spec.md, \"references/exploration.md\"]\n")
        );
        let again = parse_manifest(&rendered).expect("parses again");
        assert_eq!(requires(&again.metadata), Some(expected));
        assert_eq!(read_first(&[]), Some(Vec::new()));
        assert_eq!(requires(&[]), Some(Vec::new()));
        for broken in ["[a, b", "{a: [b]", "[a] tail", "{a}", "[[a]]"] {
            let metadata = [(READ_FIRST.to_owned(), broken.to_owned())];
            assert_eq!(read_first(&metadata), None, "{broken}");
        }
        let listed = [(REQUIRES.to_owned(), "[scripts/a.py]".to_owned())];
        assert_eq!(requires(&listed), None);
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
    }
}
