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
};
use tracing::{error, info};

use crate::cli::SelfAction;

pub async fn process_self_action(action: &SelfAction) -> SoarResult<()> {
    let self_bin =
        env::current_exe().with_context(|| "Failed to get executable path".to_string())?;
    let self_version = env!("CARGO_PKG_VERSION");

    match action {
        SelfAction::Update => {
            let is_nightly = self_version.starts_with("nightly");

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
                if target_nightly {
                    is_nightly_release && rel.name() != self_version
                } else {
                    let release_version = rel.tag_name().trim_start_matches("v");

                    let release_version = Version::parse(release_version).ok();
                    let self_version = Version::parse(self_version).ok();

                    match (release_version, self_version) {
                        (Some(release_ver), Some(self_ver)) => {
                            !is_nightly_release && (is_nightly || release_ver > self_ver)
                        }
                        _ => false,
                    }
                }
            });

            if let Some(release) = release {
                if target_nightly != is_nightly {
                    println!(
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
                let downloader = Downloader::default();
                let options = DownloadOptions {
                    url: asset.download_url().to_string(),
                    output_path: Some(self_bin.to_string_lossy().to_string()),
                    progress_callback: None,
                };
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
