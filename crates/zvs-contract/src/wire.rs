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

pub fn optional_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
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

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Patch {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "super::optional_nullable"
        )]
        days: Option<Option<u64>>,
    }

    #[test]
    fn an_optional_nullable_field_tells_absent_from_null() {
        for (wire, parsed) in [
            (json!({}), None),
            (json!({ "days": null }), Some(None)),
            (json!({ "days": 3 }), Some(Some(3))),
        ] {
            let patch: Patch = serde_json::from_value(wire.clone()).expect("parses");
            assert_eq!(patch.days, parsed);
            assert_eq!(serde_json::to_value(&patch).expect("serialises"), wire);
        }
    }
}
