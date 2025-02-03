use std::{
    env::{self, consts::ARCH},
    fs,
};

use soar_core::SoarResult;
use soar_dl::{
    downloader::{DownloadOptions, Downloader},
    github::{Github, GithubRelease},
    platform::{Release, ReleaseAsset, ReleaseHandler},
};
use tracing::{error, info};

use crate::cli::SelfAction;

pub async fn process_self_action(
    action: &SelfAction,
    self_bin: String,
    self_version: &str,
) -> SoarResult<()> {
    match action {
        SelfAction::Update => {
            let is_nightly =
                self_version.starts_with("nightly") || env::var("SOAR_NIGHTLY").is_ok();
            let handler = ReleaseHandler::<Github>::new();
            let releases = handler
                .fetch_releases::<GithubRelease>("pkgforge/soar")
                .await?;

            let release = releases.iter().find(|rel| {
                if is_nightly {
                    rel.tag_name().starts_with("nightly") && rel.name() != self_version
                } else {
                    rel.tag_name().trim_start_matches("v") > self_version
                }
            });

            if let Some(release) = release {
                info!("Found new update: {}", release.tag_name());
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
                    output_path: Some(self_bin),
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
