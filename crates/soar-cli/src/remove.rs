use nu_ansi_term::Color::{Blue, Cyan, Green, LightRed};
use soar_core::SoarResult;
use soar_operations::{remove, RemoveResolveResult, SoarContext};
use tracing::{debug, error, info, warn};

use crate::utils::{confirm_action, select_package_interactively, Colored};

pub async fn remove_packages(
    ctx: &SoarContext,
    packages: &[String],
    yes: bool,
    all: bool,
) -> SoarResult<()> {
    debug!(
        count = packages.len(),
        all = all,
        "starting package removal"
    );

    let results = remove::resolve_removals(ctx, packages, all)?;

    let mut to_remove = Vec::new();
    for result in results {
        match result {
            RemoveResolveResult::Resolved(pkgs) => {
                if pkgs.len() > 1 && !yes {
                    info!(
                        "The following {} packages will be removed:",
                        Colored(Cyan, pkgs.len())
                    );
                    for pkg in &pkgs {
                        info!(
                            "  - {}#{}:{} ({})",
                            Colored(Blue, &pkg.pkg_name),
                            Colored(Cyan, &pkg.pkg_id),
                            Colored(Green, &pkg.repo_name),
                            Colored(LightRed, &pkg.version)
                        );
                    }
                    if !confirm_action("Proceed with removal?")? {
                        info!("Removal cancelled");
                        continue;
                    }
                }
                to_remove.extend(pkgs);
            }
            RemoveResolveResult::Ambiguous {
                query,
                candidates,
            } => {
                if yes {
                    if let Some(pkg) = candidates.into_iter().next() {
                        to_remove.push(pkg);
                    }
                } else {
                    let pkg = select_package_interactively(candidates, &query)?;
                    if let Some(pkg) = pkg {
                        to_remove.push(pkg);
                    }
                }
            }
            RemoveResolveResult::NotInstalled(name) => {
                warn!("Package {} is not installed.", name);
            }
        }
    }

    if to_remove.is_empty() {
        return Ok(());
    }

    let report = remove::perform_removal(ctx, to_remove).await?;

    for removed in &report.removed {
        info!(
            "Removed {}#{}:{} ({})",
            removed.pkg_name, removed.pkg_id, removed.repo_name, removed.version
        );
    }

    for failed in &report.failed {
        error!(
            "Failed to remove {}#{}: {}",
            failed.pkg_name, failed.pkg_id, failed.error
        );
    }

    debug!("package removal completed");
    Ok(())
}
