use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check, nested, no_refinements};

pub type Timestamp = i64;

pub type Json = serde_json::Value;

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub limit: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub cursor: Option<String>,
}

impl Validate for PageRequest {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("limit", self.limit)?;
        check::int_range("limit", self.limit, 1, u64::MAX)?;
        if let Some(cursor) = &self.cursor {
            check::non_empty("cursor", cursor)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub next_cursor: Option<String>,
}

impl<T: Validate> Validate for Page<T> {
    fn validate(&self) -> Result<(), AppError> {
        nested("items", self.items.validate())?;
        if let Some(cursor) = &self.next_cursor {
            check::non_empty("nextCursor", cursor)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort<F> {
    pub field: F,
    pub direction: SortDirection,
}

impl<F: Validate> Validate for Sort<F> {
    fn validate(&self) -> Result<(), AppError> {
        nested("field", self.field.validate())
    }
}

no_refinements!(SortDirection);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_request_omits_an_absent_cursor() {
        let request = PageRequest {
            limit: 20,
            cursor: None,
        };
        assert_eq!(
            serde_json::to_value(&request).expect("serialises"),
            json!({ "limit": 20 })
        );
        assert!(request.validate().is_ok());
    }

    #[test]
    fn a_page_request_needs_a_positive_limit_and_a_nonempty_cursor() {
        let zero = PageRequest {
            limit: 0,
            cursor: None,
        };
        let empty = PageRequest {
            limit: 1,
            cursor: Some(String::new()),
        };
        assert!(zero.validate().is_err());
        assert!(empty.validate().is_err());
    }

    #[test]
    fn a_page_uses_camel_case_for_the_cursor() {
        let page = Page {
            items: vec![1, 2],
            next_cursor: Some("c".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(&page).expect("serialises"),
            json!({ "items": [1, 2], "nextCursor": "c" })
        );
    }

    #[test]
    fn sort_directions_are_lowercase() {
        let sort = Sort {
            field: "name".to_owned(),
            direction: SortDirection::Desc,
        };
        assert_eq!(
            serde_json::to_value(&sort).expect("serialises"),
            json!({ "field": "name", "direction": "desc" })
        );
        assert!(serde_json::from_value::<SortDirection>(json!("DESC")).is_err());
    }
}
