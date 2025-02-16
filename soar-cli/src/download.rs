use std::{sync::Arc, thread::sleep, time::Duration};

use indicatif::HumanBytes;
use regex::Regex;
use reqwest::StatusCode;
use serde::Deserialize;
use soar_core::{
    config::get_config,
    database::{models::Package, packages::PackageQueryBuilder},
    package::query::PackageQuery,
    SoarResult,
};
use soar_dl::{
    downloader::{DownloadOptions, DownloadState, Downloader, OciDownloadOptions, OciDownloader},
    error::DownloadError,
    github::{Github, GithubAsset, GithubRelease},
    gitlab::{Gitlab, GitlabAsset, GitlabRelease},
    platform::{
        PlatformDownloadOptions, PlatformUrl, Release, ReleaseAsset, ReleaseHandler,
        ReleasePlatform,
    },
};
use tracing::{error, info};

use crate::{
    progress::{self, create_progress_bar},
    state::AppState,
    utils::{interactive_ask, select_package_interactively},
};

pub struct DownloadContext {
    regex_patterns: Vec<Regex>,
    match_keywords: Vec<String>,
    exclude_keywords: Vec<String>,
    output: Option<String>,
    yes: bool,
    progress_callback: Arc<dyn Fn(DownloadState) + Send + Sync>,
    exact_case: bool,
}

pub async fn download(
    links: Vec<String>,
    github: Vec<String>,
    gitlab: Vec<String>,
    ghcr: Vec<String>,
    regex_patterns: Option<Vec<String>>,
    match_keywords: Option<Vec<String>>,
    exclude_keywords: Option<Vec<String>>,
    output: Option<String>,
    yes: bool,
    exact_case: bool,
) -> SoarResult<()> {
    let progress_bar = create_progress_bar();
    let progress_callback = Arc::new(move |state| progress::handle_progress(state, &progress_bar));
    let regex_patterns = create_regex_patterns(regex_patterns);
    let match_keywords = match_keywords.unwrap_or_default();
    let exclude_keywords = exclude_keywords.unwrap_or_default();

    let ctx = DownloadContext {
        regex_patterns,
        match_keywords,
        exclude_keywords,
        output: output.clone(),
        yes,
        progress_callback: progress_callback.clone(),
        exact_case,
    };

    handle_direct_downloads(&ctx, links, output.clone(), progress_callback.clone()).await?;

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
    progress_callback: Arc<dyn Fn(DownloadState) + Send + Sync>,
) -> SoarResult<()> {
    let downloader = Downloader::default();

    for link in &links {
        match PlatformUrl::parse(link) {
            Ok(PlatformUrl::DirectUrl(url)) => {
                info!("Downloading using direct link: {}", url);

                let options = DownloadOptions {
                    url: link.clone(),
                    output_path: output.clone(),
                    progress_callback: Some(progress_callback.clone()),
                };
                let _ = downloader
                    .download(options)
                    .await
                    .map_err(|e| eprintln!("{}", e));
            }
            Ok(PlatformUrl::Github(project)) => {
                info!("Detected GitHub URL, processing as GitHub release");
                let handler = ReleaseHandler::<Github>::new();
                if let Err(e) = handle_platform_download::<Github, GithubRelease, GithubAsset>(
                    ctx, &handler, &project,
                )
                .await
                {
                    eprintln!("{}", e);
                }
            }
            Ok(PlatformUrl::Gitlab(project)) => {
                info!("Detected GitLab URL, processing as GitLab release");
                let handler = ReleaseHandler::<Gitlab>::new();
                if let Err(e) = handle_platform_download::<Gitlab, GitlabRelease, GitlabAsset>(
                    ctx, &handler, &project,
                )
                .await
                {
                    eprintln!("{}", e);
                }
            }
            Ok(PlatformUrl::Oci(url)) => {
                if let Err(e) = handle_oci_download(ctx, &url).await {
                    eprintln!("{}", e);
                };
            }
            Err(_) => {
                // if it's not a url, try to parse it as package
                let state = AppState::new();
                let repo_db = state.repo_db().await?;
                let query = PackageQuery::try_from(link.as_str())?;
                let builder = PackageQueryBuilder::new(repo_db.clone());
                let builder = query.apply_filters(builder);
                let packages: Vec<Package> = builder.load()?.items;

                if packages.is_empty() {
                    eprintln!("Invalid download resource '{}'", link);
                    break;
                }

                let package = if packages.len() > 1 {
                    &select_package_interactively(packages, link)?.unwrap()
                } else {
                    packages.first().unwrap()
                };

                info!(
                    "Downloading package: {}#{}",
                    package.pkg_name, package.pkg_id
                );
                if let Some(ref url) = package.ghcr_blob {
                    let options = OciDownloadOptions {
                        url: url.to_string(),
                        output_path: None,
                        progress_callback: Some(progress_callback.clone()),
                        api: None,
                        concurrency: Some(1),
                        regex_patterns: Vec::new(),
                        exclude_keywords: Vec::new(),
                        match_keywords: Vec::new(),
                        exact_case: false,
                    };

                    let mut downloader = OciDownloader::new(options);

                    downloader.download_oci().await?;
                } else {
                    let downloader = Downloader::default();
                    let options = DownloadOptions {
                        url: package.download_url.clone(),
                        output_path: None,
                        progress_callback: Some(progress_callback.clone()),
                    };

                    downloader.download(options).await?;
                }
            }
        };
    }

    Ok(())
}

async fn handle_oci_download(ctx: &DownloadContext, reference: &str) -> SoarResult<()> {
    info!("Downloading using OCI reference: {}", reference);

    let options = OciDownloadOptions {
        url: reference.to_string(),
        output_path: ctx.output.clone(),
        progress_callback: Some(ctx.progress_callback.clone()),
        api: None,
        regex_patterns: ctx.regex_patterns.clone(),
        concurrency: get_config().ghcr_concurrency,
        match_keywords: ctx.match_keywords.clone(),
        exclude_keywords: ctx.exclude_keywords.clone(),
        exact_case: ctx.exact_case,
    };

    let mut downloader = OciDownloader::new(options);
    let mut retries = 0;
    loop {
        if retries > 5 {
            eprintln!("Max retries exhausted. Aborting.");
            break;
        }
        match downloader.download_oci().await {
            Ok(_) => break,
            Err(
                DownloadError::ResourceError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    ..
                }
                | DownloadError::ChunkError,
            ) => sleep(Duration::from_secs(5)),
            Err(err) => {
                eprintln!("{}", err);
                break;
            }
        };
        retries += 1;
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

fn create_regex_patterns(regex_patterns: Option<Vec<String>>) -> Vec<Regex> {
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

fn create_platform_options(ctx: &DownloadContext, tag: Option<String>) -> PlatformDownloadOptions {
    PlatformDownloadOptions {
        output_path: ctx.output.clone(),
        progress_callback: Some(ctx.progress_callback.clone()),
        tag,
        regex_patterns: ctx.regex_patterns.clone(),
        match_keywords: ctx.match_keywords.clone(),
        exclude_keywords: ctx.exclude_keywords.clone(),
        exact_case: ctx.exact_case,
    }
}

async fn handle_platform_download<P: ReleasePlatform, R, A>(
    ctx: &DownloadContext,
    handler: &ReleaseHandler<P>,
    project: &str,
) -> SoarResult<()>
where
    R: Release<A> + for<'de> Deserialize<'de>,
    A: ReleaseAsset + Clone,
{
    let (project, tag) = match project.trim().split_once('@') {
        Some((proj, tag)) if !tag.trim().is_empty() => (proj, Some(tag.trim())),
        _ => (project.trim_end_matches('@'), None),
    };

    let options = create_platform_options(&ctx, tag.map(String::from));
    let releases = handler.fetch_releases::<R>(project).await?;
    let assets = handler.filter_releases(&releases, &options).await?;

    let selected_asset = if assets.len() == 1 || ctx.yes {
        assets[0].clone()
    } else {
        select_asset(&assets)?
    };

    info!("Downloading asset from {}", selected_asset.download_url());
    handler.download(&selected_asset, options.clone()).await?;
    Ok(())
}

pub async fn handle_github_downloads(
    ctx: &DownloadContext,
    projects: Vec<String>,
) -> SoarResult<()> {
    let handler = ReleaseHandler::<Github>::new();
    for project in &projects {
        info!("Fetching releases from GitHub: {}", project);
        if let Err(e) =
            handle_platform_download::<_, GithubRelease, _>(ctx, &handler, project).await
        {
            eprintln!("{}", e);
        }
    }
    Ok(())
}

pub async fn handle_gitlab_downloads(
    ctx: &DownloadContext,
    projects: Vec<String>,
) -> SoarResult<()> {
    let handler = ReleaseHandler::<Gitlab>::new();
    for project in &projects {
        info!("Fetching releases from GitLab: {}", project);
        if let Err(e) =
            handle_platform_download::<_, GitlabRelease, _>(ctx, &handler, project).await
        {
            eprintln!("{}", e);
        }
    }
    Ok(())
}

fn select_asset<A>(assets: &[A]) -> SoarResult<A>
where
    A: Clone,
    A: ReleaseAsset,
{
    info!("\nAvailable assets:");
    for (i, asset) in assets.iter().enumerate() {
        let size = asset
            .size()
            .map(|s| format!(" ({})", HumanBytes(s)))
            .unwrap_or_default();
        info!("{}. {}{}", i + 1, asset.name(), size);
    }

    loop {
        let max = assets.len();
        let response = interactive_ask(&format!("Select an asset (1-{max}): "))?;
        match response.parse::<usize>() {
            Ok(n) if n > 0 && n <= max => return Ok(assets[n - 1].clone()),
            _ => error!("Invalid selection, please try again."),
        }
    }
}
