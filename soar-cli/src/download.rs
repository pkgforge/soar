use std::{sync::Arc, thread::sleep, time::Duration};

use indicatif::HumanBytes;
use regex::Regex;
use soar_core::{
    config::get_config,
    database::{models::Package, packages::PackageQueryBuilder},
    package::query::PackageQuery,
    SoarResult,
};
use soar_dl::{
    download::Download,
    error::DownloadError,
    filter::Filter,
    github::Github,
    gitlab::GitLab,
    oci::OciDownload,
    platform::PlatformUrl,
    release::ReleaseDownload,
    traits::{Asset, Platform as _, Release as _},
    types::{OverwriteMode, Progress},
};
use tracing::{error, info};

use crate::{
    state::AppState,
    utils::{interactive_ask, select_package_interactively},
};

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

pub async fn download(
    ctx: DownloadContext,
    links: Vec<String>,
    github: Vec<String>,
    gitlab: Vec<String>,
    ghcr: Vec<String>,
    progress_callback: Arc<dyn Fn(Progress) + Send + Sync>,
) -> SoarResult<()> {
    handle_direct_downloads(&ctx, links, ctx.output.clone(), progress_callback.clone()).await?;

    if !github.is_empty() {
        handle_github_downloads(&ctx, github).await?;
    }

    if !gitlab.is_empty() {
        handle_gitlab_downloads(&ctx, gitlab).await?;
    }

    if !ghcr.is_empty() {
        handle_oci_downloads(&ctx, ghcr).await?;
    }

    Ok(())
}

pub async fn handle_direct_downloads(
    ctx: &DownloadContext,
    links: Vec<String>,
    output: Option<String>,
    progress_callback: Arc<dyn Fn(Progress) + Send + Sync>,
) -> SoarResult<()> {
    for link in &links {
        match PlatformUrl::parse(link) {
            Some(PlatformUrl::Direct {
                url,
            }) => {
                info!("Downloading using direct link: {}", url);

                let mut dl = Download::new(url)
                    .overwrite(ctx.get_overwrite_mode())
                    .extract(ctx.extract);

                if let Some(ref out) = output {
                    dl = dl.output(out);
                }

                if let Some(extract_dir) = ctx.extract_dir.clone() {
                    dl = dl.extract_to(extract_dir);
                }

                let cb = ctx.progress_callback.clone();
                dl = dl.progress(move |p| {
                    cb(p);
                });

                if let Err(err) = dl.execute() {
                    error!("{}", err);
                }
            }
            Some(PlatformUrl::Github {
                project,
                tag,
            }) => {
                info!("Detected GitHub URL, processing as GitHub release");
                if let Err(err) = handle_github_release(ctx, &project, tag.as_deref()) {
                    error!("{}", err);
                }
            }
            Some(PlatformUrl::Gitlab {
                project,
                tag,
            }) => {
                info!("Detected GitLab URL, processing as GitLab release");
                if let Err(err) = handle_gitlab_release(ctx, &project, tag.as_deref()) {
                    error!("{}", err);
                }
            }
            Some(PlatformUrl::Oci {
                reference,
            }) => {
                if let Err(err) = handle_oci_download(ctx, &reference).await {
                    error!("{}", err);
                };
            }
            None => {
                // if it's not a url, try to parse it as package
                let state = AppState::new();
                let repo_db = state.repo_db().await?;
                let query = PackageQuery::try_from(link.as_str())?;
                let builder = PackageQueryBuilder::new(repo_db.clone());
                let builder = query.apply_filters(builder);
                let packages: Vec<Package> = builder.load()?.items;

                if packages.is_empty() {
                    error!("Invalid download resource '{}'", link);
                    break;
                }

                let package = if packages.len() == 1 || ctx.yes {
                    packages.first().unwrap()
                } else {
                    &select_package_interactively(packages, link)?.unwrap()
                };

                info!(
                    "Downloading package: {}#{}",
                    package.pkg_name, package.pkg_id
                );
                if let Some(ref url) = package.ghcr_blob {
                    let mut dl = OciDownload::new(url.as_str()).overwrite(OverwriteMode::Force);

                    if let Some(ref out) = output {
                        dl = dl.output(out);
                    }

                    let cb = progress_callback.clone();
                    dl = dl.progress(move |p| {
                        cb(p);
                    });

                    if let Err(err) = dl.execute() {
                        error!("{}", err);
                    }
                } else {
                    let mut dl =
                        Download::new(&package.download_url).overwrite(OverwriteMode::Force);

                    if let Some(ref out) = output {
                        dl = dl.output(out);
                    }

                    let cb = progress_callback.clone();
                    dl = dl.progress(move |p| {
                        cb(p);
                    });

                    dl.execute()?;
                }
            }
        };
    }

    Ok(())
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
            Err(err)
                if matches!(
                    err,
                    DownloadError::HttpError {
                        status: 429,
                        ..
                    } | DownloadError::Network(_)
                ) && retries < max_retries =>
            {
                retries += 1;
                info!("Retrying... ({}/{})", retries, max_retries);
                ctx.progress_callback.clone()(Progress::Recovered);
                sleep(Duration::from_secs(5));
            }
            Err(err) => {
                ctx.progress_callback.clone()(Progress::Error);
                error!("Download failed: {}", err);
                return Err(err)?;
            }
        }
    }

    Ok(())
}

pub async fn handle_oci_downloads(
    ctx: &DownloadContext,
    references: Vec<String>,
) -> SoarResult<()> {
    for reference in &references {
        handle_oci_download(ctx, reference).await?;
    }
    Ok(())
}

fn handle_github_release(
    ctx: &DownloadContext,
    project: &str,
    tag: Option<&str>,
) -> SoarResult<()> {
    let releases = Github::fetch_releases(project, tag)?;

    let release = if let Some(tag) = tag {
        releases.iter().find(|r| r.tag() == tag)
    } else {
        releases
            .iter()
            .find(|r| !r.is_prerelease())
            .or_else(|| releases.first())
    };

    let release = release.ok_or_else(|| DownloadError::InvalidResponse)?;

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

fn handle_gitlab_release(
    ctx: &DownloadContext,
    project: &str,
    tag: Option<&str>,
) -> SoarResult<()> {
    let mut dl = ReleaseDownload::<GitLab>::new(project)
        .filter(ctx.create_filter())
        .overwrite(ctx.get_overwrite_mode())
        .extract(ctx.extract);

    if let Some(tag) = tag {
        dl = dl.tag(tag);
    }

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

    let paths = dl.execute()?;

    if paths.len() > 1 && !ctx.yes {
        info!("Multiple assets found, please select one:");
        for (i, path) in paths.iter().enumerate() {
            info!("{}. {}", i + 1, path.display());
        }
    }

    Ok(())
}

pub fn create_regex_patterns(regex_patterns: Option<Vec<String>>) -> Vec<Regex> {
    regex_patterns
        .clone()
        .map(|patterns| {
            patterns
                .iter()
                .map(|pattern| Regex::new(pattern))
                .collect::<Result<Vec<Regex>, regex::Error>>()
        })
        .transpose()
        .unwrap()
        .unwrap_or_default()
}

pub async fn handle_github_downloads(
    ctx: &DownloadContext,
    projects: Vec<String>,
) -> SoarResult<()> {
    for project in &projects {
        info!("Fetching releases from GitHub: {}", project);

        let (project, tag) = match project.trim().split_once('@') {
            Some((proj, tag)) if !tag.trim().is_empty() => (proj, Some(tag.trim())),
            _ => (project.trim_end_matches('@'), None),
        };

        if let Err(err) = handle_github_release(ctx, project, tag) {
            error!("{}", err);
        }
    }
    Ok(())
}

pub async fn handle_gitlab_downloads(
    ctx: &DownloadContext,
    projects: Vec<String>,
) -> SoarResult<()> {
    for project in &projects {
        info!("Fetching releases from GitLab: {}", project);

        let (project, tag) = match project.trim().split_once('@') {
            Some((proj, tag)) if !tag.trim().is_empty() => (proj, Some(tag.trim())),
            _ => (project.trim_end_matches('@'), None),
        };

        if let Err(err) = handle_gitlab_release(ctx, project, tag) {
            error!("{}", err);
        }
    }
    Ok(())
}

fn select_asset_interactively<A>(assets: Vec<&A>) -> SoarResult<A>
where
    A: Asset + Clone,
{
    info!("\nAvailable assets:");
    for (i, asset) in assets.iter().enumerate() {
        let size = asset
            .size()
            .map(|s| format!(" ({})", HumanBytes(s)))
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
