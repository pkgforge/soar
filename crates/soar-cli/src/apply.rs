use std::io::{self, Write};

use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use soar_config::packages::PackagesConfig;
use soar_core::SoarResult;
use soar_operations::{apply, ApplyDiff, ApplyReport, SoarContext};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tracing::{info, warn};

use crate::utils::{display_settings, icon_or, Colored, Icons};

pub async fn apply_packages(
    ctx: &SoarContext,
    prune: bool,
    dry_run: bool,
    yes: bool,
    packages_config: Option<String>,
    no_verify: bool,
) -> SoarResult<()> {
    let config = PackagesConfig::load(packages_config.as_deref())?;
    let resolved = config.resolved_packages();

    if resolved.is_empty() {
        info!("No packages declared in configuration");
        return Ok(());
    }

    info!("Loaded {} package declaration(s)", resolved.len());

    let diff = apply::compute_diff(ctx, &resolved, prune).await?;

    display_diff(&diff, prune);

    if !diff.has_changes() && !diff.has_toml_updates() {
        info!("\nAll packages are in sync!");
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

    let report = apply::execute_apply(ctx, diff, no_verify).await?;
    display_apply_report(&report);

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
                format!(
                    "{}#{}",
                    Colored(Blue, &pkg.pkg_name),
                    Colored(Cyan, &pkg.pkg_id)
                ),
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
                format!(
                    "{}#{}",
                    Colored(Blue, &pkg.pkg_name),
                    Colored(Cyan, &pkg.pkg_id)
                ),
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
                    format!(
                        "{}#{}",
                        Colored(Blue, &pkg.pkg_name),
                        Colored(Cyan, &pkg.pkg_id)
                    ),
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
