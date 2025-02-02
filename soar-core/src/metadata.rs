use std::fs::{self, File};

use futures::TryStreamExt;
use reqwest::header::{self, HeaderMap};
use rusqlite::Connection;
use tracing::info;

use crate::{
    config::Repository,
    constants::METADATA_MIGRATIONS,
    database::{connection::Database, migration::MigrationManager, models::RemotePackage},
    error::SoarError,
    SoarResult,
};

pub async fn fetch_metadata(repo: Repository) -> SoarResult<()> {
    let repo_path = repo.get_path()?;
    if !repo_path.is_dir() {
        return Err(SoarError::InvalidPath);
    }

    let client = reqwest::Client::new();

    let mut header_map = HeaderMap::new();
    header_map.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    header_map.insert(header::PRAGMA, "no-cache".parse().unwrap());

    let resp = client.get(&repo.url).headers(header_map).send().await?;
    if !resp.status().is_success() {
        return Err(SoarError::FailedToFetchRemote(repo.url));
    }

    let metadata_db = repo_path.join("metadata.db");

    let etag = {
        let conn = Connection::open(&metadata_db)?;
        let etag: String = conn
            .query_row("SELECT etag FROM repository", [], |row| row.get(0))
            .unwrap_or_default();

        match resp.headers().get(header::ETAG) {
            Some(remote_etag) => {
                let remote_etag = remote_etag.to_str().unwrap();
                if etag == remote_etag {
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

    let _ = fs::remove_file(&metadata_db);
    File::create(&metadata_db)?;

    info!("Fetching metadata from {}", repo.url);

    let conn = Connection::open(&metadata_db)?;
    let mut manager = MigrationManager::new(conn)?;
    manager.migrate_from_dir(METADATA_MIGRATIONS)?;

    let mut content = Vec::new();
    let mut stream = resp.bytes_stream();

    while let Ok(Some(chunk)) = stream.try_next().await {
        content.extend_from_slice(&chunk);
    }

    let remote_metadata: Vec<RemotePackage> = serde_json::from_slice(&content).map_err(|err| {
        SoarError::Custom(format!(
            "Failed to parse metadata response from {}: {:#?}",
            repo.url, err
        ))
    })?;

    let db = Database::new(metadata_db)?;
    db.from_remote_metadata(remote_metadata.as_ref(), &repo.name, &etag)?;

    Ok(())
}
