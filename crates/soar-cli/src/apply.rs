use std::{
    collections::HashSet,
    env,
    io::{self, Write},
    path::Path,
    process::Command,
};

use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use soar_config::{config::is_system_mode, packages::PackagesConfig};
use soar_core::{error::SoarError, SoarResult};
use soar_operations::{apply, ApplyDiff, ApplyReport, SoarContext};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tracing::{info, warn};

use crate::{
    json_output::{self, ApplyDiffJson},
    progress::create_wait_job,
    utils::{display_settings, icon_or, json_enabled, Colored, Icons},
};

/// Hands the packages marked `system = true` to a privileged pass over the same
/// file.
///
/// soar installs into one tree per process: the root, the symlink directories
/// and the database all follow the mode it started in. So the system-wide
/// packages are applied by a second `soar --system apply` over the same file,
/// which escalates itself. Running the whole thing as root instead would leave
/// root-owned files throughout the user's tree.
fn apply_system_packages(count: usize, packages_path: Option<&Path>) -> SoarResult<()> {
    info!(
        "\n{} system package(s) declared, applying to the system tree",
        Colored(Blue, count)
    );

    let exe = env::current_exe()
        .map_err(|err| SoarError::Custom(format!("finding the soar executable: {err}")))?;

    // Forwarding the command as given, rather than rebuilding it flag by flag,
    // is what carries every global option through to the system pass.
    let mut cmd = Command::new(exe);
    cmd.arg("--system")
        .args(env::args_os().skip(1))
        .arg("--only-system");

    // `--packages` rejects a second occurrence, so it is named only when the
    // command being forwarded did not name it already.
    if let Some(packages_path) = packages_path {
        cmd.arg("--packages").arg(packages_path);
    }

    let status = cmd
        .status()
        .map_err(|err| SoarError::Custom(format!("running the system-wide apply: {err}")))?;

    if !status.success() {
        return Err(SoarError::Custom(
            "system-wide apply failed; the user packages above were applied".into(),
        ));
    }

    Ok(())
}

pub async fn apply_packages(
    ctx: &SoarContext,
    prune: bool,
    dry_run: bool,
    yes: bool,
    packages_config: Option<String>,
    no_verify: bool,
    only_system: bool,
) -> SoarResult<()> {
    if only_system && !is_system_mode() {
        return Err(SoarError::Custom(
            "--only-system selects from the system tree and needs --system".into(),
        ));
    }

    let packages_path = PackagesConfig::resolve_path(packages_config.as_deref());
    let config = PackagesConfig::load(packages_config.as_deref())?;

    // A user's file reaches system mode only through the privileged pass, which
    // was sent for its marked packages and has no business with the rest.
    let (resolved, deferred_system): (Vec<_>, Vec<_>) = if is_system_mode() {
        let mut declared = config.resolved_packages();
        if only_system {
            declared.retain(|pkg| pkg.system);
        }
        (declared, Vec::new())
    } else {
        config
            .resolved_packages()
            .into_iter()
            .partition(|pkg| !pkg.system)
    };

    // A dry run with --json is a question: it answers with the diff, and an
    // empty configuration is still an answer.
    let answers_with_diff = dry_run && json_enabled();

    let apply_system = |count: usize| {
        apply_system_packages(count, packages_config.is_none().then_some(&*packages_path))
    };

    if resolved.is_empty() && !answers_with_diff {
        if deferred_system.is_empty() {
            info!("No packages declared in configuration");
            return Ok(());
        }
        return apply_system(deferred_system.len());
    }

    info!("Loaded {} package declaration(s)", resolved.len());

    // Declarations backed by a remote source are resolved over the network here.
    let spinner = create_wait_job(&format!(
        "resolving {} package declaration(s)",
        resolved.len()
    ));
    let resolution = apply::compute_diff(ctx, &resolved, prune).await;
    spinner.finish_and_clear();
    let mut diff = resolution?;

    // A user's file is not the whole truth about the system tree. Removing what
    // the machine's own file declares is not that user's call.
    if only_system && prune {
        let machine_file = PackagesConfig::resolve_path(None);
        if machine_file != packages_path {
            if let Ok(machine) = PackagesConfig::load(Some(&machine_file.to_string_lossy())) {
                let declared: HashSet<String> = machine
                    .resolved_packages()
                    .into_iter()
                    .map(|pkg| pkg.name)
                    .collect();
                diff.to_remove
                    .retain(|pkg| !declared.contains(&pkg.pkg_name));
            }
        }
    }

    if answers_with_diff {
        json_output::emit(&ApplyDiffJson::new(&diff));
        if !deferred_system.is_empty() {
            return apply_system(deferred_system.len());
        }
        return Ok(());
    }

    display_diff(&diff, prune);

    if !diff.has_changes() && !diff.has_toml_updates() {
        info!("\nAll packages are in sync!");
        if !deferred_system.is_empty() {
            return apply_system(deferred_system.len());
        }
        return Ok(());
    }

    if dry_run {
        if diff.has_toml_updates() {
            info!("\nWould update packages.toml:");
            for (pkg_name, version) in &diff.pending_version_updates {
                info!(
                    "  {} {} -> {}",
                    Colored(Blue, pkg_name),
                    Colored(Yellow, "version"),
                    Colored(Green, version)
                );
            }
        }
        info!("\n{} Dry run - no changes made", icon_or("", "[DRY RUN]"));
        if !deferred_system.is_empty() {
            return apply_system(deferred_system.len());
        }
        return Ok(());
    }

    if !yes {
        print!("\nProceed? [y/N] ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Aborted");
            return Ok(());
        }
    }

    let report =
        apply::execute_apply(ctx, diff, no_verify, &packages_path.to_string_lossy()).await?;
    display_apply_report(&report);

    if !deferred_system.is_empty() {
        return apply_system(deferred_system.len());
    }

    Ok(())
}

fn display_diff(diff: &ApplyDiff, prune: bool) {
    let settings = display_settings();
    let use_icons = settings.icons();

    if !diff.to_install.is_empty()
        || !diff.to_update.is_empty()
        || (prune && !diff.to_remove.is_empty())
    {
        let mut builder = Builder::new();
        builder.push_record(["", "Package", "Version", "Repository"]);

        for (_resolved, target) in &diff.to_install {
            let pkg = &target.package;
            builder.push_record([
                format!("{}", Colored(Green, icon_or("+", "+"))),
                format!("{}", Colored(Blue, &pkg.pkg_name),),
                format!("{}", Colored(Green, &pkg.version)),
                format!("{}", Colored(Magenta, &pkg.repo_name)),
            ]);
        }

        for (_resolved, target) in &diff.to_update {
            let pkg = &target.package;
            let old_version = target
                .existing_install
                .as_ref()
                .map_or("?".to_string(), |e| e.version.clone());
            builder.push_record([
                format!("{}", Colored(Yellow, icon_or("~", "~"))),
                format!("{}", Colored(Blue, &pkg.pkg_name),),
                format!(
                    "{} -> {}",
                    Colored(Red, &old_version),
                    Colored(Green, &pkg.version)
                ),
                format!("{}", Colored(Magenta, &pkg.repo_name)),
            ]);
        }

        if prune {
            for pkg in &diff.to_remove {
                builder.push_record([
                    format!("{}", Colored(Red, icon_or("-", "-"))),
                    format!("{}", Colored(Blue, &pkg.pkg_name),),
                    format!("{}", Colored(Yellow, &pkg.version)),
                    format!("{}", Colored(Magenta, &pkg.repo_name)),
                ]);
            }
        }

        let table = builder
            .build()
            .with(Panel::header("Package Changes"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    }

    if !diff.not_found.is_empty() {
        info!("\n{} Packages not found:", icon_or(Icons::WARNING, "!"));
        for name in &diff.not_found {
            warn!("  {} {}", icon_or("?", "?"), Colored(Yellow, name));
        }
    }

    let mut summary_builder = Builder::new();

    if !diff.to_install.is_empty() {
        summary_builder.push_record([
            format!("{} To Install", icon_or("+", "+")),
            format!("{}", Colored(Green, diff.to_install.len())),
        ]);
    }
    if !diff.to_update.is_empty() {
        summary_builder.push_record([
            format!("{} To Update", icon_or("~", "~")),
            format!("{}", Colored(Yellow, diff.to_update.len())),
        ]);
    }
    if prune && !diff.to_remove.is_empty() {
        summary_builder.push_record([
            format!("{} To Remove", icon_or("-", "-")),
            format!("{}", Colored(Red, diff.to_remove.len())),
        ]);
    }
    if !diff.in_sync.is_empty() {
        summary_builder.push_record([
            format!("{} In Sync", icon_or(Icons::CHECK, "*")),
            format!("{}", Colored(Cyan, diff.in_sync.len())),
        ]);
    }
    if !diff.not_found.is_empty() {
        summary_builder.push_record([
            format!("{} Not Found", icon_or(Icons::WARNING, "?")),
            format!("{}", Colored(Yellow, diff.not_found.len())),
        ]);
    }

    if use_icons {
        let summary_table = summary_builder
            .build()
            .with(Panel::header("Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{summary_table}");
    } else {
        let total_changes = diff.to_install.len() + diff.to_update.len() + diff.to_remove.len();
        if total_changes > 0 || !diff.in_sync.is_empty() {
            info!(
                "\nSummary: {} to install, {} to update, {} to remove, {} in sync",
                diff.to_install.len(),
                diff.to_update.len(),
                if prune { diff.to_remove.len() } else { 0 },
                diff.in_sync.len()
            );
        }
    }
}

fn display_apply_report(report: &ApplyReport) {
    info!("\n{} Apply Summary", icon_or(Icons::CHECK, "*"));
    info!("  Installed: {}", report.installed_count);
    info!("  Updated:   {}", report.updated_count);
    info!("  Removed:   {}", report.removed_count);
    if report.failed_count > 0 {
        warn!("  Failed:    {}", report.failed_count);
    }
}
