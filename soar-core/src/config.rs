use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{LazyLock, RwLock},
};

use documented::{Documented, DocumentedFields};
use serde::{de::Error, Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};
use tracing::{info, warn};

use crate::{
    error::{ConfigError, SoarError},
    repositories::get_platform_repositories,
    toml::{annotate_toml_array_of_tables, annotate_toml_table},
    utils::{
        build_path, default_install_patterns, get_platform, home_config_path, home_data_path,
        parse_duration,
    },
    SoarResult,
};

type Result<T> = std::result::Result<T, ConfigError>;

/// A profile defines a local package store and its configuration.
#[derive(Clone, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct Profile {
    /// Root directory for this profile’s data and packages.
    ///
    /// If `packages_path` is not set, packages will be stored in `root_path/packages`.
    pub root_path: String,

    /// Optional path where packages are stored.
    ///
    /// If unset, defaults to `root_path/packages`.
    pub packages_path: Option<String>,
}

impl Profile {
    fn get_bin_path(&self) -> SoarResult<PathBuf> {
        Ok(self.get_root_path()?.join("bin"))
    }

    fn get_db_path(&self) -> SoarResult<PathBuf> {
        Ok(self.get_root_path()?.join("db"))
    }

    pub fn get_packages_path(&self) -> SoarResult<PathBuf> {
        if let Some(ref packages_path) = self.packages_path {
            build_path(packages_path)
        } else {
            Ok(self.get_root_path()?.join("packages"))
        }
    }

    pub fn get_cache_path(&self) -> SoarResult<PathBuf> {
        Ok(self.get_root_path()?.join("cache"))
    }

    fn get_repositories_path(&self) -> SoarResult<PathBuf> {
        Ok(self.get_root_path()?.join("repos"))
    }

    pub fn get_root_path(&self) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_ROOT") {
            return build_path(&env_path);
        }
        build_path(&self.root_path)
    }
}

/// Defines a remote repository that provides packages.
#[derive(Clone, Deserialize, Serialize, Documented, DocumentedFields)]
pub struct Repository {
    /// Unique name of the repository.
    pub name: String,

    /// URL to the repository's metadata file.
    pub url: String,

    /// Enables desktop integration for packages from this repository.
    /// Default: false
    pub desktop_integration: Option<bool>,

    /// URL to the repository's public key (for signature verification).
    pub pubkey: Option<String>,

    /// Whether the repository is enabled.
    /// Default: true
    pub enabled: Option<bool>,

    /// Enables signature verification for this repository.
    /// Default is derived based on the existence of `pubkey`
    signature_verification: Option<bool>,

    /// Optional sync interval (e.g., "1h", "12h", "1d").
    /// Default: "3h"
    sync_interval: Option<String>,
}

impl Repository {
    pub fn get_path(&self) -> std::result::Result<PathBuf, SoarError> {
        Ok(get_config().get_repositories_path()?.join(&self.name))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn signature_verification(&self) -> bool {
        if let Some(global_override) = get_config().signature_verification {
            return global_override;
        }
        if self.pubkey.is_none() {
            return false;
        };
        self.signature_verification.unwrap_or(true)
    }

    pub fn sync_interval(&self) -> u128 {
        match get_config()
            .sync_interval
            .clone()
            .or(self.sync_interval.clone())
            .as_deref()
            .unwrap_or("3h")
        {
            "always" => 0,
            "never" => u128::MAX,
            "auto" => 3 * 3_600_000,
            value => parse_duration(value).unwrap_or(3_600_000),
        }
    }
}

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
    /// Default: $SOAR_ROOT/packages
    pub repositories_path: Option<String>,

    /// If true, enables parallel downloading of packages.
    /// Default: true
    pub parallel: Option<bool>,

    /// Maximum number of parallel downloads.
    /// Default: 4
    pub parallel_limit: Option<u32>,

    /// Maximum number of concurrent requests for GHCR (GitHub Container Registry).
    /// Default: 8
    pub ghcr_concurrency: Option<u64>,

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
}

pub static CONFIG: LazyLock<RwLock<Option<Config>>> = LazyLock::new(|| RwLock::new(None));
pub static CURRENT_PROFILE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

pub static CONFIG_PATH: LazyLock<RwLock<PathBuf>> = LazyLock::new(|| {
    RwLock::new(match std::env::var("SOAR_CONFIG") {
        Ok(path_str) => PathBuf::from(path_str),
        Err(_) => PathBuf::from(home_config_path())
            .join("soar")
            .join("config.toml"),
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
        let soar_root =
            std::env::var("SOAR_ROOT").unwrap_or_else(|_| format!("{}/soar", home_data_path()));

        let default_profile = Profile {
            root_path: soar_root.clone(),
            packages_path: Some(format!("{}/packages", soar_root)),
        };
        let default_profile_name = "default".to_string();

        let current_platform = get_platform();
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
                    url: format!(
                        "{}",
                        repo_info.url_template.replace("{}", &current_platform)
                    ),
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

            bin_path: Some(format!("{}/bin", soar_root)),
            cache_path: Some(format!("{}/cache", soar_root)),
            db_path: Some(format!("{}/db", soar_root)),
            repositories_path: Some(format!("{}/repos", soar_root)),

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
            Ok(content) => match toml::from_str(&content) {
                Ok(c) => Ok(c),
                Err(err) => Err(ConfigError::TomlDeError(err)),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default_config::<&str>(false, &[]))
            }
            Err(err) => Err(ConfigError::IoError(err)),
        }?;

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
            .ok_or_else(|| unreachable!())
    }

    pub fn get_profile(&self, name: &str) -> Result<&Profile> {
        self.profile
            .get(name)
            .ok_or(ConfigError::MissingProfile(name.to_string()))
    }

    pub fn get_bin_path(&self) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_BIN") {
            return build_path(&env_path);
        }
        if let Some(bin_path) = &self.bin_path {
            return build_path(bin_path);
        }
        self.default_profile()?.get_bin_path()
    }

    pub fn get_db_path(&self) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_DB") {
            return build_path(&env_path);
        }
        if let Some(soar_db) = &self.db_path {
            return build_path(soar_db);
        }
        self.default_profile()?.get_db_path()
    }

    pub fn get_packages_path(&self, profile_name: Option<String>) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_PACKAGES") {
            return build_path(&env_path);
        }
        let profile_name = profile_name.unwrap_or_else(get_current_profile);
        self.get_profile(&profile_name)?.get_packages_path()
    }

    pub fn get_cache_path(&self) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_CACHE") {
            return build_path(&env_path);
        }
        if let Some(soar_cache) = &self.cache_path {
            return build_path(soar_cache);
        }
        self.get_profile(&get_current_profile())?.get_cache_path()
    }

    pub fn get_repositories_path(&self) -> SoarResult<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_REPOSITORIES") {
            return build_path(&env_path);
        }
        if let Some(repositories_path) = &self.repositories_path {
            return build_path(repositories_path);
        }
        self.default_profile()?.get_repositories_path()
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
            .map_or(false, |repo| repo.desktop_integration.unwrap_or(false))
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
        let toml_string = toml::to_string_pretty(self).map_err(ConfigError::TomlSerError)?;
        let mut doc = toml_string
            .parse::<DocumentMut>()
            .map_err(|e| ConfigError::TomlDeError(toml::de::Error::custom(e.to_string())))?;

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

    fs::create_dir_all(config_path.parent().unwrap())?;

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
