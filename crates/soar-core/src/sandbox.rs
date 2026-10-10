//! Sandbox module for restricting hook and build command execution using Landlock.
//!
//! Landlock is a Linux security module (available since kernel 5.13) that allows
//! unprivileged processes to restrict their own filesystem access rights.

use std::{
    os::{fd::AsFd as _, unix::process::CommandExt as _},
    path::{Path, PathBuf},
    process::Command,
};

use landlock::{
    Access as _, AccessFs, AccessNet, BitFlags, NetPort, PathBeneath, PathFd, Ruleset,
    RulesetAttr as _, RulesetCreatedAttr as _, ABI,
};
use soar_utils::path::{xdg_cache_home, xdg_config_home, xdg_data_home};
use tracing::{debug, warn};

use crate::{error::SoarError, SoarResult};

/// Network access configuration.
#[derive(Clone, Debug, Default)]
pub struct NetworkConfig {
    /// Allow all outbound network connections.
    pub allow_all: bool,
    /// Specific TCP ports to allow for binding.
    pub allow_bind_tcp: Vec<u16>,
    /// Specific TCP ports to allow for connecting.
    pub allow_connect_tcp: Vec<u16>,
}

impl NetworkConfig {
    /// Allow all network access.
    pub fn allow_all() -> Self {
        Self {
            allow_all: true,
            ..Default::default()
        }
    }

    /// Allow connecting to common HTTPS/HTTP ports.
    pub fn allow_https() -> Self {
        Self {
            allow_connect_tcp: vec![80, 443],
            ..Default::default()
        }
    }
}

/// Sandbox configuration for hook/build execution.
#[derive(Clone, Debug, Default)]
pub struct SandboxConfig {
    /// Whether sandboxing is enabled. If false, commands run without restrictions.
    pub enabled: bool,
    /// Paths that can be read (in addition to defaults).
    pub fs_read: Vec<PathBuf>,
    /// Paths that can be written (in addition to defaults).
    pub fs_write: Vec<PathBuf>,
    /// Network access configuration (requires Landlock V4+).
    pub network: NetworkConfig,
    /// Fail instead of running with widened access.
    ///
    /// When set and Landlock is unavailable, or the kernel cannot
    /// enforce a requested network denial, the command fails instead
    /// of running unsandboxed or with full network access.
    pub required: bool,
    /// Whether to include default read paths (e.g., /usr, /lib, /etc essentials).
    pub include_default_read_paths: bool,
    /// Whether to include user cache/config directories in write paths.
    pub include_user_dirs: bool,
}

impl SandboxConfig {
    /// Create a new sandbox config with sensible defaults.
    pub fn new() -> Self {
        Self {
            enabled: true,
            include_default_read_paths: true,
            include_user_dirs: false,
            ..Default::default()
        }
    }

    /// Create a disabled sandbox config (no restrictions).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Add a readable path.
    pub fn add_read_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.fs_read.push(path.into());
        self
    }

    /// Add a writable path.
    pub fn add_write_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.fs_write.push(path.into());
        self
    }

    /// Set network configuration.
    pub fn with_network(mut self, network: NetworkConfig) -> Self {
        self.network = network;
        self
    }

    /// Require enforcement: fail instead of running with widened access.
    pub fn require(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Include user directories (~/.cache, ~/.config, ~/.local).
    pub fn with_user_dirs(mut self) -> Self {
        self.include_user_dirs = true;
        self
    }
}

/// Builder for running sandboxed commands.
pub struct SandboxedCommand<'a> {
    command: &'a str,
    working_dir: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    config: SandboxConfig,
    extra_read_paths: Vec<PathBuf>,
    extra_write_paths: Vec<PathBuf>,
}

impl<'a> SandboxedCommand<'a> {
    /// Create a new sandboxed command builder.
    ///
    /// Set a working directory with [`working_dir`](Self::working_dir)
    /// before [`run`](Self::run). A deleted current directory is an
    /// error, never a silent fallback to `/`.
    pub fn new(command: &'a str) -> Self {
        Self {
            command,
            working_dir: None,
            env_vars: Vec::new(),
            config: SandboxConfig::new(),
            extra_read_paths: Vec::new(),
            extra_write_paths: Vec::new(),
        }
    }

    /// Set the working directory.
    pub fn working_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add an environment variable.
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Add multiple environment variables.
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars
            .extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Set the sandbox configuration.
    pub fn config(mut self, config: SandboxConfig) -> Self {
        self.config = config;
        self
    }

    /// Add an extra readable path.
    pub fn read_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.extra_read_paths.push(path.into());
        self
    }

    /// Add an extra writable path (also grants read access).
    pub fn write_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.extra_write_paths.push(path.into());
        self
    }

    /// Disable sandboxing entirely.
    pub fn no_sandbox(mut self) -> Self {
        self.config.enabled = false;
        self
    }

    /// Run the command and return the exit status.
    pub fn run(self) -> SoarResult<std::process::ExitStatus> {
        let working_dir = match self.working_dir {
            Some(dir) => dir,
            None => {
                std::env::current_dir().map_err(|e| {
                    SoarError::Custom(format!(
                        "cannot determine working directory for sandboxed command: {e}"
                    ))
                })?
            }
        };
        run_sandboxed_command(
            self.command,
            &working_dir,
            &self.env_vars,
            &self.config,
            &self.extra_read_paths,
            &self.extra_write_paths,
        )
    }
}

/// Check if Landlock is supported on this system.
pub fn is_landlock_supported() -> bool {
    match Ruleset::default().handle_access(AccessFs::from_all(ABI::V1)) {
        Ok(_) => {
            debug!("Landlock is supported on this system");
            true
        }
        Err(e) => {
            debug!("Landlock not supported: {}", e);
            false
        }
    }
}

/// Get the best available Landlock ABI version.
fn get_best_abi() -> ABI {
    for abi in [ABI::V5, ABI::V4, ABI::V3, ABI::V2, ABI::V1] {
        if Ruleset::default()
            .handle_access(AccessFs::from_all(abi))
            .is_ok()
        {
            debug!("Using Landlock ABI {:?}", abi);
            return abi;
        }
    }
    ABI::V1
}

/// Check if network restrictions are supported (V4+).
fn is_network_supported(abi: ABI) -> bool {
    matches!(abi, ABI::V4 | ABI::V5)
}

/// Default read-only paths that are always allowed.
fn default_read_paths() -> Vec<PathBuf> {
    [
        "/usr",
        "/lib",
        "/lib64",
        "/bin",
        "/sbin",
        "/nix/store",
        "/run/current-system/sw",
        "/etc/ld.so.cache",
        "/etc/ld.so.conf",
        "/etc/ld.so.conf.d",
        "/etc/ssl/certs",
        "/etc/ca-certificates",
        "/etc/pki",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/passwd",
        "/etc/group",
        "/etc/nsswitch.conf",
        "/etc/localtime",
        "/proc",
        "/sys",
        "/dev/null",
        "/dev/zero",
        "/dev/urandom",
        "/dev/random",
        "/dev/fd",
        "/dev/stdin",
        "/dev/stdout",
        "/dev/stderr",
        "/dev/tty",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

/// Default writable paths (device nodes that need write access).
fn default_write_paths() -> Vec<PathBuf> {
    [
        "/dev/null",
        "/dev/zero",
        "/dev/tty",
        "/dev/stdin",
        "/dev/stdout",
        "/dev/stderr",
        "/dev/fd",
        "/dev/pts",
        "/dev/ptmx",
        "/tmp",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

/// Get user-specific directories that might need access.
fn user_dirs() -> Vec<PathBuf> {
    vec![xdg_cache_home(), xdg_config_home(), xdg_data_home()]
}

/// Add filesystem path rules to a Landlock ruleset.
fn add_path_rules(
    ruleset: landlock::RulesetCreated,
    paths: &[PathBuf],
    access: BitFlags<AccessFs>,
) -> Result<landlock::RulesetCreated, SoarError> {
    let mut current_ruleset = ruleset;

    for path in paths {
        if !path.exists() {
            debug!("Skipping non-existent path: {}", path.display());
            continue;
        }

        match PathFd::new(path) {
            Ok(fd) => {
                // Landlock rejects pipes and sockets as rule objects.
                // Skip them so piped stdio keeps working. Mask with
                // S_IFMT first: the type values overlap, so intersects
                // matches everything.
                let kind = nix::sys::stat::fstat(fd.as_fd())
                    .map(|st| st.st_mode & nix::libc::S_IFMT)
                    .unwrap_or(0);
                if kind == nix::libc::S_IFSOCK || kind == nix::libc::S_IFIFO {
                    debug!("Skipping non-file sandbox path: {}", path.display());
                    continue;
                }
                // Directory-only rights on a file record partial
                // compatibility, which would fail a required sandbox
                // even on a new kernel. Mask files down to file
                // rights, mirroring the landlock crate's ACCESS_FILE.
                let effective = if kind == nix::libc::S_IFDIR {
                    access
                } else {
                    access
                        & (AccessFs::ReadFile
                            | AccessFs::WriteFile
                            | AccessFs::Execute
                            | AccessFs::Truncate
                            | AccessFs::IoctlDev
                            | AccessFs::ResolveUnix)
                };
                current_ruleset = current_ruleset
                    .add_rule(PathBeneath::new(fd, effective))
                    .map_err(|e| {
                        SoarError::SandboxPathRule {
                            path: path.display().to_string(),
                            reason: e.to_string(),
                        }
                    })?;
                debug!("Added sandbox rule for: {}", path.display());
            }
            Err(e) => {
                warn!(
                    "Failed to open path for sandbox rule: {} ({})",
                    path.display(),
                    e
                );
            }
        }
    }

    Ok(current_ruleset)
}

/// Add network port rules to a Landlock ruleset.
fn add_network_rules(
    ruleset: landlock::RulesetCreated,
    config: &NetworkConfig,
) -> Result<landlock::RulesetCreated, SoarError> {
    let mut current_ruleset = ruleset;

    for &port in &config.allow_bind_tcp {
        current_ruleset = current_ruleset
            .add_rule(NetPort::new(port, AccessNet::BindTcp))
            .map_err(|e| {
                SoarError::SandboxNetworkRule {
                    port,
                    reason: e.to_string(),
                }
            })?;
        debug!("Added network bind rule for port: {}", port);
    }

    for &port in &config.allow_connect_tcp {
        current_ruleset = current_ruleset
            .add_rule(NetPort::new(port, AccessNet::ConnectTcp))
            .map_err(|e| {
                SoarError::SandboxNetworkRule {
                    port,
                    reason: e.to_string(),
                }
            })?;
        debug!("Added network connect rule for port: {}", port);
    }

    Ok(current_ruleset)
}

/// Build a Landlock ruleset without enforcing it.
///
/// Allocating, opening fds, and logging all happen here, in the parent.
/// The child only calls `restrict_self` on the returned ruleset.
///
/// * `read_paths` - Paths to allow read access
/// * `write_paths` - Paths to allow full (read/write) access
/// * `network_config` - Network restriction configuration
/// * `required` - fail a requested network denial the kernel cannot
///   enforce, instead of running with full network access.
fn build_ruleset(
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
    network_config: &NetworkConfig,
    required: bool,
) -> SoarResult<landlock::RulesetCreated> {
    build_ruleset_with_abi(
        get_best_abi(),
        read_paths,
        write_paths,
        network_config,
        required,
    )
}

/// [`build_ruleset`] with an explicit ABI, for tests.
fn build_ruleset_with_abi(
    abi: ABI,
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
    network_config: &NetworkConfig,
    required: bool,
) -> SoarResult<landlock::RulesetCreated> {
    let read_access = AccessFs::from_read(abi);
    let write_access = AccessFs::from_all(abi);

    let ruleset_builder = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| {
            SoarError::SandboxRulesetCreation(format!("Landlock FS access setup failed: {e}"))
        })?;

    let restrict_network = if network_config.allow_all {
        false
    } else if is_network_supported(abi) {
        true
    } else if required {
        return Err(SoarError::Custom(format!(
            "network restriction requires Landlock V4+ (kernel 6.7+), found ABI {abi:?}; \
             refusing to run with full network access \
             (set sandbox.require = false to allow it)"
        )));
    } else {
        warn!(
            "Landlock ABI {abi:?} cannot restrict network access; \
             running with full network access although a denial was requested"
        );
        false
    };

    let ruleset_builder = if restrict_network {
        ruleset_builder
            .handle_access(AccessNet::from_all(abi))
            .map_err(|e| {
                SoarError::SandboxRulesetCreation(format!(
                    "Landlock network access setup failed: {e}"
                ))
            })?
    } else {
        ruleset_builder
    };

    let ruleset = ruleset_builder.create().map_err(|e| {
        SoarError::SandboxRulesetCreation(format!("Landlock ruleset creation failed: {e}"))
    })?;

    let ruleset = add_path_rules(ruleset, read_paths, read_access)?;
    let ruleset = add_path_rules(ruleset, write_paths, write_access)?;

    let ruleset = if restrict_network {
        add_network_rules(ruleset, network_config)?
    } else {
        ruleset
    };

    Ok(ruleset)
}

/// Enforce a prebuilt ruleset in the forked child.
///
/// Moves the `io::Error` out instead of formatting it.
fn restrict_self_error(error: landlock::RestrictSelfError) -> std::io::Error {
    match error {
        landlock::RestrictSelfError::SetNoNewPrivsCall {
            source, ..
        }
        | landlock::RestrictSelfError::RestrictSelfCall {
            source, ..
        } => source,
        _ => std::io::Error::last_os_error(),
    }
}

/// Fail a required sandbox when Landlock is unavailable.
fn check_landlock_support(supported: bool, required: bool) -> SoarResult<()> {
    if !supported && required {
        return Err(SoarError::SandboxNotSupported);
    }
    Ok(())
}

/// Execute a shell command with Landlock sandbox restrictions.
fn run_sandboxed_command(
    command: &str,
    working_dir: &Path,
    env_vars: &[(String, String)],
    config: &SandboxConfig,
    extra_read_paths: &[PathBuf],
    extra_write_paths: &[PathBuf],
) -> SoarResult<std::process::ExitStatus> {
    // If sandbox is disabled, run directly
    if !config.enabled {
        debug!("Sandbox disabled, running command directly");
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(working_dir);
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        return cmd
            .status()
            .map_err(|e| SoarError::SandboxExecution(e.to_string()));
    }

    // Check Landlock support. A required sandbox fails closed.
    let supported = is_landlock_supported();
    check_landlock_support(supported, config.required)?;
    if !supported {
        warn!("Landlock not supported, running command without sandbox");
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(working_dir);
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        return cmd
            .status()
            .map_err(|e| SoarError::SandboxExecution(e.to_string()));
    }

    // Build the ruleset in the parent. The child only enforces it.
    let mut read_paths: Vec<PathBuf> = Vec::new();
    if config.include_default_read_paths {
        read_paths.extend(default_read_paths());
    }
    read_paths.extend(config.fs_read.clone());
    read_paths.extend(extra_read_paths.iter().cloned());

    let mut write_paths: Vec<PathBuf> = vec![working_dir.to_path_buf()];
    write_paths.extend(default_write_paths());
    if config.include_user_dirs {
        write_paths.extend(user_dirs());
    }
    write_paths.extend(config.fs_write.clone());
    write_paths.extend(extra_write_paths.iter().cloned());

    let network_config = config.network.clone();
    let required = config.required;
    let ruleset = build_ruleset(&read_paths, &write_paths, &network_config, required)?;

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(working_dir);

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // SAFETY: runs after fork in a multithreaded program. Only
    // `restrict_self` runs here. The slot satisfies `FnMut` while
    // `restrict_self` consumes the ruleset; it is taken exactly once.
    // A required sandbox also checks the enforcement status: anything
    // less than fully enforced with no_new_privs fails closed.
    let mut slot = Some(ruleset);
    unsafe {
        cmd.pre_exec(move || {
            let ruleset = slot
                .take()
                .ok_or_else(|| std::io::Error::from_raw_os_error(nix::libc::EINVAL))?;
            match ruleset.restrict_self() {
                Ok(status) => {
                    if required
                        && (status.ruleset != landlock::RulesetStatus::FullyEnforced
                            || !status.no_new_privs)
                    {
                        return Err(std::io::Error::from_raw_os_error(nix::libc::EPERM));
                    }
                    Ok(())
                }
                Err(landlock::RulesetError::RestrictSelf(error)) => Err(restrict_self_error(error)),
                Err(_) => Err(std::io::Error::last_os_error()),
            }
        });
    }

    cmd.status()
        .map_err(|e| SoarError::SandboxExecution(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_downgrade_fails_when_required() {
        let denied = NetworkConfig::default();
        assert!(!denied.allow_all);
        let result = build_ruleset_with_abi(ABI::V1, &[], &[], &denied, true);
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("full network"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn network_downgrade_warns_but_passes_when_optional() {
        let denied = NetworkConfig::default();
        assert!(build_ruleset_with_abi(ABI::V1, &[], &[], &denied, false).is_ok());
    }

    #[test]
    fn network_deny_enforced_on_capable_abi() {
        if !kernel_enforces(ABI::V4) {
            eprintln!("skipping: kernel lacks Landlock V4");
            return;
        }
        let denied = NetworkConfig::default();
        assert!(build_ruleset_with_abi(ABI::V4, &[], &[], &denied, true).is_ok());
        assert!(build_ruleset_with_abi(ABI::V4, &[], &[], &denied, false).is_ok());
    }

    fn kernel_enforces(abi: ABI) -> bool {
        use landlock::Compatible as _;
        landlock::Ruleset::default()
            .set_compatibility(landlock::CompatLevel::HardRequirement)
            .handle_access(landlock::AccessFs::from_all(abi))
            .and_then(|ruleset| ruleset.create())
            .is_ok()
    }

    #[test]
    fn open_network_needs_no_restriction_on_any_abi() {
        let open = NetworkConfig::allow_all();
        assert!(build_ruleset_with_abi(ABI::V1, &[], &[], &open, true).is_ok());
    }

    #[test]
    fn unsupported_landlock_fails_only_when_required() {
        assert!(check_landlock_support(true, true).is_ok());
        assert!(check_landlock_support(true, false).is_ok());
        assert!(check_landlock_support(false, false).is_ok());
        assert!(matches!(
            check_landlock_support(false, true),
            Err(SoarError::SandboxNotSupported)
        ));
    }

    #[test]
    fn default_write_paths_keep_tmp() {
        let writes = default_write_paths();
        assert!(writes.iter().any(|p| p.as_os_str() == "/tmp"));
    }

    #[test]
    fn default_read_paths_cover_nix() {
        let paths = default_read_paths();
        for expected in ["/nix/store", "/run/current-system/sw"] {
            assert!(
                paths.iter().any(|p| p.as_os_str() == expected),
                "{expected} missing from default read paths"
            );
        }
    }

    struct ProbeSubscriber;

    impl tracing::Subscriber for ProbeSubscriber {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, _: &tracing::Event<'_>) {}

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn sandboxed_run_succeeds_with_debug_subscriber_installed() {
        let dispatch = tracing::dispatcher::Dispatch::new(ProbeSubscriber);
        let _guard = tracing::dispatcher::set_default(&dispatch);
        let status = SandboxedCommand::new("echo sandbox-probe")
            .working_dir(std::env::temp_dir())
            .run()
            .expect("sandboxed run failed");
        assert!(status.success());
    }

    #[test]
    fn required_sandbox_runs_where_fully_enforceable() {
        if !kernel_enforces(ABI::V5) {
            eprintln!("skipping: kernel cannot fully enforce the V5 ruleset");
            return;
        }
        let status = SandboxedCommand::new("echo required-probe")
            .working_dir(std::env::temp_dir())
            .config(SandboxConfig::new().require(true))
            .run()
            .expect("sandboxed run failed");
        assert!(status.success());
    }

    #[test]
    fn sandboxed_run_keeps_tmp_writable() {
        let probe = std::env::temp_dir().join(format!("soar-sandbox-probe-{}", std::process::id()));
        let _ = std::fs::remove_file(&probe);
        let command = format!("touch {} && echo tmp-ok", probe.display());
        let status = SandboxedCommand::new(&command)
            .working_dir(std::env::temp_dir())
            .config(SandboxConfig::new())
            .run()
            .expect("sandboxed run failed");
        let wrote = probe.exists();
        let _ = std::fs::remove_file(&probe);
        assert!(status.success());
        assert!(wrote, "/tmp write inside the sandbox failed");
    }

    #[test]
    fn deleted_working_dir_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let pid = unsafe { nix::libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            let code = match run_without_cwd(&path) {
                Ok(()) => 0,
                Err(()) => 1,
            };
            unsafe { nix::libc::_exit(code) };
        }
        let mut status = 0;
        let waited = unsafe { nix::libc::waitpid(pid, &mut status, 0) };
        assert_eq!(waited, pid, "waitpid failed");
        assert!(
            nix::libc::WIFEXITED(status) && nix::libc::WEXITSTATUS(status) == 0,
            "child did not observe the expected error"
        );

        fn run_without_cwd(dir: &std::path::Path) -> Result<(), ()> {
            std::env::set_current_dir(dir).map_err(|_| ())?;
            std::fs::remove_dir_all(dir).map_err(|_| ())?;
            match SandboxedCommand::new("true")
                .config(SandboxConfig::disabled())
                .run()
            {
                Err(error) if error.to_string().contains("working directory") => Ok(()),
                _ => Err(()),
            }
        }
    }
}
