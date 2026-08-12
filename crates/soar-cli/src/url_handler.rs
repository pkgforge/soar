//! Handling of `soar://` links.
//!
//! Any web page can send the browser to a `soar://` URL, so what arrives here
//! is attacker-controlled input that reached the machine without the user
//! typing anything. It is matched against a strict allowlist, never handed to
//! a shell, and always confirmed on the terminal before soar acts on it.

use std::{env, fs, io::IsTerminal, path::PathBuf, process::Command, sync::OnceLock};

use nu_ansi_term::Color::{Blue, Yellow};
use regex::Regex;
use soar_core::{
    error::{ErrorContext, SoarError},
    SoarResult,
};
use soar_operations::SoarContext;
use soar_utils::path::xdg_data_home;
use tracing::info;

use crate::{
    install::install_packages,
    utils::{interactive_ask, Colored},
};

/// Long enough for any real package query, short enough to bound the work.
const MAX_URL_LEN: usize = 512;

const DESKTOP_FILE_NAME: &str = "soar-url-handler.desktop";
const SCHEME_MIME: &str = "x-scheme-handler/soar";

/// Set on the copy of soar running inside the terminal, so a terminal that
/// fails to give it one cannot start the search over again.
const IN_TERMINAL: &str = "SOAR_URL_IN_TERMINAL";

/// How each terminal takes a command to run. There is no agreed flag, and
/// `Terminal=true` in a desktop entry only covers the handful GLib knows, so
/// the search happens here instead.
const TERMINALS: &[(&str, &[&str])] = &[
    // The freedesktop tool for exactly this, when it is installed.
    ("xdg-terminal-exec", &[]),
    ("wezterm", &["start", "--"]),
    ("ghostty", &["-e"]),
    ("kitty", &[]),
    ("foot", &[]),
    ("alacritty", &["-e"]),
    ("wterm", &["-e"]),
    ("konsole", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("xfce4-terminal", &["-x"]),
    ("st", &["-e"]),
    ("urxvt", &["-e"]),
    ("xterm", &["-e"]),
];

/// What a `soar://` link is asking for.
#[derive(Debug, PartialEq, Eq)]
pub enum UrlRequest {
    /// Install a package, given as a normal `family/name@version:repo` query.
    Install(String),
}

fn invalid(reason: &str) -> SoarError {
    SoarError::Custom(format!("Invalid soar:// URL: {reason}"))
}

/// Parse a `soar://` URL into the action it requests.
///
/// Rejects anything outside the allowlist rather than trying to sanitize it,
/// so a name can never turn into a flag, a path, or a shell word.
pub fn parse(url: &str) -> SoarResult<UrlRequest> {
    if url.len() > MAX_URL_LEN {
        return Err(invalid("too long"));
    }

    let rest = url
        .strip_prefix("soar://")
        .or_else(|| url.strip_prefix("SOAR://"))
        .ok_or_else(|| invalid("expected a soar:// link"))?;

    // The allowed characters never need escaping, so an escape is a sign of
    // someone trying to smuggle one past the allowlist.
    if rest.contains(['%', '?', '#']) {
        return Err(invalid("escapes and query strings are not accepted"));
    }

    let (action, spec) = rest
        .split_once('/')
        .ok_or_else(|| invalid("expected soar://install/<package>"))?;
    // A browser may keep the trailing slash that ends an authority.
    let spec = spec.trim_end_matches('/');

    match action {
        "install" => {
            validate_spec(spec)?;
            Ok(UrlRequest::Install(spec.to_string()))
        }
        other => Err(invalid(&format!("unknown action `{other}`"))),
    }
}

/// Accept only the shape of a package query, spelled out character by
/// character. Notably a segment cannot start with `-`, so no part of a link
/// can arrive at soar looking like a flag.
fn validate_spec(spec: &str) -> SoarResult<()> {
    static SPEC_RE: OnceLock<Regex> = OnceLock::new();
    let re = SPEC_RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^
            (?:[A-Za-z0-9][A-Za-z0-9._+-]*/)?   # optional family
            [A-Za-z0-9][A-Za-z0-9._+-]*         # name
            (?:@[A-Za-z0-9][A-Za-z0-9._+-]*)?   # optional version
            (?::[A-Za-z0-9][A-Za-z0-9._-]*)?    # optional repo
            $
            ",
        )
        .unwrap()
    });

    if spec.is_empty() {
        return Err(invalid("no package given"));
    }
    if !re.is_match(spec) {
        return Err(invalid(&format!("`{spec}` is not a package query")));
    }
    Ok(())
}

fn desktop_entry(exe: &str) -> String {
    // Quoted only when it has to be: xdg-open's generic fallback resolves the
    // Exec path with a plain `command -v` and finds nothing behind quotes.
    let exec = if exe.contains(|c: char| c.is_whitespace() || "\"\\$`".contains(c)) {
        format!("\"{exe}\"")
    } else {
        exe.to_string()
    };

    // Terminal=false: soar opens the terminal itself, because GLib only knows
    // a fixed list of them and silently does nothing when yours is not on it.
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Soar\n\
         Comment=Install packages from soar:// links\n\
         Exec={exec} url %u\n\
         Terminal=false\n\
         NoDisplay=true\n\
         MimeType={SCHEME_MIME};\n"
    )
}

/// Find a terminal, preferring the one the user named.
fn find_terminal() -> Option<(String, Vec<String>)> {
    let known = |name: &str| {
        TERMINALS
            .iter()
            .find(|(term, _)| *term == name)
            .map(|(_, args)| args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    };

    if let Some(preferred) = env::var_os("TERMINAL") {
        let preferred = preferred.to_string_lossy().into_owned();
        let base = preferred
            .rsplit('/')
            .next()
            .unwrap_or(&preferred)
            .to_string();
        // An unrecognized terminal still gets a try: `-e` is the common form.
        let args = known(&base).unwrap_or_else(|| vec!["-e".to_string()]);
        if which(&preferred).is_some() {
            return Some((preferred, args));
        }
    }

    TERMINALS.iter().find_map(|(term, args)| {
        which(term).map(|path| (path, args.iter().map(|a| a.to_string()).collect()))
    })
}

fn which(name: &str) -> Option<String> {
    if name.contains('/') {
        return fs::metadata(name).is_ok().then(|| name.to_string());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
            .map(|candidate| candidate.to_string_lossy().into_owned())
    })
}

/// Re-run soar inside a terminal, so a link opened from a browser has
/// somewhere to show what it is about to do and to ask.
fn relaunch_in_terminal(url: &str) -> SoarResult<()> {
    let exe = env::current_exe()
        .map_err(|e| SoarError::Custom(format!("Failed to get current executable path: {e}")))?;

    let Some((terminal, args)) = find_terminal() else {
        // Nothing is watching stderr when a browser starts this, so say it
        // somewhere the user will actually see.
        let _ = Command::new("notify-send")
            .args(["Soar", "No terminal found to open the soar:// link in"])
            .status();
        return Err(SoarError::Custom(
            "No terminal found to open the soar:// link in".into(),
        ));
    };

    Command::new(&terminal)
        .args(&args)
        .arg(exe)
        .args(["url", url])
        .env(IN_TERMINAL, "1")
        .spawn()
        .with_context(|| format!("starting {terminal}"))?;
    Ok(())
}

/// Register soar as the handler for `soar://` links for the current user.
pub fn register() -> SoarResult<PathBuf> {
    let exe = env::current_exe()
        .map_err(|e| SoarError::Custom(format!("Failed to get current executable path: {e}")))?;
    let dir = xdg_data_home().join("applications");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let path = dir.join(DESKTOP_FILE_NAME);
    fs::write(&path, desktop_entry(&exe.to_string_lossy()))
        .with_context(|| format!("writing {}", path.display()))?;

    // Both are best-effort: the entry is already in place, and a desktop that
    // reads it directly will find it without them.
    let _ = Command::new("update-desktop-database").arg(&dir).status();
    let _ = Command::new("xdg-mime")
        .args(["default", DESKTOP_FILE_NAME, SCHEME_MIME])
        .status();

    Ok(path)
}

/// Act on a `soar://` link, or register soar as their handler.
pub async fn handle(ctx: &SoarContext, url: Option<String>, register_only: bool) -> SoarResult<()> {
    if register_only {
        let path = register()?;
        info!("Registered soar for soar:// links: {}", path.display());
        return Ok(());
    }

    let url = url.ok_or_else(|| {
        SoarError::Custom("Pass a soar:// link, or --register to handle them".into())
    })?;
    let UrlRequest::Install(spec) = parse(&url)?;

    // Started from a browser there is no terminal to ask in, so get one first.
    if !std::io::stdin().is_terminal() && env::var_os(IN_TERMINAL).is_none() {
        return relaunch_in_terminal(&url);
    }

    // A link can arrive from any page the user happens to visit. The browser
    // does not say which one, so the warning claims no more than it knows.
    info!("\n{}\n", Colored(Blue, "Install request from a link"));
    info!("    {}\n", Colored(Yellow, &spec));
    info!("A page you opened asked for this, rather than you typing it.");
    info!("Continue only if you trust where the link came from.");

    let answer = interactive_ask(&format!("\nInstall {spec}? [y/N]: "))?;
    if !answer.eq_ignore_ascii_case("y") && !answer.eq_ignore_ascii_case("yes") {
        info!("Aborted");
        return wait_for_close();
    }

    let result = install_packages(
        ctx,
        &[spec],
        false,
        false,
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        false,
        false,
        None,
        None,
        None,
        None,
        false,
    )
    .await;

    // The terminal a desktop entry opens closes the moment this returns, so
    // the outcome is held on screen either way.
    if let Err(ref err) = result {
        info!("{err}");
    }
    wait_for_close()?;
    result
}

fn wait_for_close() -> SoarResult<()> {
    interactive_ask("\nPress Enter to close: ")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, UrlRequest};

    #[test]
    fn accepts_the_shapes_a_package_query_can_take() {
        assert_eq!(
            parse("soar://install/ripgrep").unwrap(),
            UrlRequest::Install("ripgrep".into())
        );
        assert_eq!(
            parse("soar://install/bat/bat@0.24.0:bincache").unwrap(),
            UrlRequest::Install("bat/bat@0.24.0:bincache".into())
        );
        assert_eq!(
            parse("soar://install/ripgrep/").unwrap(),
            UrlRequest::Install("ripgrep".into())
        );
    }

    #[test]
    fn rejects_anything_that_could_become_a_flag() {
        for url in [
            "soar://install/--version",
            "soar://install/-y",
            "soar://install/ripgrep --force",
            "soar://install/%2D%2Dforce",
        ] {
            assert!(parse(url).is_err(), "{url} should be rejected");
        }
    }

    #[test]
    fn rejects_shell_and_path_characters() {
        for url in [
            "soar://install/ripgrep;rm -rf /",
            "soar://install/ripgrep$(id)",
            "soar://install/ripgrep`id`",
            "soar://install/../../etc/passwd",
            "soar://install/a/b/c",
            "soar://install/rip grep",
        ] {
            assert!(parse(url).is_err(), "{url} should be rejected");
        }
    }

    #[test]
    fn rejects_other_schemes_and_actions() {
        assert!(parse("https://install/ripgrep").is_err());
        assert!(parse("soar://run/ripgrep").is_err());
        assert!(parse("soar://install").is_err());
        assert!(parse("soar://install/").is_err());
    }

    #[test]
    fn rejects_an_overlong_url() {
        let long = format!("soar://install/{}", "a".repeat(600));
        assert!(parse(&long).is_err());
    }
}
