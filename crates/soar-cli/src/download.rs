use std::{sync::Arc, time::Duration};

use regex::Regex;
use soar_config::config::get_config;
use soar_core::{
    SoarResult, database::models::Package, error::SoarError, package::query::PackageQuery,
};
use soar_db::repository::metadata::MetadataRepository;
use soar_dl::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    forge::Forge,
    oci::OciDownload,
    platform::{PlatformUrl, parse_gitea_target},
    releasekit::Asset,
    types::{OverwriteMode, Progress},
};
use soar_utils::bytes::format_bytes;
use tokio::time::sleep;
use tracing::{error, info};

use crate::utils::{interactive_ask, select_one};

pub struct DownloadContext {
    pub regexes: Vec<Regex>,
    pub globs: Vec<String>,
    pub match_keywords: Vec<String>,
    pub exclude_keywords: Vec<String>,
    pub output: Option<String>,
    pub yes: bool,
    pub progress_callback: Arc<dyn Fn(Progress) + Send + Sync>,
    pub exact_case: bool,
    pub extract: bool,
    pub extract_dir: Option<String>,
    pub skip_existing: bool,
    pub force_overwrite: bool,
}

impl DownloadContext {
    fn get_overwrite_mode(&self) -> OverwriteMode {
        if self.force_overwrite || self.yes {
            OverwriteMode::Force
        } else if self.skip_existing {
            OverwriteMode::Skip
        } else {
            OverwriteMode::Prompt
        }
    }

    fn create_filter(&self) -> Filter {
        Filter {
            regexes: self.regexes.clone(),
            globs: self.globs.clone(),
            include: self.match_keywords.clone(),
            exclude: self.exclude_keywords.clone(),
            case_sensitive: self.exact_case,
        }
    }
}

/// Downloads every link and project given, carrying on past one that fails.
///
/// Each failure is reported as it happens, and any failure makes the whole
/// command fail at the end, so a script sees that not everything arrived.
pub async fn download(
    ctx: DownloadContext,
    links: Vec<String>,
    github: Vec<String>,
    gitlab: Vec<String>,
    codeberg: Vec<String>,
    gitea: Vec<String>,
    ghcr: Vec<String>,
) -> SoarResult<()> {
    let mut total = 0;
    let mut failed = 0;
    let mut record = |result: SoarResult<()>| {
        total += 1;
        if let Err(err) = result {
            error!("{}", err);
            failed += 1;
        }
    };

    for link in &links {
        record(download_link(&ctx, link).await);
    }

    for (forge, projects) in [
        (Forge::GitHub, github),
        (Forge::GitLab, gitlab),
        (Forge::Codeberg, codeberg),
    ] {
        for project in &projects {
            info!("Fetching releases from {}: {}", forge, project);
            let (project, tag) = split_tag(project);
            record(handle_forge_release(&ctx, &forge, project, tag));
        }
    }

    for target in &gitea {
        record(download_gitea(&ctx, target));
    }

    for reference in &ghcr {
        record(handle_oci_download(&ctx, reference).await);
    }

    if failed > 0 {
        return Err(SoarError::Custom(format!(
            "{failed} of {total} downloads failed"
        )));
    }
    Ok(())
}

/// Downloads one link: a URL, a release or OCI reference, or a package name.
async fn download_link(ctx: &DownloadContext, link: &str) -> SoarResult<()> {
    match PlatformUrl::parse(link) {
        Some(PlatformUrl::Direct {
            url,
        }) => {
            info!("Downloading using direct link: {}", url);

            let mut dl = Download::new(url)
                .overwrite(ctx.get_overwrite_mode())
                .extract(ctx.extract);

            if let Some(ref out) = ctx.output {
                dl = dl.output(out);
            }

            if let Some(extract_dir) = ctx.extract_dir.clone() {
                dl = dl.extract_to(extract_dir);
            }

            let cb = ctx.progress_callback.clone();
            dl = dl.progress(move |p| {
                cb(p);
            });

            dl.execute()?;
            Ok(())
        }
        Some(PlatformUrl::Github {
            project,
            tag,
        }) => {
            info!("Detected GitHub URL, processing as GitHub release");
            handle_forge_release(ctx, &Forge::GitHub, &project, tag.as_deref())
        }
        Some(PlatformUrl::Gitlab {
            project,
            tag,
        }) => {
            info!("Detected GitLab URL, processing as GitLab release");
            handle_forge_release(ctx, &Forge::GitLab, &project, tag.as_deref())
        }
        Some(PlatformUrl::Codeberg {
            project,
            tag,
        }) => {
            info!("Detected Codeberg URL, processing as Codeberg release");
            handle_forge_release(ctx, &Forge::Codeberg, &project, tag.as_deref())
        }
        Some(PlatformUrl::Gitea {
            instance,
            project,
            tag,
        }) => {
            info!("Detected Gitea URL, processing as Gitea release");
            let forge = Forge::Gitea {
                instance,
            };
            handle_forge_release(ctx, &forge, &project, tag.as_deref())
        }
        Some(PlatformUrl::Oci {
            reference,
        }) => handle_oci_download(ctx, &reference).await,
        None => {
            // if it's not a url, try to parse it as package
            let (ctx_inner, _) = crate::create_context();
            let metadata_mgr = ctx_inner.metadata_manager().await?;
            let query = PackageQuery::try_from(link)?;

            // Query packages across all repos
            let packages: Vec<Package> = if let Some(ref repo_name) = query.repo_name {
                metadata_mgr
                    .query_repo(repo_name, |conn| {
                        MetadataRepository::find_filtered(
                            conn,
                            query.name.as_deref(),
                            query.pkg_id.as_deref(),
                            query.family.as_deref(),
                            None,
                            None,
                            None,
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
                        query.family.as_deref(),
                        None,
                        None,
                        None,
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

            let packages: Vec<Package> = if let Some(ref version) = query.version {
                packages
                    .into_iter()
                    .filter(|p| p.has_version(version))
                    .collect()
            } else {
                packages
            };

            if packages.is_empty() {
                return Err(SoarError::Custom(format!(
                    "Invalid download resource '{link}'"
                )));
            }

            let mut packages = packages;
            let package = if packages.len() == 1 || ctx.yes {
                packages.swap_remove(0)
            } else {
                select_one(packages, link)?
            };

            let package = package.resolve(query.version.as_deref());

            info!("Downloading package: {}", package.pkg_name);
            if let Some(ref url) = package.ghcr_blob {
                let mut dl = OciDownload::new(url.as_str()).overwrite(ctx.get_overwrite_mode());

                if let Some(ref out) = ctx.output {
                    dl = dl.output(out);
                }

                let cb = ctx.progress_callback.clone();
                dl = dl.progress(move |p| {
                    cb(p);
                });

                dl.execute()?;
            } else {
                let mut dl =
                    Download::new(&package.download_url).overwrite(ctx.get_overwrite_mode());

                if let Some(ref out) = ctx.output {
                    dl = dl.output(out);
                }

                let cb = ctx.progress_callback.clone();
                dl = dl.progress(move |p| {
                    cb(p);
                });

                dl.execute()?;
            }
            Ok(())
        }
    }
}

async fn handle_oci_download(ctx: &DownloadContext, reference: &str) -> SoarResult<()> {
    info!("Downloading using OCI reference: {}", reference);

    let mut dl = OciDownload::new(reference)
        .filter(ctx.create_filter())
        .parallel(get_config().ghcr_concurrency.unwrap_or(8))
        .overwrite(ctx.get_overwrite_mode());

    if let Some(ref output) = ctx.output {
        dl = dl.output(output);
    }

    let cb = ctx.progress_callback.clone();
    dl = dl.progress(move |p| {
        cb(p);
    });

    let mut retries = 0;
    let max_retries = 5;

    loop {
        match dl.clone().execute() {
            Ok(_) => {
                info!("Download completed successfully");
                break;
            }
            Err(err) if err.is_transient() && retries < max_retries => {
                retries += 1;
                info!("Retrying... ({}/{})", retries, max_retries);
                ctx.progress_callback.clone()(Progress::Recovered);
                sleep(Duration::from_secs(5)).await;
            }
            Err(err) => {
                ctx.progress_callback.clone()(Progress::Error);
                return Err(err.into());
            }
        }
    }

    Ok(())
}

/// Downloads an asset of a release published on `forge`.
///
/// Without a tag the newest release that is not a prerelease is taken.
fn handle_forge_release(
    ctx: &DownloadContext,
    forge: &Forge,
    project: &str,
    tag: Option<&str>,
) -> SoarResult<()> {
    let releases = forge.fetch_releases(project, tag)?;

    let release = if let Some(tag) = tag {
        releases.iter().find(|r| r.tag() == tag)
    } else {
        releases
            .iter()
            .find(|r| !r.is_prerelease())
            .or_else(|| releases.first())
    };

    let release = release.ok_or_else(|| {
        DownloadError::ReleaseNotFound {
            tag: tag.map(String::from),
        }
    })?;

    info!("Found release: {}", release.tag());
    let filter = ctx.create_filter();

    let assets: Vec<_> = release
        .assets()
        .iter()
        .filter(|a| filter.matches(a.name()))
        .collect();

    if assets.is_empty() {
        let available = release
            .assets()
            .iter()
            .map(|a| a.name().to_string())
            .collect::<Vec<String>>();

        Err(DownloadError::NoMatch {
            available,
        })?
    }

    let selected_asset = if assets.len() == 1 || ctx.yes {
        assets[0]
    } else {
        &select_asset_interactively(assets)?
    };

    info!("Downloading asset: {}", selected_asset.name());

    let mut dl = Download::new(selected_asset.url())
        .overwrite(ctx.get_overwrite_mode())
        .extract(ctx.extract);

    if let Some(ref out) = ctx.output {
        dl = dl.output(out);
    }

    if let Some(ref extract_dir) = ctx.extract_dir {
        dl = dl.extract_to(extract_dir);
    }

    let cb = ctx.progress_callback.clone();
    dl = dl.progress(move |p| {
        cb(p);
    });

    dl.execute()?;

    Ok(())
}

pub fn create_regex_patterns(regex_patterns: Option<Vec<String>>) -> SoarResult<Vec<Regex>> {
    match regex_patterns {
        Some(patterns) => {
            patterns
                .iter()
                .map(|pattern| Regex::new(pattern).map_err(|err| err.into()))
                .collect()
        }
        None => Ok(Vec::new()),
    }
}

/// Downloads from a Gitea or Forgejo repository URL.
///
/// The instance is not one soar knows by name, so the target carries its own:
/// `https://git.example.com/owner/repo@v1.0`.
fn download_gitea(ctx: &DownloadContext, target: &str) -> SoarResult<()> {
    let (instance, project, tag) = parse_gitea_target(target, false)
        .ok_or_else(|| SoarError::Custom(format!("Invalid Gitea repository URL '{target}'")))?;

    info!("Fetching releases from {}: {}", instance, project);

    let forge = Forge::Gitea {
        instance,
    };
    handle_forge_release(ctx, &forge, &project, tag.as_deref())
}

/// A project reference split from the tag written after its `@`.
fn split_tag(project: &str) -> (&str, Option<&str>) {
    match project.trim().split_once('@') {
        Some((proj, tag)) if !tag.trim().is_empty() => (proj, Some(tag.trim())),
        _ => (project.trim().trim_end_matches('@'), None),
    }
}

fn select_asset_interactively(assets: Vec<&Asset>) -> SoarResult<Asset> {
    info!("\nAvailable assets:");
    for (i, asset) in assets.iter().enumerate() {
        let size = asset
            .size()
            .map(|s| format!(" ({})", format_bytes(s, 2)))
            .unwrap_or_default();
        info!("  {}. {}{}", i + 1, asset.name(), size);
    }

    loop {
        let max = assets.len();
        let response = interactive_ask(&format!("Select an asset (1-{}): ", max))?;
        match response.trim().parse::<usize>() {
            Ok(n) if n > 0 && n <= max => return Ok(assets[n - 1].clone()),
            _ => error!("Invalid selection, please try again."),
        }
    }
}
