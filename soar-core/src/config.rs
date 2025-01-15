use std::{
    collections::HashSet,
    env::{self, consts::ARCH},
    fs,
    path::PathBuf,
};

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::{
    constants::repositories_path,
    error::SoarError,
    utils::{home_config_path, home_data_path},
};

type Result<T> = std::result::Result<T, SoarError>;

/// Application's configuration
#[derive(Deserialize, Serialize)]
pub struct Config {
    /// Path to the directory where app data is stored.
    pub soar_root: String,

    /// Path to the directory where cache is stored.
    #[serde(skip_serializing)]
    pub soar_cache: Option<String>,

    /// Path to the directory where binary symlinks is stored.
    #[serde(skip_serializing)]
    pub soar_bin: Option<String>,

    /// Path to the directory where installation database is stored.
    #[serde(skip_serializing)]
    pub soar_db: Option<String>,

    /// Path to the directory where repositories database is stored.
    #[serde(skip_serializing)]
    pub soar_repositories: Option<String>,

    /// Path to the directory where packages are stored.
    #[serde(skip_serializing)]
    pub soar_packages: Option<String>,

    /// A list of remote repositories to fetch packages from.
    pub repositories: Vec<Repository>,

    /// Indicates whether downloads should be performed in parallel.
    #[serde(skip_serializing)]
    pub parallel: Option<bool>,

    /// Limit the number of parallel downloads
    #[serde(skip_serializing)]
    pub parallel_limit: Option<u32>,

    /// Limit the number of search results to display
    #[serde(skip_serializing)]
    pub search_limit: Option<usize>,
}

/// Struct representing a repository configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct Repository {
    /// Name of the repository.
    pub name: String,

    /// URL of the repository.
    pub url: String,

    /// Optional field specifying a custom metadata file for the repository. Default:
    /// `metadata.json`
    pub metadata: Option<String>,
}

impl Repository {
    pub fn get_path(&self) -> PathBuf {
        repositories_path().join(&self.name)
    }
}

impl Config {
    pub fn get() -> Result<&'static Config> {
        static CONFIG: OnceCell<Config> = OnceCell::new();
        CONFIG.get_or_try_init(Config::new)
    }

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

        config.soar_root = env::var("SOAR_ROOT").unwrap_or(config.soar_root);
        config.soar_bin = Some(env::var("SOAR_BIN").unwrap_or_else(|_| {
            config
                .soar_bin
                .unwrap_or_else(|| format!("{}/bin", config.soar_root))
        }));
        config.soar_cache = Some(env::var("SOAR_CACHE").unwrap_or_else(|_| {
            config
                .soar_cache
                .unwrap_or_else(|| format!("{}/cache", config.soar_root))
        }));
        config.soar_packages = Some(env::var("SOAR_PACKAGE").unwrap_or_else(|_| {
            config
                .soar_packages
                .unwrap_or_else(|| format!("{}/packages", config.soar_root))
        }));
        config.soar_repositories = Some(env::var("SOAR_REPOSITORIES").unwrap_or_else(|_| {
            config
                .soar_repositories
                .unwrap_or_else(|| format!("{}/repos", config.soar_root))
        }));

        config.soar_db = Some(format!("{}/db", config.soar_root));
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
}

impl Default for Config {
    fn default() -> Self {
        let soar_root =
            env::var("SOAR_ROOT").unwrap_or_else(|_| format!("{}/soar", home_data_path()));

        Self {
            soar_root: soar_root.clone(),
            soar_bin: Some(format!("{}/bin", soar_root)),
            soar_cache: Some(format!("{}/cache", soar_root)),
            soar_db: Some(format!("{}/db", soar_root)),
            soar_packages: Some(format!("{}/packages", soar_root)),
            soar_repositories: Some(format!("{}/repos", soar_root)),
            repositories: vec![Repository {
                name: "pkgforge".to_owned(),
                url: format!("https://bin.pkgforge.dev/{ARCH}"),
                metadata: Some("METADATA.AIO.json".to_owned()),
            }],
            parallel: Some(true),
            parallel_limit: Some(4),
            search_limit: Some(20),
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
