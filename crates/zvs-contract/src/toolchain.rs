use serde::{Deserialize, Serialize};

use crate::downloads::check_item_ref;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::validate::{Validate, check, invalid};

wire_enum!(ToolchainRuntime {
    Uv => "uv",
    Node => "node",
});

wire_enum!(ToolchainSource {
    Managed => "managed",
    Path => "path",
    Manual => "manual",
});

wire_enum!(ToolchainStatus {
    Missing => "missing",
    Installing => "installing",
    Ready => "ready",
    Broken => "broken",
});

pub const TOOLCHAIN_VERSION_MAX: usize = 64;
pub const TOOLCHAIN_PATH_MAX: usize = 4096;
pub const TOOLCHAIN_PROBLEM_MAX: usize = 600;
pub const TOOLCHAIN_MANAGED_MAX: usize = 16;
pub const TOOLCHAIN_REFS_MAX: usize = 4;

fn check_optional(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| {
        check::non_empty(field, value)?;
        check::max_len(field, value, max)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainDto {
    pub runtime: ToolchainRuntime,
    pub display_name: String,
    pub status: ToolchainStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub source: Option<ToolchainSource>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub executable_path: Option<String>,
    pub minimum_version: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub latest_version: Option<String>,
    pub update_available: bool,
    #[serde(default)]
    pub managed_versions: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub problem: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

impl Validate for ToolchainDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("displayName", &self.display_name)?;
        check::max_len("displayName", &self.display_name, 120)?;
        check_optional("version", self.version.as_ref(), TOOLCHAIN_VERSION_MAX)?;
        check_optional(
            "executablePath",
            self.executable_path.as_ref(),
            TOOLCHAIN_PATH_MAX,
        )?;
        check::non_empty("minimumVersion", &self.minimum_version)?;
        check::max_len(
            "minimumVersion",
            &self.minimum_version,
            TOOLCHAIN_VERSION_MAX,
        )?;
        check_optional(
            "latestVersion",
            self.latest_version.as_ref(),
            TOOLCHAIN_VERSION_MAX,
        )?;
        if self.update_available && self.latest_version.is_none() {
            return Err(invalid("updateAvailable", "latestVersion"));
        }
        if self.managed_versions.len() > TOOLCHAIN_MANAGED_MAX {
            return Err(invalid("managedVersions", "maxItems"));
        }
        for (index, version) in self.managed_versions.iter().enumerate() {
            let field = format!("managedVersions.{index}");
            check::non_empty(&field, version)?;
            check::max_len(&field, version, TOOLCHAIN_VERSION_MAX)?;
        }
        check_optional("problem", self.problem.as_ref(), TOOLCHAIN_PROBLEM_MAX)?;
        if self.refs.len() > TOOLCHAIN_REFS_MAX {
            return Err(invalid("refs", "maxItems"));
        }
        self.refs
            .iter()
            .enumerate()
            .try_for_each(|(index, item)| check_item_ref(&format!("refs.{index}"), item))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainListInput {
    #[serde(default)]
    pub check_updates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainRef {
    pub runtime: ToolchainRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainSelectInput {
    pub runtime: ToolchainRuntime,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub path: Option<String>,
}

impl Validate for ToolchainSelectInput {
    fn validate(&self) -> Result<(), AppError> {
        self.path.as_ref().map_or(Ok(()), |path| {
            check::non_empty("path", path)?;
            check::max_len("path", path, TOOLCHAIN_PATH_MAX)
        })
    }
}

crate::validate::no_refinements!(ToolchainListInput, ToolchainRef);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    fn ready() -> serde_json::Value {
        json!({
            "runtime": "node",
            "displayName": "Node.js",
            "status": "ready",
            "source": "managed",
            "version": "24.11.0",
            "executablePath": "C:/data/mcp-toolchain/node/24.11.0/node.exe",
            "minimumVersion": "18.0.0",
            "updateAvailable": false,
            "managedVersions": ["24.11.0"],
            "refs": [],
        })
    }

    #[test]
    fn a_ready_runtime_round_trips_and_writes_its_defaults() {
        let parsed = parse::<ToolchainDto>(ready()).expect("parses");
        assert_eq!(serde_json::to_value(&parsed).expect("serialises"), ready());
        let mut bare = ready();
        bare.as_object_mut()
            .expect("an object")
            .remove("managedVersions");
        let written =
            serde_json::to_value(parse::<ToolchainDto>(bare).expect("parses")).expect("serialises");
        assert_eq!(written["managedVersions"], json!([]));
    }

    #[test]
    fn an_update_needs_the_version_it_offers() {
        let mut offered = ready();
        offered["updateAvailable"] = json!(true);
        assert!(parse::<ToolchainDto>(offered.clone()).is_err());
        offered["latestVersion"] = json!("24.12.0");
        assert!(parse::<ToolchainDto>(offered).is_ok());
    }

    #[test]
    fn a_selection_names_a_known_runtime_and_a_non_empty_path() {
        assert!(parse::<ToolchainSelectInput>(json!({ "runtime": "uv" })).is_ok());
        assert!(parse::<ToolchainSelectInput>(json!({ "runtime": "python" })).is_err());
        assert!(parse::<ToolchainSelectInput>(json!({ "runtime": "uv", "path": "" })).is_err());
        assert!(parse::<ToolchainSelectInput>(json!({ "runtime": "uv", "path": null })).is_err());
        assert_eq!(
            parse::<ToolchainListInput>(json!({})).expect("parses"),
            ToolchainListInput {
                check_updates: false
            }
        );
    }
}
