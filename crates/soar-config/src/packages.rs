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

/// Binary mapping for custom symlink creation.
/// Maps a source executable path to a custom symlink name.
#[derive(Clone, Debug, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct BinaryMapping {
    /// Path to the executable within the package (relative to install dir).
    pub source: String,

    /// Name for the symlink in the bin directory.
    pub link_as: String,
}

/// Hook commands to run at various stages of package installation.
/// Commands are run with environment variables for paths.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct PackageHooks {
    /// Command to run after the package is downloaded (before extraction).
    pub post_download: Option<String>,

    /// Command to run after the package is extracted.
    pub post_extract: Option<String>,

    /// Command to run after the package is fully installed (symlinks created).
    pub post_install: Option<String>,

    /// Command to run before the package is removed.
    pub pre_remove: Option<String>,
}

/// Build configuration for compiling packages from source.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct BuildConfig {
    /// Shell commands to run for building the package.
    /// Commands are executed in sequence. If any command fails, the build stops.
    /// Available environment variables: $INSTALL_DIR, $PKG_NAME, $PKG_VERSION, $NPROC
    pub commands: Vec<String>,

    /// Optional list of dependencies required for building.
    /// Soar will warn if these are not found in PATH.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Sandbox configuration for restricting hook and build command execution.
/// Uses Landlock (Linux 5.13+) to restrict filesystem access.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct SandboxConfig {
    /// Require sandbox - fail if Landlock is not available instead of falling back
    /// to unsandboxed execution. Use this for builds you don't trust to run unsandboxed.
    #[serde(default)]
    pub require: bool,

    /// Additional paths that can be read (beyond defaults like /usr, /lib, etc).
    #[serde(default)]
    pub fs_read: Vec<String>,

    /// Additional paths that can be written (beyond install_dir and /tmp).
    #[serde(default)]
    pub fs_write: Vec<String>,

    /// Whether to allow network access (requires Landlock V4+, kernel 6.7+).
    #[serde(default)]
    pub network: bool,
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

    /// GitHub repository in "owner/repo" format for installing from releases.
    /// When set, soar fetches the latest release and downloads the matching asset.
    pub github: Option<String>,

    /// GitLab repository in "owner/repo" format for installing from releases.
    /// When set, soar fetches the latest release and downloads the matching asset.
    pub gitlab: Option<String>,

    /// Glob pattern to match release asset filename (e.g., "*linux*.AppImage").
    /// Required when github/gitlab is set to select the correct asset.
    pub asset_pattern: Option<String>,

    /// Whether to include pre-release versions when using github/gitlab sources.
    #[serde(default)]
    pub include_prerelease: Option<bool>,

    /// Glob pattern to match release tag names (e.g., "v*-stable", "nightly-*").
    /// If not set, the first matching release is used.
    pub tag_pattern: Option<String>,

    /// Custom command to fetch version and download URL.
    /// Output format: line 1 = version, line 2 = download URL, line 3 = size in bytes (optional).
    /// If not set and github/gitlab is used, version is fetched from releases API.
    pub version_command: Option<String>,

    /// Package type for URL installs (e.g., appimage, flatimage, archive).
    pub pkg_type: Option<String>,

    /// Entrypoint executable name (for URL packages where the binary name differs from package name).
    pub entrypoint: Option<String>,

    /// Multiple binary mappings (source path -> symlink name).
    /// Use this when a package provides multiple executables.
    pub binaries: Option<Vec<BinaryMapping>>,

    /// Path to a nested archive to extract after the main extraction.
    /// Useful for packages that contain archives within archives.
    pub nested_extract: Option<String>,

    /// Subdirectory within the extracted archive to treat as the root.
    /// Useful when archives have a versioned root folder like "app-v1.0/".
    pub extract_root: Option<String>,

    /// Hook commands to run at various stages of installation.
    pub hooks: Option<PackageHooks>,

    /// Build configuration for compiling from source.
    pub build: Option<BuildConfig>,

    /// Sandbox configuration for restricting hook and build command execution.
    /// When set, commands are restricted to read/write only specific paths.
    pub sandbox: Option<SandboxConfig>,

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

    /// Update source configuration for remote packages.
    pub update: Option<UpdateSource>,
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

/// Update source configuration for remote packages.
/// Specifies how to check for newer versions.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum UpdateSource {
    /// GitHub releases (auto-detected from github.com URLs).
    #[serde(rename = "github")]
    GitHub {
        /// Repository in "owner/repo" format.
        repo: String,
        /// Glob pattern to match asset filename (e.g., "*nvim*.appimage").
        asset_pattern: Option<String>,
        /// Whether to include pre-release versions.
        include_prerelease: Option<bool>,
        /// Glob pattern to match release tag names (e.g., "v*-stable").
        tag_pattern: Option<String>,
    },
    /// GitLab releases.
    #[serde(rename = "gitlab")]
    GitLab {
        /// Repository in "owner/repo" format.
        repo: String,
        /// Glob pattern to match asset filename.
        asset_pattern: Option<String>,
        /// Whether to include pre-release versions.
        include_prerelease: Option<bool>,
        /// Glob pattern to match release tag names (e.g., "v*-stable").
        tag_pattern: Option<String>,
    },
    /// Custom URL endpoint that returns JSON with version/download info.
    #[serde(rename = "url")]
    Url {
        /// URL that returns JSON response.
        url: String,
        /// JSON path to version field (e.g., "tag_name" or "version").
        version_path: String,
        /// JSON path to download URL field.
        download_path: String,
    },
}

/// Resolved package specification with name included.
#[derive(Clone, Debug, Default)]
pub struct ResolvedPackage {
    pub name: String,
    pub pkg_id: Option<String>,
    pub version: Option<String>,
    pub repo: Option<String>,
    pub url: Option<String>,
    pub github: Option<String>,
    pub gitlab: Option<String>,
    pub asset_pattern: Option<String>,
    pub include_prerelease: Option<bool>,
    pub tag_pattern: Option<String>,
    pub version_command: Option<String>,
    pub pkg_type: Option<String>,
    pub entrypoint: Option<String>,
    pub binaries: Option<Vec<BinaryMapping>>,
    pub nested_extract: Option<String>,
    pub extract_root: Option<String>,
    pub hooks: Option<PackageHooks>,
    pub build: Option<BuildConfig>,
    pub sandbox: Option<SandboxConfig>,
    pub pinned: bool,
    pub profile: Option<String>,
    pub portable: Option<PortableConfig>,
    pub install_patterns: Option<Vec<String>>,
    pub binary_only: bool,
    pub update: Option<UpdateSource>,
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
                    github: None,
                    gitlab: None,
                    asset_pattern: None,
                    include_prerelease: None,
                    tag_pattern: None,
                    version_command: None,
                    pkg_type: None,
                    entrypoint: None,
                    binaries: None,
                    nested_extract: None,
                    extract_root: None,
                    hooks: None,
                    build: None,
                    sandbox: None,
                    pinned,
                    profile: defaults.and_then(|d| d.profile.clone()),
                    portable: None,
                    install_patterns: defaults.and_then(|d| d.install_patterns.clone()),
                    binary_only: defaults.and_then(|d| d.binary_only).unwrap_or(false),
                    update: None,
                }
            }
            PackageSpec::Detailed(opts) => {
                // Treat "*" as None (latest version)
                let version = opts.version.as_ref().filter(|v| v.as_str() != "*").cloned();
                // URL/GitHub/GitLab packages: only pinned if explicitly set
                // Other packages: pinned if explicitly set or if a specific version is requested
                let is_remote =
                    opts.url.is_some() || opts.github.is_some() || opts.gitlab.is_some();
                let pinned = opts.pinned || (version.is_some() && !is_remote);
                ResolvedPackage {
                    name: name.to_string(),
                    pkg_id: opts.pkg_id.clone(),
                    version,
                    repo: opts.repo.clone(),
                    url: opts.url.clone(),
                    github: opts.github.clone(),
                    gitlab: opts.gitlab.clone(),
                    asset_pattern: opts.asset_pattern.clone(),
                    include_prerelease: opts.include_prerelease,
                    tag_pattern: opts.tag_pattern.clone(),
                    version_command: opts.version_command.clone(),
                    pkg_type: opts.pkg_type.clone(),
                    entrypoint: opts.entrypoint.clone(),
                    binaries: opts.binaries.clone(),
                    nested_extract: opts.nested_extract.clone(),
                    extract_root: opts.extract_root.clone(),
                    hooks: opts.hooks.clone(),
                    build: opts.build.clone(),
                    sandbox: opts.sandbox.clone(),
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
                    update: opts.update.clone(),
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

    /// Update package fields in packages.toml.
    ///
    /// This preserves comments and formatting in the file.
    /// - If `new_url` is provided, the `url` field is updated and `version` is updated only if it already exists.
    /// - If only `new_version` is provided, the `version` field is added/updated.
    /// - Simple string specs are skipped (user explicitly set a version).
    pub fn update_package(
        package_name: &str,
        new_url: Option<&str>,
        new_version: Option<&str>,
        config_path: Option<&str>,
    ) -> Result<()> {
        if new_url.is_none() && new_version.is_none() {
            return Ok(());
        }

        let config_path = match config_path {
            Some(p) => PathBuf::from(p),
            None => PACKAGES_CONFIG_PATH.read().unwrap().clone(),
        };

        if !config_path.exists() {
            return Err(ConfigError::PackagesConfigNotFound(
                config_path.display().to_string(),
            ));
        }

        let content = fs::read_to_string(&config_path)?;

        if let (None, Some(version)) = (new_url, new_version) {
            let doc = content.parse::<DocumentMut>()?;
            let packages = doc
                .get("packages")
                .and_then(|p| p.as_table())
                .ok_or_else(|| ConfigError::Custom("No [packages] section found".into()))?;

            let package = packages.get(package_name).ok_or_else(|| {
                ConfigError::Custom(format!("Package '{}' not found in config", package_name))
            })?;

            match package {
                toml_edit::Item::Value(toml_edit::Value::String(_)) => {
                    return Ok(());
                }
                toml_edit::Item::Value(toml_edit::Value::InlineTable(table)) => {
                    if table.contains_key("version") {
                        // Update in-place using string replacement to preserve formatting
                        let updated =
                            replace_version_in_inline_table(&content, package_name, version)?;
                        fs::write(&config_path, updated)?;
                    } else {
                        let updated = add_version_to_inline_table(&content, package_name, version)?;
                        fs::write(&config_path, updated)?;
                    }
                }
                toml_edit::Item::Table(_) => {
                    let mut doc = content.parse::<DocumentMut>()?;
                    if let Some(toml_edit::Item::Table(t)) = doc
                        .get_mut("packages")
                        .and_then(|p| p.as_table_mut())
                        .and_then(|t| t.get_mut(package_name))
                    {
                        t.insert("version", toml_edit::value(version));
                    }
                    fs::write(&config_path, doc.to_string())?;
                }
                _ => {
                    return Err(ConfigError::Custom(format!(
                        "Unexpected package format for '{}'",
                        package_name
                    )));
                }
            }

            info!(
                "Updated version to {} for '{}' in {}",
                version,
                package_name,
                config_path.display()
            );
            return Ok(());
        }

        let mut doc = content.parse::<DocumentMut>()?;

        let packages = doc
            .get_mut("packages")
            .and_then(|p| p.as_table_mut())
            .ok_or_else(|| ConfigError::Custom("No [packages] section found".into()))?;

        let package = packages.get_mut(package_name).ok_or_else(|| {
            ConfigError::Custom(format!("Package '{}' not found in config", package_name))
        })?;

        let url = new_url.unwrap();
        match package {
            toml_edit::Item::Value(toml_edit::Value::InlineTable(table)) => {
                table.insert("url", url.into());
                if let Some(version) = new_version {
                    if table.contains_key("version") {
                        table.insert("version", version.into());
                    }
                }
            }
            toml_edit::Item::Table(table) => {
                table.insert("url", toml_edit::value(url));
                if let Some(version) = new_version {
                    if table.contains_key("version") {
                        table.insert("version", toml_edit::value(version));
                    }
                }
            }
            _ => {
                return Err(ConfigError::Custom(format!(
                    "Unexpected package format for '{}'",
                    package_name
                )));
            }
        }

        fs::write(&config_path, doc.to_string())?;

        let updated = match new_version {
            Some(v) => format!("URL and version ({})", v),
            None => "URL".to_string(),
        };
        info!(
            "Updated {} for '{}' in {}",
            updated,
            package_name,
            config_path.display()
        );

        Ok(())
    }
}

/// Add version field to an inline table using string manipulation.
fn add_version_to_inline_table(content: &str, package_name: &str, version: &str) -> Result<String> {
    let search = format!("{} = {{", package_name);
    let Some(brace_pos) = content.find(&search).map(|p| p + search.len() - 1) else {
        let start = content
            .find(&format!("{} =", package_name))
            .ok_or_else(|| ConfigError::Custom(format!("Package '{}' not found", package_name)))?;
        let brace_pos = content[start..]
            .find('{')
            .map(|p| start + p)
            .ok_or_else(|| {
                ConfigError::Custom(format!("No inline table for '{}'", package_name))
            })?;
        return Ok(insert_version_at(content, brace_pos, version));
    };

    Ok(insert_version_at(content, brace_pos, version))
}

/// Insert version field after opening brace, preserving indentation for multiline tables.
fn insert_version_at(content: &str, brace_pos: usize, version: &str) -> String {
    let after_brace = &content[brace_pos + 1..];
    let is_multiline = after_brace.starts_with('\n') || after_brace.starts_with("\r\n");

    if is_multiline {
        let next_line_start = brace_pos + 1 + after_brace.find('\n').map(|p| p + 1).unwrap_or(0);
        let indent: String = content[next_line_start..]
            .chars()
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .collect();
        format!(
            "{}\n{}version = \"{}\",{}",
            &content[..=brace_pos],
            indent,
            version,
            &content[brace_pos + 1..]
        )
    } else {
        format!(
            "{} version = \"{}\",{}",
            &content[..=brace_pos],
            version,
            after_brace
        )
    }
}

/// Replace version value in an inline table in-place.
fn replace_version_in_inline_table(
    content: &str,
    package_name: &str,
    version: &str,
) -> Result<String> {
    let err = || ConfigError::Custom(format!("Failed to update version for '{}'", package_name));

    let pkg_start = content
        .find(&format!("{} = ", package_name))
        .ok_or_else(err)?;
    let table_start = content[pkg_start..]
        .find('{')
        .map(|p| pkg_start + p)
        .ok_or_else(err)?;
    let table_end = content[table_start..]
        .find('}')
        .map(|p| table_start + p)
        .ok_or_else(err)?;

    let table = &content[table_start..=table_end];
    let ver_pos = table.find("version = \"").ok_or_else(err)?;
    let value_start = table_start + ver_pos + "version = \"".len();
    let value_end = content[value_start..]
        .find('"')
        .map(|p| value_start + p)
        .ok_or_else(err)?;

    Ok(format!(
        "{}{}{}",
        &content[..value_start],
        version,
        &content[value_end..]
    ))
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

    #[test]
    fn test_replace_version_in_inline_table() {
        let content = r#"[packages]
nvim-prerelease = { version = "2025-01-15T05:17:43Z", github = "neovim/neovim", asset_pattern = "*linux-x86_64.appimage" }
other = { version = "1.0.0" }
"#;
        let result =
            replace_version_in_inline_table(content, "nvim-prerelease", "2026-01-16T10:00:00Z")
                .unwrap();

        // Version should be updated in-place
        assert!(result.contains("version = \"2026-01-16T10:00:00Z\""));
        // Other fields should remain in the same position
        assert!(result.contains(
            "nvim-prerelease = { version = \"2026-01-16T10:00:00Z\", github = \"neovim/neovim\""
        ));
        // Other package should be unchanged
        assert!(result.contains("other = { version = \"1.0.0\" }"));
    }

    #[test]
    fn test_add_version_to_inline_table() {
        let content = r#"[packages]
nvim = { github = "neovim/neovim", asset_pattern = "*.appimage" }
"#;
        let result = add_version_to_inline_table(content, "nvim", "1.0.0").unwrap();

        // Version should be added after the opening brace
        assert!(result.contains("{ version = \"1.0.0\","));
        // Other fields should still be present
        assert!(result.contains("github = \"neovim/neovim\""));
    }
}
