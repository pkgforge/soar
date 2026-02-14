use nu_ansi_term::Color::{Blue, Cyan, Green, Red, Yellow};
use soar_core::SoarResult;
use soar_operations::{health, SoarContext};
use tabled::{
    builder::Builder,
    settings::{peaker::PriorityMax, themes::BorderCorrection, Panel, Style, Width},
};
use tracing::info;

use crate::utils::{icon_or, term_width, Colored, Icons};

pub async fn display_health(ctx: &SoarContext) -> SoarResult<()> {
    let report = health::check_health(ctx)?;

    let mut builder = Builder::new();

    let path_status = if report.path_configured {
        format!("{} Configured", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} not in PATH",
            Colored(Yellow, icon_or(Icons::WARNING, "!")),
            Colored(Blue, report.bin_path.display())
        )
    };
    builder.push_record(["PATH".to_string(), path_status]);

    let pkg_status = if report.broken_packages.is_empty() {
        format!("{} None", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} found",
            Colored(Red, icon_or(Icons::CROSS, "!")),
            Colored(Red, report.broken_packages.len())
        )
    };
    builder.push_record(["Broken Packages".to_string(), pkg_status]);

    let sym_status = if report.broken_symlinks.is_empty() {
        format!("{} None", Colored(Green, icon_or(Icons::CHECK, "OK")))
    } else {
        format!(
            "{} {} found",
            Colored(Red, icon_or(Icons::CROSS, "!")),
            Colored(Red, report.broken_symlinks.len())
        )
    };
    builder.push_record(["Broken Symlinks".to_string(), sym_status]);

    let table = builder
        .build()
        .with(Panel::header("System Health Check"))
        .with(Style::rounded())
        .with(BorderCorrection {})
        .with(Width::wrap(term_width()).priority(PriorityMax::default()))
        .to_string();

    info!("\n{table}");

    if !report.broken_packages.is_empty() {
        info!("\nBroken packages:");
        for pkg in &report.broken_packages {
            info!(
                "  {} {}#{}: {}",
                Icons::ARROW,
                Colored(Blue, &pkg.pkg_name),
                Colored(Cyan, &pkg.pkg_id),
                Colored(Yellow, &pkg.installed_path)
            );
        }
        info!("Run {} to remove", Colored(Green, "soar clean --broken"));
    }

    if !report.broken_symlinks.is_empty() {
        info!("\nBroken symlinks:");
        for path in &report.broken_symlinks {
            info!("  {} {}", Icons::ARROW, Colored(Yellow, path.display()));
        }
        info!(
            "Run {} to remove",
            Colored(Green, "soar clean --broken-symlinks")
        );
    }

    Ok(())
}

pub async fn remove_broken_packages(ctx: &SoarContext) -> SoarResult<()> {
    let report = health::remove_broken_packages(ctx).await?;

    if report.removed.is_empty() && report.failed.is_empty() {
        info!("No broken packages found.");
        return Ok(());
    }

    for removed in &report.removed {
        info!("Removed {}#{}", removed.pkg_name, removed.pkg_id);
    }

    for failed in &report.failed {
        tracing::error!(
            "Failed to remove {}#{}: {}",
            failed.pkg_name,
            failed.pkg_id,
            failed.error
        );
    }

    if !report.removed.is_empty() {
        info!("Removed all broken packages");
    }

    Ok(())
}
