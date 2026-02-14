use std::collections::HashSet;

use nu_ansi_term::Color::{Blue, Cyan, Green, LightRed, Magenta, Red, Yellow};
use soar_core::SoarResult;
use soar_operations::{list, search, SoarContext};
use soar_utils::bytes::format_bytes;
use tabled::{
    builder::Builder,
    settings::{peaker::PriorityMax, themes::BorderCorrection, Panel, Style, Width},
};
use tracing::{debug, info};

use crate::utils::{
    display_settings, icon_or, pretty_package_size, term_width, vec_string, Colored, Icons,
};

pub async fn search_packages(
    ctx: &SoarContext,
    query: String,
    case_sensitive: bool,
    limit: Option<usize>,
) -> SoarResult<()> {
    debug!(
        query = query,
        case_sensitive = case_sensitive,
        limit = ?limit,
        "searching packages"
    );

    let result = search::search_packages(ctx, &query, case_sensitive, limit).await?;

    let total = result.total_count;
    let display_count = result.packages.len();

    let mut installed_count = 0;
    let mut available_count = 0;

    for entry in &result.packages {
        let state_icon = if entry.installed {
            installed_count += 1;
            icon_or(Icons::INSTALLED, "+")
        } else {
            available_count += 1;
            icon_or(Icons::NOT_INSTALLED, "-")
        };

        let package = &entry.package;
        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            description = package.description,
            size = package.ghcr_size.or(package.size),
            "[{}] {}#{}:{} | {} | {} - {} ({})",
            state_icon,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Green, &package.repo_name),
            Colored(LightRed, &package.version),
            package
                .pkg_type
                .as_ref()
                .map(|pkg_type| format!("{}", Colored(Magenta, &pkg_type)))
                .unwrap_or_default(),
            package.description,
            pretty_package_size(package.ghcr_size, package.size)
        );
    }

    let settings = display_settings();
    if settings.icons() {
        let mut builder = Builder::new();
        builder.push_record([
            format!("{} Found", Icons::PACKAGE),
            format!(
                "{} (showing {})",
                Colored(Cyan, total),
                Colored(Green, display_count)
            ),
        ]);
        builder.push_record([
            format!("{} Installed", icon_or(Icons::INSTALLED, "+")),
            format!("{}", Colored(Green, installed_count)),
        ]);
        builder.push_record([
            format!("{} Available", icon_or(Icons::NOT_INSTALLED, "-")),
            format!("{}", Colored(Blue, available_count)),
        ]);

        let table = builder
            .build()
            .with(Panel::header("Search Results"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    } else {
        info!(
            "{}",
            Colored(
                Red,
                format!(
                    "Showing {} of {} ({} installed, {} available)",
                    display_count, total, installed_count, available_count
                )
            )
        );
    }

    Ok(())
}

pub async fn query_package(ctx: &SoarContext, query_str: String) -> SoarResult<()> {
    debug!(query = query_str, "querying package info");

    let packages = search::query_package(ctx, &query_str).await?;

    for package in packages {
        let mut builder = Builder::new();

        builder.push_record([
            format!("{} Name", Icons::PACKAGE),
            format!(
                "{}#{}:{}",
                Colored(Blue, &package.pkg_name),
                Colored(Cyan, &package.pkg_id),
                Colored(Green, &package.repo_name)
            ),
        ]);

        builder.push_record([
            format!("{} Description", Icons::DESCRIPTION),
            package.description.clone(),
        ]);

        builder.push_record([
            format!("{} Version", Icons::VERSION),
            Colored(Blue, &package.version).to_string(),
        ]);

        builder.push_record([
            format!("{} Size", Icons::SIZE),
            pretty_package_size(package.ghcr_size, package.size),
        ]);

        if let Some(ref cs) = package.bsum {
            builder.push_record([
                format!("{} Checksum", Icons::CHECKSUM),
                format!("{} (blake3)", Colored(Blue, cs)),
            ]);
        }

        if let Some(ref homepages) = package.homepages {
            builder.push_record([
                format!("{} Homepages", Icons::HOME),
                homepages
                    .iter()
                    .map(|h| Colored(Blue, h).to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ]);
        }

        if let Some(ref licenses) = package.licenses {
            builder.push_record([format!("{} Licenses", Icons::LICENSE), licenses.join(", ")]);
        }

        if let Some(ref maintainers) = package.maintainers {
            let maintainer_strs: Vec<String> = maintainers.iter().map(|m| m.to_string()).collect();
            builder.push_record([
                format!("{} Maintainers", Icons::MAINTAINER),
                maintainer_strs.join(", "),
            ]);
        }

        if let Some(ref notes) = package.notes {
            builder.push_record([format!("{} Notes", Icons::NOTE), notes.join("\n")]);
        }

        if let Some(ref pkg_type) = package.pkg_type {
            builder.push_record([
                format!("{} Type", Icons::TYPE),
                Colored(Magenta, pkg_type).to_string(),
            ]);
        }

        if let Some(ref action) = package.build_action {
            let build_info = format!(
                "{}{}",
                Colored(Blue, action),
                package
                    .build_id
                    .as_ref()
                    .map(|id| format!(" ({})", Colored(Yellow, id)))
                    .unwrap_or_default()
            );
            builder.push_record([format!("{} Build CI", Icons::BUILD), build_info]);
        }

        if let Some(ref date) = package.build_date {
            builder.push_record([format!("{} Build Date", Icons::CALENDAR), date.clone()]);
        }

        if let Some(ref log) = package.build_log {
            builder.push_record([
                format!("{} Build Log", Icons::LOG),
                Colored(Blue, log).to_string(),
            ]);
        }

        if let Some(ref script) = package.build_script {
            builder.push_record([
                format!("{} Build Script", Icons::SCRIPT),
                Colored(Blue, script).to_string(),
            ]);
        }

        if let Some(ref blob) = package.ghcr_blob {
            builder.push_record([
                format!("{} GHCR Blob", Icons::LINK),
                Colored(Blue, blob).to_string(),
            ]);
        } else {
            builder.push_record([
                format!("{} Download URL", Icons::LINK),
                Colored(Blue, &package.download_url).to_string(),
            ]);
        }

        if let Some(ref pkg) = package.ghcr_pkg {
            builder.push_record([
                format!("{} GHCR Package", Icons::PACKAGE),
                Colored(Blue, format!("https://{pkg}")).to_string(),
            ]);
        }

        if let Some(ref webindex) = package.pkg_webpage {
            builder.push_record([
                format!("{} Index", Icons::LINK),
                Colored(Blue, webindex).to_string(),
            ]);
        }

        let table = builder
            .build()
            .with(Style::rounded())
            .with(Width::wrap(term_width()).priority(PriorityMax::default()))
            .to_string();

        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            pkg_type = package.pkg_type,
            repo_name = package.repo_name,
            description = package.description,
            version = package.version,
            bsum = package.bsum,
            homepages = vec_string(package.homepages),
            source_urls = vec_string(package.source_urls),
            licenses = vec_string(package.licenses),
            maintainers = vec_string(package.maintainers),
            notes = vec_string(package.notes),
            snapshots = vec_string(package.snapshots),
            size = package.size,
            download_url = package.download_url,
            build_id = package.build_id,
            build_date = package.build_date,
            build_action = package.build_action,
            build_log = package.build_log,
            build_script = package.build_script,
            ghcr_blob = package.ghcr_blob,
            ghcr_pkg = package.ghcr_pkg,
            pkg_webpage = package.pkg_webpage,
            "\n{table}"
        );
    }

    Ok(())
}

pub async fn list_packages(ctx: &SoarContext, repo_name: Option<String>) -> SoarResult<()> {
    debug!(repo = ?repo_name, "listing packages");

    let result = list::list_packages(ctx, repo_name.as_deref()).await?;

    let total = result.total;
    let mut installed_count = 0;
    let mut available_count = 0;

    for entry in &result.packages {
        let state_icon = if entry.installed {
            installed_count += 1;
            icon_or(Icons::INSTALLED, "+")
        } else {
            available_count += 1;
            icon_or(Icons::NOT_INSTALLED, "-")
        };

        let package = &entry.package;
        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            "[{}] {}#{}:{} | {} | {}",
            state_icon,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Cyan, &package.repo_name),
            Colored(LightRed, &package.version),
            package
                .pkg_type
                .as_ref()
                .map(|pkg_type| format!("{}", Colored(Magenta, &pkg_type)))
                .unwrap_or_default(),
        );
    }

    let settings = display_settings();
    if settings.icons() {
        let mut builder = Builder::new();
        builder.push_record([
            format!("{} Total", Icons::PACKAGE),
            format!("{}", Colored(Cyan, total)),
        ]);
        builder.push_record([
            format!("{} Installed", icon_or(Icons::INSTALLED, "+")),
            format!("{}", Colored(Green, installed_count)),
        ]);
        builder.push_record([
            format!("{} Available", icon_or(Icons::NOT_INSTALLED, "-")),
            format!("{}", Colored(Blue, available_count)),
        ]);

        let table = builder
            .build()
            .with(Panel::header("Package List"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!("\n{table}");
    } else {
        info!(
            "Total: {} ({} installed, {} available)",
            total, installed_count, available_count
        );
    }

    Ok(())
}

pub async fn list_installed_packages(
    ctx: &SoarContext,
    repo_name: Option<String>,
    count: bool,
) -> SoarResult<()> {
    debug!(repo = ?repo_name, count_only = count, "listing installed packages");

    if count {
        let count = list::count_installed(ctx, repo_name.as_deref())?;
        info!("{}", count);
        return Ok(());
    }

    let result = list::list_installed(ctx, repo_name.as_deref())?;

    let mut unique_pkgs = HashSet::new();
    let settings = display_settings();
    let use_icons = settings.icons();

    let (installed_count, unique_count, broken_count, installed_size, broken_size) =
        result.packages.iter().fold(
            (0, 0, 0, 0u64, 0u64),
            |(installed_count, unique_count, broken_count, installed_size, broken_size), entry| {
                let package = &entry.package;
                let size = entry.disk_size;

                let status = if entry.is_healthy {
                    String::new()
                } else if use_icons {
                    format!(
                        " {} {}",
                        icon_or(Icons::BROKEN, "!"),
                        Colored(Red, "Broken")
                    )
                } else {
                    Colored(Red, " [Broken]").to_string()
                };

                info!(
                    pkg_name = package.pkg_name,
                    version = package.version,
                    repo_name = package.repo_name,
                    installed_date = package.installed_date.clone(),
                    size = %package.size,
                    "{}-{}:{} ({}) ({}){}",
                    Colored(Blue, &package.pkg_name),
                    Colored(Magenta, &package.version),
                    Colored(Cyan, &package.repo_name),
                    Colored(Green, &package.installed_date.clone()),
                    format_bytes(size, 2),
                    status,
                );

                if entry.is_healthy {
                    let unique_count = unique_pkgs
                        .insert(format!("{}-{}", package.pkg_id, package.pkg_name))
                        as u32
                        + unique_count;
                    (
                        installed_count + 1,
                        unique_count,
                        broken_count,
                        installed_size + size,
                        broken_size,
                    )
                } else {
                    (
                        installed_count,
                        unique_count,
                        broken_count + 1,
                        installed_size,
                        broken_size + size,
                    )
                }
            },
        );

    if use_icons {
        let mut builder = Builder::new();

        builder.push_record([
            format!("{} Installed", icon_or(Icons::CHECK, "+")),
            format!(
                "{}{} ({})",
                Colored(Green, installed_count),
                if installed_count != unique_count {
                    format!(", {} distinct", Colored(Cyan, unique_count))
                } else {
                    String::new()
                },
                Colored(Magenta, format_bytes(installed_size, 2))
            ),
        ]);

        if broken_count > 0 {
            builder.push_record([
                format!("{} Broken", icon_or(Icons::CROSS, "!")),
                format!(
                    "{} ({})",
                    Colored(Red, broken_count),
                    Colored(Magenta, format_bytes(broken_size, 2))
                ),
            ]);

            let total_count = installed_count + broken_count;
            let total_size = installed_size + broken_size;
            builder.push_record([
                format!("{} Total", Icons::PACKAGE),
                format!(
                    "{} ({})",
                    Colored(Blue, total_count),
                    Colored(Magenta, format_bytes(total_size, 2))
                ),
            ]);
        }

        let table = builder
            .build()
            .with(Panel::header("Summary"))
            .with(Style::rounded())
            .with(BorderCorrection {})
            .to_string();

        info!(installed_count, unique_count, installed_size, "\n{table}");
    } else {
        info!(
            installed_count,
            unique_count,
            installed_size,
            "Installed: {}{} ({})",
            installed_count,
            if installed_count != unique_count {
                format!(", {unique_count} distinct")
            } else {
                String::new()
            },
            format_bytes(installed_size, 2),
        );

        if broken_count > 0 {
            info!(
                broken_count,
                broken_size,
                "Broken: {} ({})",
                broken_count,
                format_bytes(broken_size, 2)
            );

            let total_count = installed_count + broken_count;
            let total_size = installed_size + broken_size;
            info!(
                total_count,
                total_size,
                "Total: {} ({})",
                total_count,
                format_bytes(total_size, 2)
            );
        }
    }

    Ok(())
}
