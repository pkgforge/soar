use std::{
    env::{self, consts::ARCH},
    fs,
};

use semver::Version;
use soar_core::{error::ErrorContext, SoarResult};
use soar_dl::{
    downloader::{DownloadOptions, Downloader},
    github::{Github, GithubRelease},
    platform::{Release, ReleaseAsset, ReleaseHandler},
    utils::FileMode,
};
use tracing::{debug, error, info};

use crate::cli::SelfAction;

pub async fn process_self_action(action: &SelfAction) -> SoarResult<()> {
    let self_bin =
        env::current_exe().with_context(|| "Failed to get executable path".to_string())?;
    let self_version = env!("CARGO_PKG_VERSION");

    debug!("Executable path: {}", self_bin.display());

    match action {
        SelfAction::Update => {
            let is_nightly = self_version.starts_with("nightly");
            debug!("Current version: {}", self_version);

            let target_nightly = match (env::var("SOAR_NIGHTLY"), env::var("SOAR_RELEASE")) {
                (Ok(_), Err(_)) => true,
                (Err(_), Ok(_)) => false,
                _ => is_nightly,
            };

            let handler = ReleaseHandler::<Github>::new();
            let releases = handler
                .fetch_releases::<GithubRelease>("pkgforge/soar", None)
                .await?;

            let release = releases.iter().find(|rel| {
                let is_nightly_release = rel.tag_name().starts_with("nightly");

                debug!(
                    "Checking release: {}, Release Channel: {}",
                    rel.tag_name(),
                    if is_nightly_release {
                        "nightly"
                    } else {
                        "stable"
                    }
                );
                if target_nightly {
                    is_nightly_release && rel.name() != self_version
                } else {
                    let release_version = rel.tag_name().trim_start_matches("v");

                    let parsed_release_version = Version::parse(release_version).ok();
                    let parsed_self_version = Version::parse(self_version).ok();

                    match (parsed_release_version, parsed_self_version) {
                        (Some(release_ver), Some(self_ver)) => {
                            let should_update = !is_nightly_release && release_ver > self_ver;
                            debug!(
                                "Comparing versions: release_ver={}, self_ver={}, should_update={}",
                                release_ver, self_ver, should_update
                            );
                            should_update
                        }
                        (_, None) => is_nightly,
                        _ => {
                            debug!(
                                "Skipping release {} due to invalid version.",
                                release_version
                            );
                            false
                        }
                    }
                }
            });

            if let Some(release) = release {
                if target_nightly != is_nightly {
                    info!(
                        "Switching from {} to {} channel",
                        if is_nightly { "nightly" } else { "stable" },
                        if target_nightly { "nightly" } else { "stable" }
                    );
                } else {
                    info!("Found new update: {}", release.tag_name());
                }
                let assets = release.assets();
                let asset = assets
                    .iter()
                    .find(|a| {
                        a.name.contains(ARCH) && !a.name.contains("tar") && !a.name.contains("sum")
                    })
                    .unwrap();

                debug!("Selected asset: {}", asset.name);

                let downloader = Downloader::default();
                let options = DownloadOptions {
                    url: asset.download_url().to_string(),
                    output_path: Some(self_bin.to_string_lossy().to_string()),
                    progress_callback: None,
                    extract_archive: false,
                    extract_dir: None,
                    file_mode: FileMode::ForceOverwrite,
                    prompt: None,
                };

                debug!("Downloading update from: {}", options.url);
                downloader.download(options).await?;
                info!("Soar updated to {}", release.tag_name());
            } else {
                eprintln!("No updates found.");
            }
        }
        SelfAction::Uninstall => {
            match fs::remove_file(self_bin) {
                Ok(_) => {
                    info!("Soar has been uninstalled successfully.");
                    info!("You should remove soar config and data files manually.");
                }
                Err(err) => {
                    error!("{}\nFailed to uninstall soar.", err.to_string());
                }
            };
        }
    };

    Ok(())
}
