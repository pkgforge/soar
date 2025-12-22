use std::{fmt::Display, fs, path::PathBuf};

use indicatif::HumanBytes;
use soar_core::{
    database::{connection::DieselDatabase, models::Package},
    error::ErrorContext,
    package::query::PackageQuery,
    SoarResult,
};
use soar_db::repository::{
    core::{CoreRepository, SortDirection},
    metadata::MetadataRepository,
};
use soar_dl::http_client::SHARED_AGENT;
use tracing::{error, info};
use ureq::http::header::CONTENT_LENGTH;

use crate::{
    progress::create_spinner,
    state::AppState,
    utils::{display_settings, interactive_ask, select_package_interactively},
};

pub enum InspectType {
    BuildLog,
    BuildScript,
}

impl Display for InspectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InspectType::BuildLog => write!(f, "log"),
            InspectType::BuildScript => write!(f, "script"),
        }
    }
}

fn get_installed_path(
    diesel_db: &DieselDatabase,
    package: &Package,
) -> SoarResult<Option<PathBuf>> {
    let installed_pkg = diesel_db.with_conn(|conn| {
        CoreRepository::find_exact(
            conn,
            &package.repo_name,
            &package.pkg_name,
            &package.pkg_id,
            &package.version,
        )
    })?;

    if let Some(pkg) = installed_pkg {
        if pkg.is_installed {
            return Ok(Some(PathBuf::from(pkg.installed_path)));
        }
    }
    Ok(None)
}

pub async fn inspect_log(package: &str, inspect_type: InspectType) -> SoarResult<()> {
    let state = AppState::new();
    let metadata_mgr = state.metadata_manager().await?;
    let diesel_db = state.diesel_core_db()?;

    let query = PackageQuery::try_from(package)?;

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

    if packages.is_empty() {
        error!("Package {} not found", package);
    } else {
        let selected_pkg = if packages.len() > 1 {
            &select_package_interactively(packages, &query.name.unwrap_or(package.to_string()))?
                .unwrap()
        } else {
            packages.first().unwrap()
        };

        if let Some(installed_path) = get_installed_path(diesel_db, selected_pkg)? {
            let file = if matches!(inspect_type, InspectType::BuildLog) {
                installed_path.join(format!("{}.log", selected_pkg.pkg_name))
            } else {
                installed_path.join("SBUILD")
            };

            if file.exists() && file.is_file() {
                info!(
                    "Reading build {inspect_type} from {} [{}]",
                    file.display(),
                    HumanBytes(
                        file.metadata()
                            .with_context(|| format!("reading file metadata {}", file.display()))?
                            .len()
                    )
                );
                let output = fs::read_to_string(&file)
                    .with_context(|| format!("reading file content from {}", file.display()))?
                    .replace("\r", "\n");

                info!("\n{}", output);
                return Ok(());
            }
        };

        let url = if matches!(inspect_type, InspectType::BuildLog) {
            &selected_pkg.build_log
        } else {
            &selected_pkg.build_script
        };

        let Some(url) = url else {
            error!(
                "No build {} found for {}",
                inspect_type, selected_pkg.pkg_name
            );
            return Ok(());
        };

        let url = if url.starts_with("https://github.com") {
            &url.replacen("/tree/", "/raw/refs/heads/", 1)
                .replacen("/blob/", "/raw/refs/heads/", 1)
        } else {
            url
        };

        let settings = display_settings();
        let spinner = if settings.spinners() {
            let s = create_spinner(&format!("Fetching build {inspect_type}..."));
            Some(s)
        } else {
            None
        };

        let resp = SHARED_AGENT.get(url).call()?;

        if let Some(ref s) = spinner {
            s.finish_and_clear();
        }

        if !resp.status().is_success() {
            error!(
                "Error fetching build {inspect_type} from {} [{}]",
                url,
                resp.status()
            );
            return Ok(());
        }

        let content_length = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|h| h.to_str().ok())
            .and_then(|len| len.parse::<u64>().ok())
            .unwrap_or(0);

        if content_length > 1_048_576 {
            let response = interactive_ask(
                "The {inspect_type} file is too large. Do you really want to view it (y/N)?",
            )?;
            if !response.starts_with('y') {
                return Ok(());
            }
        }

        info!(
            "Fetching build {inspect_type} from {} [{}]",
            url,
            HumanBytes(content_length)
        );

        let content = resp.into_body().read_to_vec()?;
        let output = String::from_utf8_lossy(&content).replace("\r", "\n");

        info!("\n{}", output);
    }

    Ok(())
}
