use std::sync::OnceLock;

use regex::Regex;

use crate::{database::packages::PackageFilter, error::SoarError};

#[derive(Debug)]
pub struct PackageQuery {
    pub name: String,
    pub repo_name: Option<String>,
    pub family: Option<String>,
    pub version: Option<String>,
}

impl PackageFilter {
    pub fn from_query(query: PackageQuery) -> Self {
        PackageFilter {
            repo_name: query.repo_name,
            exact_pkg_name: Some(query.name),
            family: query.family,
            ..Default::default()
        }
    }
}

impl TryFrom<&str> for PackageQuery {
    type Error = SoarError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        static PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
        let re = PACKAGE_RE.get_or_init(|| {
            Regex::new(
                r"(?x)
            (?P<name>[^\/\#\@:]+)               # optional package name
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

        let name = caps.name("name").map(|m| m.as_str().to_string()).ok_or(
            SoarError::InvalidPackageQuery("Package name is required".into()),
        )?;

        if name.is_empty() {
            return Err(SoarError::InvalidPackageQuery(
                "Package name cannot be empty".into(),
            ));
        }

        Ok(PackageQuery {
            repo_name: caps.name("repo").map(|m| m.as_str().to_string()),
            family: caps.name("pkg_id").map(|m| m.as_str().to_string()),
            name,
            version: caps.name("version").map(|m| m.as_str().to_string()),
        })
    }
}
