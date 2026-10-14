use crate::enums::wire_enum;

wire_enum!(ProviderKind {
    Ollama => "ollama",
    Openrouter => "openrouter",
    Mistral => "mistral",
    OpenaiCompatible => "openai-compatible",
});

wire_enum!(ProviderCapability {
    Text => "text",
    Embedding => "embedding",
    Image => "image",
});

wire_enum!(ProviderStatus {
    Unknown => "unknown",
    Ok => "ok",
    Degraded => "degraded",
    Failed => "failed",
    NeedsRelink => "needs-relink",
});

wire_enum!(ModelCapability {
    Tools => "tools",
    Vision => "vision",
    Streaming => "streaming",
    Reasoning => "reasoning",
    Code => "code",
});

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_enums_round_trip_through_their_wire_names() {
        for (value, wire) in [
            (
                serde_json::to_value(ProviderKind::OpenaiCompatible),
                "openai-compatible",
            ),
            (
                serde_json::to_value(ProviderStatus::NeedsRelink),
                "needs-relink",
            ),
            (
                serde_json::to_value(ProviderCapability::Embedding),
                "embedding",
            ),
            (
                serde_json::to_value(ModelCapability::Reasoning),
                "reasoning",
            ),
        ] {
            assert_eq!(value.expect("serialises"), json!(wire));
        }
        assert_eq!(
            serde_json::from_value::<Vec<ModelCapability>>(json!(["tools", "vision"]))
                .expect("parses"),
            [ModelCapability::Tools, ModelCapability::Vision]
        );
        assert_eq!(
            ProviderKind::parse("openrouter"),
            Some(ProviderKind::Openrouter)
        );
        assert_eq!(ProviderStatus::parse("broken"), None);
    }
}
