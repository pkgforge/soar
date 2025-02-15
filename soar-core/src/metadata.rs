use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};

use futures::TryStreamExt;
use reqwest::header::{self, HeaderMap};
use rusqlite::{params, Connection};
use tracing::info;

use crate::{
    config::Repository,
    constants::{METADATA_MIGRATIONS, SQLITE_MAGIC_BYTES, ZST_MAGIC_BYTES},
    database::{connection::Database, migration::MigrationManager, models::RemotePackage},
    error::SoarError,
    utils::calc_magic_bytes,
    SoarResult,
};

fn handle_json_metadata<P: AsRef<Path>>(
    metadata: &[RemotePackage],
    metadata_db: P,
    repo: &Repository,
    etag: &str,
) -> SoarResult<()> {
    let metadata_db = metadata_db.as_ref();
    if metadata_db.exists() {
        fs::remove_file(metadata_db)?;
    }

    let conn = Connection::open(&metadata_db)?;
    let mut manager = MigrationManager::new(conn)?;
    manager.migrate_from_dir(METADATA_MIGRATIONS)?;

    let db = Database::new(metadata_db)?;
    db.from_remote_metadata(metadata.as_ref(), &repo.name, &etag)?;

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
    fs::write(pubkey_file, content)?;

    Ok(())
}

pub async fn fetch_metadata(repo: Repository, force: bool) -> SoarResult<()> {
    let repo_path = repo.get_path()?;
    if !repo_path.is_dir() {
        return Err(SoarError::InvalidPath);
    }

    let metadata_db = repo_path.join("metadata.db");
    if !force {
        let file_info = metadata_db.metadata()?;
        if let Ok(created) = file_info.created() {
            if repo.sync_interval() >= created.elapsed()?.as_millis() as u128 {
                return Ok(());
            }
        }
    }

    let client = reqwest::Client::new();

    if let Some(ref pubkey_url) = repo.pubkey {
        fetch_public_key(&client, &repo_path, &pubkey_url).await?;
    }

    let mut header_map = HeaderMap::new();
    header_map.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    header_map.insert(header::PRAGMA, "no-cache".parse().unwrap());

    let resp = client.get(&repo.url).headers(header_map).send().await?;
    if !resp.status().is_success() {
        let msg = format!("{} [{}]", repo.url, resp.status());
        return Err(SoarError::FailedToFetchRemote(msg));
    }

    let etag = {
        let conn = Connection::open(&metadata_db)?;
        let etag: String = conn
            .query_row("SELECT etag FROM repository", [], |row| row.get(0))
            .unwrap_or_default();

        match resp.headers().get(header::ETAG) {
            Some(remote_etag) => {
                let remote_etag = remote_etag.to_str().unwrap();
                if !force && etag == remote_etag {
                    return Ok(());
                }
                remote_etag.to_string()
            }
            None => {
                return Err(SoarError::Custom(
                    "etag is required in metadata response header.".to_string(),
                ))
            }
        }
    };

    info!("Fetching metadata from {}", repo.url);

    let mut content = Vec::new();
    let mut stream = resp.bytes_stream();

    while let Ok(Some(chunk)) = stream.try_next().await {
        content.extend_from_slice(&chunk);
    }

    if content[..4] == ZST_MAGIC_BYTES {
        let tmp_path = format!("{}.part", metadata_db.display());
        let mut tmp_file = File::create(&tmp_path)?;

        let mut decoder = zstd::Decoder::new(content.as_slice())?;
        io::copy(&mut decoder, &mut tmp_file)?;

        let magic_bytes = calc_magic_bytes(&tmp_path, 4)?;
        if magic_bytes == SQLITE_MAGIC_BYTES {
            fs::rename(&tmp_path, &metadata_db)?;
            let conn = Connection::open(&metadata_db)?;
            conn.execute(
                "UPDATE repository SET name = ?, etag = ?",
                params![repo.name, etag],
            )?;
        } else {
            let tmp_file = File::open(&tmp_path)?;
            let reader = BufReader::new(tmp_file);
            let metadata: Vec<RemotePackage> = serde_json::from_reader(reader).map_err(|err| {
                SoarError::Custom(format!(
                    "Failed to parse JSON metadata from {}: {:#?}",
                    tmp_path, err
                ))
            })?;

            handle_json_metadata(&metadata, metadata_db, &repo, &etag)?;
            fs::remove_file(tmp_path)?;
        }
    } else if content[..4] == SQLITE_MAGIC_BYTES {
        let mut writer = BufWriter::new(File::create(&metadata_db)?);
        writer.write_all(&content)?;
        let conn = Connection::open(&metadata_db)?;
        conn.execute(
            "UPDATE repository SET name = ?, etag = ?",
            params![repo.name, etag],
        )?;
    } else {
        let remote_metadata: Vec<RemotePackage> =
            serde_json::from_slice(&content).map_err(|err| {
                SoarError::Custom(format!(
                    "Failed to parse JSON metadata response from {}: {:#?}",
                    repo.url, err
                ))
            })?;

        handle_json_metadata(&remote_metadata, metadata_db, &repo, &etag)?;
    }

    Ok(())
}
