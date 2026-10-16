use crate::enums::wire_enum;

wire_enum!(ToolTier {
    Auto => "auto",
    Ask => "ask",
    Off => "off",
});

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tiers_use_the_wire_names() {
        assert_eq!(
            serde_json::to_value(ToolTier::ALL).expect("serialises"),
            json!(["auto", "ask", "off"])
        );
        assert_eq!(ToolTier::parse("ask"), Some(ToolTier::Ask));
        assert_eq!(ToolTier::parse("ASK"), None);
    }
}
