use std::fs;

use soar_config::{
    config::{Config, CONFIG},
    repository::Repository,
};
use soar_core::{error::ErrorContext, SoarResult};

use crate::SoarContext;

/// Fields that can be updated on an existing repository.
pub struct RepoUpdate {
    pub url: Option<String>,
    pub enabled: Option<bool>,
    pub pubkey: Option<String>,
    pub desktop_integration: Option<bool>,
    pub signature_verification: Option<bool>,
    pub sync_interval: Option<String>,
}

/// Loads a fresh config from disk, applies the mutation, validates, saves, and updates the global.
fn modify_config(mutate: impl FnOnce(&mut Config) -> SoarResult<()>) -> SoarResult<()> {
    let mut config = Config::new()?;
    mutate(&mut config)?;
    config.save()?;

    let mut global = CONFIG.write()?;
    *global = Some(config);
    Ok(())
}

impl SoarContext {
    /// Add a new repository to the configuration.
    pub fn add_repository(&self, repo: Repository) -> SoarResult<()> {
        modify_config(|config| {
            if config.repositories.iter().any(|r| r.name == repo.name) {
                return Err(soar_config::error::ConfigError::DuplicateRepositoryName(
                    repo.name.clone(),
                )
                .into());
            }

            config.repositories.push(repo);
            config.resolve()?;
            Ok(())
        })
    }

    /// Update an existing repository's configuration.
    pub fn update_repository(&self, name: &str, update: RepoUpdate) -> SoarResult<()> {
        modify_config(|config| {
            let repo = config
                .repositories
                .iter_mut()
                .find(|r| r.name == name)
                .ok_or_else(|| {
                    soar_config::error::ConfigError::InvalidRepository(name.to_string())
                })?;

            if let Some(url) = update.url {
                repo.url = url;
            }
            if let Some(enabled) = update.enabled {
                repo.enabled = Some(enabled);
            }
            if let Some(pubkey) = update.pubkey {
                repo.pubkey = Some(pubkey);
            }
            if let Some(desktop_integration) = update.desktop_integration {
                repo.desktop_integration = Some(desktop_integration);
            }
            if let Some(signature_verification) = update.signature_verification {
                repo.signature_verification = Some(signature_verification);
            }
            if let Some(sync_interval) = update.sync_interval {
                repo.sync_interval = Some(sync_interval);
            }

            config.resolve()?;
            Ok(())
        })
    }

    /// Remove a repository from the configuration and clean up its data.
    pub fn remove_repository(&self, name: &str) -> SoarResult<()> {
        modify_config(|config| {
            let idx = config
                .repositories
                .iter()
                .position(|r| r.name == name)
                .ok_or_else(|| {
                    soar_config::error::ConfigError::InvalidRepository(name.to_string())
                })?;

            let repo = config.repositories.remove(idx);

            // Clean up the repository's data directory
            if let Ok(repo_path) = repo.get_path() {
                if repo_path.exists() {
                    fs::remove_dir_all(&repo_path).with_context(|| {
                        format!("removing repository data at {}", repo_path.display())
                    })?;
                }
            }

            Ok(())
        })
    }
}
