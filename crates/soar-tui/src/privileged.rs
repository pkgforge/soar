//! Running the administrator pass: the password secret, and what a probe
//! learned about getting root here.

use std::{
    io::Write as _,
    process::{Command, Stdio},
};

use soar_operations::privilege::{
    ESCALATION_DIRS, EscalationKind, EscalationTool, find_escalation_tool, runs_without_password,
};

/// A password on its way to sudo. Zeroed when dropped, never printed.
pub(crate) struct Secret(Vec<u8>);

impl Secret {
    /// Wraps the password bytes. The dialog builds one when the user
    /// submits it.
    pub(crate) fn new(bytes: Vec<u8>) -> Secret {
        Secret(bytes)
    }

    /// The bytes to write to sudo's input.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(hidden)")
    }
}

/// What it takes to run the system pass.
pub(crate) enum EscalationProbe {
    /// Root already, or the tool runs without asking.
    Ready,
    /// sudo needs a password, which the dialog can supply.
    Password,
    /// doas needs a password, which only it can ask for, in the
    /// terminal. The probe carries the tool so the pass can run through
    /// it once the user agrees.
    Terminal(EscalationTool),
    /// No way to get root. The message is the same one the command line
    /// prints, and lands in the job's report.
    Unavailable(String),
}

/// Checks how the system pass could run here. Probing runs
/// `tool -n true` with no terminal input, so it never prompts.
pub(crate) fn probe_escalation() -> EscalationProbe {
    if soar_utils::system::is_root() {
        return EscalationProbe::Ready;
    }
    let Some(tool) = find_escalation_tool() else {
        return EscalationProbe::Unavailable(format!(
            "System mode requires root privileges. Neither 'doas' nor 'sudo' found in {}.",
            ESCALATION_DIRS.join(", ")
        ));
    };
    if runs_without_password(&tool) {
        return EscalationProbe::Ready;
    }
    match tool.kind {
        EscalationKind::Sudo => EscalationProbe::Password,
        EscalationKind::Doas => EscalationProbe::Terminal(tool),
    }
}

/// Writes the password to `sudo -S -v`, which caches the credentials, so
/// the system pass itself later runs with `sudo -n` and never sees the
/// password. It is never logged, formatted, or kept past this call. A
/// failed write ends the attempt instead of leaving sudo reading the
/// interface's own input.
pub(crate) fn authenticate(secret: Secret) -> Result<(), String> {
    let Some(tool) = find_escalation_tool() else {
        return Err("No escalation tool found".to_string());
    };
    if !matches!(tool.kind, EscalationKind::Sudo) {
        return Err(format!(
            "{} cannot take a password from the interface",
            tool.path.display()
        ));
    }
    let mut child = Command::new(&tool.path)
        .args(["-S", "-v", "-p", ""])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("starting {}: {err}", tool.path.display()))?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.wait();
        return Err("sudo would not take the password".to_string());
    };
    if let Err(err) = stdin
        .write_all(secret.bytes())
        .and_then(|()| stdin.write_all(b"\n"))
    {
        // A failed write means the pipe broke and sudo is going away, so
        // the wait cannot hang; dropping the input first also signals EOF
        // if it lingers.
        drop(stdin);
        let _ = child.wait();
        return Err(format!("writing the password to sudo: {err}"));
    }
    // Closing the input tells sudo the password is complete; the secret
    // is gone from memory before sudo answers.
    drop(stdin);
    drop(secret);
    let output = child
        .wait_with_output()
        .map_err(|err| format!("waiting for {}: {err}", tool.path.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lowered = stderr.to_lowercase();
    if lowered.contains("incorrect password")
        || lowered.contains("sorry")
        || lowered.contains("no password")
        || lowered.contains("a password is required")
    {
        return Err("Incorrect password, try again".to_string());
    }
    Err(stderr.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debugging_a_secret_hides_the_password() {
        let secret = Secret::new("hunter2".bytes().collect());
        let printed = format!("{secret:?}");
        assert_eq!(printed, "Secret(hidden)");
        assert!(!printed.contains("hunter2"));
    }
}
