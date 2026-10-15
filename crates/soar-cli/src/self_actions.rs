use std::{
    env::{self, consts::ARCH},
    fs,
};

use semver::Version;
use soar_core::{
    SoarResult,
    error::{ErrorContext, SoarError},
};
use soar_dl::{
    download::Download, error::describe_request_error, forge::Forge, http_client::SHARED_AGENT,
    types::OverwriteMode,
};
use soar_utils::bytes::format_bytes;
use tracing::{debug, error, info};

use crate::{
    cli::SelfAction,
    progress::{create_download_job, handle_download_progress},
    utils::confirm_action,
};

pub async fn process_self_action(action: &SelfAction) -> SoarResult<()> {
    let self_bin =
        env::current_exe().with_context(|| "Failed to get executable path".to_string())?;
    let self_version = env!("CARGO_PKG_VERSION");

    debug!("Executable path: {}", self_bin.display());

    match action {
        SelfAction::Update {
            yes,
        } => {
            let is_nightly = self_version.starts_with("nightly");
            debug!("Current version: {}", self_version);

            let target_nightly = match (env::var("SOAR_NIGHTLY"), env::var("SOAR_RELEASE")) {
                (Ok(_), Err(_)) => true,
                (Err(_), Ok(_)) => false,
                _ => is_nightly,
            };

            let releases = Forge::GitHub.fetch_releases("pkgforge/soar", None)?;

            let release = releases.iter().find(|release| {
                let is_nightly_release = release.tag().starts_with("nightly");
                debug!(
                    "Checking release: {}, Release Channel: {}",
                    release.tag(),
                    if is_nightly_release {
                        "nightly"
                    } else {
                        "stable"
                    }
                );

                if target_nightly {
                    is_nightly_release && release.name() != Some(self_version)
                } else {
                    let release_version = release.tag().trim_start_matches("v");
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
                let new_version = release.tag();

                if let Some(body) = release.body() {
                    if !body.is_empty() {
                        println!("\nRelease Notes for {}:\n", new_version);
                        println!("{}", body);
                        println!();
                    } else {
                        println!("No release notes available for this update.");
                    }
                } else {
                    println!("No release notes available for this update.");
                }

                if !yes && !confirm_action(&format!("Update to {new_version}?"))? {
                    info!("Update cancelled.");
                    return Ok(());
                }

                if target_nightly != is_nightly {
                    info!(
                        "Switching from {} to {} channel",
                        if is_nightly { "nightly" } else { "stable" },
                        if target_nightly { "nightly" } else { "stable" }
                    );
                } else {
                    info!("Found new update: {}", release.tag());
                }
                let assets = release.assets();
                let asset = assets
                    .iter()
                    .find(|a| {
                        a.name().contains(ARCH)
                            && !a.name().contains("tar")
                            && !a.name().contains("sum")
                    })
                    .ok_or_else(|| {
                        SoarError::Custom(format!("No matching asset found for {}", ARCH))
                    })?;

                debug!("Selected asset: {}", asset.name());

                if let Some(size) = asset.size() {
                    info!("Download size: {}", format_bytes(size, 2));
                }

                let pb = create_download_job("Downloading");

                // Stage the download next to the target and swap it in atomically,
                // so an interrupted or corrupt update never replaces the running
                // binary with a truncated file.
                let staging = {
                    let mut path = self_bin.clone();
                    let mut name = path.file_name().unwrap_or_default().to_os_string();
                    name.push(".update.part");
                    path.set_file_name(name);
                    path
                };

                // The running binary is only ever replaced by bytes that match
                // the checksum published beside them.
                let checksum_name = format!("{}.b3sum", asset.name());
                let checksum_asset = assets
                    .iter()
                    .find(|a| a.name() == checksum_name)
                    .ok_or_else(|| {
                        SoarError::Custom(format!(
                            "No checksum published for {}; refusing to update unverified",
                            asset.name()
                        ))
                    })?;
                let expected_checksum = fetch_expected_checksum(checksum_asset.url())
                    .map_err(|err| {
                        SoarError::Custom(format!(
                            "Failed to fetch published checksum {checksum_name}: {err}; aborting update"
                        ))
                    })?;

                let dl = Download::new(asset.url())
                    .output(staging.to_string_lossy())
                    .overwrite(OverwriteMode::Force)
                    .checksum(expected_checksum)
                    .progress({
                        let pb = pb.clone();
                        move |p| handle_download_progress(p, &pb)
                    });

                debug!("Downloading update from: {}", asset.url());
                match dl.execute() {
                    Ok(_) => {
                        fs::rename(&staging, &self_bin).with_context(|| {
                            format!("replacing {} with the updated binary", self_bin.display())
                        })?;
                        info!("Soar updated to {}", release.tag());
                    }
                    Err(err) => {
                        let _ = fs::remove_file(&staging);
                        return Err(err.into());
                    }
                }
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

/// Fetches a published `.b3sum` asset and extracts the BLAKE3 hex digest.
fn fetch_expected_checksum(url: &str) -> Result<String, String> {
    let body = SHARED_AGENT
        .get(url)
        .call()
        .map_err(|err| describe_request_error(&err, url))?
        .into_body()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    parse_b3sum(&body).ok_or_else(|| "it holds no BLAKE3 digest".to_string())
}

/// The digest in a `b3sum` file, which follows the `<hex>  <filename>`
/// convention.
fn parse_b3sum(body: &str) -> Option<String> {
    body.split_whitespace()
        .next()
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_b3sum_file_yields_its_digest() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_b3sum(&format!("{digest}  soar-x86_64-linux\n")),
            Some(digest)
        );
        assert_eq!(parse_b3sum(""), None);
        assert_eq!(parse_b3sum("<html>not found</html>"), None);
        assert_eq!(parse_b3sum(&"z".repeat(64)), None);
    }
}
