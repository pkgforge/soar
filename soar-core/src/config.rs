use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{LazyLock, RwLock, RwLockReadGuard},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::SoarError,
    utils::{build_path, get_platform, home_config_path, home_data_path},
};

type Result<T> = std::result::Result<T, SoarError>;

#[derive(Deserialize, Serialize)]
pub struct Profile {
    pub root_path: String,

    #[serde(skip_serializing)]
    pub cache_path: Option<String>,

    #[serde(skip_serializing)]
    pub packages_path: Option<String>,
}

impl Profile {
    fn get_bin_path(&self) -> PathBuf {
        build_path(&self.root_path).unwrap().join("bin")
    }

    fn get_db_path(&self) -> PathBuf {
        build_path(&self.root_path).unwrap().join("db")
    }

    pub fn get_packages_path(&self) -> PathBuf {
        build_path(
            &self
                .packages_path
                .clone()
                .unwrap_or_else(|| format!("{}/packages", self.root_path)),
        )
        .unwrap()
    }

    pub fn get_cache_path(&self) -> PathBuf {
        build_path(&self.root_path).unwrap().join("cache")
    }

    fn get_repositories_path(&self) -> PathBuf {
        build_path(&self.root_path).unwrap().join("repos")
    }
}

/// Struct representing a repository configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct Repository {
    /// Name of the repository.
    pub name: String,

    /// Metadata URL.
    pub url: String,
}

impl Repository {
    pub fn get_path(&self) -> Result<PathBuf> {
        Ok(get_config().get_repositories_path()?.join(&self.name))
    }
}

/// Application's configuration
#[derive(Deserialize, Serialize)]
pub struct Config {
    pub repositories: Vec<Repository>,
    pub profile: HashMap<String, Profile>,

    /// Path to the directory where app data is stored.
    #[serde(skip_serializing)]
    pub db_path: Option<String>,

    /// Path to the directory where binary symlinks is stored.
    #[serde(skip_serializing)]
    pub bin_path: Option<String>,

    #[serde(skip_serializing)]
    pub repositories_path: Option<String>,

    /// Indicates whether downloads should be performed in parallel.
    #[serde(skip_serializing)]
    pub parallel: Option<bool>,

    /// Limit the number of parallel downloads
    #[serde(skip_serializing)]
    pub parallel_limit: Option<u32>,

    /// GHCR Layer concurrency
    #[serde(skip_serializing)]
    pub ghcr_concurrency: Option<u64>,

    /// Limit the number of search results to display
    #[serde(skip_serializing)]
    pub search_limit: Option<usize>,

    /// Default profile to use
    pub default_profile: String,

    /// Whether to allow cross-repo updates
    #[serde(skip_serializing)]
    pub cross_repo_updates: Option<bool>,
}

pub fn init() {
    let _ = &*CONFIG;
}

pub static CONFIG: LazyLock<RwLock<Config>> =
    LazyLock::new(|| RwLock::new(Config::new().expect("Failed to initialize config")));
pub static CURRENT_PROFILE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

pub fn get_config() -> RwLockReadGuard<'static, Config> {
    CONFIG.read().unwrap()
}

pub fn get_current_profile() -> String {
    let config = get_config();
    let current_profile = CURRENT_PROFILE.read().unwrap();
    current_profile
        .clone()
        .unwrap_or_else(|| config.default_profile.clone())
}

pub fn set_current_profile(name: &str) -> Result<()> {
    let config = get_config();
    let mut profile = CURRENT_PROFILE.write().unwrap();
    match config.profile.contains_key(name) {
        true => *profile = Some(name.to_string()),
        false => return Err(SoarError::InvalidProfile(name.to_string())),
    }
    Ok(())
}

impl Config {
    /// Creates a new configuration by loading it from the configuration file.
    /// If the configuration file is not found, it uses the default configuration.
    pub fn new() -> Result<Self> {
        let home_config = home_config_path();
        let pkg_config = PathBuf::from(home_config).join("soar");
        let config_path = pkg_config.join("config.toml");

        let mut config = match fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(c) => Ok(c),
                Err(_) => Err(SoarError::InvalidConfig),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err(SoarError::InvalidConfig),
        }?;

        if !config.profile.contains_key(&config.default_profile) {
            return Err(SoarError::InvalidConfig);
        }

        if config.parallel.unwrap_or(true) {
            config.parallel_limit = config.parallel_limit.or(Some(4));
        }

        let mut seen = HashSet::new();
        for repo in &config.repositories {
            if repo.name == "local" {
                return Err(SoarError::InvalidConfig);
            }
            if !seen.insert(&repo.name) {
                return Err(SoarError::InvalidConfig);
            }
        }

        Ok(config)
    }

    pub fn default_profile(&self) -> Result<&Profile> {
        self.profile
            .get(&self.default_profile)
            .ok_or(SoarError::InvalidConfig)
    }

    pub fn get_profile(&self, name: &str) -> Result<&Profile> {
        self.profile.get(name).ok_or(SoarError::InvalidConfig)
    }

    pub fn get_root_path(&self) -> Result<PathBuf> {
        Ok(build_path(
            &self.get_profile(&get_current_profile())?.root_path,
        )?)
    }

    pub fn get_bin_path(&self) -> Result<PathBuf> {
        Ok(self.default_profile()?.get_bin_path())
    }

    pub fn get_db_path(&self) -> Result<PathBuf> {
        if let Some(soar_db) = &self.db_path {
            build_path(soar_db)
        } else {
            Ok(self.default_profile()?.get_db_path())
        }
    }

    pub fn get_packages_path(&self, profile_name: Option<String>) -> Result<PathBuf> {
        let profile_name = profile_name.unwrap_or_else(|| get_current_profile());
        Ok(self.get_profile(&profile_name)?.get_packages_path())
    }

    pub fn get_cache_path(&self) -> Result<PathBuf> {
        Ok(self.get_profile(&get_current_profile())?.get_cache_path())
    }

    pub fn get_repositories_path(&self) -> Result<PathBuf> {
        Ok(self.default_profile()?.get_repositories_path())
    }
}

impl Default for Config {
    fn default() -> Self {
        let soar_root = format!("{}/soar", home_data_path());
        let default_profile = Profile {
            root_path: soar_root.clone(),
            cache_path: Some(format!("{}/cache", soar_root)),
            packages_path: Some(format!("{}/packages", soar_root)),
        };

        let default_profile_name = "default".to_string();

        Self {
            profile: HashMap::from([(default_profile_name.clone(), default_profile)]),
            default_profile: default_profile_name,
            bin_path: Some(format!("{}/bin", soar_root)),
            db_path: Some(format!("{}/db", soar_root)),
            repositories_path: Some(format!("{}/repos", soar_root)),
            repositories: vec![
                Repository {
                    name: "bincache".to_owned(),
                    url: format!(
                        "https://meta.pkgforge.dev/bincache/{}.sdb.zstd",
                        get_platform()
                    ),
                },
                Repository {
                    name: "pkgcache".to_owned(),
                    url: format!(
                        "https://meta.pkgforge.dev/pkgcache/{}.sdb.zstd",
                        get_platform()
                    ),
                },
            ],
            parallel: Some(true),
            parallel_limit: Some(4),
            search_limit: Some(20),
            ghcr_concurrency: Some(8),
            cross_repo_updates: Some(false),
        }
    }
}

pub fn generate_default_config() -> Result<()> {
    let home_config = home_config_path();
    let config_path = PathBuf::from(home_config).join("soar").join("config.toml");

    if config_path.exists() {
        return Err(SoarError::ConfigAlreadyExists);
    }

    fs::create_dir_all(config_path.parent().unwrap())?;

    let def_config = Config::default();
    let serialized = toml::to_string_pretty(&def_config)?;
    fs::write(&config_path, &serialized)?;

    Ok(())
}
