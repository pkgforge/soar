use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Purple, Red, White, Yellow};
use rusqlite::Connection;
use soar_core::{
    config::get_config,
    database::{
        models::Package,
        packages::{
            get_installed_packages, get_packages, Filter, FilterOp, PaginatedIterator,
            QueryOptions, SortOrder,
        },
    },
    SoarResult,
};
use tracing::info;

use crate::{
    state::AppState,
    utils::{vec_string, Colored},
};

fn get_package_install_state(
    core_db: &Arc<Mutex<Connection>>,
    filters: &HashMap<String, Filter>,
    package: &Package,
) -> SoarResult<String> {
    let mut filters = filters.clone();
    filters.insert(
        "repo_name".to_string(),
        (FilterOp::Eq, package.repo_name.clone().into()).into(),
    );
    filters.insert(
        "pkg_id".to_string(),
        (FilterOp::Eq, package.pkg_id.clone().into()).into(),
    );
    filters.insert(
        "pkg_name".to_string(),
        (FilterOp::Eq, package.pkg_name.clone().into()).into(),
    );
    let options = QueryOptions {
        limit: 1,
        filters,
        ..Default::default()
    };

    let installed_pkgs = get_installed_packages(core_db.clone(), options)?.items;

    let install_status = match installed_pkgs {
        _ if installed_pkgs.is_empty() => "-",
        _ if installed_pkgs.first().unwrap().is_installed => "+",
        _ => "?",
    };

    Ok(install_status.to_string())
}

pub async fn search_packages(
    query: String,
    case_sensitive: bool,
    limit: Option<usize>,
) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();
    let core_db = state.core_db().clone();

    let mut filters = HashMap::new();

    let op = if case_sensitive {
        FilterOp::Like
    } else {
        FilterOp::ILike
    };

    let filter: Filter = (op, query.clone().into()).into();
    filters.insert("pkg_name".to_string(), filter.clone());

    // TODO: need to handle OR operation for WHERE
    // Probably need to implement some sort of query builder to simplify things
    //
    // filters.insert("pkg_id".to_string(), filter.clone());
    // filters.insert("pkg".to_string(), filter);

    let packages = get_packages(
        repo_db,
        QueryOptions {
            limit: limit.or(get_config().search_limit).unwrap_or(20) as u32,
            filters: filters.clone(),
            ..Default::default()
        },
    )?;

    for package in packages.items {
        let install_state = get_package_install_state(&core_db, &filters, &package)?;

        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            version_upstream = package.version_upstream,
            description = package.description,
            size = package.ghcr_size.unwrap_or(package.size),
            "[{}] {}#{}:{} ({}-{}{}) - {} ({})",
            install_state,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Cyan, &package.repo_name),
            Colored(Magenta, &package.pkg_type),
            Colored(Magenta, &package.version),
            package
                .version_upstream
                .as_ref()
                .filter(|_| package.version.starts_with("HEAD"))
                .map(|upstream| format!(":{}", Colored(Yellow, &upstream)))
                .unwrap_or_default(),
            package.description,
            Colored(
                Magenta,
                HumanBytes(package.ghcr_size.unwrap_or(package.size))
            )
        );
    }

    info!(
        "{}",
        Colored(
            Red,
            format!(
                "Showing {} of {}",
                std::cmp::min(packages.limit as u64, packages.total),
                packages.total
            )
        )
    );

    Ok(())
}

pub async fn query_package(query: String) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();

    let mut filters = HashMap::new();
    filters.insert("pkg_name".to_string(), (FilterOp::Eq, query.into()).into());

    let options = QueryOptions {
        filters,
        limit: 1,
        ..Default::default()
    };

    let packages = get_packages(repo_db, options)?.items;

    for package in packages {
        let fields = [
            format!(
                "\n{}: {} ({1}#{}:{})",
                Colored(Purple, "Name"),
                Colored(Cyan, &package.pkg_name),
                Colored(Blue, &package.pkg_id),
                Colored(Green, &package.repo_name),
            ),
            format!(
                "{}: {}",
                Colored(Purple, "Description"),
                Colored(White, &package.description)
            ),
            package
                .rank
                .map(|rank| {
                    format!(
                        "{}: #{}{}",
                        Colored(Purple, "Rank"),
                        Colored(Yellow, &rank),
                        package
                            .download_count_week
                            .map(|count| format!(" ({} weekly downloads)", count))
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_default(),
            format!(
                "{}: {}{}",
                Colored(Purple, "Version"),
                Colored(Blue, &package.version),
                package
                    .version_upstream
                    .as_ref()
                    .filter(|_| package.version.starts_with("HEAD"))
                    .map(|upstream| format!(" ({})", Colored(Yellow, &upstream)))
                    .unwrap_or_default()
            ),
            format!(
                "{}: {}",
                Colored(Purple, "Size"),
                Colored(Green, HumanBytes(package.ghcr_size.unwrap_or(package.size)))
            ),
            format!("{}:", Colored(Purple, "Checksums")),
            format!("  - {} (blake3)", Colored(Blue, &package.bsum)),
            format!("  - {} (sha256)", Colored(Blue, &package.shasum)),
            package
                .homepages
                .as_ref()
                .map(|homepages| {
                    let key = format!("{}:", Colored(Purple, "Homepages"));
                    let values = homepages
                        .iter()
                        .map(|homepage| format!("  - {}", Colored(Blue, homepage)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{}\n{}", key, values)
                })
                .unwrap_or_default(),
            package
                .licenses
                .as_ref()
                .map(|licenses| {
                    let key = format!("{}:", Colored(Purple, "Licenses"));
                    let values = licenses
                        .iter()
                        .map(|license| format!("  - {}", Colored(Blue, license)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{}\n{}", key, values)
                })
                .unwrap_or_default(),
            format!("{}:", Colored(Purple, "Maintainers")),
            package
                .maintainers
                .iter()
                .map(|maintainer| format!("  - {}", Colored(Blue, maintainer)))
                .collect::<Vec<String>>()
                .join("\n"),
            package
                .notes
                .as_ref()
                .map(|notes| {
                    let key = format!("{}:", Colored(Purple, "Notes"));
                    let values = notes
                        .iter()
                        .map(|note| format!("  - {}", Colored(Blue, note)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{}\n{}", key, values)
                })
                .unwrap_or_default(),
            package
                .snapshots
                .as_ref()
                .map(|snapshots| {
                    let key = format!("{}:", Colored(Purple, "Snapshots"));
                    let values = snapshots
                        .iter()
                        .map(|snapshot| format!("  - {}", Colored(Blue, snapshot)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{}\n{}", key, values)
                })
                .unwrap_or_default(),
            package
                .source_urls
                .as_ref()
                .map(|sources| {
                    let key = format!("{}:", Colored(Purple, "Sources"));
                    let values = sources
                        .iter()
                        .map(|source| format!("  - {}", Colored(Blue, source)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{}\n{}", key, values)
                })
                .unwrap_or_default(),
            format!(
                "{}: {}",
                Colored(Purple, "Type"),
                Colored(Blue, &package.pkg_type)
            ),
            package
                .build_action
                .as_ref()
                .map(|action| {
                    format!(
                        "{}: {}{}",
                        Colored(Purple, "Build CI"),
                        Colored(Blue, &action),
                        package
                            .build_id
                            .as_ref()
                            .map(|id| format!(" ({})", Colored(Yellow, id)))
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_default(),
            package
                .build_date
                .as_ref()
                .map(|date| format!("{}: {}", Colored(Purple, "Build Date"), Colored(Blue, date)))
                .unwrap_or_default(),
            package
                .build_log
                .as_ref()
                .map(|log| format!("{}: {}", Colored(Purple, "Build Log"), Colored(Blue, log)))
                .unwrap_or_default(),
            package
                .build_script
                .as_ref()
                .map(|script| {
                    format!(
                        "{}: {}",
                        Colored(Purple, "Build Script"),
                        Colored(Blue, script)
                    )
                })
                .unwrap_or_default(),
            package
                .ghcr_blob
                .as_ref()
                .map(|blob| format!("{}: {}", Colored(Purple, "GHCR Blob"), Colored(Blue, blob)))
                .unwrap_or_else(|| {
                    format!(
                        "{}: {}",
                        Colored(Purple, "Download URL"),
                        Colored(Blue, &package.download_url)
                    )
                }),
            package
                .ghcr_pkg
                .as_ref()
                .map(|pkg| {
                    let url = format!("https://{}", pkg);
                    format!(
                        "{}: {}",
                        Colored(Purple, "GHCR Package"),
                        Colored(Blue, url)
                    )
                })
                .unwrap_or_default(),
            package
                .pkg_webpage
                .as_ref()
                .map(|webindex| {
                    format!("{}: {}", Colored(Purple, "Index"), Colored(Blue, webindex))
                })
                .unwrap_or_default(),
        ];

        info!(
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            pkg_type = package.pkg_type,
            repo_name = package.repo_name,
            description = package.description,
            rank = package.rank,
            version = package.version,
            version_upstream = package.version_upstream,
            bsum = package.bsum,
            shasum = package.shasum,
            homepages = vec_string(package.homepages),
            source_urls = vec_string(package.source_urls),
            licenses = vec_string(package.licenses),
            maintainers = vec_string(Some(package.maintainers)),
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
            "{}",
            fields.join("\n")
        );
    }

    Ok(())
}

pub async fn list_packages(repo_name: Option<String>) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();
    let core_db = state.core_db().clone();

    let fetch_packages = |query_options: QueryOptions| get_packages(repo_db.clone(), query_options);

    let mut filters = HashMap::new();
    if let Some(repo_name) = repo_name {
        filters.insert(
            "repo_name".to_string(),
            (FilterOp::Eq, repo_name.into()).into(),
        );
    }

    let options = QueryOptions {
        sort_by: vec![("pkg_name".into(), SortOrder::Asc)],
        filters: filters.clone(),
        ..Default::default()
    };

    let package_iterator = PaginatedIterator::new(&fetch_packages, options);

    for result in package_iterator {
        let packages = result?;
        for package in packages {
            let install_state = get_package_install_state(&core_db, &filters, &package)?;

            info!(
                pkg_name = package.pkg_name,
                pkg_id = package.pkg_id,
                repo_name = package.repo_name,
                pkg_type = package.pkg_type,
                version = package.version,
                version_upstream = package.version_upstream,
                "[{}] {}#{}:{} ({}-{}{})",
                install_state,
                Colored(Blue, &package.pkg_name),
                Colored(Cyan, &package.pkg_id),
                Colored(Cyan, &package.repo_name),
                Colored(Magenta, &package.pkg_type),
                Colored(Magenta, &package.version),
                package
                    .version_upstream
                    .as_ref()
                    .filter(|_| package.version.starts_with("HEAD"))
                    .map(|upstream| format!(":{}", Colored(Yellow, &upstream)))
                    .unwrap_or_default()
            );
        }
    }

    Ok(())
}

pub async fn list_installed_packages(repo_name: Option<String>) -> SoarResult<()> {
    let state = AppState::new().await?;
    let core_db = state.core_db().clone();

    let mut filters = HashMap::new();
    if let Some(repo_name) = repo_name {
        filters.insert(
            "repo_name".to_string(),
            (FilterOp::Eq, repo_name.into()).into(),
        );
    }
    let options = QueryOptions {
        filters,
        ..Default::default()
    };
    let packages = get_installed_packages(core_db.clone(), options)?.items;

    let (installed_count, broken_count, installed_size, broken_size) = packages.iter().fold(
        (0, 0, 0, 0),
        |(installed_count, broken_count, installed_size, broken_size), package| {
            if package.is_installed {
                info!(
                    pkg_name = package.pkg_name,
                    version = package.version,
                    repo_name = package.repo_name,
                    installed_date = package.installed_date.clone().unwrap(),
                    size = %package.size,
                    "{}-{}:{} ({}) ({})",
                    Colored(Red, &package.pkg_name),
                    package.version,
                    package.repo_name,
                    package.installed_date.clone().unwrap(),
                    HumanBytes(package.size)
                );
                (
                    installed_count + 1,
                    broken_count,
                    installed_size + package.size,
                    broken_size,
                )
            } else {
                (
                    installed_count,
                    broken_count + 1,
                    installed_size,
                    broken_size + package.size,
                )
            }
        },
    );

    info!(
        installed_count,
        installed_size,
        "Installed: {} ({})",
        installed_count,
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

    Ok(())
}
