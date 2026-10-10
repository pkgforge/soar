use std::env;

use nix::unistd::{geteuid, User};

/// Retrieves the platform string in the format `ARCH-OS`.
///
/// This function combines the architecture (e.g., `x86_64`) and the operating
/// system (e.g., `linux`) into a single string to identify the platform.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

trait UsernameSource {
    fn env_var(&self, key: &str) -> Option<String>;
    fn uid_name(&self) -> Option<String>;
}

struct SystemSource;

impl UsernameSource for SystemSource {
    fn env_var(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }

    fn uid_name(&self) -> Option<String> {
        User::from_uid(geteuid())
            .ok()
            .and_then(|u| u.map(|u| u.name))
    }
}

fn get_username_with<S: UsernameSource>(src: &S) -> Option<String> {
    // The uid answers for the process; the environment answers for whoever
    // set it. Trust the former first: `USER` is trivially spoofed and feeds
    // the home-directory fallback in `path`.
    src.uid_name()
        .or_else(|| src.env_var("USER"))
        .or_else(|| src.env_var("LOGNAME"))
}

/// Returns the username of the current user, if it can be determined.
///
/// This function prefers the name the effective uid resolves to over the
/// `USER` and `LOGNAME` environment variables, which anyone can set. Returns
/// `None` when every source comes up empty instead of panicking.
pub fn get_username() -> Option<String> {
    get_username_with(&SystemSource)
}

/// Whether this process runs with an effective uid of root.
pub fn is_root() -> bool {
    geteuid().is_root()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysNone;
    impl UsernameSource for AlwaysNone {
        fn env_var(&self, _: &str) -> Option<String> {
            None
        }

        fn uid_name(&self) -> Option<String> {
            None
        }
    }

    #[test]
    fn test_platform() {
        #[cfg(target_arch = "x86_64")]
        #[cfg(target_os = "linux")]
        assert_eq!(platform(), "x86_64-linux");

        #[cfg(target_arch = "aarch64")]
        #[cfg(target_os = "linux")]
        assert_eq!(platform(), "aarch64-linux");
    }

    #[test]
    fn test_returns_none_when_all_sources_missing() {
        assert_eq!(get_username_with(&AlwaysNone), None);
    }

    #[test]
    fn test_get_username() {
        let username = get_username();
        assert!(username.is_some_and(|u| !u.is_empty()));
    }

    #[test]
    fn test_uid_is_preferred_over_spoofed_env() {
        struct Spoofed;
        impl UsernameSource for Spoofed {
            fn env_var(&self, _: &str) -> Option<String> {
                Some("spoofed".to_string())
            }

            fn uid_name(&self) -> Option<String> {
                Some("real".to_string())
            }
        }
        assert_eq!(get_username_with(&Spoofed), Some("real".to_string()));
    }

    #[test]
    fn test_get_username_missing_env_vars() {
        env::remove_var("USER");
        env::remove_var("LOGNAME");

        let username = get_username();
        assert!(username.is_some_and(|u| !u.is_empty()));
    }
}
