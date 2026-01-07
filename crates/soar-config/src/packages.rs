use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{LazyLock, RwLock},
};

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::path::xdg_config_home;
use toml_edit::DocumentMut;
use tracing::info;

use crate::{
    annotations::annotate_toml_table,
    error::{ConfigError, Result},
};

/// Path to the packages configuration file
pub static PACKAGES_CONFIG_PATH: LazyLock<RwLock<PathBuf>> = LazyLock::new(|| {
    RwLock::new(match std::env::var("SOAR_PACKAGES_CONFIG") {
        Ok(path_str) => PathBuf::from(path_str),
        Err(_) => xdg_config_home().join("soar").join("packages.toml"),
    })
});

/// Declarative package configuration.
/// Defines the desired set of packages to be installed.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct PackagesConfig {
    /// Default settings applied to all packages unless overridden.
    pub defaults: Option<PackageDefaults>,

    /// Map of package names to their specifications.
    /// Supports both simple string form (version) and detailed table form.
    #[serde(default)]
    pub packages: HashMap<String, PackageSpec>,
}

/// Default settings for all packages.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct PackageDefaults {
    /// Default profile to use for installations.
    pub profile: Option<String>,

    /// Whether to install binary only (exclude logs, desktop files, etc).
    pub binary_only: Option<bool>,

    /// Default install patterns.
    pub install_patterns: Option<Vec<String>>,
}

/// Flexible package specification.
/// Can be either a simple string (version) or a detailed options table.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PackageSpec {
    /// Simple form: just a version string (e.g., "1.8.1" or "*" for latest)
    Simple(String),
    /// Detailed form: full options table
    Detailed(Box<PackageOptions>),
}

impl Default for PackageSpec {
    fn default() -> Self {
        Self::Simple("*".to_string())
    }
}

/// Full package options for detailed specification.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct PackageOptions {
    /// Specific package ID (for disambiguation when multiple packages share the same name).
    pub pkg_id: Option<String>,

    /// Specific version to install.
    pub version: Option<String>,

    /// Repository to install from.
    pub repo: Option<String>,

    /// Direct URL to download the package from (makes it a "local" package).
    pub url: Option<String>,

    /// Package type for URL installs (e.g., appimage, flatimage, archive).
    pub pkg_type: Option<String>,

    /// Entrypoint executable name (for URL packages where the binary name differs from package name).
    pub entrypoint: Option<String>,

    /// Whether to pin this package (prevents automatic updates).
    #[serde(default)]
    pub pinned: bool,

    /// Profile to install to (overrides default).
    pub profile: Option<String>,

    /// Portable directory configuration.
    pub portable: Option<PortableConfig>,

    /// Custom install patterns (overrides default).
    pub install_patterns: Option<Vec<String>>,

    /// Whether to install binary only.
    pub binary_only: Option<bool>,
}

/// Portable directory configuration for a package.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct PortableConfig {
    /// Base portable path (sets portable_home and portable_config).
    pub path: Option<String>,

    /// Portable home directory.
    pub home: Option<String>,

    /// Portable config directory.
    pub config: Option<String>,

    /// Portable share directory.
    pub share: Option<String>,

    /// Portable cache directory.
    pub cache: Option<String>,
}

/// Resolved package specification with name included.
#[derive(Clone, Debug)]
pub struct ResolvedPackage {
    pub name: String,
    pub pkg_id: Option<String>,
    pub version: Option<String>,
    pub repo: Option<String>,
    pub url: Option<String>,
    pub pkg_type: Option<String>,
    pub entrypoint: Option<String>,
    pub pinned: bool,
    pub profile: Option<String>,
    pub portable: Option<PortableConfig>,
    pub install_patterns: Option<Vec<String>>,
    pub binary_only: bool,
}

impl PackageSpec {
    /// Resolve the package specification with defaults applied.
    pub fn resolve(&self, name: &str, defaults: Option<&PackageDefaults>) -> ResolvedPackage {
        match self {
            PackageSpec::Simple(version_str) => {
                let version = if version_str == "*" {
                    None
                } else {
                    Some(version_str.clone())
                };
                let pinned = version.is_some();
                ResolvedPackage {
                    name: name.to_string(),
                    pkg_id: None,
                    version,
                    repo: None,
                    url: None,
                    pkg_type: None,
                    entrypoint: None,
                    pinned,
                    profile: defaults.and_then(|d| d.profile.clone()),
                    portable: None,
                    install_patterns: defaults.and_then(|d| d.install_patterns.clone()),
                    binary_only: defaults.and_then(|d| d.binary_only).unwrap_or(false),
                }
            }
            PackageSpec::Detailed(opts) => {
                // Treat "*" as None (latest version)
                let version = opts.version.as_ref().filter(|v| v.as_str() != "*").cloned();
                // URL packages are always pinned
                let pinned = opts.pinned || version.is_some() || opts.url.is_some();
                ResolvedPackage {
                    name: name.to_string(),
                    pkg_id: opts.pkg_id.clone(),
                    version,
                    repo: opts.repo.clone(),
                    url: opts.url.clone(),
                    pkg_type: opts.pkg_type.clone(),
                    entrypoint: opts.entrypoint.clone(),
                    pinned,
                    profile: opts
                        .profile
                        .clone()
                        .or_else(|| defaults.and_then(|d| d.profile.clone())),
                    portable: opts.portable.clone(),
                    install_patterns: opts
                        .install_patterns
                        .clone()
                        .or_else(|| defaults.and_then(|d| d.install_patterns.clone())),
                    binary_only: opts
                        .binary_only
                        .or_else(|| defaults.and_then(|d| d.binary_only))
                        .unwrap_or(false),
                }
            }
        }
    }
}

impl PackagesConfig {
    /// Load packages configuration from file.
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = match path {
            Some(p) => PathBuf::from(p),
            None => PACKAGES_CONFIG_PATH.read().unwrap().clone(),
        };

        if !config_path.exists() {
            return Err(ConfigError::PackagesConfigNotFound(
                config_path.display().to_string(),
            ));
        }

        let content = fs::read_to_string(&config_path)?;
        let config: PackagesConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Get all packages resolved with defaults applied.
    pub fn resolved_packages(&self) -> Vec<ResolvedPackage> {
        self.packages
            .iter()
            .map(|(name, spec): (&String, &PackageSpec)| spec.resolve(name, self.defaults.as_ref()))
            .collect()
    }

    /// Create a default configuration.
    pub fn default_config() -> Self {
        Self {
            defaults: Some(PackageDefaults {
                profile: Some("default".to_string()),
                binary_only: Some(false),
                install_patterns: None,
            }),
            packages: HashMap::new(),
        }
    }

    /// Convert config to an annotated TOML document with field documentation.
    pub fn to_annotated_document(&self) -> Result<DocumentMut> {
        use toml_edit::Item;

        let toml_string = toml::to_string_pretty(self)?;
        let mut doc = toml_string.parse::<DocumentMut>()?;

        let header = r#"# Soar Declarative Package Configuration
# Run `soar apply` to install packages defined here.
# Run `soar apply --prune` to also remove packages not listed.
#
# Package format:
#   package_name = "*"                    # Latest version
#   package_name = "1.2.3"                # Specific version (pinned)
#   package_name = { version = "1.2" }    # Same as above
#   package_name = { pkg_id = "pkg-bin", repo = "bincache" }
#   package_name = { pinned = true, portable = { home = "~/.pkg" } }

"#;
        doc.as_table_mut().decor_mut().set_prefix(header);

        annotate_toml_table::<PackagesConfig>(doc.as_table_mut(), true)?;

        if let Some(Item::Table(defaults_table)) = doc.get_mut("defaults") {
            annotate_toml_table::<PackageDefaults>(defaults_table, false)?;
        }

        Ok(doc)
    }
}

/// Generate a default packages configuration file.
pub fn generate_default_packages_config() -> Result<()> {
    let config_path = PACKAGES_CONFIG_PATH.read().unwrap().clone();

    if config_path.exists() {
        return Err(ConfigError::PackagesConfigAlreadyExists);
    }

    let def_config = PackagesConfig::default_config();
    let annotated_doc = def_config.to_annotated_document()?;

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&config_path, annotated_doc.to_string())?;
    info!(
        "Default packages configuration generated at: {}",
        config_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_package_spec() {
        let toml_str = r#"
[packages]
curl = "*"
jq = "1.8.1"
"#;
        let config: PackagesConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.packages.len(), 2);

        let resolved = config.resolved_packages();
        let curl = resolved.iter().find(|p| p.name == "curl").unwrap();
        let jq = resolved.iter().find(|p| p.name == "jq").unwrap();

        assert_eq!(curl.version, None);
        assert_eq!(jq.version, Some("1.8.1".to_string()));
    }

    #[test]
    fn test_detailed_package_spec() {
        let toml_str = r#"
[packages]
neovim = { pkg_id = "neovim-appimage", repo = "bincache", pinned = true }
"#;
        let config: PackagesConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_packages();

        assert_eq!(resolved[0].name, "neovim");
        assert_eq!(resolved[0].pkg_id, Some("neovim-appimage".to_string()));
        assert_eq!(resolved[0].repo, Some("bincache".to_string()));
        assert!(resolved[0].pinned);
    }

    #[test]
    fn test_defaults_applied() {
        let toml_str = r#"
[defaults]
profile = "work"
binary_only = true

[packages]
curl = "*"
"#;
        let config: PackagesConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_packages();

        assert_eq!(resolved[0].profile, Some("work".to_string()));
        assert!(resolved[0].binary_only);
    }

    #[test]
    fn test_package_override_defaults() {
        let toml_str = r#"
[defaults]
profile = "default"

[packages]
special = { profile = "isolated" }
"#;
        let config: PackagesConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_packages();

        assert_eq!(resolved[0].profile, Some("isolated".to_string()));
    }

    #[test]
    fn test_portable_config() {
        let toml_str = r#"
[packages]
firefox = { portable = { home = "~/.firefox-home", config = "~/.firefox-config" } }
"#;
        let config: PackagesConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_packages();

        let portable = resolved[0].portable.as_ref().unwrap();
        assert_eq!(portable.home, Some("~/.firefox-home".to_string()));
        assert_eq!(portable.config, Some("~/.firefox-config".to_string()));
    }

    #[test]
    fn test_annotated_document() {
        let config = PackagesConfig::default_config();
        let doc = config.to_annotated_document();

        assert!(doc.is_ok());
        let doc_str = doc.unwrap().to_string();

        // Should contain header
        assert!(doc_str.contains("Soar Declarative Package Configuration"));
        assert!(doc_str.contains("soar apply"));

        // Should contain field documentation comments
        assert!(doc_str.contains("#"));
    }
}
