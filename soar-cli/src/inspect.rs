use std::fmt::Display;

use futures::StreamExt;
use indicatif::HumanBytes;
use soar_core::{
    database::{
        models::Package,
        packages::{PackageQueryBuilder, PaginatedResponse},
    },
    package::query::PackageQuery,
    SoarResult,
};
use tracing::{error, info};

use crate::{state::AppState, utils::interactive_ask};

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

pub async fn inspect_log(package: &str, inspect_type: InspectType) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();

    let query = PackageQuery::try_from(package)?;
    let builder = PackageQueryBuilder::new(repo_db).limit(1);
    let builder = query.apply_filters(builder);

    let packages: PaginatedResponse<Package> = builder.load()?;

    if packages.items.is_empty() {
        error!("Package {} not found", package);
    } else {
        let first_pkg = packages.items.first().unwrap();

        let url = if matches!(inspect_type, InspectType::BuildLog) {
            &first_pkg.build_log
        } else {
            &first_pkg.build_script
        };

        let Some(url) = url else {
            error!("No build {} found for {}", inspect_type, first_pkg.pkg_name);
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
