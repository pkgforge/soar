use std::{fs, process::Command, sync::Arc};

use soar_core::{
    database::{
        models::Package,
        packages::{FilterCondition, PackageQueryBuilder},
    },
    error::{ErrorContext, SoarError},
    package::query::PackageQuery,
    utils::get_extract_dir,
    SoarResult,
};
use soar_dl::{download::Download, oci::OciDownload, types::OverwriteMode};
use soar_utils::hash::calculate_checksum;

use crate::{
    progress::{self, create_progress_bar},
    state::AppState,
    utils::{interactive_ask, select_package_interactively},
};

pub async fn run_package(
    command: &[String],
    yes: bool,
    repo_name: Option<&str>,
    pkg_id: Option<&str>,
) -> SoarResult<()> {
    let state = AppState::new();
    let cache_bin = state.config().get_cache_path()?.join("bin");

    let package_name = &command[0];

    let query = PackageQuery::try_from(package_name.as_str())?;
    let package_name = &query.name.unwrap_or_else(|| package_name.to_string());
    let repo_name = query.repo_name.as_deref().or(repo_name);
    let pkg_id = query.pkg_id.as_deref().or(pkg_id);
    let version = query.version.as_deref();

    let args = if command.len() > 1 {
        &command[1..]
    } else {
        &[]
    };

    let output_path = cache_bin.join(package_name);
    if !output_path.exists() {
        let repo_db = state.repo_db().await?;

        let mut builder = PackageQueryBuilder::new(repo_db.clone())
            .where_and("pkg_name", FilterCondition::Eq(package_name.clone()));

        if let Some(repo_name) = repo_name {
            builder = builder.where_and("repo_name", FilterCondition::Eq(repo_name.to_string()));
        }

        if let Some(pkg_id) = pkg_id {
            builder = builder.where_and("pkg_id", FilterCondition::Eq(pkg_id.to_string()));
        }

        if let Some(version) = version {
            builder = builder.where_and("version", FilterCondition::Eq(version.to_string()));
        }

        let packages: Vec<Package> = builder.load()?.items;

        let package = match packages.len() {
            0 => return Err(SoarError::PackageNotFound(package_name.clone())),
            1 => packages.into_iter().next(),
            _ if yes => packages.into_iter().next(),
            _ => select_package_interactively(packages, package_name)?,
        }
        .unwrap();

        fs::create_dir_all(&cache_bin)
            .with_context(|| format!("creating directory {}", cache_bin.display()))?;

        let progress_bar = create_progress_bar();
        let progress_callback = Arc::new(move |state| {
            progress::handle_progress(state, &progress_bar);
        });

        if let Some(url) = package.ghcr_blob {
            let mut dl = OciDownload::new(url.as_str())
                .output(output_path.to_string_lossy())
                .overwrite(OverwriteMode::Force);
            let cb = progress_callback.clone();
            dl = dl.progress(move |p| {
                cb(p);
            });

            dl.execute()?;
        } else {
            let extract_dir = get_extract_dir(&cache_bin);
            let mut dl = Download::new(&package.download_url)
                .output(output_path.to_string_lossy())
                .overwrite(OverwriteMode::Force)
                .extract(true)
                .extract_to(&extract_dir);

            let cb = progress_callback.clone();
            dl = dl.progress(move |p| {
                cb(p);
            });

            let file_name = dl.execute()?;
            if extract_dir.exists() {
                fs::remove_file(file_name).ok();

                for entry in fs::read_dir(&extract_dir)
                    .with_context(|| format!("reading {} directory", extract_dir.display()))?
                {
                    let entry = entry.with_context(|| {
                        format!("reading entry from directory {}", extract_dir.display())
                    })?;
                    let from = entry.path();
                    let to = cache_bin.join(entry.file_name());
                    fs::rename(&from, &to).with_context(|| {
                        format!("renaming {} to {}", from.display(), to.display())
                    })?;
                }

                fs::remove_dir_all(&extract_dir).ok();
            }
        }

        let checksum = calculate_checksum(&output_path)?;
        if let Some(bsum) = package.bsum {
            if checksum != bsum {
                let response = interactive_ask("Invalid checksum. Do you want to continue (y/N)?")?;
                if !response.to_lowercase().starts_with("y") {
                    return Err(SoarError::InvalidChecksum);
                }
            }
        }
    }

    Command::new(&output_path)
        .args(args)
        .status()
        .with_context(|| format!("executing command {}", output_path.display()))?;

    Ok(())
}
