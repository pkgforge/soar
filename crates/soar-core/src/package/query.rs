use std::sync::OnceLock;

use regex::Regex;
use tracing::warn;

use crate::error::SoarError;

/// Parsed package query string.
///
/// Supports `family/name@version:repo`, where the family narrows a name that
/// more than one project publishes.
#[derive(Debug)]
pub struct PackageQuery {
    pub name: Option<String>,
    pub family: Option<String>,
    pub repo_name: Option<String>,
    /// Deprecated. Repositories no longer publish a package id.
    pub pkg_id: Option<String>,
    pub version: Option<String>,
}

impl TryFrom<&str> for PackageQuery {
    type Error = SoarError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        static PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
        let re = PACKAGE_RE.get_or_init(|| {
            Regex::new(
                r"(?x)
            ^                                   # anchored: a/b/c is not a query
            (?:(?P<family>[^\/\#\@:]+)\/)?      # optional family before /
            (?P<name>[^\/\#\@:]+)?              # optional package name
            (?:\#(?P<pkg_id>[^@:]+))?           # deprecated pkg_id after #
            (?:@(?P<version>[^:]+))?            # optional version after @
            (?::(?P<repo>[^:]+))?$              # optional repo after :
            ",
            )
            .unwrap()
        });

        let query = value.trim().to_lowercase();
        if query.is_empty() {
            return Err(SoarError::InvalidPackageQuery(
                "Package query can't be empty".into(),
            ));
        }

        let caps = re.captures(&query).ok_or(SoarError::InvalidPackageQuery(
            "Invalid package query format".into(),
        ))?;

        let name = caps.name("name").map(|m| m.as_str().to_string());
        let family = caps.name("family").map(|m| m.as_str().to_string());
        let pkg_id = caps.name("pkg_id").map(|m| m.as_str().to_string());
        if pkg_id.is_some() {
            warn!("#pkg_id is deprecated and will be removed; use family/name instead");
        }
        if pkg_id.is_none() && name.is_none() {
            return Err(SoarError::InvalidPackageQuery(
                "Either package name or pkg_id is required".into(),
            ));
        }

        if let Some(ref pkg_id) = pkg_id {
            if pkg_id == "all" && name.is_none() {
                return Err(SoarError::InvalidPackageQuery(
                    "For all, package name is required.".into(),
                ));
            }
        }

        Ok(PackageQuery {
            repo_name: caps.name("repo").map(|m| m.as_str().to_string()),
            family,
            pkg_id,
            name,
            version: caps.name("version").map(|m| m.as_str().to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PackageQuery;

    #[test]
    fn parses_every_supported_shape() {
        let q = PackageQuery::try_from("ripgrep").unwrap();
        assert_eq!(q.name.as_deref(), Some("ripgrep"));
        assert_eq!(q.family, None);

        let q = PackageQuery::try_from("bat/bat@0.24.0:bincache").unwrap();
        assert_eq!(q.family.as_deref(), Some("bat"));
        assert_eq!(q.name.as_deref(), Some("bat"));
        assert_eq!(q.version.as_deref(), Some("0.24.0"));
        assert_eq!(q.repo_name.as_deref(), Some("bincache"));
    }

    #[test]
    fn a_third_segment_is_not_a_query() {
        // Unanchored, this matched from the middle and silently dropped `a`.
        assert!(PackageQuery::try_from("a/b/c").is_err());
    }
}
