use zvs_contract::mcp::{is_env_name, is_header_name};
use zvs_contract::{CreateMcpServerInput, McpEnv, McpHeaders, McpTransport, McpValue};

use crate::copy;

pub fn split(text: &str) -> Result<Vec<String>, &'static str> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for character in text.chars() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => current.push(character),
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                started = true;
            }
            None if character.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(character);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(copy::MCP_ADD_QUOTE);
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

fn is_url(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

pub fn parse<I>(words: I) -> Result<CreateMcpServerInput, &'static str>
where
    I: IntoIterator<Item = String>,
{
    let mut words = words.into_iter().peekable();
    let mut env = McpEnv::new();
    let mut headers = McpHeaders::new();
    while let Some(flag) = words.next_if(|word| word.starts_with('-')) {
        let value = words.next().ok_or(copy::MCP_ADD_USAGE)?;
        match flag.as_str() {
            "-e" | "--env" => {
                let (name, value) = value
                    .split_once('=')
                    .filter(|(name, _)| is_env_name(name))
                    .ok_or(copy::MCP_ADD_ENV)?;
                env.insert(name.to_owned(), McpValue::literal(value));
            }
            "-H" | "--header" => {
                let (name, value) = value
                    .split_once(':')
                    .map(|(name, value)| (name.trim(), value.trim()))
                    .filter(|(name, _)| is_header_name(name))
                    .ok_or(copy::MCP_ADD_HEADER)?;
                headers.insert(name.to_owned(), McpValue::literal(value));
            }
            _ => return Err(copy::MCP_ADD_USAGE),
        }
    }
    let name = words.next().ok_or(copy::MCP_ADD_USAGE)?;
    let target = words.next().ok_or(copy::MCP_ADD_USAGE)?;
    let args: Vec<String> = words.collect();
    let http = is_url(&target);
    if http && (!args.is_empty() || !env.is_empty()) {
        return Err(copy::MCP_ADD_HTTP_EXTRA);
    }
    if !http && !headers.is_empty() {
        return Err(copy::MCP_ADD_STDIO_HEADERS);
    }
    let (transport, command, url) = if http {
        (McpTransport::Http, None, Some(Some(target)))
    } else {
        (McpTransport::Stdio, Some(Some(target)), None)
    };
    Ok(CreateMcpServerInput {
        name,
        namespace: None,
        transport,
        command,
        args: (!args.is_empty()).then_some(args),
        env: (!env.is_empty()).then_some(env),
        url,
        headers: (!headers.is_empty()).then_some(headers),
        enabled: Some(true),
        startup: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        split(text).expect("splits")
    }

    #[test]
    fn quotes_group_words_and_backslashes_stay_literal() {
        assert_eq!(
            words(r#"files npx -y "C:\My Work" 'a b' """#),
            ["files", "npx", "-y", r"C:\My Work", "a b", ""]
        );
        assert_eq!(split("docs \"open"), Err(copy::MCP_ADD_QUOTE));
    }

    #[test]
    fn an_address_makes_an_http_server_with_headers() {
        let input = parse(words(
            r#"-H "X-Team: core" docs https://mcp.example.com/mcp"#,
        ))
        .expect("parses");
        assert_eq!(input.name, "docs");
        assert_eq!(input.transport, McpTransport::Http);
        assert_eq!(
            input.url,
            Some(Some("https://mcp.example.com/mcp".to_owned()))
        );
        assert_eq!(input.command, None);
        assert_eq!(
            input.headers.expect("headers")["X-Team"],
            McpValue::literal("core")
        );
        assert_eq!(input.enabled, Some(true));
    }

    #[test]
    fn a_command_makes_a_stdio_server_and_later_flags_belong_to_it() {
        let input = parse(words(
            "-e TOKEN=a=b files npx -y @modelcontextprotocol/server-filesystem --root D:\\work",
        ))
        .expect("parses");
        assert_eq!(input.transport, McpTransport::Stdio);
        assert_eq!(input.command, Some(Some("npx".to_owned())));
        assert_eq!(
            input.args.expect("args"),
            [
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "--root",
                "D:\\work"
            ]
        );
        assert_eq!(input.env.expect("env")["TOKEN"], McpValue::literal("a=b"));
        assert_eq!(input.url, None);
    }

    #[test]
    fn malformed_input_is_explained() {
        assert_eq!(parse(words("docs")), Err(copy::MCP_ADD_USAGE));
        assert_eq!(parse(words("-e")), Err(copy::MCP_ADD_USAGE));
        assert_eq!(parse(words("--wat x docs npx")), Err(copy::MCP_ADD_USAGE));
        assert_eq!(parse(words("-e 1A=x docs npx")), Err(copy::MCP_ADD_ENV));
        assert_eq!(
            parse(words("-H nocolon docs https://a.b")),
            Err(copy::MCP_ADD_HEADER)
        );
        assert_eq!(
            parse(words("docs https://a.b extra")),
            Err(copy::MCP_ADD_HTTP_EXTRA)
        );
        assert_eq!(
            parse(words("-H \"A: b\" docs npx")),
            Err(copy::MCP_ADD_STDIO_HEADERS)
        );
    }
}
