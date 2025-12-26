use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Green, LightRed, Magenta, Red, Yellow};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use soar_config::config::get_config;
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::query::PackageQuery,
    SoarResult,
};
use soar_db::{
    models::metadata::PackageListing,
    repository::{
        core::{CoreRepository, SortDirection},
        metadata::MetadataRepository,
    },
};
use soar_utils::fs::dir_size;
use tabled::{
    builder::Builder,
    settings::{peaker::PriorityMax, themes::BorderCorrection, Panel, Style, Width},
};
use tracing::{debug, info, trace};

use crate::{
    state::AppState,
    utils::{
        display_settings, icon_or, pretty_package_size, term_width, vec_string, Colored, Icons,
    },
};

pub async fn search_packages(
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
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?;

    let search_limit = limit.or(get_config().search_limit).unwrap_or(20) as i64;
    trace!(search_limit = search_limit, "using search limit");

    let packages: Vec<Package> = metadata_mgr.query_all_flat(|repo_name, conn| {
        let pkgs = if case_sensitive {
            MetadataRepository::search_case_sensitive(conn, &query, Some(search_limit))?
        } else {
            MetadataRepository::search(conn, &query, Some(search_limit))?
        };
        Ok(pkgs
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.to_string();
                pkg
            })
            .collect())
    })?;

    let installed_pkgs: HashMap<(String, String, String), bool> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, None, None, None, None, None, None, None, None)
        })?
        .into_par_iter()
        .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
        .collect();

    let total = packages.len();
    let display_count = std::cmp::min(search_limit as usize, total);

    let mut installed_count = 0;
    let mut available_count = 0;

    for package in packages.into_iter().take(display_count) {
        let key = (
            package.repo_name.clone(),
            package.pkg_id.clone(),
            package.pkg_name.clone(),
        );
        let state_icon = match installed_pkgs.get(&key) {
            Some(is_installed) => {
                if *is_installed {
                    installed_count += 1;
                    icon_or(Icons::INSTALLED, "+")
                } else {
                    "?"
                }
            }
            None => {
                available_count += 1;
                icon_or(Icons::NOT_INSTALLED, "-")
            }
        };

        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            version_upstream = package.version_upstream,
            description = package.description,
            size = package.ghcr_size.or(package.size),
            "[{}] {}#{}:{} | {}{} | {} - {} ({})",
            state_icon,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Green, &package.repo_name),
            Colored(LightRed, &package.version),
            package
                .version_upstream
                .as_ref()
                .filter(|_| package.version.starts_with("HEAD"))
                .map(|upstream| format!(":{}", Colored(Yellow, &upstream)))
                .unwrap_or_default(),
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

pub async fn query_package(query_str: String) -> SoarResult<()> {
    debug!(query = query_str, "querying package info");
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;

    let query = PackageQuery::try_from(query_str.as_str())?;
    trace!(
        name = ?query.name,
        pkg_id = ?query.pkg_id,
        version = ?query.version,
        repo = ?query.repo_name,
        "parsed query"
    );

    let packages: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
        metadata_mgr
            .query_repo(repo_name, |conn| {
                MetadataRepository::find_filtered(
                    conn,
                    query.name.as_deref(),
                    query.pkg_id.as_deref(),
                    query.version.as_deref(),
                    None,
                    Some(SortDirection::Asc),
                )
            })?
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let mut pkg: Package = p.into();
                pkg.repo_name = repo_name.clone();
                pkg
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::find_filtered(
                conn,
                query.name.as_deref(),
                query.pkg_id.as_deref(),
                query.version.as_deref(),
                None,
                Some(SortDirection::Asc),
            )?;
            Ok(pkgs
                .into_iter()
                .map(|p| {
                    let mut pkg: Package = p.into();
                    pkg.repo_name = repo_name.to_string();
                    pkg
                })
                .collect())
        })?
    };

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

        builder.push_record(["Description".to_string(), package.description.clone()]);

        let version = format!(
            "{}{}",
            Colored(Blue, &package.version),
            package
                .version_upstream
                .as_ref()
                .filter(|_| package.version.starts_with("HEAD"))
                .map(|u| format!(" ({})", Colored(Yellow, u)))
                .unwrap_or_default()
        );
        builder.push_record([format!("{} Version", Icons::VERSION), version]);

        builder.push_record([
            format!("{} Size", Icons::SIZE),
            pretty_package_size(package.ghcr_size, package.size),
        ]);

        if let Some(ref cs) = package.bsum {
            builder.push_record([
                "Checksum".to_string(),
                format!("{} (blake3)", Colored(Blue, cs)),
            ]);
        }

        if let Some(ref homepages) = package.homepages {
            builder.push_record([
                "Homepages".to_string(),
                homepages
                    .iter()
                    .map(|h| Colored(Blue, h).to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ]);
        }

        if let Some(ref licenses) = package.licenses {
            builder.push_record(["Licenses".to_string(), licenses.join(", ")]);
        }

        if let Some(ref maintainers) = package.maintainers {
            let maintainer_strs: Vec<String> = maintainers.iter().map(|m| m.to_string()).collect();
            builder.push_record(["Maintainers".to_string(), maintainer_strs.join(", ")]);
        }

        if let Some(ref notes) = package.notes {
            builder.push_record(["Notes".to_string(), notes.join("\n")]);
        }

        if let Some(ref pkg_type) = package.pkg_type {
            builder.push_record(["Type".to_string(), Colored(Magenta, pkg_type).to_string()]);
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
            builder.push_record(["Build CI".to_string(), build_info]);
        }

        if let Some(ref date) = package.build_date {
            builder.push_record(["Build Date".to_string(), date.clone()]);
        }

        if let Some(ref log) = package.build_log {
            builder.push_record(["Build Log".to_string(), Colored(Blue, log).to_string()]);
        }

        if let Some(ref script) = package.build_script {
            builder.push_record([
                "Build Script".to_string(),
                Colored(Blue, script).to_string(),
            ]);
        }

        if let Some(ref blob) = package.ghcr_blob {
            builder.push_record(["GHCR Blob".to_string(), Colored(Blue, blob).to_string()]);
        } else {
            builder.push_record([
                "Download URL".to_string(),
                Colored(Blue, &package.download_url).to_string(),
            ]);
        }

        if let Some(ref pkg) = package.ghcr_pkg {
            builder.push_record([
                "GHCR Package".to_string(),
                Colored(Blue, format!("https://{pkg}")).to_string(),
            ]);
        }

        if let Some(ref webindex) = package.pkg_webpage {
            builder.push_record(["Index".to_string(), Colored(Blue, webindex).to_string()]);
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
            version_upstream = package.version_upstream,
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

/// Lightweight struct for listing with repo name attached
struct PackageListingWithRepo {
    repo_name: String,
    pkg: PackageListing,
}

pub async fn list_packages(repo_name: Option<String>) -> SoarResult<()> {
    debug!(repo = ?repo_name, "listing packages");
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?;

    let packages: Vec<PackageListingWithRepo> = if let Some(ref repo_name) = repo_name {
        metadata_mgr
            .query_repo(repo_name, MetadataRepository::list_all_minimal)?
            .unwrap_or_default()
            .into_iter()
            .map(|pkg| {
                PackageListingWithRepo {
                    repo_name: repo_name.clone(),
                    pkg,
                }
            })
            .collect()
    } else {
        metadata_mgr.query_all_flat(|repo_name, conn| {
            let pkgs = MetadataRepository::list_all_minimal(conn)?;
            Ok(pkgs
                .into_iter()
                .map(|pkg| {
                    PackageListingWithRepo {
                        repo_name: repo_name.to_string(),
                        pkg,
                    }
                })
                .collect())
        })?
    };

    let installed_pkgs: HashMap<(String, String, String), bool> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(conn, None, None, None, None, None, None, None, None)
        })?
        .into_par_iter()
        .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
        .collect();

    let total = packages.len();
    let mut installed_count = 0;
    let mut available_count = 0;

    for entry in &packages {
        let key = (
            entry.repo_name.clone(),
            entry.pkg.pkg_id.clone(),
            entry.pkg.pkg_name.clone(),
        );
        let state_icon = match installed_pkgs.get(&key) {
            Some(is_installed) => {
                if *is_installed {
                    installed_count += 1;
                    icon_or(Icons::INSTALLED, "+")
                } else {
                    "?"
                }
            }
            None => {
                available_count += 1;
                icon_or(Icons::NOT_INSTALLED, "-")
            }
        };

        info!(
            pkg_name = entry.pkg.pkg_name,
            pkg_id = entry.pkg.pkg_id,
            repo_name = entry.repo_name,
            pkg_type = entry.pkg.pkg_type,
            version = entry.pkg.version,
            version_upstream = entry.pkg.version_upstream,
            "[{}] {}#{}:{} | {}{} | {}",
            state_icon,
            Colored(Blue, &entry.pkg.pkg_name),
            Colored(Cyan, &entry.pkg.pkg_id),
            Colored(Cyan, &entry.repo_name),
            Colored(LightRed, &entry.pkg.version),
            entry
                .pkg
                .version_upstream
                .as_ref()
                .filter(|_| entry.pkg.version.starts_with("HEAD"))
                .map(|upstream| format!(":{}", Colored(Yellow, &upstream)))
                .unwrap_or_default(),
            entry
                .pkg
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

pub async fn list_installed_packages(repo_name: Option<String>, count: bool) -> SoarResult<()> {
    debug!(repo = ?repo_name, count_only = count, "listing installed packages");
    let state = AppState::new();
    let diesel_db = state.diesel_core_db()?;

    if count {
        let count = diesel_db.with_conn(|conn| {
            CoreRepository::count_distinct_installed(conn, repo_name.as_deref())
        })?;
        info!("{}", count);
        return Ok(());
    }

    // Get installed packages
    let packages: Vec<InstalledPackage> = diesel_db
        .with_conn(|conn| {
            CoreRepository::list_filtered(
                conn,
                repo_name.as_deref(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
        })?
        .into_iter()
        .map(Into::into)
        .collect();
    trace!(count = packages.len(), "fetched installed packages");

    let mut unique_pkgs = HashSet::new();
    let settings = display_settings();
    let use_icons = settings.icons();

    let (installed_count, unique_count, broken_count, installed_size, broken_size) =
        packages.iter().fold(
            (0, 0, 0, 0, 0),
            |(installed_count, unique_count, broken_count, installed_size, broken_size),
             package| {
                let installed_path = PathBuf::from(&package.installed_path);
                let size = dir_size(&installed_path).unwrap_or(0);
                let is_installed = package.is_installed && installed_path.exists();

                let status = if is_installed {
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
                    HumanBytes(size),
                    status,
                );

                if is_installed {
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
                Colored(Magenta, HumanBytes(installed_size))
            ),
        ]);

        if broken_count > 0 {
            builder.push_record([
                format!("{} Broken", icon_or(Icons::CROSS, "!")),
                format!(
                    "{} ({})",
                    Colored(Red, broken_count),
                    Colored(Magenta, HumanBytes(broken_size))
                ),
            ]);

            let total_count = installed_count + broken_count;
            let total_size = installed_size + broken_size;
            builder.push_record([
                format!("{} Total", Icons::PACKAGE),
                format!(
                    "{} ({})",
                    Colored(Blue, total_count),
                    Colored(Magenta, HumanBytes(total_size))
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
            HumanBytes(installed_size),
        );

        if broken_count > 0 {
            info!(
                broken_count,
                broken_size,
                "Broken: {} ({})",
                broken_count,
                HumanBytes(broken_size)
            );

            let total_count = installed_count + broken_count;
            let total_size = installed_size + broken_size;
            info!(
                total_count,
                total_size,
                "Total: {} ({})",
                total_count,
                HumanBytes(total_size)
            );
        }
    }

    Ok(())
}
