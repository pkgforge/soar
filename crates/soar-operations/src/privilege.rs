//! Finding a privilege escalation tool and building commands that run through it.

use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use soar_config::config::{PATH_ENV_SUFFIXES, path_env_var};

/// Directories a privilege escalation tool is taken from.
///
/// They are writable only by root. A tool found through `PATH` could be any
/// program the user's environment points at, which would then show the
/// password prompt.
pub const ESCALATION_DIRS: &[&str] = &[
    "/run/wrappers/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Which program grants root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationKind {
    /// The `doas` program.
    Doas,
    /// The `sudo` program.
    Sudo,
}

/// A privilege escalation program found in a root-owned directory.
#[derive(Debug, Clone)]
pub struct EscalationTool {
    /// Absolute path of the tool.
    pub path: PathBuf,
    /// Which program the path holds.
    pub kind: EscalationKind,
}

/// Finds `doas`, or else `sudo`, in [`ESCALATION_DIRS`].
///
/// Probing by running the escalation tool would prompt for a password just to
/// learn whether it is installed, which is what the caller is trying to avoid.
pub fn find_escalation_tool() -> Option<EscalationTool> {
    [
        ("doas", EscalationKind::Doas),
        ("sudo", EscalationKind::Sudo),
    ]
    .iter()
    .find_map(|(name, kind)| {
        ESCALATION_DIRS
            .iter()
            .map(|dir| Path::new(dir).join(name))
            .find(|path| {
                fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
            .map(|path| {
                EscalationTool {
                    path,
                    kind: *kind,
                }
            })
    })
}

/// Builds `tool tool_args... [env VAR=value...] exe args...`.
///
/// sudo and doas reset the environment, so the system path overrides are
/// handed to the privileged process explicitly. `tool_args` are options
/// for the tool itself, such as `-n`.
pub fn escalated_command(
    tool: &EscalationTool,
    tool_args: &[&str],
    exe: &Path,
    args: &[OsString],
) -> Command {
    let forwarded: Vec<OsString> = PATH_ENV_SUFFIXES
        .iter()
        .filter_map(|suffix| {
            let var = path_env_var(suffix, true);
            env::var_os(&var).map(|value| {
                let mut assignment = OsString::from(format!("{var}="));
                assignment.push(value);
                assignment
            })
        })
        .collect();

    let mut command = Command::new(&tool.path);
    command.args(tool_args);
    if !forwarded.is_empty() {
        command.arg("env").args(&forwarded);
    }
    command.arg(exe).args(args);
    command
}

/// Whether `tool` would run a command without asking for a password,
/// because the user's credentials are cached or no password is required.
///
/// Runs `tool -n true` with no terminal input, so it never prompts.
pub fn runs_without_password(tool: &EscalationTool) -> bool {
    Command::new(&tool.path)
        .arg("-n")
        .arg("true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_escalation_tool_comes_from_a_root_owned_directory() {
        if let Some(tool) = find_escalation_tool() {
            assert!(
                ESCALATION_DIRS
                    .iter()
                    .any(|dir| tool.path.parent() == Some(Path::new(dir)))
            );
        }
    }
}
