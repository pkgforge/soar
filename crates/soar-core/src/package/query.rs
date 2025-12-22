use std::sync::OnceLock;

use regex::Regex;

use crate::error::SoarError;

/// Parsed package query string.
/// Supports format: `name#pkg_id@version:repo`
#[derive(Debug)]
pub struct PackageQuery {
    pub name: Option<String>,
    pub repo_name: Option<String>,
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
            (?P<name>[^\/\#\@:]+)?              # optional package name
            (?:\#(?P<pkg_id>[^@:]+))?           # optional pkg_id after #
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
        let pkg_id = caps.name("pkg_id").map(|m| m.as_str().to_string());
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
            pkg_id,
            name,
            version: caps.name("version").map(|m| m.as_str().to_string()),
        })
    }
}
