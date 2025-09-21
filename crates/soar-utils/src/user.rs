use std::env;

use nix::unistd::{geteuid, User};

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

fn get_username_with<S: UsernameSource>(src: &S) -> String {
    src.env_var("USER")
        .or_else(|| src.env_var("LOGNAME"))
        .or_else(|| src.uid_name())
        .expect("Couldn't determine username.")
}

/// Returns the username of the current user.
///
/// This function first checks the `USER` and `LOGNAME` environment variables. If not set, it
/// falls back to fetching the username using the effective user ID.
///
/// # Panics
///
/// This function will panic if it cannot determine the username.
pub fn get_username() -> String {
    get_username_with(&SystemSource)
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
    #[should_panic(expected = "Couldn't determine username.")]
    fn test_fails_when_all_sources_missing() {
        get_username_with(&AlwaysNone);
    }

    #[test]
    fn test_get_username() {
        let username = get_username();
        assert!(!username.is_empty());
    }

    #[test]
    fn test_get_username_missing_env_vars() {
        env::remove_var("USER");
        env::remove_var("LOGNAME");

        let username = get_username();
        assert!(!username.is_empty());
    }
}
