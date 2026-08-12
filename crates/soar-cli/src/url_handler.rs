//! Handling of `soar://` links.
//!
//! Any web page can send the browser here, so a link is untrusted input that
//! reached the machine without the user typing anything: it is matched against
//! an allowlist and confirmed on a terminal before soar acts on it.

use std::{
    env, fs,
    io::IsTerminal,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

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
    utils::{interactive_ask, json_enabled, Colored},
};

const MAX_URL_LEN: usize = 512;

const DESKTOP_FILE_NAME: &str = "soar-url-handler.desktop";
const SCHEME_MIME: &str = "x-scheme-handler/soar";

/// The entry a package would ship, which registering rewrites for one user.
const DESKTOP_ENTRY: &str = include_str!("../assets/soar-url-handler.desktop");

/// The line in it that names the soar to run. A test keeps the two in step.
const PACKAGED_EXEC: &str = "Exec=soar url %u";

/// Guards against searching for a terminal again inside the one just opened.
const IN_TERMINAL: &str = "SOAR_URL_IN_TERMINAL";

/// How each terminal takes a command to run, since they never agreed on a flag.
const TERMINALS: &[(&str, &[&str])] = &[
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

#[derive(Debug, PartialEq, Eq)]
pub enum UrlRequest {
    /// Install a package, given as a normal `family/name@version:repo` query.
    Install(String),
}

fn invalid(reason: &str) -> SoarError {
    SoarError::Custom(format!("Invalid soar:// URL: {reason}"))
}

/// Parse a `soar://` URL, rejecting anything outside the allowlist rather
/// than trying to sanitize it.
pub fn parse(url: &str) -> SoarResult<UrlRequest> {
    if url.len() > MAX_URL_LEN {
        return Err(invalid("too long"));
    }

    // Schemes are case-insensitive.
    let (_, rest) = url
        .split_once("://")
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("soar"))
        .ok_or_else(|| invalid("expected a soar:// link"))?;

    // Nothing in the allowlist needs escaping, so an escape is only ever an
    // attempt to smuggle something past it.
    if rest.contains(['%', '?', '#']) {
        return Err(invalid("escapes and query strings are not accepted"));
    }

    let (action, spec) = rest
        .split_once('/')
        .ok_or_else(|| invalid("expected soar://install/<package>"))?;
    let spec = spec.trim_end_matches('/');

    match action {
        "install" => {
            validate_spec(spec)?;
            Ok(UrlRequest::Install(spec.to_string()))
        }
        other => Err(invalid(&format!("unknown action `{other}`"))),
    }
}

/// A segment cannot start with `-`, so no part of a link can reach soar
/// looking like a flag.
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

/// Encode a path for the `Exec` key.
///
/// Two layers apply: `%` starts a field code, so a literal one doubles, and
/// inside quotes a backslash guards `"`, a backtick, `$` and itself, which the
/// desktop-entry string layer then escapes a second time.
fn escape_exec(exe: &str) -> String {
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";

    let exe = exe.replace('%', "%%");
    // Quoted only when it has to be: xdg-open's generic fallback resolves the
    // Exec path with a plain `command -v` and finds nothing behind quotes.
    if !exe.contains(|c| RESERVED.contains(c)) {
        return exe;
    }

    let mut out = String::with_capacity(exe.len() + 2);
    out.push('"');
    for c in exe.chars() {
        match c {
            '\\' => out.push_str(r"\\\\"),
            '"' | '`' | '$' => {
                out.push_str(r"\\");
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The shipped entry, pointed at the soar doing the registering rather than at
/// whichever one `PATH` happens to hold.
fn desktop_entry(exe: &str) -> String {
    let exec = format!("Exec={} url %u", escape_exec(exe));
    DESKTOP_ENTRY.replace(PACKAGED_EXEC, &exec)
}

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
        // An unknown terminal still gets a try: `-e` is the common form.
        let args = known(&base).unwrap_or_else(|| vec!["-e".to_string()]);
        if which(&preferred).is_some() {
            return Some((preferred, args));
        }
    }

    TERMINALS.iter().find_map(|(term, args)| {
        which(term).map(|path| (path, args.iter().map(|a| a.to_string()).collect()))
    })
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn which(name: &str) -> Option<String> {
    if name.contains('/') {
        return is_executable(Path::new(name)).then(|| name.to_string());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| is_executable(candidate))
            .map(|candidate| candidate.to_string_lossy().into_owned())
    })
}

/// Re-run soar inside a terminal, which a link from a browser has none of.
fn relaunch_in_terminal(url: &str) -> SoarResult<()> {
    let exe = env::current_exe()
        .map_err(|e| SoarError::Custom(format!("Failed to get current executable path: {e}")))?;

    let Some((terminal, args)) = find_terminal() else {
        // Nothing is watching stderr when a browser starts this.
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

    // Best-effort: the entry is already in place without them.
    let _ = Command::new("update-desktop-database").arg(&dir).status();
    let _ = Command::new("xdg-mime")
        .args(["default", DESKTOP_FILE_NAME, SCHEME_MIME])
        .status();

    Ok(path)
}

/// Act on a `soar://` link, or register soar as their handler.
pub async fn handle(ctx: &SoarContext, url: Option<String>, register_only: bool) -> SoarResult<()> {
    // The prompt writes straight to stdout, so there is no coherent document to
    // be had here. Refuse rather than hand back a stream with a question in it.
    if json_enabled() {
        return Err(SoarError::Custom("soar url has no JSON output".into()));
    }

    if register_only {
        let path = register()?;
        info!("Registered soar for soar:// links: {}", path.display());
        return Ok(());
    }

    let url = url.ok_or_else(|| {
        SoarError::Custom("Pass a soar:// link, or --register to handle them".into())
    })?;
    let UrlRequest::Install(spec) = parse(&url)?;

    // Started from a browser there is nowhere to ask, so get a terminal first.
    if !std::io::stdin().is_terminal() && env::var_os(IN_TERMINAL).is_none() {
        return relaunch_in_terminal(&url);
    }

    // Printed rather than logged: this is the context for the prompt below, and
    // `--quiet` or `--json` would filter it out while leaving the prompt behind.
    // The browser never says which page sent the link, so it claims no more.
    println!("\n{}\n", Colored(Blue, "Install request from a link"));
    println!("    {}\n", Colored(Yellow, &spec));
    println!("A page you opened asked for this, rather than you typing it.");
    println!("Continue only if you trust where the link came from.");

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

    // The terminal closes the moment this returns, so hold the outcome. Failing
    // to wait must not stand in for what actually went wrong.
    match result {
        Err(err) => {
            info!("{err}");
            let _ = wait_for_close();
            Err(err)
        }
        Ok(()) => wait_for_close(),
    }
}

fn wait_for_close() -> SoarResult<()> {
    interactive_ask("\nPress Enter to close: ")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        desktop_entry, parse, validate_spec, UrlRequest, DESKTOP_ENTRY, PACKAGED_EXEC, SCHEME_MIME,
    };

    fn exec_line(exe: &str) -> String {
        desktop_entry(exe)
            .lines()
            .find_map(|line| line.strip_prefix("Exec=").map(str::to_string))
            .expect("desktop entry has an Exec line")
    }

    #[test]
    fn the_shipped_entry_still_says_what_registering_rewrites() {
        // Drift here would leave the registered entry pointing at whatever soar
        // is on PATH, or at nothing at all.
        assert!(DESKTOP_ENTRY.contains(PACKAGED_EXEC));
        assert!(DESKTOP_ENTRY.contains(&format!("MimeType={SCHEME_MIME};")));
        assert!(DESKTOP_ENTRY.contains("Terminal=false"));
    }

    #[test]
    fn leaves_an_ordinary_path_unquoted() {
        // xdg-open's generic fallback resolves this with a plain `command -v`.
        assert_eq!(exec_line("/usr/bin/soar"), "/usr/bin/soar url %u");
    }

    #[test]
    fn encodes_paths_the_exec_key_would_otherwise_misread() {
        // `%` starts a field code, so a literal one doubles. It is not itself
        // reserved, so that alone is enough and the path stays unquoted.
        assert_eq!(exec_line("/opt/100%/soar"), "/opt/100%%/soar url %u");
        assert_eq!(
            exec_line("/opt/my apps/soar"),
            "\"/opt/my apps/soar\" url %u"
        );
        // Inside quotes these take a backslash, which the string layer escapes
        // a second time.
        assert_eq!(exec_line("/opt/a\"b/soar"), "\"/opt/a\\\\\"b/soar\" url %u");
        assert_eq!(exec_line("/opt/a$b/soar"), "\"/opt/a\\\\$b/soar\" url %u");
        assert_eq!(exec_line("/opt/a`b/soar"), "\"/opt/a\\\\`b/soar\" url %u");
        assert_eq!(
            exec_line("/opt/a\\b/soar"),
            "\"/opt/a\\\\\\\\b/soar\" url %u"
        );
        // Reserved without needing an escape of its own.
        assert_eq!(exec_line("/opt/a&b/soar"), "\"/opt/a&b/soar\" url %u");
    }

    #[test]
    fn rejects_a_version_that_is_not_there() {
        assert!(validate_spec("ripgrep@").is_err());
        assert!(validate_spec("ripgrep@1.2.3").is_ok());
    }

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
    fn accepts_the_scheme_in_any_case() {
        for url in [
            "SOAR://install/ripgrep",
            "Soar://install/ripgrep",
            "sOaR://install/ripgrep",
        ] {
            assert_eq!(
                parse(url).unwrap(),
                UrlRequest::Install("ripgrep".into()),
                "{url} should be accepted"
            );
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
