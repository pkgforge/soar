use std::path::PathBuf;

use documented::{Documented, DocumentedFields};
use serde::{Deserialize, Serialize};

use crate::{
    config::{path_env, resolve_mode_path},
    error::Result,
};

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
    pub(crate) fn get_bin_path(&self, system_mode: bool) -> Result<PathBuf> {
        Ok(self.get_root_path(system_mode)?.join("bin"))
    }

    pub(crate) fn get_db_path(&self, system_mode: bool) -> Result<PathBuf> {
        Ok(self.get_root_path(system_mode)?.join("db"))
    }

    /// Directory holding this profile's packages.
    pub fn get_packages_path(&self, system_mode: bool) -> Result<PathBuf> {
        if let Some(ref packages_path) = self.packages_path {
            Ok(resolve_mode_path(packages_path, system_mode)?)
        } else {
            Ok(self.get_root_path(system_mode)?.join("packages"))
        }
    }

    /// Directory holding this profile's download cache.
    pub fn get_cache_path(&self, system_mode: bool) -> Result<PathBuf> {
        Ok(self.get_root_path(system_mode)?.join("cache"))
    }

    pub(crate) fn get_repositories_path(&self, system_mode: bool) -> Result<PathBuf> {
        Ok(self.get_root_path(system_mode)?.join("repos"))
    }

    pub(crate) fn get_portable_dirs(&self, system_mode: bool) -> Result<PathBuf> {
        Ok(self.get_root_path(system_mode)?.join("portable-dirs"))
    }

    /// Root of this profile's tree.
    ///
    /// The mode is a parameter rather than the global flag so a config built
    /// for one mode never resolves through the other mode's variables.
    pub fn get_root_path(&self, system_mode: bool) -> Result<PathBuf> {
        if let Some(env_path) = path_env("ROOT", system_mode) {
            return Ok(resolve_mode_path(&env_path, system_mode)?);
        }
        Ok(resolve_mode_path(&self.root_path, system_mode)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::with_env;

    #[test]
    fn test_profile_creation() {
        let profile = Profile {
            root_path: "/test/root".to_string(),
            packages_path: Some("/test/packages".to_string()),
        };

        assert_eq!(profile.root_path, "/test/root");
        assert_eq!(profile.packages_path, Some("/test/packages".to_string()));
    }

    #[test]
    fn test_profile_get_packages_path_explicit() {
        let profile = Profile {
            root_path: "/test/root".to_string(),
            packages_path: Some("/custom/packages".to_string()),
        };

        let path = profile.get_packages_path(false).unwrap();
        assert!(path.ends_with("packages"));
    }

    #[test]
    fn test_profile_get_packages_path_default() {
        let profile = Profile {
            root_path: "/test/root".to_string(),
            packages_path: None,
        };

        let path = profile.get_packages_path(false).unwrap();
        assert!(path.ends_with("packages"));
    }

    #[test]
    fn test_profile_get_root_path_env_override() {
        with_env(vec![("SOAR_ROOT", "/custom/root")], || {
            let profile = Profile {
                root_path: "/test/root".to_string(),
                packages_path: None,
            };

            let path = profile.get_root_path(false).unwrap();
            assert_eq!(path, PathBuf::from("/custom/root"));
        });
    }
}
