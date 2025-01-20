use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use indicatif::HumanBytes;
use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
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

use crate::state::AppState;

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
    if case_sensitive {
        filters.insert(
            "pkg_name".to_string(),
            (FilterOp::Like, query.into()).into(),
        );
    } else {
        filters.insert(
            "pkg_name".to_string(),
            (FilterOp::ILike, query.into()).into(),
        );
    }

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
            pkg_name = %package.pkg_name,
            pkg_id = %package.pkg_id,
            description = %package.description,
            version = %package.version,
            repo_name = %package.repo_name,
            "[{}] {}#{}-{}:{} - {} ({})",
            install_state,
            Blue.paint(package.pkg_name.clone()),
            Cyan.paint(package.pkg_id.clone()),
            Magenta.paint(package.version.clone()),
            Cyan.paint(package.repo_name.clone()),
            package.description,
            HumanBytes(package.size)
        );
    }

    info!(
        "{}",
        Red.paint(format!(
            "Showing {} of {}",
            std::cmp::min(packages.limit as u64, packages.total),
            packages.total
        ))
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
        info!(
            pkg_name = %package.pkg_name,
            pkg_id = %package.pkg_id,
            repo_name = %package.repo_name,
            description = %package.description,
            homepage = ?package.homepages,
            source_url = ?package.source_urls,
            version = %package.version,
            checksum = %package.checksum,
            size = %package.size,
            download_url = %package.download_url,
            build_date = ?package.build_date,
            build_log = ?package.build_log,
            build_script = ?package.build_script,
            concat!(
                "\n{}: {} ({1}#{}:{})\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}\n",
            "{}\n",
            "{}",
            ),
            Red.paint("Name"), Green.paint(package.pkg_name.clone()), Cyan.paint(package.pkg_id.clone()), Red.paint(package.repo_name.clone()),
            Red.paint("Description"), Yellow.paint(package.description.clone()),
            Red.paint("Homepages"), Blue.paint(serde_json::to_string_pretty(&package.homepages.clone()).unwrap()),
            Red.paint("Sources"), Blue.paint(serde_json::to_string_pretty(&package.source_urls.clone()).unwrap()),
            Red.paint("Version"), Magenta.paint(package.version.clone()),
            Red.paint("Checksum"), Magenta.paint(package.checksum.clone()),
            Red.paint("Size"), Magenta.paint(HumanBytes(package.size).to_string()),
            Red.paint("Download URL"), Blue.paint(package.download_url.clone()),
            if let Some(ref build_date) = package.build_date {
                format!("{}: {}", Red.paint("Build Date"), Magenta.paint(build_date.clone()))
            } else {
                String::new()
            },

            if let Some(ref build_log) = package.build_log {
                format!("{}: {}", Red.paint("Build Log"), Blue.paint(build_log.clone()))
            } else {
                String::new()
            },

            if let Some(ref build_script) = package.build_script {
                format!("{}: {}", Red.paint("Build Script"), Blue.paint(build_script.clone()))
            } else {
                String::new()
            },
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
            "r.name".to_string(),
            (FilterOp::Eq, repo_name.into()).into(),
        );
    }

    let package_iterator = PaginatedIterator::new(
        &fetch_packages,
        QueryOptions {
            limit: 2000,
            sort_by: vec![("pkg_name".into(), SortOrder::Asc)],
            filters: filters.clone(),
            ..Default::default()
        },
    );

    for result in package_iterator {
        let packages = result?;
        for package in packages {
            let install_state = get_package_install_state(&core_db, &filters, &package)?;
            info!(
                pkg_name = %package.pkg_name,
                version = %package.version,
                repo_name = %package.repo_name,
                "[{}] {}-{}:{}",
                install_state,
                Red.paint(package.pkg_name.clone()),
                package.version,
                package.repo_name
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

    let mut count = 0;
    let mut broken_count = 0;
    let mut total_size = 0;
    let mut broken_size = 0;

    for package in packages {
        if package.is_installed {
            info!(
                pkg_name = %package.pkg_name,
                version = %package.version,
                repo_name = %package.repo_name,
                installed_date = %package.installed_date.clone().unwrap(),
                size = %package.size,
                "{}-{}:{} ({}) ({})",
                Red.paint(package.pkg_name.clone()),
                package.version,
                package.repo_name,
                package.installed_date.clone().unwrap(),
                HumanBytes(package.size)
            );

            count += 1;
            total_size += package.size;
        } else {
            broken_count += 1;
            broken_size += package.size;
        }
    }

    info!(
        total_count = %count,
        broken_count = %broken_count,
        total_size = %total_size,
        "Total: {} ({})",
        count,
        HumanBytes(total_size),
    );
    info!(
        broken_count = %broken_count,
        total_size = %broken_size,
        "Broken: {} ({})",
        broken_count,
        HumanBytes(broken_size)
    );

    Ok(())
}
