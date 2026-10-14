use zvs_contract::{AdapterFamily, AuthMode, ToolMode, TunableParameter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Honours {
    pub temperature: bool,
    pub top_k: bool,
    pub top_p: bool,
    pub max_output_tokens: bool,
}

impl Honours {
    pub const ALL: Self = Self {
        temperature: true,
        top_k: true,
        top_p: true,
        max_output_tokens: true,
    };

    pub const NONE: Self = Self {
        temperature: false,
        top_k: false,
        top_p: false,
        max_output_tokens: false,
    };

    pub const fn get(&self, parameter: TunableParameter) -> bool {
        match parameter {
            TunableParameter::Temperature => self.temperature,
            TunableParameter::TopK => self.top_k,
            TunableParameter::TopP => self.top_p,
            TunableParameter::MaxOutputTokens => self.max_output_tokens,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterCapabilities {
    pub family: AdapterFamily,
    pub auth_modes: &'static [AuthMode],
    pub streaming: bool,
    pub live_model_list: bool,
    pub embedding: bool,
    pub image: bool,
    pub honours: Honours,
    pub tools: ToolMode,
}

impl AdapterCapabilities {
    pub fn supports_auth_mode(&self, mode: AuthMode) -> bool {
        self.auth_modes.contains(&mode)
    }

    pub fn unsupported_parameters(&self, requested: &[TunableParameter]) -> Vec<TunableParameter> {
        TunableParameter::ALL
            .iter()
            .copied()
            .filter(|parameter| requested.contains(parameter) && !self.honours.get(*parameter))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: AdapterCapabilities = AdapterCapabilities {
        family: AdapterFamily::OpenaiCompatible,
        auth_modes: &[AuthMode::Api],
        streaming: true,
        live_model_list: true,
        embedding: true,
        image: false,
        honours: Honours {
            temperature: true,
            top_k: false,
            top_p: true,
            max_output_tokens: true,
        },
        tools: ToolMode::Native,
    };

    #[test]
    fn unsupported_parameters_are_the_requested_ones_the_family_ignores_in_wire_order() {
        assert_eq!(
            SAMPLE.unsupported_parameters(&[TunableParameter::TopK, TunableParameter::Temperature]),
            [TunableParameter::TopK]
        );
        assert!(SAMPLE.unsupported_parameters(&[]).is_empty());
        let none = AdapterCapabilities {
            honours: Honours::NONE,
            ..SAMPLE
        };
        assert_eq!(
            none.unsupported_parameters(&[
                TunableParameter::MaxOutputTokens,
                TunableParameter::Temperature,
            ]),
            [
                TunableParameter::Temperature,
                TunableParameter::MaxOutputTokens
            ]
        );
    }

    #[test]
    fn an_auth_mode_is_supported_only_when_declared() {
        assert!(SAMPLE.supports_auth_mode(AuthMode::Api));
        assert!(!SAMPLE.supports_auth_mode(AuthMode::Account));
    }
}
