use std::path::PathBuf;

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};
use soar_utils::path::resolve_path;

use crate::error::Result;

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
    pub(crate) fn get_bin_path(&self) -> Result<PathBuf> {
        Ok(self.get_root_path()?.join("bin"))
    }

    pub(crate) fn get_db_path(&self) -> Result<PathBuf> {
        Ok(self.get_root_path()?.join("db"))
    }

    pub fn get_packages_path(&self) -> Result<PathBuf> {
        if let Some(ref packages_path) = self.packages_path {
            Ok(resolve_path(packages_path)?)
        } else {
            Ok(self.get_root_path()?.join("packages"))
        }
    }

    pub fn get_cache_path(&self) -> Result<PathBuf> {
        Ok(self.get_root_path()?.join("cache"))
    }

    pub(crate) fn get_repositories_path(&self) -> Result<PathBuf> {
        Ok(self.get_root_path()?.join("repos"))
    }

    pub(crate) fn get_portable_dirs(&self) -> Result<PathBuf> {
        Ok(self.get_root_path()?.join("portable-dirs"))
    }

    pub fn get_root_path(&self) -> Result<PathBuf> {
        if let Ok(env_path) = std::env::var("SOAR_ROOT") {
            return Ok(resolve_path(&env_path)?);
        }
        Ok(resolve_path(&self.root_path)?)
    }
}
