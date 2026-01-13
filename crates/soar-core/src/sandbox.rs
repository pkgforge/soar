//! Sandbox module for restricting hook and build command execution using Landlock.
//!
//! Landlock is a Linux security module (available since kernel 5.13) that allows
//! unprivileged processes to restrict their own filesystem access rights.

use std::path::{Path, PathBuf};

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

    /// Include user directories (~/.cache, ~/.config, ~/.local).
    pub fn with_user_dirs(mut self) -> Self {
        self.include_user_dirs = true;
        self
    }
}

/// Builder for running sandboxed commands.
pub struct SandboxedCommand<'a> {
    command: &'a str,
    working_dir: PathBuf,
    env_vars: Vec<(String, String)>,
    config: SandboxConfig,
    extra_read_paths: Vec<PathBuf>,
    extra_write_paths: Vec<PathBuf>,
}

impl<'a> SandboxedCommand<'a> {
    /// Create a new sandboxed command builder.
    pub fn new(command: &'a str) -> Self {
        Self {
            command,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            env_vars: Vec::new(),
            config: SandboxConfig::new(),
            extra_read_paths: Vec::new(),
            extra_write_paths: Vec::new(),
        }
    }

    /// Set the working directory.
    pub fn working_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.working_dir = dir.into();
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
        run_sandboxed_command(
            self.command,
            &self.working_dir,
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
                current_ruleset = current_ruleset
                    .add_rule(PathBeneath::new(fd, access))
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

/// Apply Landlock restrictions to the current process.
///
/// This function is designed to be called from a `pre_exec` hook, which runs
/// in the forked child process before exec(). It sets up filesystem and
/// optionally network restrictions using the Landlock LSM.
///
/// # Signal Safety
///
/// This function is async-signal-safe because:
/// - Landlock operations are direct syscalls (landlock_create_ruleset,
///   landlock_add_rule, landlock_restrict_self)
/// - No heap allocations occur after the function starts (all data is pre-allocated)
/// - No locks are acquired
/// - No async-signal-unsafe functions are called
///
/// # Arguments
///
/// * `read_paths` - Paths to allow read access
/// * `write_paths` - Paths to allow full (read/write) access
/// * `network_config` - Network restriction configuration
fn apply_landlock_restrictions(
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
    network_config: &NetworkConfig,
) -> std::io::Result<()> {
    let abi = get_best_abi();
    let read_access = AccessFs::from_read(abi);
    let write_access = AccessFs::from_all(abi);

    let ruleset_builder = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| std::io::Error::other(format!("Landlock FS access setup failed: {}", e)))?;

    let ruleset_builder = if is_network_supported(abi) && !network_config.allow_all {
        ruleset_builder
            .handle_access(AccessNet::from_all(abi))
            .map_err(|e| {
                std::io::Error::other(format!("Landlock network access setup failed: {}", e))
            })?
    } else {
        ruleset_builder
    };

    let ruleset = ruleset_builder
        .create()
        .map_err(|e| std::io::Error::other(format!("Landlock ruleset creation failed: {}", e)))?;

    let ruleset =
        add_path_rules(ruleset, read_paths, read_access).map_err(std::io::Error::other)?;

    let ruleset =
        add_path_rules(ruleset, write_paths, write_access).map_err(std::io::Error::other)?;

    let ruleset = if is_network_supported(abi) && !network_config.allow_all {
        add_network_rules(ruleset, network_config).map_err(std::io::Error::other)?
    } else {
        ruleset
    };

    // Enforce the sandbox
    ruleset
        .restrict_self()
        .map_err(|e| std::io::Error::other(format!("Failed to enforce Landlock: {}", e)))?;

    debug!("Landlock sandbox activated");
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
    use std::{os::unix::process::CommandExt, process::Command};

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

    // Check Landlock support
    if !is_landlock_supported() {
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

    // Pre-allocate all paths before entering the unsafe pre_exec context.
    // This ensures no heap allocations occur in the forked child.
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

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(working_dir);

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // SAFETY: `pre_exec` runs in the forked child process after fork() but before exec().
    unsafe {
        cmd.pre_exec(move || {
            apply_landlock_restrictions(&read_paths, &write_paths, &network_config)
        });
    }

    cmd.status()
        .map_err(|e| SoarError::SandboxExecution(e.to_string()))
}
