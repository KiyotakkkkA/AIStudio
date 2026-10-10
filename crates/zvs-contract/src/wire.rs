use serde::{Deserialize, Deserializer};

pub fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Sample {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "super::present"
        )]
        maybe_text: Option<String>,
        #[serde(deserialize_with = "super::nullable")]
        known_or_null: Option<u64>,
    }

    #[test]
    fn an_optional_field_is_absent_never_null() {
        let sample: Sample = serde_json::from_value(json!({ "knownOrNull": 1 })).expect("parses");
        assert_eq!(sample.maybe_text, None);
        assert_eq!(
            serde_json::to_value(&sample).expect("serialises"),
            json!({ "knownOrNull": 1 })
        );
        assert!(
            serde_json::from_value::<Sample>(json!({ "maybeText": null, "knownOrNull": 1 }))
                .is_err()
        );
    }

    #[test]
    fn a_nullable_field_is_required_and_may_be_null() {
        let sample: Sample =
            serde_json::from_value(json!({ "knownOrNull": null })).expect("parses");
        assert_eq!(
            serde_json::to_value(&sample).expect("serialises"),
            json!({ "knownOrNull": null })
        );
        assert!(serde_json::from_value::<Sample>(json!({})).is_err());
    }
}
