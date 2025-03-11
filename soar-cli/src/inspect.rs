use std::{
    fmt::Display,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use indicatif::HumanBytes;
use rusqlite::Connection;
use soar_core::{
    database::{
        models::Package,
        packages::{FilterCondition, PackageQueryBuilder, PaginatedResponse},
    },
    error::ErrorContext,
    package::query::PackageQuery,
    SoarResult,
};
use tracing::{error, info};

use crate::{
    state::AppState,
    utils::{interactive_ask, select_package_interactively},
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
    core_db: &Arc<Mutex<Connection>>,
    package: &Package,
) -> SoarResult<Option<PathBuf>> {
    let installed_pkgs = PackageQueryBuilder::new(core_db.clone())
        .where_and("repo_name", FilterCondition::Eq(package.repo_name.clone()))
        .where_and("pkg_id", FilterCondition::Eq(package.pkg_id.clone()))
        .where_and("pkg_name", FilterCondition::Eq(package.pkg_name.clone()))
        .where_and("version", FilterCondition::Eq(package.version.clone()))
        .limit(1)
        .load_installed()?
        .items;

    if !installed_pkgs.is_empty() {
        let pkg = installed_pkgs.first().unwrap();
        if pkg.is_installed {
            return Ok(Some(PathBuf::from(pkg.installed_path.clone())));
        }
    }
    Ok(None)
}

pub async fn inspect_log(package: &str, inspect_type: InspectType) -> SoarResult<()> {
    let state = AppState::new();
    let core_db = state.core_db()?;
    let repo_db = state.repo_db().await?;

    let query = PackageQuery::try_from(package)?;
    let builder = PackageQueryBuilder::new(repo_db.clone());
    let builder = query.apply_filters(builder);

    let packages: PaginatedResponse<Package> = builder.load()?;

    if packages.items.is_empty() {
        error!("Package {} not found", package);
    } else {
        let selected_pkg = if packages.total > 1 {
            &select_package_interactively(
                packages.items,
                &query.name.unwrap_or(package.to_string()),
            )?
            .unwrap()
        } else {
            packages.items.first().unwrap()
        };

        if let Some(installed_path) = get_installed_path(core_db, selected_pkg)? {
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

        let resp = reqwest::get(url).await?;
        if !resp.status().is_success() {
            error!(
                "Error fetching build {inspect_type} from {} [{}]",
                url,
                resp.status()
            );
            return Ok(());
        }

        let content_length = resp.content_length().unwrap_or_default();
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

        let mut stream = resp.bytes_stream();
        let mut content = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            content.extend_from_slice(&chunk);
        }
        let output = String::from_utf8_lossy(&content).replace("\r", "\n");

        info!("\n{}", output);
    }

    Ok(())
}
