use std::fs::{self, File};

use rusqlite::Connection;

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

    let checksum_file = repo_path.join("metadata.bsum");
    let remote_checksum_url = format!("{}.bsum", repo.url);
    let resp = reqwest::get(&remote_checksum_url).await?;
    if !resp.status().is_success() {
        return Err(SoarError::FailedToFetchRemote(remote_checksum_url));
    }
    let remote_checksum = resp.text().await?;
    if let Ok(checksum) = fs::read_to_string(&checksum_file) {
        if checksum == remote_checksum {
            return Ok(());
        }
    }

    let metadata_db = repo_path.join("metadata.db");

    let _ = fs::remove_file(&metadata_db);
    File::create(&metadata_db)?;

    let conn = Connection::open(&metadata_db)?;
    let mut manager = MigrationManager::new(conn)?;
    manager.migrate_from_dir(METADATA_MIGRATIONS)?;

    let resp = reqwest::get(&repo.url).await?;
    if !resp.status().is_success() {
        return Err(SoarError::FailedToFetchRemote(repo.url));
    }
    let remote_metadata: Vec<RemotePackage> = resp.json().await?;

    let db = Database::new(metadata_db)?;
    db.from_remote_metadata(remote_metadata.as_ref(), &repo.name)?;

    fs::write(checksum_file, remote_checksum)?;

    Ok(())
}
