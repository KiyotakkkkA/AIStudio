use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::validate::no_refinements;

wire_enum!(AdapterFamily {
    OpenaiCompatible => "openai-compatible",
    QwenWeb => "qwen-web",
    ZaiWeb => "zai-web",
    Local => "local",
});

wire_enum!(AuthMode {
    Api => "api",
    Account => "account",
});

wire_enum!(AccountFamily {
    QwenWeb => "qwen-web",
    ZaiWeb => "zai-web",
});

wire_enum!(TunableParameter {
    Temperature => "temperature",
    TopK => "topK",
    TopP => "topP",
    MaxOutputTokens => "maxOutputTokens",
});

wire_enum!(ToolMode {
    Native => "native",
    Injected => "injected",
    None => "none",
});

wire_enum!(AccountStatus {
    Linked => "linked",
    NeedsRelink => "needs-relink",
    Revoked => "revoked",
});

pub const LOCAL_BASE_URL: &str = "local://models";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HonoursDto {
    pub temperature: bool,
    pub top_k: bool,
    pub top_p: bool,
    pub max_output_tokens: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCapabilitiesDto {
    pub family: AdapterFamily,
    pub auth_modes: Vec<AuthMode>,
    pub streaming: bool,
    pub live_model_list: bool,
    pub embedding: bool,
    pub image: bool,
    pub honours: HonoursDto,
    pub tools: ToolMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterDescriptorDto {
    #[serde(flatten)]
    pub capabilities: AdapterCapabilitiesDto,
    pub implemented: bool,
}

pub type ProvidersAdaptersOutput = Vec<AdapterDescriptorDto>;

no_refinements!(HonoursDto, AdapterCapabilitiesDto, AdapterDescriptorDto);

impl AdapterFamily {
    pub const fn is_local(self) -> bool {
        matches!(self, Self::Local)
    }

    pub const fn account_family(self) -> Option<AccountFamily> {
        match self {
            Self::QwenWeb => Some(AccountFamily::QwenWeb),
            Self::ZaiWeb => Some(AccountFamily::ZaiWeb),
            Self::OpenaiCompatible | Self::Local => None,
        }
    }
}

pub const fn resolve_tool_mode(adapter: ToolMode, model_supports_tools: Option<bool>) -> ToolMode {
    match (adapter, model_supports_tools) {
        (ToolMode::Native, Some(false)) => ToolMode::None,
        _ => adapter,
    }
}

pub fn model_tool_support<S: AsRef<str>>(model: &[S], siblings: &[&[S]]) -> Option<bool> {
    let has_tools = |capabilities: &[S]| {
        capabilities
            .iter()
            .any(|capability| capability.as_ref() == "tools")
    };
    if has_tools(model) {
        return Some(true);
    }
    siblings
        .iter()
        .any(|capabilities| has_tools(capabilities))
        .then_some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_enums_speak_the_shared_wire_names() {
        assert_eq!(
            AdapterFamily::ALL
                .iter()
                .map(|family| family.as_str())
                .collect::<Vec<_>>(),
            ["openai-compatible", "qwen-web", "zai-web", "local"]
        );
        assert_eq!(
            serde_json::to_value(TunableParameter::MaxOutputTokens).expect("serialises"),
            json!("maxOutputTokens")
        );
        assert_eq!(AuthMode::parse("account"), Some(AuthMode::Account));
        assert_eq!(AdapterFamily::parse("made-up"), None);
        assert_eq!(ToolMode::None.to_string(), "none");
        assert!(serde_json::from_value::<AccountStatus>(json!("needs_relink")).is_err());
        assert!(AdapterFamily::Local.is_local());
        assert_eq!(
            AdapterFamily::QwenWeb.account_family(),
            Some(AccountFamily::QwenWeb)
        );
        assert_eq!(
            AdapterFamily::ZaiWeb.account_family(),
            Some(AccountFamily::ZaiWeb)
        );
        assert_eq!(AdapterFamily::OpenaiCompatible.account_family(), None);
        assert_eq!(LOCAL_BASE_URL, "local://models");
    }

    #[test]
    fn a_native_adapter_falls_back_to_none_only_for_models_the_provider_reports_as_tool_less() {
        let with_tools: &[&str] = &["streaming", "tools"];
        let without: &[&str] = &["streaming"];
        assert_eq!(
            model_tool_support(with_tools, &[with_tools, without]),
            Some(true)
        );
        assert_eq!(
            model_tool_support(without, &[with_tools, without]),
            Some(false)
        );
        assert_eq!(model_tool_support(without, &[without]), None);
        assert_eq!(
            resolve_tool_mode(ToolMode::Native, Some(false)),
            ToolMode::None
        );
        assert_eq!(resolve_tool_mode(ToolMode::Native, None), ToolMode::Native);
        assert_eq!(
            resolve_tool_mode(ToolMode::Injected, Some(false)),
            ToolMode::Injected
        );
        assert_eq!(
            resolve_tool_mode(ToolMode::None, Some(true)),
            ToolMode::None
        );
    }
}
