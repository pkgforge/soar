use std::path::Path;

use soar_config::{config::get_config, packages::SandboxConfig};
use tracing::{debug, warn};

use crate::{error::ErrorContext, sandbox, SoarError, SoarResult};

/// Environment variables passed to hook commands.
pub struct HookEnv<'a> {
    pub install_dir: &'a Path,
    pub pkg_name: &'a str,
    pub pkg_id: &'a str,
    pub pkg_version: &'a str,
}

/// Run a hook command with environment variables set.
///
/// This is the shared hook execution logic used by both package installation
/// and removal operations.
pub fn run_hook(
    hook_name: &str,
    command: &str,
    env: &HookEnv,
    sandbox_config: Option<&SandboxConfig>,
) -> SoarResult<()> {
    debug!("running {} hook: {}", hook_name, command);

    let bin_dir = get_config().get_bin_path()?;

    let env_vars: Vec<(&str, &str)> = vec![
        ("INSTALL_DIR", env.install_dir.to_str().unwrap_or("")),
        ("BIN_DIR", bin_dir.to_str().unwrap_or("")),
        ("PKG_NAME", env.pkg_name),
        ("PKG_ID", env.pkg_id),
        ("PKG_VERSION", env.pkg_version),
    ];

    let use_sandbox = sandbox::is_landlock_supported();

    let status = if use_sandbox {
        debug!("running {} hook with Landlock sandbox", hook_name);
        let mut cmd = sandbox::SandboxedCommand::new(command)
            .working_dir(env.install_dir)
            .read_path(&bin_dir)
            .envs(env_vars);

        if let Some(s) = sandbox_config {
            let config = sandbox::SandboxConfig::new().with_network(if s.network {
                sandbox::NetworkConfig::allow_all()
            } else {
                sandbox::NetworkConfig::default()
            });
            cmd = cmd.config(config);
            for path in &s.fs_read {
                cmd = cmd.read_path(path);
            }
            for path in &s.fs_write {
                cmd = cmd.write_path(path);
            }
        }
        cmd.run()?
    } else {
        if sandbox_config.is_some_and(|s| s.require) {
            return Err(SoarError::Custom(format!(
                "{} hook requires sandbox but Landlock is not available on this system. \
                 Either upgrade to Linux 5.13+ or set sandbox.require = false.",
                hook_name
            )));
        }

        use std::process::Command;
        warn!(
            "Landlock not supported, running {} hook without sandbox",
            hook_name
        );
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("INSTALL_DIR", env.install_dir)
            .env("BIN_DIR", &bin_dir)
            .env("PKG_NAME", env.pkg_name)
            .env("PKG_ID", env.pkg_id)
            .env("PKG_VERSION", env.pkg_version)
            .current_dir(env.install_dir)
            .status()
            .with_context(|| format!("executing {} hook", hook_name))?
    };

    if !status.success() {
        return Err(SoarError::Custom(format!(
            "{} hook failed with exit code: {}",
            hook_name,
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}
