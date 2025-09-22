use std::{env, path::PathBuf};

use crate::{
    error::{PathError, PathResult},
    user::get_username,
};

pub trait PathResolver {
    /// Resolves a path string that may contain environment variables
    ///
    /// This method expands environment variables in the format `$VAR` or `${VAR}`, resolves tilde
    /// (`~`) to the user's home directory when it appears at the start of the path, and converts
    /// relative paths to absolute paths based on the current working directory.
    ///
    /// # Arguments
    ///
    /// * `path` - The path string that may contain environment variables and tilde expansion
    ///
    /// # Returns
    ///
    /// Returns an absolute [`PathBuf`] with all variables expanded, or a [`PathError`] if the path
    /// is invalid or variables cannot be resolved.
    ///
    /// # Errors
    ///
    /// * [`PathError::Empty`] if the path is empty
    /// * [`PathError::CurrentDir`] if the current directory cannot be determined
    /// * [`PathError::MissingEnvVar`] if the environment variables are undefined
    ///
    /// # Example
    ///
    /// ```
    /// use soar_utils::error::PathResult;
    /// use soar_utils::path::{PathResolver, SystemPathResolver};
    ///
    /// fn main() -> PathResult<()> {
    ///     let resolver = SystemPathResolver;
    ///     let resolved = resolver.resolve_path("$HOME/path/to/file")?;
    ///     println!("Resolved path is {:#?}", resolved);
    ///     Ok(())
    /// }
    /// ```
    fn resolve_path(&self, path: &str) -> PathResult<PathBuf>;

    /// Returns the user's home directory
    ///
    /// This method first checks the `HOME` environment variables. If not set, it falls back to
    /// constructing the path `/home/{username}` where username is obtained from the system.
    ///
    /// # Example
    ///
    /// ```
    /// use soar_utils::path::{PathResolver, SystemPathResolver};
    ///
    /// let resolver = SystemPathResolver;
    /// let home = resolver.home_dir();
    /// println!("Home dir is {:#?}", home);
    /// ```
    fn home_dir(&self) -> PathBuf;

    /// Returns the user's config directory following XDG Base Directory Specification
    ///
    /// This method checks the `XDG_CONFIG_HOME` environment variable. If not set, it defaults to
    /// `$HOME/.config`
    ///
    /// # Example
    ///
    /// ```
    /// use soar_utils::path::{PathResolver, SystemPathResolver};
    ///
    /// let resolver = SystemPathResolver;
    /// let config = resolver.xdg_config_home();
    /// println!("Config dir is {:#?}", config);
    /// ```
    fn xdg_config_home(&self) -> PathBuf;

    /// Returns the user's data directory following XDG Base Directory Specification
    ///
    /// This method checks the `XDG_DATA_HOME` environment variable. If not set, it defaults to
    /// `$HOME/.local/share`
    ///
    /// # Example
    ///
    /// ```
    /// use soar_utils::path::{PathResolver, SystemPathResolver};
    ///
    /// let resolver = SystemPathResolver;
    /// let data = resolver.xdg_data_home();
    /// println!("Data dir is {:#?}", data);
    /// ```
    fn xdg_data_home(&self) -> PathBuf;

    /// Returns the user's cache directory following XDG Base Directory Specification
    ///
    /// This method checks the `XDG_CACHE_HOME` environment variable. If not set, it defaults to
    /// `$HOME/.cache`
    ///
    /// # Example
    ///
    /// ```
    /// use soar_utils::path::{PathResolver, SystemPathResolver};
    ///
    /// let resolver = SystemPathResolver;
    /// let cache = resolver.xdg_cache_home();
    /// println!("Cache dir is {:#?}", cache);
    /// ```
    fn xdg_cache_home(&self) -> PathBuf;
}

/// The default [`PathResolver`] implementation using environment variables and filesystem calls.
pub struct SystemPathResolver;

impl PathResolver for SystemPathResolver {
    fn resolve_path(&self, path: &str) -> PathResult<PathBuf> {
        let path = path.trim();

        if path.is_empty() {
            return Err(PathError::Empty);
        }

        let resolved = self.expand_variables(path)?;
        let path_buf = PathBuf::from(resolved);

        if path_buf.is_absolute() {
            Ok(path_buf)
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(path_buf))
                .map_err(|err| PathError::CurrentDir { source: err })
        }
    }

    fn home_dir(&self) -> PathBuf {
        env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(format!("/home/{}", get_username())))
    }

    fn xdg_config_home(&self) -> PathBuf {
        env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.home_dir().join(".config"))
    }

    fn xdg_data_home(&self) -> PathBuf {
        env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.home_dir().join(".local/share"))
    }

    fn xdg_cache_home(&self) -> PathBuf {
        env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.home_dir().join(".cache"))
    }
}

impl SystemPathResolver {
    fn expand_variables(&self, path: &str) -> PathResult<String> {
        let mut result = String::with_capacity(path.len());
        let mut chars = path.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '$' => {
                    if chars.peek() == Some(&'{') {
                        chars.next();
                        let var_name = self.consume_until(&mut chars, '}')?;
                        self.expand_env_var(&var_name, &mut result, path)?;
                    } else {
                        let var_name = self.consume_var_name(&mut chars);
                        if var_name.is_empty() {
                            result.push('$');
                        } else {
                            self.expand_env_var(&var_name, &mut result, path)?;
                        }
                    }
                }
                '~' if result.is_empty() => result.push_str(&self.home_dir().to_string_lossy()),
                _ => result.push(c),
            }
        }

        Ok(result)
    }

    fn consume_until(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
        delimiter: char,
    ) -> PathResult<String> {
        let mut var_name = String::new();

        for c in chars.by_ref() {
            if c == delimiter {
                return Ok(var_name);
            }
            var_name.push(c);
        }

        Err(PathError::UnclosedVariable {
            input: format!("${{{var_name}"),
        })
    }

    fn consume_var_name(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut var_name = String::new();

        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                var_name.push(chars.next().unwrap());
            } else {
                break;
            }
        }

        var_name
    }

    fn expand_env_var(
        &self,
        var_name: &str,
        result: &mut String,
        original: &str,
    ) -> PathResult<()> {
        match var_name {
            "HOME" => result.push_str(&self.home_dir().to_string_lossy()),
            "XDG_CONFIG_HOME" => result.push_str(&self.xdg_config_home().to_string_lossy()),
            "XDG_DATA_HOME" => result.push_str(&self.xdg_data_home().to_string_lossy()),
            "XDG_CACHE_HOME" => result.push_str(&self.xdg_cache_home().to_string_lossy()),
            _ => {
                let value = env::var(var_name).map_err(|_| PathError::MissingEnvVar {
                    input: original.into(),
                    var: var_name.into(),
                })?;
                result.push_str(&value);
            }
        }
        Ok(())
    }
}

/// Resolves a path string using the system path resolver.
///
/// This is a convenience function that creates a [`SystemPathResolver`] and calls
/// [`PathResolver::resolve_path`] on it.
///
/// See [`PathResolver::resolve_path`] for detailed documentation.
pub fn resolve_path(path: &str) -> PathResult<PathBuf> {
    SystemPathResolver.resolve_path(path)
}

/// Returns the user's home directory using the system path resolver.
///
/// This is a convenience function that creates a [`SystemPathResolver`] and calls
/// [`PathResolver::home_dir`] on it.
///
/// See [`PathResolver::home_dir`] for detailed documentation.
pub fn home_dir() -> PathBuf {
    SystemPathResolver.home_dir()
}

/// Returns the user's config directory using the system path resolver.
///
/// This is a convenience function that creates a [`SystemPathResolver`] and calls
/// [`PathResolver::xdg_config_home`] on it.
///
/// See [`PathResolver::xdg_config_home`] for detailed documentation.
pub fn xdg_config_home() -> PathBuf {
    SystemPathResolver.xdg_config_home()
}

/// Returns the user's data directory using the system path resolver.
///
/// This is a convenience function that creates a [`SystemPathResolver`] and calls
/// [`PathResolver::xdg_data_home`] on it.
///
/// See [`PathResolver::xdg_data_home`] for detailed documentation.
pub fn xdg_data_home() -> PathBuf {
    SystemPathResolver.xdg_data_home()
}

/// Returns the user's cache directory using the system path resolver.
///
/// This is a convenience function that creates a [`SystemPathResolver`] and calls
/// [`PathResolver::xdg_cache_home`] on it.
///
/// See [`PathResolver::xdg_cache_home`] for detailed documentation.
pub fn xdg_cache_home() -> PathBuf {
    SystemPathResolver.xdg_cache_home()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, sync::Mutex};

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn with_env_test<F>(f: F)
    where
        F: FnOnce(),
    {
        let _guard = ENV_MUTEX.lock().unwrap();
        f()
    }

    fn setup_test_env(vars: &[(&str, &str)]) {
        for (var, value) in vars {
            env::set_var(*var, *value);
        }
    }

    fn cleanup_test_env(vars: &[&str]) {
        for key in vars {
            env::remove_var(key);
        }
    }

    #[test]
    fn test_expand_variables_simple() {
        with_env_test(|| {
            setup_test_env(&[("TEST_VAR", "test_value")]);
            let resolver = SystemPathResolver;
            let result = resolver.expand_variables("$TEST_VAR/path").unwrap();
            assert_eq!(result, "test_value/path");
        });
    }

    #[test]
    fn test_expand_variables_braces() {
        with_env_test(|| {
            setup_test_env(&[("TEST_VAR", "test_value")]);
            let resolver = SystemPathResolver;
            let result = resolver.expand_variables("${TEST_VAR}/path").unwrap();
            assert_eq!(result, "test_value/path");
        });
    }

    #[test]
    fn test_expand_variables_missing_braces() {
        with_env_test(|| {
            setup_test_env(&[("TEST_VAR", "test_value")]);
            let resolver = SystemPathResolver;
            let result = resolver.expand_variables("${TEST_VAR");
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_expand_variables_missing_var() {
        with_env_test(|| {
            let resolver = SystemPathResolver;
            let result = resolver.expand_variables("$THIS_VAR_DOESNT_EXIST");
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_consume_var_name() {
        with_env_test(|| {
            let resolver = SystemPathResolver;
            let mut chars = "VAR_NAME_123/extra".chars().peekable();
            let var_name = resolver.consume_var_name(&mut chars);
            assert_eq!(var_name, "VAR_NAME_123");
        });
    }

    #[test]
    fn test_xdg_directories() {
        with_env_test(|| {
            setup_test_env(&[("HOME", "/tmp/home")]);

            let resolver = SystemPathResolver;
            let home = resolver.home_dir();
            assert_eq!(home, PathBuf::from("/tmp/home"));

            // Test without XDG variables set
            cleanup_test_env(&["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"]);

            let config = resolver.xdg_config_home();
            let data = resolver.xdg_data_home();
            let cache = resolver.xdg_cache_home();

            assert_eq!(config, home.join(".config"));
            assert_eq!(data, home.join(".local/share"));
            assert_eq!(cache, home.join(".cache"));
            assert!(config.is_absolute());
            assert!(data.is_absolute());
            assert!(cache.is_absolute());

            // Test with XDG variables set
            setup_test_env(&[
                ("XDG_CONFIG_HOME", "/tmp/config"),
                ("XDG_DATA_HOME", "/tmp/data"),
                ("XDG_CACHE_HOME", "/tmp/cache"),
            ]);

            assert_eq!(resolver.xdg_config_home(), PathBuf::from("/tmp/config"));
            assert_eq!(resolver.xdg_data_home(), PathBuf::from("/tmp/data"));
            assert_eq!(resolver.xdg_cache_home(), PathBuf::from("/tmp/cache"));

            cleanup_test_env(&["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "HOME"]);
        });
    }

    #[test]
    fn test_resolve_path() {
        with_env_test(|| {
            setup_test_env(&[("HOME", "/tmp/home")]);
            let resolver = SystemPathResolver;

            assert!(resolver.resolve_path("").is_err());

            // Absolute path
            assert_eq!(
                resolver.resolve_path("/absolute/path").unwrap(),
                PathBuf::from("/absolute/path")
            );

            // Relative path
            let expected_relative = env::current_dir().unwrap().join("relative/path");
            assert_eq!(
                resolver.resolve_path("relative/path").unwrap(),
                expected_relative
            );

            // Tilde path
            let home = resolver.home_dir();
            assert_eq!(resolver.resolve_path("~/path").unwrap(), home.join("path"));
            assert_eq!(resolver.resolve_path("~").unwrap(), home);

            // Tilde not at start
            let expected_tilde_middle = env::current_dir().unwrap().join("not/at/~/start");
            assert_eq!(
                resolver.resolve_path("not/at/~/start").unwrap(),
                expected_tilde_middle
            );

            // Unclosed variable
            let result = resolver.resolve_path("${VAR");
            assert!(result.is_err());

            // Missing variable
            let result = resolver.resolve_path("${VAR}");
            assert!(result.is_err());

            cleanup_test_env(&["HOME"]);
        });
    }

    #[test]
    fn test_home_dir() {
        with_env_test(|| {
            // Test with HOME set
            setup_test_env(&[("HOME", "/tmp/home")]);
            let resolver = SystemPathResolver;
            assert_eq!(resolver.home_dir(), PathBuf::from("/tmp/home"));

            // Test with HOME unset
            cleanup_test_env(&["HOME"]);
            let expected = PathBuf::from(format!("/home/{}", get_username()));
            assert_eq!(resolver.home_dir(), expected);
        });
    }

    #[test]
    fn test_expand_variables_edge_cases() {
        with_env_test(|| {
            setup_test_env(&[("HOME", "/tmp/home")]);
            let resolver = SystemPathResolver;

            // Dollar at the end
            assert_eq!(resolver.expand_variables("path/$").unwrap(), "path/$");

            // Dollar with invalid char
            assert_eq!(
                resolver.expand_variables("path/$!invalid").unwrap(),
                "path/$!invalid"
            );

            // Multiple variables
            setup_test_env(&[("VAR1", "val1"), ("VAR2", "val2")]);
            assert_eq!(
                resolver.expand_variables("$VAR1/${VAR2}").unwrap(),
                "val1/val2"
            );
            cleanup_test_env(&["VAR1", "VAR2"]);

            // Tilde expansion
            let home_str = resolver.home_dir().to_string_lossy().to_string();
            assert_eq!(
                resolver.expand_variables("~/path").unwrap(),
                format!("{}/path", home_str)
            );
            assert_eq!(resolver.expand_variables("~").unwrap(), home_str);
            assert_eq!(resolver.expand_variables("a/~/b").unwrap(), "a/~/b");
            cleanup_test_env(&["HOME"]);
        });
    }

    #[test]
    fn test_public_functions() {
        with_env_test(|| {
            setup_test_env(&[("HOME", "/tmp/home")]);

            assert_eq!(resolve_path("~").unwrap(), PathBuf::from("/tmp/home"));
            assert_eq!(home_dir(), PathBuf::from("/tmp/home"));
            assert_eq!(xdg_config_home(), PathBuf::from("/tmp/home/.config"));
            assert_eq!(xdg_data_home(), PathBuf::from("/tmp/home/.local/share"));
            assert_eq!(xdg_cache_home(), PathBuf::from("/tmp/home/.cache"));

            cleanup_test_env(&["HOME"]);
        });
    }

    #[test]
    fn test_resolve_path_invalid_cwd() {
        let resolver = SystemPathResolver;
        let temp_dir = tempfile::tempdir().unwrap();
        let invalid_path = temp_dir.path().join("invalid");
        std::fs::create_dir(&invalid_path).unwrap();

        let original_cwd = env::current_dir().unwrap();
        env::set_current_dir(&invalid_path).unwrap();
        std::fs::remove_dir(&invalid_path).unwrap();

        let result = resolver.resolve_path("relative/path");
        assert!(result.is_err());

        // Restore cwd
        env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_expand_env_var_special_vars() {
        with_env_test(|| {
            setup_test_env(&[("HOME", "/tmp/home")]);
            let resolver = SystemPathResolver;

            let mut result = String::new();
            resolver
                .expand_env_var("HOME", &mut result, "$HOME")
                .unwrap();
            assert_eq!(result, "/tmp/home");

            result.clear();
            resolver
                .expand_env_var("XDG_CONFIG_HOME", &mut result, "$XDG_CONFIG_HOME")
                .unwrap();
            assert_eq!(result, "/tmp/home/.config");

            result.clear();
            resolver
                .expand_env_var("XDG_DATA_HOME", &mut result, "$XDG_DATA_HOME")
                .unwrap();
            assert_eq!(result, "/tmp/home/.local/share");

            result.clear();
            resolver
                .expand_env_var("XDG_CACHE_HOME", &mut result, "$XDG_CACHE_HOME")
                .unwrap();
            assert_eq!(result, "/tmp/home/.cache");

            cleanup_test_env(&["HOME"]);
        });
    }
}
