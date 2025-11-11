use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};

use rusqlite::Connection;
use soar_dl::{download::Download, http_client::SHARED_AGENT, types::OverwriteMode};
use soar_utils::{fs::read_file_signature, system::platform};
use tracing::info;
use ureq::http::{
    header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH, PRAGMA},
    StatusCode,
};
use url::Url;

use crate::{
    config::{self, Repository},
    constants::{METADATA_MIGRATIONS, SQLITE_MAGIC_BYTES, ZST_MAGIC_BYTES},
    database::{
        connection::Database,
        migration::{DbKind, MigrationManager},
        models::RemotePackage,
        nests::models::Nest,
    },
    error::{ErrorContext, SoarError},
    SoarResult,
};

fn construct_nest_url(url: &str) -> SoarResult<String> {
    let url = if let Some(repo) = url.strip_prefix("github:") {
        format!(
            "https://github.com/{}/releases/download/soar-nest/{}.json",
            repo,
            platform()
        )
    } else {
        url.to_string()
    };
    Url::parse(&url).map_err(|err| SoarError::Custom(err.to_string()))?;
    Ok(url)
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

    let mut req = SHARED_AGENT
        .get(&url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache");

    if !etag.is_empty() {
        req = req.header(IF_NONE_MATCH, etag);
    }

    let resp = req
        .call()
        .map_err(|err| SoarError::FailedToFetchRemote(err.to_string()))?;

    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(String::from)
        .ok_or_else(|| SoarError::Custom("etag not found in metadata response".to_string()))?;

    info!("Fetching nest from {}", url);

    let content = resp.into_body().read_to_vec()?;

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

        let magic_bytes = read_file_signature(&tmp_path, 4)?;
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
    manager.migrate_from_dir(METADATA_MIGRATIONS, DbKind::Metadata)?;

    let db = Database::new(metadata_db)?;
    db.from_remote_metadata(metadata.as_ref(), repo_name)?;

    Ok(())
}

pub async fn fetch_public_key<P: AsRef<Path>>(repo_path: P, pubkey_url: &str) -> SoarResult<()> {
    let repo_path = repo_path.as_ref();
    let pubkey_file = repo_path.join("minisign.pub");

    if pubkey_file.exists() {
        // skip if we already have the public key file
        return Ok(());
    }

    info!("Fetching public key from {}", pubkey_url);

    Download::new(pubkey_url)
        .output(pubkey_file.to_string_lossy().to_string())
        .overwrite(OverwriteMode::Force)
        .execute()?;

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

    Url::parse(&repo.url).map_err(|err| SoarError::Custom(err.to_string()))?;

    if let Some(ref pubkey_url) = repo.pubkey {
        fetch_public_key(&repo_path, pubkey_url).await?;
    }

    let mut req = SHARED_AGENT
        .get(&repo.url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache");

    if !etag.is_empty() {
        req = req.header(IF_NONE_MATCH, etag);
    }

    let resp = req
        .call()
        .map_err(|err| SoarError::FailedToFetchRemote(err.to_string()))?;

    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", repo.url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(String::from)
        .ok_or_else(|| SoarError::Custom("etag not found in metadata response".to_string()))?;

    info!("Fetching metadata from {}", repo.url);

    let content = resp.into_body().read_to_vec()?;

    process_metadata_content(content, &metadata_db, &repo.name, &repo.url)?;

    Ok(Some(etag))
}
