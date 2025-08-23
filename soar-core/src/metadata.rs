use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};

use reqwest::header::{self, HeaderMap};
use rusqlite::Connection;
use tracing::info;

use crate::{
    config::{self, Repository},
    constants::{METADATA_MIGRATIONS, SQLITE_MAGIC_BYTES, ZST_MAGIC_BYTES},
    database::{
        connection::Database, migration::MigrationManager, models::RemotePackage,
        nests::models::Nest,
    },
    error::{ErrorContext, SoarError},
    utils::{calc_magic_bytes, get_platform},
    SoarResult,
};

fn construct_nest_url(url: &str) -> SoarResult<reqwest::Url> {
    let url = if let Some(repo) = url.strip_prefix("github:") {
        let platform = get_platform();
        format!(
            "https://github.com/{}/releases/download/soar-nest/{}.json",
            repo, platform
        )
    } else {
        url.to_string()
    };
    reqwest::Url::parse(&url).map_err(|err| SoarError::Custom(err.to_string()))
}

pub async fn fetch_nest_metadata(
    nest: &Nest,
    force: bool,
    nests_repo_path: &Path,
) -> SoarResult<Option<String>> {
    let nest_path = nests_repo_path.join(&nest.name);
    let metadata_db = nest_path.join("metadata.db");

    if !metadata_db.exists() {
        fs::create_dir_all(&nest_path)
            .with_context(|| format!("creating directory {}", nest_path.display()))?;
    }

    let etag = if metadata_db.exists() {
        let conn = Connection::open(&metadata_db)?;
        let etag: String = conn
            .query_row("SELECT etag FROM repository", [], |row| row.get(0))
            .unwrap_or_default();

        if !force && !etag.is_empty() {
            let file_info = metadata_db
                .metadata()
                .with_context(|| format!("reading file metadata from {}", metadata_db.display()))?;
            let sync_interval = config::get_config().get_nests_sync_interval();
            if let Ok(created) = file_info.created() {
                if sync_interval >= created.elapsed()?.as_millis() {
                    return Ok(None);
                }
            }
        }
        drop(conn);
        etag
    } else {
        String::new()
    };

    let url = construct_nest_url(&nest.url)?;

    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());
    if !etag.is_empty() {
        headers.insert(header::IF_NONE_MATCH, etag.parse().unwrap());
    }

    let resp = client.get(url.clone()).headers(headers).send().await?;

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let etag = resp
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| SoarError::Custom("etag not found in metadata response".to_string()))?;

    info!("Fetching nest from {}", url);

    let content = resp.bytes().await?.to_vec();

    let nest_name = format!("nest-{}", nest.name);
    process_metadata_content(content, &metadata_db, &nest_name, &nest.url)?;

    Ok(Some(etag))
}

fn process_metadata_content(
    content: Vec<u8>,
    metadata_db_path: &Path,
    repo_name: &str,
    repo_url: &str,
) -> SoarResult<()> {
    if content.len() < 4 {
        return Err(SoarError::Custom("Metadata content is too short".into()));
    }
    if content[..4] == ZST_MAGIC_BYTES {
        let tmp_path = format!("{}.part", metadata_db_path.display());
        let mut tmp_file = File::create(&tmp_path)
            .with_context(|| format!("creating temporary file {tmp_path}"))?;

        let mut decoder = zstd::Decoder::new(content.as_slice())
            .with_context(|| "creating zstd decoder".to_string())?;
        io::copy(&mut decoder, &mut tmp_file)
            .with_context(|| format!("decoding zstd from {tmp_path}"))?;

        let magic_bytes = calc_magic_bytes(&tmp_path, 4)?;
        if magic_bytes == SQLITE_MAGIC_BYTES {
            fs::rename(&tmp_path, metadata_db_path).with_context(|| {
                format!("renaming {} to {}", tmp_path, metadata_db_path.display())
            })?;
        } else {
            let tmp_file = File::open(&tmp_path)
                .with_context(|| format!("opening temporary file {tmp_path}"))?;
            let reader = BufReader::new(tmp_file);
            let metadata: Vec<RemotePackage> = serde_json::from_reader(reader).map_err(|err| {
                SoarError::Custom(format!(
                    "Failed to parse JSON metadata from {tmp_path}: {err:#?}",
                ))
            })?;

            handle_json_metadata(&metadata, metadata_db_path, repo_name)?;
            fs::remove_file(tmp_path.clone())
                .with_context(|| format!("removing temporary file {tmp_path}"))?;
        }
    } else if content[..4] == SQLITE_MAGIC_BYTES {
        let mut writer =
            BufWriter::new(File::create(metadata_db_path).with_context(|| {
                format!("creating metadata file {}", metadata_db_path.display())
            })?);
        writer
            .write_all(&content)
            .with_context(|| format!("writing to metadata file {}", metadata_db_path.display()))?;
    } else {
        let remote_metadata: Vec<RemotePackage> =
            serde_json::from_slice(&content).map_err(|err| {
                SoarError::Custom(format!(
                    "Failed to parse JSON metadata response from {}: {:#?}",
                    repo_url, err
                ))
            })?;

        handle_json_metadata(&remote_metadata, metadata_db_path, repo_url)?;
    }
    Ok(())
}

fn handle_json_metadata<P: AsRef<Path>>(
    metadata: &[RemotePackage],
    metadata_db: P,
    repo_name: &str,
) -> SoarResult<()> {
    let metadata_db = metadata_db.as_ref();
    if metadata_db.exists() {
        fs::remove_file(metadata_db)
            .with_context(|| format!("removing metadata file {}", metadata_db.display()))?;
    }

    let conn = Connection::open(metadata_db)?;
    let mut manager = MigrationManager::new(conn)?;
    manager.migrate_from_dir(METADATA_MIGRATIONS)?;

    let db = Database::new(metadata_db)?;
    db.from_remote_metadata(metadata.as_ref(), repo_name)?;

    Ok(())
}

pub async fn fetch_public_key<P: AsRef<Path>>(
    client: &reqwest::Client,
    repo_path: P,
    pubkey_url: &str,
) -> SoarResult<()> {
    let repo_path = repo_path.as_ref();
    let pubkey_file = repo_path.join("minisign.pub");

    if pubkey_file.exists() {
        // skip if we already have the public key file
        return Ok(());
    }

    let resp = client.get(pubkey_url).send().await?;

    info!("Fetching public key from {}", pubkey_url);

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", pubkey_url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let content = resp.bytes().await?;
    fs::write(&pubkey_file, content)
        .with_context(|| format!("writing minisign key {}", pubkey_file.display()))?;

    Ok(())
}

pub async fn fetch_metadata(repo: Repository, force: bool) -> SoarResult<Option<String>> {
    let repo_path = repo.get_path()?;
    let metadata_db = repo_path.join("metadata.db");

    if !metadata_db.exists() {
        fs::create_dir_all(&repo_path)
            .with_context(|| format!("creating directory {}", repo_path.display()))?;
    }

    let etag = if metadata_db.exists() {
        let conn = Connection::open(&metadata_db)?;
        let etag: String = conn
            .query_row("SELECT etag FROM repository", [], |row| row.get(0))
            .unwrap_or_default();

        if !force && !etag.is_empty() {
            let file_info = metadata_db
                .metadata()
                .with_context(|| format!("reading file metadata from {}", metadata_db.display()))?;
            if let Ok(created) = file_info.created() {
                if repo.sync_interval() >= created.elapsed()?.as_millis() {
                    return Ok(None);
                }
            }
        }
        drop(conn);
        etag
    } else {
        String::new()
    };

    let url = reqwest::Url::parse(&repo.url).map_err(|err| SoarError::Custom(err.to_string()))?;

    let client = reqwest::Client::new();

    if let Some(ref pubkey_url) = repo.pubkey {
        fetch_public_key(&client, &repo_path, pubkey_url).await?;
    }

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());
    if !etag.is_empty() {
        headers.insert(header::IF_NONE_MATCH, etag.parse().unwrap());
    }

    let resp = client.get(url).headers(headers).send().await?;

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", repo.url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let etag = resp
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            SoarError::Custom("etag is required in metadata response header".to_string())
        })?;

    info!("Fetching metadata from {}", repo.url);

    let content = resp.bytes().await?.to_vec();

    process_metadata_content(content, &metadata_db, &repo.name, &repo.url)?;

    Ok(Some(etag))
}
