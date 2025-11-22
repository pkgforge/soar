use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{LazyLock, RwLock},
};

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::{
    path::{resolve_path, xdg_config_home, xdg_data_home},
    system::platform,
    time::parse_duration,
};
use toml_edit::DocumentMut;
use tracing::{info, warn};

use crate::{
    annotations::{annotate_toml_array_of_tables, annotate_toml_table},
    error::{ConfigError, Result},
    profile::Profile,
    repository::{get_platform_repositories, Repository},
    utils::default_install_patterns,
};

/// Application's configuration
#[derive(Clone, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct Config {
    /// The name of the default profile to use.
    pub default_profile: String,

    /// A map of profile names to their configurations.
    pub profile: HashMap<String, Profile>,

    /// List of configured repositories.
    pub repositories: Vec<Repository>,

    /// Path to the local cache directory.
    /// Default: $SOAR_ROOT/cache
    pub cache_path: Option<String>,

    /// Path where the Soar package database is stored.
    /// Default: $SOAR_ROOT/db
    pub db_path: Option<String>,

    /// Directory where binary symlinks are placed.
    /// Default: $SOAR_ROOT/bin
    pub bin_path: Option<String>,

    /// Path to the local clone of all repositories.
    /// Default: $SOAR_ROOT/repos
    pub repositories_path: Option<String>,

    /// Portable dirs path
    /// Default: $SOAR_ROOT/portable-dirs
    pub portable_dirs: Option<String>,

    /// If true, enables parallel downloading of packages.
    /// Default: true
    pub parallel: Option<bool>,

    /// Maximum number of parallel downloads.
    /// Default: 4
    pub parallel_limit: Option<u32>,

    /// Maximum number of concurrent requests for GHCR (GitHub Container Registry).
    /// Default: 8
    pub ghcr_concurrency: Option<usize>,

    /// Limits the number of results returned by a search.
    /// Default: 20
    pub search_limit: Option<usize>,

    /// Allows packages to be updated across different repositories.
    /// NOTE: This is not yet implemented
    pub cross_repo_updates: Option<bool>,

    /// Glob patterns for package files that should be included during install.
    /// Default: ["!*.log", "!SBUILD", "!*.json", "!*.version"]
    pub install_patterns: Option<Vec<String>>,

    /// Global override for signature verification
    pub signature_verification: Option<bool>,

    /// Global override for desktop integration
    pub desktop_integration: Option<bool>,

    /// Global override for sync interval
    pub sync_interval: Option<String>,

    /// Sync interval for nests
    pub nests_sync_interval: Option<String>,
}

pub static CONFIG: LazyLock<RwLock<Option<Config>>> = LazyLock::new(|| RwLock::new(None));
pub static CURRENT_PROFILE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

pub static CONFIG_PATH: LazyLock<RwLock<PathBuf>> = LazyLock::new(|| {
    RwLock::new(match std::env::var("SOAR_CONFIG") {
        Ok(path_str) => PathBuf::from(path_str),
        Err(_) => xdg_config_home().join("soar").join("config.toml"),
    })
});

pub fn init() -> Result<()> {
    let config = Config::new()?;
    let mut global_config = CONFIG.write().unwrap();
    *global_config = Some(config);
    Ok(())
}

fn ensure_config_initialized() {
    let mut config_guard = CONFIG.write().unwrap();
    if config_guard.is_none() {
        *config_guard = Some(Config::default_config::<&str>(false, &[]));
    }
}

pub fn get_config() -> Config {
    {
        let config_guard = CONFIG.read().unwrap();
        if config_guard.is_some() {
            drop(config_guard);
            return CONFIG.read().unwrap().as_ref().unwrap().clone();
        }
    }

    ensure_config_initialized();

    CONFIG.read().unwrap().as_ref().unwrap().clone()
}

pub fn get_current_profile() -> String {
    let current_profile = CURRENT_PROFILE.read().unwrap();
    current_profile
        .clone()
        .unwrap_or_else(|| get_config().default_profile.clone())
}

pub fn set_current_profile(name: &str) -> Result<()> {
    let config = get_config();
    if !config.profile.contains_key(name) {
        return Err(ConfigError::InvalidProfile(name.to_string()));
    }
    let mut profile = CURRENT_PROFILE.write().unwrap();
    *profile = Some(name.to_string());
    Ok(())
}

impl Config {
    pub fn default_config<T: AsRef<str>>(external: bool, selected_repos: &[T]) -> Self {
        let soar_root = std::env::var("SOAR_ROOT")
            .unwrap_or_else(|_| format!("{}/soar", xdg_data_home().display()));

        let default_profile = Profile {
            root_path: soar_root.clone(),
            packages_path: Some(format!("{soar_root}/packages")),
        };
        let default_profile_name = "default".to_string();

        let current_platform = platform();
        let mut repositories = Vec::new();
        let selected_set: HashSet<&str> = selected_repos.iter().map(|s| s.as_ref()).collect();

        for repo_info in get_platform_repositories().into_iter() {
            // Check if repository supports the current platform
            if !repo_info.platforms.contains(&current_platform.as_str()) {
                continue;
            }

            if repo_info.is_core || external || selected_set.contains(repo_info.name) {
                repositories.push(Repository {
                    name: repo_info.name.to_string(),
                    url: repo_info.url_template.replace("{}", &current_platform),
                    pubkey: repo_info.pubkey.map(String::from),
                    desktop_integration: repo_info.desktop_integration,
                    enabled: repo_info.enabled,
                    signature_verification: repo_info.signature_verification,
                    sync_interval: repo_info.sync_interval.map(String::from),
                });
            }
        }

        // Filter by selected repositories if specified
        let repositories = if selected_repos.is_empty() {
            repositories
        } else {
            repositories
                .into_iter()
                .filter(|repo| selected_set.contains(repo.name.as_str()))
                .collect()
        };

        // Show warning if no repositories are available for this platform
        if repositories.is_empty() {
            if selected_repos.is_empty() {
                warn!(
                    "No official repositories available for {}. You can add custom repositories in your config file.",
                    current_platform
                );
            } else {
                warn!("No repositories enabled.");
            }
        }

        Self {
            profile: HashMap::from([(default_profile_name.clone(), default_profile)]),
            default_profile: default_profile_name,

            bin_path: Some(format!("{soar_root}/bin")),
            cache_path: Some(format!("{soar_root}/cache")),
            db_path: Some(format!("{soar_root}/db")),
            repositories_path: Some(format!("{soar_root}/repos")),
            portable_dirs: Some(format!("{soar_root}/portable-dirs")),

            repositories,
            parallel: Some(true),
            parallel_limit: Some(4),
            search_limit: Some(20),
            ghcr_concurrency: Some(8),
            cross_repo_updates: Some(false),
            install_patterns: Some(default_install_patterns()),

            signature_verification: None,
            desktop_integration: None,
            sync_interval: None,
            nests_sync_interval: None,
        }
    }

    /// Creates a new configuration by loading it from the configuration file.
    /// If the configuration file is not found, it uses the default configuration.
    pub fn new() -> Result<Self> {
        if std::env::var("SOAR_STEALTH").is_ok() {
            return Ok(Self::default_config::<&str>(false, &[]));
        }

        let config_path = CONFIG_PATH.read().unwrap().to_path_buf();

        let mut config = match fs::read_to_string(&config_path) {
            Ok(content) => toml::from_str(&content)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Self::default_config::<&str>(false, &[])
            }
            Err(err) => return Err(ConfigError::IoError(err)),
        };

        config.resolve()?;

        Ok(config)
    }

    pub fn resolve(&mut self) -> Result<()> {
        if !self.profile.contains_key(&self.default_profile) {
            return Err(ConfigError::MissingDefaultProfile(
                self.default_profile.clone(),
            ));
        }

        if self.parallel.unwrap_or(true) {
            self.parallel_limit.get_or_insert(4);
        }

        if self.install_patterns.is_none() {
            self.install_patterns = Some(default_install_patterns());
        }

        self.ghcr_concurrency.get_or_insert(8);
        self.search_limit.get_or_insert(20);
        self.cross_repo_updates.get_or_insert(false);

        let mut seen_repos = HashSet::new();

        for repo in &mut self.repositories {
            if repo.name == "local" {
                return Err(ConfigError::ReservedRepositoryName);
            }
            if repo.name.starts_with("nest-") {
                return Err(ConfigError::InvalidRepositoryNameStartsWithNest);
            }
            if !seen_repos.insert(&repo.name) {
                return Err(ConfigError::DuplicateRepositoryName(repo.name.clone()));
            }

            repo.enabled.get_or_insert(true);

            if repo.desktop_integration.is_none() {
                match repo.name.as_str() {
                    "bincache" => repo.desktop_integration = Some(false),
                    "pkgcache" | "ivan-hc-am" | "appimage.github.io" => {
                        repo.desktop_integration = Some(true)
                    }
                    _ => {}
                }
            }

            if repo.pubkey.is_none() {
                match repo.name.as_str() {
                    "bincache" => {
                        repo.pubkey =
                            Some("https://meta.pkgforge.dev/bincache/minisign.pub".to_string())
                    }
                    "pkgcache" => {
                        repo.pubkey =
                            Some("https://meta.pkgforge.dev/pkgcache/minisign.pub".to_string())
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    pub fn default_profile(&self) -> Result<&Profile> {
        self.profile
            .get(&self.default_profile)
            .ok_or_else(|| ConfigError::MissingDefaultProfile(self.default_profile.clone()))
    }

    pub fn get_profile(&self, name: &str) -> Result<&Profile> {
        self.profile
            .get(name)
            .ok_or(ConfigError::MissingProfile(name.to_string()))
    }

    pub fn get_bin_path(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_BIN") {
            return Ok(resolve_path(&env_path)?);
        }
        if let Some(bin_path) = &self.bin_path {
            return Ok(resolve_path(bin_path)?);
        }
        self.default_profile()?.get_bin_path()
    }

    pub fn get_db_path(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_DB") {
            return Ok(resolve_path(&env_path)?);
        }
        if let Some(soar_db) = &self.db_path {
            return Ok(resolve_path(soar_db)?);
        }
        self.default_profile()?.get_db_path()
    }

    pub fn get_packages_path(&self, profile_name: Option<String>) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_PACKAGES") {
            return Ok(resolve_path(&env_path)?);
        }
        let profile_name = profile_name.unwrap_or_else(get_current_profile);
        self.get_profile(&profile_name)?.get_packages_path()
    }

    pub fn get_cache_path(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_CACHE") {
            return Ok(resolve_path(&env_path)?);
        }
        if let Some(soar_cache) = &self.cache_path {
            return Ok(resolve_path(soar_cache)?);
        }
        self.get_profile(&get_current_profile())?.get_cache_path()
    }

    pub fn get_repositories_path(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_REPOSITORIES") {
            return Ok(resolve_path(&env_path)?);
        }
        if let Some(repositories_path) = &self.repositories_path {
            return Ok(resolve_path(repositories_path)?);
        }
        self.default_profile()?.get_repositories_path()
    }

    pub fn get_portable_dirs(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_PORTABLE_DIRS") {
            return Ok(resolve_path(&env_path)?);
        }

        if let Some(portable_dirs) = &self.portable_dirs {
            return Ok(resolve_path(portable_dirs)?);
        }
        self.default_profile()?.get_portable_dirs()
    }

    pub fn get_nests_sync_interval(&self) -> u128 {
        match self.nests_sync_interval.as_deref().unwrap_or("3h") {
            "always" => 0,
            "never" => u128::MAX,
            "auto" => 3 * 3_600_000,
            value => parse_duration(value).unwrap_or(3_600_000),
        }
    }

    pub fn get_repository(&self, repo_name: &str) -> Option<&Repository> {
        self.repositories
            .iter()
            .find(|repo| repo.name == repo_name && repo.is_enabled())
    }

    pub fn has_desktop_integration(&self, repo_name: &str) -> bool {
        if let Some(global_override) = self.desktop_integration {
            return global_override;
        }
        self.get_repository(repo_name)
            .is_some_and(|repo| repo.desktop_integration.unwrap_or(false))
    }

    pub fn save(&self) -> Result<()> {
        let config_path = CONFIG_PATH.read().unwrap().to_path_buf();
        let serialized = toml::to_string_pretty(self)?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, serialized)?;
        info!("Configuration saved to {}", config_path.display());
        Ok(())
    }

    pub fn to_annotated_document(&self) -> Result<DocumentMut> {
        use toml_edit::Item;

        let toml_string = toml::to_string_pretty(self)?;
        let mut doc = toml_string.parse::<DocumentMut>()?;

        annotate_toml_table::<Config>(doc.as_table_mut(), true)?;

        if let Some(profiles_map_table_item) = doc.get_mut("profile") {
            if let Some(profiles_map_table) = profiles_map_table_item.as_table_mut() {
                for (_profile_name, profile_item) in profiles_map_table.iter_mut() {
                    if let Item::Table(profile_table) = profile_item {
                        annotate_toml_table::<Profile>(profile_table, false)?;
                    }
                }
            }
        }

        if let Some(repositories_item) = doc.get_mut("repositories") {
            if let Some(repositories_array) = repositories_item.as_array_of_tables_mut() {
                annotate_toml_array_of_tables::<Repository>(repositories_array)?;
            }
        }

        Ok(doc)
    }
}

pub fn generate_default_config<T: AsRef<str>>(external: bool, repos: &[T]) -> Result<()> {
    let config_path = CONFIG_PATH.read().unwrap().to_path_buf();

    if config_path.exists() {
        return Err(ConfigError::ConfigAlreadyExists);
    }

    let def_config = Config::default_config(external, repos);
    let annotated_doc = def_config.to_annotated_document()?;

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&config_path, annotated_doc.to_string())?;
    info!(
        "Default configuration file generated with documentation at: {}",
        config_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ConfigError, test_utils::with_env};

    #[test]
    fn test_default_config_creation() {
        let config = Config::default_config::<&str>(false, &[]);

        assert_eq!(config.default_profile, "default");
        assert!(config.profile.contains_key("default"));
        assert!(config.parallel.unwrap_or(false));
        assert_eq!(config.parallel_limit, Some(4));
        assert_eq!(config.search_limit, Some(20));
        assert_eq!(config.ghcr_concurrency, Some(8));
        assert_eq!(config.cross_repo_updates, Some(false));
    }

    #[test]
    fn test_default_config_with_selected_repos() {
        let config = Config::default_config(false, &["bincache"]);

        assert!(!config.repositories.is_empty());
        assert!(config.repositories.iter().any(|r| r.name == "bincache"));
    }

    #[test]
    fn test_default_config_external_repos() {
        let config = Config::default_config::<&str>(true, &[]);

        let has_external = config
            .repositories
            .iter()
            .any(|r| r.name == "ivan-hc-am" || r.name == "appimage-github-io");
        assert!(has_external || config.repositories.is_empty()); // depends on platform
    }

    #[test]
    fn test_config_resolve_missing_default_profile() {
        let mut config = Config::default_config::<&str>(false, &[]);
        config.default_profile = "nonexistent".to_string();

        let result = config.resolve();
        assert!(matches!(result, Err(ConfigError::MissingDefaultProfile(_))));
    }

    #[test]
    fn test_config_resolve_reserved_repo_name() {
        let mut config = Config::default_config::<&str>(false, &[]);
        config.repositories.push(Repository {
            name: "local".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        });

        let result = config.resolve();
        assert!(matches!(result, Err(ConfigError::ReservedRepositoryName)));
    }

    #[test]
    fn test_config_resolve_nest_prefix() {
        let mut config = Config::default_config::<&str>(false, &[]);
        config.repositories.push(Repository {
            name: "nest-invalid".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        });

        let result = config.resolve();
        assert!(matches!(
            result,
            Err(ConfigError::InvalidRepositoryNameStartsWithNest)
        ));
    }

    #[test]
    fn test_config_resolve_duplicate_repo() {
        let mut config = Config::default_config::<&str>(false, &[]);
        config.repositories.push(Repository {
            name: "duplicate".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        });
        config.repositories.push(Repository {
            name: "duplicate".to_string(),
            url: "https://example2.com".to_string(),
            desktop_integration: None,
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        });

        let result = config.resolve();
        assert!(matches!(
            result,
            Err(ConfigError::DuplicateRepositoryName(_))
        ));
    }

    #[test]
    fn test_config_resolve_sets_defaults() {
        let mut config = Config::default_config::<&str>(false, &[]);
        config.ghcr_concurrency = None;
        config.search_limit = None;
        config.cross_repo_updates = None;
        config.install_patterns = None;

        config.resolve().unwrap();

        assert_eq!(config.ghcr_concurrency, Some(8));
        assert_eq!(config.search_limit, Some(20));
        assert_eq!(config.cross_repo_updates, Some(false));
        assert!(config.install_patterns.is_some());
    }

    #[test]
    fn test_get_profile() {
        let config = Config::default_config::<&str>(false, &[]);

        let profile = config.get_profile("default");
        assert!(profile.is_ok());

        let missing = config.get_profile("nonexistent");
        assert!(matches!(missing, Err(ConfigError::MissingProfile(_))));
    }

    #[test]
    fn test_get_repository() {
        let config = Config::default_config::<&str>(false, &[]);

        if let Some(repo) = config.repositories.first() {
            let found = config.get_repository(&repo.name);
            assert!(found.is_some());
        }

        let missing = config.get_repository("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_has_desktop_integration() {
        let mut config = Config::default_config::<&str>(false, &[]);

        config.desktop_integration = Some(true);
        assert!(config.has_desktop_integration("any_repo"));

        config.desktop_integration = Some(false);
        assert!(!config.has_desktop_integration("any_repo"));

        config.desktop_integration = None;
        config.repositories.push(Repository {
            name: "test_repo".to_string(),
            url: "https://example.com".to_string(),
            desktop_integration: Some(true),
            pubkey: None,
            enabled: Some(true),
            signature_verification: None,
            sync_interval: None,
        });
        assert!(config.has_desktop_integration("test_repo"));
    }

    #[test]
    fn test_get_nests_sync_interval() {
        let mut config = Config::default_config::<&str>(false, &[]);

        config.nests_sync_interval = Some("always".to_string());
        assert_eq!(config.get_nests_sync_interval(), 0);

        config.nests_sync_interval = Some("never".to_string());
        assert_eq!(config.get_nests_sync_interval(), u128::MAX);

        config.nests_sync_interval = Some("auto".to_string());
        assert_eq!(config.get_nests_sync_interval(), 3 * 3_600_000);

        config.nests_sync_interval = Some("1h".to_string());
        assert_eq!(config.get_nests_sync_interval(), 3_600_000);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default_config::<&str>(false, &[]);
        let serialized = toml::to_string(&config);
        assert!(serialized.is_ok());

        let deserialized: std::result::Result<Config, _> = toml::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok());
    }

    #[test]
    fn test_config_path_env_override() {
        with_env(vec![("SOAR_BIN", "/custom/bin")], || {
            let config = Config::default_config::<&str>(false, &[]);
            let bin_path = config.get_bin_path().unwrap();
            assert_eq!(bin_path, PathBuf::from("/custom/bin"));
        });
    }
}
