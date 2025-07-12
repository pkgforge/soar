use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Green, LightRed, Magenta, Purple, Red, White, Yellow};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use soar_core::{
    config::get_config,
    database::{
        models::{FromRow, Package},
        packages::{FilterCondition, PackageQueryBuilder, PaginatedResponse, SortDirection},
    },
    package::query::PackageQuery,
    utils::calculate_dir_size,
    SoarResult,
};
use tracing::info;

use crate::{
    state::AppState,
    utils::{pretty_package_size, vec_string, Colored},
};

#[derive(Debug, Clone)]
pub struct PackageSearchList {
    pkg_id: String,
    pkg_name: String,
    repo_name: String,
    pkg_type: Option<String>,
    version: String,
    version_upstream: Option<String>,
    description: String,
    ghcr_size: Option<u64>,
    size: Option<u64>,
}

impl FromRow for PackageSearchList {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(PackageSearchList {
            pkg_id: row.get("pkg_id")?,
            pkg_name: row.get("pkg_name")?,
            repo_name: row.get("repo_name")?,
            pkg_type: row.get("pkg_type")?,
            version: row.get("version")?,
            version_upstream: row.get("version_upstream")?,
            description: row.get("description")?,
            ghcr_size: row.get("ghcr_size")?,
            size: row.get("size")?,
        })
    }
}

pub async fn search_packages(
    query: String,
    case_sensitive: bool,
    limit: Option<usize>,
) -> SoarResult<()> {
    let state = AppState::new();
    let repo_db = state.repo_db().await?;
    let core_db = state.core_db()?;

    let filter_condition = if case_sensitive {
        FilterCondition::Like(query)
    } else {
        FilterCondition::ILike(query)
    };
    let packages: PaginatedResponse<PackageSearchList> = PackageQueryBuilder::new(repo_db.clone())
        .where_or("pkg_name", filter_condition.clone())
        .where_or("pkg_id", filter_condition.clone())
        .where_or("pkg", filter_condition.clone())
        .sort_by("pkg_name", SortDirection::Asc)
        .json_where_or("provides", "target_name", filter_condition.clone())
        .limit(limit.or(get_config().search_limit).unwrap_or(20) as u32)
        .select(&[
            "pkg_id",
            "pkg_name",
            "pkg_type",
            "version",
            "version_upstream",
            "description",
            "ghcr_size",
            "size",
        ])
        .load()?;

    let installed_pkgs: HashMap<(String, String, String), bool> =
        PackageQueryBuilder::new(core_db.clone())
            .load_installed()?
            .items
            .into_par_iter()
            .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
            .collect();

    for package in packages.items {
        let key = (
            package.repo_name.clone(),
            package.pkg_id.clone(),
            package.pkg_name.clone(),
        );
        let install_state = match installed_pkgs.get(&key) {
            Some(is_installed) => match is_installed {
                true => "+",
                false => "?",
            },
            None => "-",
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
            install_state,
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

    info!(
        "{}",
        Colored(
            Red,
            format!(
                "Showing {} of {}",
                std::cmp::min(packages.limit.unwrap_or(0) as u64, packages.total),
                packages.total
            )
        )
    );

    Ok(())
}

pub async fn query_package(query: String) -> SoarResult<()> {
    let state = AppState::new();
    let repo_db = state.repo_db().await?;

    let query = PackageQuery::try_from(query.as_str())?;
    let builder = PackageQueryBuilder::new(repo_db.clone());
    let builder = query.apply_filters(builder);
    let packages: Vec<Package> = builder.load()?.items;

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
                            .map(|count| format!(" ({count} weekly downloads)"))
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
                pretty_package_size(package.ghcr_size, package.size)
            ),
            format!("{}:", Colored(Purple, "Checksums")),
            package
                .bsum
                .as_ref()
                .map(|cs| format!("  - {} (blake3)", Colored(Blue, cs)))
                .unwrap_or_default(),
            package
                .shasum
                .as_ref()
                .map(|cs| format!("  - {} (sha256)", Colored(Blue, cs)))
                .unwrap_or_default(),
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
                    format!("{key}\n{values}")
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
                    format!("{key}\n{values}")
                })
                .unwrap_or_default(),
            package
                .maintainers
                .as_ref()
                .map(|maintainers| {
                    let key = format!("{}:", Colored(Purple, "Maintainers"));
                    let values = maintainers
                        .iter()
                        .map(|maintainer| format!("  - {}", Colored(Blue, maintainer)))
                        .collect::<Vec<String>>()
                        .join("\n");
                    format!("{key}\n{values}")
                })
                .unwrap_or_default(),
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
                    format!("{key}\n{values}")
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
                    format!("{key}\n{values}")
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
                    format!("{key}\n{values}")
                })
                .unwrap_or_default(),
            package
                .pkg_type
                .as_ref()
                .map(|pkg_type| format!("{}: {}", Colored(Purple, "Type"), Colored(Blue, pkg_type)))
                .unwrap_or_default(),
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
                    let url = format!("https://{pkg}");
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
            "{}",
            fields
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| s.as_str())
                .collect::<Vec<&str>>()
                .join("\n")
        );
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct PackageList {
    pkg_id: String,
    pkg_name: String,
    repo_name: String,
    pkg_type: Option<String>,
    version: String,
    version_upstream: Option<String>,
}

impl FromRow for PackageList {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(PackageList {
            pkg_id: row.get("pkg_id")?,
            pkg_name: row.get("pkg_name")?,
            repo_name: row.get("repo_name")?,
            pkg_type: row.get("pkg_type")?,
            version: row.get("version")?,
            version_upstream: row.get("version_upstream")?,
        })
    }
}

pub async fn list_packages(repo_name: Option<String>) -> SoarResult<()> {
    let state = AppState::new();
    let repo_db = state.repo_db().await?;
    let core_db = state.core_db()?;

    let mut builder = PackageQueryBuilder::new(repo_db.clone())
        .sort_by("pkg_name", SortDirection::Asc)
        .limit(3000);

    if let Some(repo_name) = repo_name {
        builder = builder.where_and("repo_name", FilterCondition::Eq(repo_name));
    }

    builder = builder.select(&[
        "pkg_id",
        "pkg_name",
        "pkg_type",
        "version",
        "version_upstream",
    ]);

    let installed_pkgs: HashMap<(String, String, String), bool> =
        PackageQueryBuilder::new(core_db.clone())
            .load_installed()?
            .items
            .into_par_iter()
            .map(|pkg| ((pkg.repo_name, pkg.pkg_id, pkg.pkg_name), pkg.is_installed))
            .collect();

    loop {
        let packages: PaginatedResponse<PackageList> = builder.load()?;

        for package in &packages.items {
            let key = (
                package.repo_name.clone(),
                package.pkg_id.clone(),
                package.pkg_name.clone(),
            );
            let install_state = match installed_pkgs.get(&key) {
                Some(is_installed) => match is_installed {
                    true => "+",
                    false => "?",
                },
                None => "-",
            };

            info!(
                pkg_name = package.pkg_name,
                pkg_id = package.pkg_id,
                repo_name = package.repo_name,
                pkg_type = package.pkg_type,
                version = package.version,
                version_upstream = package.version_upstream,
                "[{}] {}#{}:{} | {}{} | {}",
                install_state,
                Colored(Blue, &package.pkg_name),
                Colored(Cyan, &package.pkg_id),
                Colored(Cyan, &package.repo_name),
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
            );
        }

        if !packages.has_next {
            break;
        }

        builder = builder.page(packages.page + 1);
    }

    Ok(())
}

pub async fn list_installed_packages(repo_name: Option<String>, count: bool) -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;

    if count {
        let conn = core_db.lock().unwrap();
        let mut query = String::from(
            "SELECT COUNT(DISTINCT pkg_id || pkg_name) FROM packages WHERE is_installed = true",
        );
        let mut params: [&dyn rusqlite::ToSql; 1] = [&""];

        let param_slice: &[&dyn rusqlite::ToSql] = if let Some(ref repo_name) = repo_name {
            query.push_str(" AND repo_name = ?");
            params[0] = repo_name;
            &params
        } else {
            &[]
        };

        let count: u32 = conn.query_row(&query, param_slice, |row| row.get(0))?;
        info!("{}", count);
        return Ok(());
    }

    let mut builder = PackageQueryBuilder::new(core_db.clone());
    if let Some(repo_name) = repo_name {
        builder = builder.where_and("repo_name", FilterCondition::Eq(repo_name));
    }

    let packages = builder.load_installed()?.items;
    let mut unique_pkgs = HashSet::new();

    let (installed_count, unique_count, broken_count, installed_size, broken_size) =
        packages.iter().fold(
            (0, 0, 0, 0, 0),
            |(installed_count, unique_count, broken_count, installed_size, broken_size),
             package| {
                let installed_path = PathBuf::from(&package.installed_path);
                let size = calculate_dir_size(&installed_path).unwrap_or(0);
                let is_installed = package.is_installed && installed_path.exists();
                info!(
                    pkg_name = package.pkg_name,
                    version = package.version,
                    repo_name = package.repo_name,
                    installed_date = package.installed_date.clone(),
                    size = %package.size,
                    "{}-{}:{} ({}) ({}){}",
                    Colored(Red, &package.pkg_name),
                    Colored(Magenta, &package.version),
                    Colored(Cyan, &package.repo_name),
                    Colored(Blue, &package.installed_date.clone()),
                    HumanBytes(size),
                    if is_installed {
                        "".to_string()
                    } else {
                        Colored(Red, " [Broken]").to_string()
                    },
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

    Ok(())
}
