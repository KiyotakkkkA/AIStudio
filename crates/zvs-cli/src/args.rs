use zvs_contract::ConversationId;

use crate::copy;
use crate::version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Args {
    Run { resume: Option<ConversationId> },
    Version,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    Unknown(String),
    MissingResume,
    InvalidResume(String),
}

pub fn usage() -> String {
    format!(
        "{}\n\n{}:\n  aistudio\n  aistudio --resume <id>\n  aistudio --version\n  aistudio --help",
        version::version_line(),
        copy::USAGE_COMMANDS
    )
}

fn fail(error: &ArgsError) -> String {
    match error {
        ArgsError::Unknown(argument) => format!("{}: {argument}", copy::USAGE_UNKNOWN),
        ArgsError::MissingResume => copy::USAGE_RESUME_MISSING.to_owned(),
        ArgsError::InvalidResume(value) => format!("{}: {value}", copy::USAGE_RESUME_INVALID),
    }
}

pub fn report(error: &ArgsError) -> String {
    format!("{}\n\n{}", fail(error), usage())
}

pub fn parse<I>(arguments: I) -> Result<Args, ArgsError>
where
    I: IntoIterator<Item = String>,
{
    let mut resume = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" | "-V" => return Ok(Args::Version),
            "--help" | "-h" => return Ok(Args::Help),
            "--resume" => {
                let Some(value) = arguments.next() else {
                    return Err(ArgsError::MissingResume);
                };
                if value.starts_with('-') {
                    return Err(ArgsError::MissingResume);
                }
                let Some(id) = ConversationId::parse(&value) else {
                    return Err(ArgsError::InvalidResume(value));
                };
                resume = Some(id);
            }
            other if let Some(value) = other.strip_prefix("--resume=") => {
                let Some(id) = ConversationId::parse(value) else {
                    return Err(ArgsError::InvalidResume(value.to_owned()));
                };
                resume = Some(id);
            }
            other => return Err(ArgsError::Unknown(other.to_owned())),
        }
    }
    Ok(Args::Run { resume })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0199aa11-2222-7111-8111-000000000001";

    fn parse(arguments: &[&str]) -> Result<Args, ArgsError> {
        super::parse(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn no_arguments_run_a_fresh_session() {
        assert_eq!(parse(&[]), Ok(Args::Run { resume: None }));
    }

    #[test]
    fn resume_accepts_a_value_after_the_flag_or_after_an_equals_sign() {
        let id = ConversationId::parse(ID);
        assert_eq!(parse(&["--resume", ID]), Ok(Args::Run { resume: id }));
        assert_eq!(
            parse(&[&format!("--resume={ID}")]),
            Ok(Args::Run { resume: id })
        );
    }

    #[test]
    fn resume_requires_a_valid_conversation_id() {
        assert_eq!(parse(&["--resume"]), Err(ArgsError::MissingResume));
        assert_eq!(
            parse(&["--resume", "--version"]),
            Err(ArgsError::MissingResume)
        );
        assert_eq!(
            parse(&["--resume", "nope"]),
            Err(ArgsError::InvalidResume("nope".to_owned()))
        );
    }

    #[test]
    fn version_help_and_unknown_flags() {
        assert_eq!(parse(&["--version"]), Ok(Args::Version));
        assert_eq!(parse(&["-V"]), Ok(Args::Version));
        assert_eq!(parse(&["--help"]), Ok(Args::Help));
        assert_eq!(parse(&["-h"]), Ok(Args::Help));
        assert_eq!(
            parse(&["--wat"]),
            Err(ArgsError::Unknown("--wat".to_owned()))
        );
    }

    #[test]
    fn errors_come_with_the_usage_text() {
        let message = report(&ArgsError::MissingResume);
        assert!(message.contains("--resume"));
        assert!(message.contains("aistudio"));
    }
}
