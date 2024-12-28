use std::fs::{self, File};

use crate::{
    config::Repository,
    database::{connection::Database, models::RemotePackageMetadata},
    error::SoarError,
    SoarResult,
};

pub async fn fetch_metadata(repo: Repository) -> SoarResult<()> {
    let repo_path = repo.get_path();
    let remote_url = format!(
        "{}/{}",
        repo.url,
        repo.metadata.unwrap_or("metadata.json".into())
    );
    if !repo_path.is_dir() {
        return Err(SoarError::InvalidPath);
    }

    let checksum_file = repo_path.join("metadata.bsum");
    let remote_checksum_url = format!("{}.bsum", remote_url);
    let resp = reqwest::get(&remote_checksum_url).await?;
    if !resp.status().is_success() {
        return Err(SoarError::FailedToFetchRemote);
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
    soar_db::metadata::init_db(&metadata_db).unwrap();

    let resp = reqwest::get(&remote_url).await?;
    if !resp.status().is_success() {
        return Err(SoarError::FailedToFetchRemote);
    }
    let remote_metadata: RemotePackageMetadata = resp.json().await?;

    let db = Database::new(metadata_db)?;
    db.from_json_metadata(remote_metadata, &repo.name)?;

    fs::write(checksum_file, remote_checksum)?;

    Ok(())
}
