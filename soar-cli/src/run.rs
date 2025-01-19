use std::{fs, process::Command, sync::Arc};

use soar_core::{
    database::{
        models::Package,
        packages::{get_packages, QueryOptions},
    },
    error::SoarError,
    package::query::PackageQuery,
    utils::calculate_checksum,
    SoarResult,
};
use soar_dl::downloader::{DownloadOptions, Downloader};

use crate::{
    progress::{self, create_progress_bar},
    state::AppState,
    utils::interactive_ask,
};

pub async fn run_package(command: &[String]) -> SoarResult<()> {
    let state = AppState::new().await?;
    let repo_db = state.repo_db().clone();

    let package_name = &command[0];
    let args = if command.len() > 1 {
        &command[1..]
    } else {
        &[]
    };

    let query = PackageQuery::try_from(package_name.as_str())?;
    let filters = query.create_filter();
    let options = QueryOptions {
        filters,
        ..Default::default()
    };
    let packages: Vec<Package> = get_packages(repo_db, options)?.items;

    if packages.is_empty() {
        return Err(SoarError::PackageNotFound(package_name.clone()));
    }

    let package = packages.first().unwrap();
    let cache_bin = state.config().get_cache_path()?.join("bin");
    fs::create_dir_all(&cache_bin)?;

    let output_path = cache_bin.join(&package.pkg_name);
    if !output_path.exists() {
        let progress_bar = create_progress_bar();
        let progress_callback = Arc::new(move |state| {
            progress::handle_progress(state, &progress_bar);
        });

        let downloader = Downloader::default();
        let options = DownloadOptions {
            url: package.download_url.clone(),
            output_path: Some(output_path.to_string_lossy().to_string()),
            progress_callback: Some(progress_callback),
        };

        downloader.download(options).await?;

        let checksum = calculate_checksum(&output_path)?;
        if checksum != package.checksum {
            let response = interactive_ask("Invalid checksum. Do you want to continue (y/N)?")?;
            if !response.to_lowercase().starts_with("y") {
                return Err(SoarError::InvalidChecksum);
            }
        }
    }

    Command::new(output_path).args(args).status()?;

    Ok(())
}
