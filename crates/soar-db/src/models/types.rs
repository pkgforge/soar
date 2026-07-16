use serde::{Deserialize, Serialize};
use soar_utils::path::is_safe_component;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ProvideStrategy {
    KeepTargetOnly,
    KeepBoth,
    Alias,
}

impl std::fmt::Display for ProvideStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            ProvideStrategy::KeepTargetOnly => "=>",
            ProvideStrategy::KeepBoth => "==",
            ProvideStrategy::Alias => ":",
        };
        write!(f, "{msg}")
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct PackageProvide {
    pub name: String,
    pub target: Option<String>,
    pub strategy: Option<ProvideStrategy>,
    pub symlink_to_bin: bool,
}

impl PackageProvide {
    /// Returns the symlink names this provide creates in the bin directory,
    /// mirroring the install logic in `setup_provide_symlinks`.
    pub fn bin_symlink_names(&self) -> Vec<&str> {
        if self.symlink_to_bin {
            // @name -> bin/name
            return vec![&self.name];
        }
        match (&self.target, &self.strategy) {
            (Some(target), Some(ProvideStrategy::KeepBoth)) => vec![&self.name, target],
            (Some(target), Some(ProvideStrategy::KeepTargetOnly | ProvideStrategy::Alias)) => {
                vec![target]
            }
            _ => vec![&self.name],
        }
    }

    /// Returns `true` if the provide's name and target are safe single path
    /// components, i.e. they cannot escape the bin directory when used as
    /// symlink paths.
    pub fn is_safe(&self) -> bool {
        is_safe_component(&self.name) && self.target.as_deref().is_none_or(is_safe_component)
    }

    pub fn from_string(provide: &str) -> Self {
        let (symlink_to_bin, provide) = if let Some(stripped) = provide.strip_prefix('@') {
            (true, stripped)
        } else {
            (false, provide)
        };

        if let Some((name, target_name)) = provide.split_once("==") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::KeepBoth),
                symlink_to_bin,
            }
        } else if let Some((name, target_name)) = provide.split_once("=>") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::KeepTargetOnly),
                symlink_to_bin,
            }
        } else if let Some((name, target_name)) = provide.split_once(":") {
            Self {
                name: name.to_string(),
                target: Some(target_name.to_string()),
                strategy: Some(ProvideStrategy::Alias),
                symlink_to_bin,
            }
        } else {
            Self {
                name: provide.to_string(),
                target: None,
                strategy: None,
                symlink_to_bin,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bin_symlink_names_plain() {
        let p = PackageProvide::from_string("clipcatd");
        assert_eq!(p.bin_symlink_names(), vec!["clipcatd"]);
    }

    #[test]
    fn test_bin_symlink_names_at_prefix() {
        let p = PackageProvide::from_string("@clipcat-menu");
        assert!(p.symlink_to_bin);
        assert_eq!(p.bin_symlink_names(), vec!["clipcat-menu"]);
    }

    #[test]
    fn test_bin_symlink_names_keep_both() {
        let p = PackageProvide::from_string("clipcatd==clipcat");
        assert_eq!(p.bin_symlink_names(), vec!["clipcatd", "clipcat"]);
    }

    #[test]
    fn test_bin_symlink_names_keep_target_only() {
        let p = PackageProvide::from_string("clipcatd=>clipcat");
        assert_eq!(p.bin_symlink_names(), vec!["clipcat"]);
    }

    #[test]
    fn test_bin_symlink_names_alias() {
        let p = PackageProvide::from_string("clipcatd:clipcat");
        assert_eq!(p.bin_symlink_names(), vec!["clipcat"]);
    }

    #[test]
    fn test_provide_is_safe() {
        assert!(PackageProvide::from_string("clipcatd").is_safe());
        assert!(PackageProvide::from_string("clipcatd==clipcat").is_safe());
        assert!(PackageProvide::from_string("@clipcat-menu").is_safe());

        assert!(!PackageProvide::from_string("clipcatd=>../../../../home/user/.bashrc").is_safe());
        assert!(!PackageProvide::from_string("../evil").is_safe());
        assert!(!PackageProvide::from_string("ok==../evil").is_safe());
        assert!(!PackageProvide::from_string("@../evil").is_safe());
    }
}
