use nu_ansi_term::Color::{Blue, Cyan, Green, Magenta, Red, Yellow};
use soar_core::{
    database::{
        models::InstalledPackage,
        packages::{get_installed_packages_with_filter, get_packages_with_filter, PackageFilter},
    },
    utils::parse_size,
    SoarResult,
};
use tracing::info;

use crate::state::AppState;

pub async fn search_packages(
    query: String,
    case_sensitive: bool,
    limit: Option<usize>,
) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();
    let core_db = state.core_db().clone();

    let packages = get_packages_with_filter(
        repo_db,
        1024,
        PackageFilter {
            pkg_name: Some(query),
            exact_case: case_sensitive,
            ..Default::default()
        },
    )?;

    let mut count = 0;
    let show_count = limit.or(state.config().search_limit).unwrap_or(20);

    for package in packages {
        count += 1;
        if count > show_count {
            continue;
        }
        let package = package?;
        let filter = PackageFilter {
            repo_name: Some(package.repo_name.clone()),
            exact_pkg_name: Some(package.pkg_name.clone()),
            family: Some(package.family),
            ..Default::default()
        };

        let installed_pkgs: Vec<InstalledPackage> =
            get_installed_packages_with_filter(core_db.clone(), 128, filter.clone())?
                .into_iter()
                .filter_map(Result::ok)
                .collect();

        let mut install_status = "-";
        if !installed_pkgs.is_empty() {
            if installed_pkgs.first().unwrap().is_installed {
                install_status = "+";
            } else {
                install_status = "?";
            }
        }

        info!(
            pkg_name = %package.pkg_name,
            description = %package.description,
            version = %package.version,
            repo_name = %package.repo_name,
            "[{}] {}-{}:{} - {}",
            install_status,
            Blue.paint(package.pkg_name.clone()),
            Magenta.paint(package.version.clone()),
            Cyan.paint(package.repo_name.clone()),
            package.description
        );
    }

    info!(
        "{}",
        Red.paint(format!(
            "Showing {} of {}",
            std::cmp::min(show_count, count),
            count
        ))
    );

    Ok(())
}

pub async fn query_package(query: String) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();

    let packages = get_packages_with_filter(
        repo_db,
        1024,
        PackageFilter {
            exact_pkg_name: Some(query),
            ..Default::default()
        },
    )?;

    for package in packages {
        let package = package?;
        info!(
            pkg_name = %package.pkg_name,
            family = %package.family,
            repo_name = %package.repo_name,
            description = %package.description,
            homepage = %package.homepage,
            source_url = %package.source_url,
            version = %package.version,
            checksum = %package.checksum,
            size = %package.size,
            download_url = %package.download_url,
            build_date = %package.build_date,
            build_log = %package.build_log,
            build_script = %package.build_script,
            category = %package.category,
            concat!(
                "\n{}: {} ({}/{1}:{})\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}\n",
            "{}: {}"
            ),
            Red.paint("Name"), Green.paint(package.pkg_name.clone()), Cyan.paint(package.family.clone()), Red.paint(package.repo_name.clone()),
            Red.paint("Description"), Yellow.paint(package.description.clone()),
            Red.paint("Homepage"), Blue.paint(package.homepage.clone()),
            Red.paint("Source"), Blue.paint(package.source_url.clone()),
            Red.paint("Version"), Magenta.paint(package.version.clone()),
            Red.paint("Checksum"), Magenta.paint(package.checksum.clone()),
            Red.paint("Size"), Magenta.paint(package.size.clone()),
            Red.paint("Download URL"), Blue.paint(package.download_url.clone()),
            Red.paint("Build Date"), Magenta.paint(package.build_date.clone()),
            Red.paint("Build Log"), Blue.paint(package.build_log.clone()),
            Red.paint("Build Script"), Blue.paint(package.build_script.clone()),
            Red.paint("Category"), Cyan.paint(package.category.clone())
        );
    }

    Ok(())
}

pub async fn list_packages(repo_name: Option<String>) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();
    let core_db = state.core_db().clone();

    let packages = get_packages_with_filter(
        repo_db,
        1024,
        PackageFilter {
            repo_name: repo_name.clone(),
            ..Default::default()
        },
    )?;

    for package in packages {
        let package = package?;
        let filter = PackageFilter {
            repo_name: Some(package.repo_name.clone()),
            exact_pkg_name: Some(package.pkg_name.clone()),
            family: Some(package.family),
            ..Default::default()
        };

        let installed_pkgs: Vec<InstalledPackage> =
            get_installed_packages_with_filter(core_db.clone(), 128, filter.clone())?
                .into_iter()
                .filter_map(Result::ok)
                .collect();

        let mut install_status = "-";
        if !installed_pkgs.is_empty() {
            if installed_pkgs.first().unwrap().is_installed {
                install_status = "+";
            } else {
                install_status = "?";
            }
        }

        info!(
            pkg_name = %package.pkg_name,
            version = %package.version,
            repo_name = %package.repo_name,
            "[{}] {}-{}:{}",
            install_status,
            Red.paint(package.pkg_name.clone()),
            package.version,
            package.repo_name
        );
    }

    Ok(())
}

pub async fn list_installed_packages(repo_name: Option<String>) -> SoarResult<()> {
    let state = AppState::new().await?;
    let core_db = state.core_db().clone();

    let filter = PackageFilter {
        repo_name,
        ..Default::default()
    };
    let packages = get_installed_packages_with_filter(core_db.clone(), 128, filter.clone())?;

    let mut count = 0;
    let mut broken_count = 0;
    let mut total_size = 0;
    let mut broken_size = 0;

    for package in packages {
        let package = package?;

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
                package.size
            );

            count += 1;
            total_size += parse_size(&package.size).unwrap_or(0);
        } else {
            broken_count += 1;
            broken_size += parse_size(&package.size).unwrap_or(0);
        }
    }

    info!(
        total_count = %count,
        broken_count = %broken_count,
        total_size = %total_size,
        "Total: {} ({})",
        count,
        total_size,
    );
    info!(
        broken_count = %broken_count,
        total_size = %broken_size,
        "Broken: {} ({})",
        broken_count,
        broken_size
    );

    Ok(())
}
