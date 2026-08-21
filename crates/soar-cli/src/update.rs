use nu_ansi_term::Color::{Blue, Cyan, Green, Red};
use soar_core::SoarResult;
use soar_operations::{update, SoarContext, UpdateInfo, UpdateReport};
use tabled::{
    builder::Builder,
    settings::{themes::BorderCorrection, Panel, Style},
};
use tracing::{error, info};

use crate::{
    json_output::{self, Listing, UpdateJson},
    progress::create_wait_job,
    utils::{ask_target_action, display_settings, icon_or, json_enabled, Colored, Icons},
};

pub async fn update_packages(
    ctx: &SoarContext,
    packages: Option<Vec<String>>,
    keep: bool,
    ask: bool,
    check: bool,
    no_verify: bool,
) -> SoarResult<()> {
    // Packages installed from a remote source are checked over the network here.
    let spinner = create_wait_job("checking for updates");
    let checked = update::check_updates(ctx, packages.as_deref()).await;
    spinner.finish_and_clear();
    let updates = checked?;

    if check {
        return report_pending(&updates);
    }

    if updates.is_empty() {
        info!("No packages to update.");
        return Ok(());
    }

    display_pending(&updates);

    if ask {
        let install_targets: Vec<_> = updates.iter().map(|u| u.target.clone()).collect();
        ask_target_action(&install_targets, "update")?;
    }

    let report = update::perform_update(ctx, updates, keep, no_verify).await?;
    display_update_report(&report);

    Ok(())
}

/// Say what is waiting to be updated, and stop there.
fn report_pending(updates: &[UpdateInfo]) -> SoarResult<()> {
    if json_enabled() {
        let items: Vec<UpdateJson> = updates.iter().map(Into::into).collect();
        json_output::emit(&Listing::new(items, updates.len()));
        return Ok(());
    }

    if updates.is_empty() {
        info!("No packages to update.");
        return Ok(());
    }

    display_pending(updates);

    Ok(())
}

fn display_pending(updates: &[UpdateInfo]) {
    for update_info in updates {
        info!(
            "{}: {} -> {}",
            Colored(Blue, &update_info.pkg_name),
            Colored(Red, &update_info.current_version),
            Colored(Green, &update_info.new_version),
        );
    }
}

fn display_update_report(report: &UpdateReport) {
    let settings = display_settings();
    let use_icons = settings.icons();

    for err_info in &report.failed {
        error!("Failed to update {}: {}", err_info.pkg_name, err_info.error);
    }

    let updated_count = report.updated.len();
    let failed_count = report.failed.len();
    let total_packages = updated_count + failed_count;

    if use_icons {
        let mut builder = Builder::new();

        if updated_count > 0 {
            builder.push_record([
                format!("{} Updated", icon_or(Icons::CHECK, "+")),
                format!(
                    "{}/{}",
                    Colored(Green, updated_count),
                    Colored(Cyan, total_packages)
                ),
            ]);
        }
        if failed_count > 0 {
            builder.push_record([
                format!("{} Failed", icon_or(Icons::CROSS, "!")),
                format!("{}", Colored(Red, failed_count)),
            ]);
        }
        if updated_count == 0 && failed_count == 0 {
            builder.push_record([
                format!("{} Status", icon_or(Icons::WARNING, "!")),
                "No packages updated".to_string(),
            ]);
        }

        let table = builder
            .build()
            .with(Panel::header("Update Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    } else {
        info!(
            "Updated {}/{} packages{}",
            updated_count,
            total_packages,
            if failed_count > 0 {
                format!(", {} failed", failed_count)
            } else {
                String::new()
            }
        );
    }
}
