//! Remote package metadata structures.
//!
//! This module defines the [`RemotePackage`] struct which represents package
//! metadata as received from a repository. It handles various serialization
//! quirks in the metadata format, including flexible boolean parsing and
//! optional number fields.

use std::fmt;

use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};

/// Internal enum for deserializing boolean values that may be strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum FlexiBool {
    Bool(bool),
    String(String),
}

fn empty_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty()))
}

fn optional_number<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptU64Visitor;

    impl<'de> Visitor<'de> for OptU64Visitor {
        type Value = Option<u64>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a positive integer, string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok((v >= 0).then_some(v as u64))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                return Ok(None);
            }

            v.parse::<i64>()
                .ok()
                .filter(|&n| n >= 0)
                .map(|n| n as u64)
                .ok_or_else(|| E::custom("invalid number"))
                .map(Some)
                .or(Ok(None))
        }
    }

    deserializer.deserialize_any(OptU64Visitor)
}

fn flexible_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<FlexiBool>::deserialize(deserializer)? {
        Some(FlexiBool::Bool(b)) => Ok(Some(b)),
        Some(FlexiBool::String(s)) => {
            match s.to_lowercase().as_str() {
                "true" | "yes" | "1" => Ok(Some(true)),
                "false" | "no" | "0" => Ok(Some(false)),
                "" => Ok(None),
                _ => {
                    Err(de::Error::invalid_value(
                        de::Unexpected::Str(&s),
                        &"a valid boolean (true/false, yes/no, 1/0)",
                    ))
                }
            }
        }
        None => Ok(None),
    }
}

/// Package metadata as received from a remote repository.
///
/// This struct represents the complete metadata for a package available in a
/// repository. It handles various quirks in the serialization format:
///
/// - Boolean fields accept both actual booleans and string representations
///   (`"true"`, `"false"`, `"yes"`, `"no"`, `"1"`, `"0"`)
/// - Numeric fields accept string representations of numbers
/// - Empty strings are normalized to `None`
/// - Various field aliases are supported for backward compatibility
///
/// # Required Fields
///
/// - `pkg_id` - Unique package identifier
/// - `pkg_name` - Human-readable package name
/// - `description` - Package description
/// - `version` - Package version string
/// - `download_url` - URL to download the package
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RemotePackage {
    #[serde(default, deserialize_with = "flexible_bool", alias = "_disabled")]
    pub disabled: Option<bool>,

    #[serde(alias = "_disabled_reason")]
    pub disabled_reason: Option<serde_json::Value>,

    pub pkg_id: String,
    pub pkg_name: String,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub pkg_family: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub pkg_type: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub pkg_webpage: Option<String>,

    pub description: String,
    pub version: String,

    pub download_url: String,

    #[serde(default, deserialize_with = "optional_number")]
    pub size_raw: Option<u64>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub ghcr_pkg: Option<String>,

    #[serde(default, deserialize_with = "optional_number")]
    pub ghcr_size_raw: Option<u64>,

    pub ghcr_files: Option<Vec<String>>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub ghcr_blob: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub ghcr_url: Option<String>,

    #[serde(alias = "src_url")]
    pub src_urls: Option<Vec<String>>,

    #[serde(alias = "homepage")]
    pub homepages: Option<Vec<String>>,

    #[serde(alias = "license")]
    pub licenses: Option<Vec<String>>,

    #[serde(alias = "maintainer")]
    pub maintainers: Option<Vec<String>>,

    #[serde(alias = "note")]
    pub notes: Option<Vec<String>>,

    #[serde(alias = "tag")]
    pub tags: Option<Vec<String>>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub bsum: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_id: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_date: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none", alias = "build_gha")]
    pub build_action: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_script: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub build_log: Option<String>,

    #[serde(alias = "category")]
    pub categories: Option<Vec<String>>,

    pub provides: Option<Vec<String>>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub icon: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub desktop: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub appstream: Option<String>,

    #[serde(default, deserialize_with = "empty_is_none")]
    pub app_id: Option<String>,

    #[serde(default, deserialize_with = "flexible_bool")]
    pub soar_syms: Option<bool>,

    #[serde(default, deserialize_with = "flexible_bool")]
    pub deprecated: Option<bool>,

    #[serde(default, deserialize_with = "flexible_bool")]
    pub desktop_integration: Option<bool>,

    #[serde(default, deserialize_with = "flexible_bool")]
    pub portable: Option<bool>,

    pub repology: Option<Vec<String>>,
    pub snapshots: Option<Vec<String>>,
    pub replaces: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_package_deserialization() {
        let json = r#"{
            "pkg_id": "test-pkg",
            "pkg_name": "test",
            "description": "A test package",
            "version": "1.0.0",
            "download_url": "https://example.com/test.tar.gz"
        }"#;

        let pkg: RemotePackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.pkg_id, "test-pkg");
        assert_eq!(pkg.pkg_name, "test");
        assert_eq!(pkg.version, "1.0.0");
    }

    #[test]
    fn test_flexible_bool() {
        let json = r#"{
            "pkg_id": "test",
            "pkg_name": "test",
            "description": "test",
            "version": "1.0.0",
            "download_url": "https://example.com",
            "disabled": "true"
        }"#;

        let pkg: RemotePackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.disabled, Some(true));
    }
}
