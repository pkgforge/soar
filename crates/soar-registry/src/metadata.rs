//! Metadata fetching and processing for package repositories.
//!
//! This module provides functions for fetching package metadata from remote
//! repositories, handling both SQLite database and JSON formats.

use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};

use soar_config::repository::Repository;
use soar_dl::{download::Download, http_client::SHARED_AGENT, types::OverwriteMode};
use tracing::debug;
use ureq::http::{
    header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH, PRAGMA},
    StatusCode,
};
use url::Url;

use crate::{
    error::{ErrorContext, RegistryError, Result},
    package::RemotePackage,
};

/// Magic bytes for SQLite database files.
pub const SQLITE_MAGIC_BYTES: [u8; 4] = [0x53, 0x51, 0x4c, 0x69];

/// Magic bytes for Zstandard compressed files.
pub const ZST_MAGIC_BYTES: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Represents the processed content of fetched metadata.
///
/// Metadata from repositories can come in two formats:
/// - Pre-built SQLite databases (more efficient for large repositories)
/// - JSON arrays of packages (simpler format)
///
/// The caller is responsible for handling each variant appropriately,
/// typically by either writing the SQLite bytes directly to disk or
/// importing JSON packages into a new database.
pub enum MetadataContent {
    /// Raw SQLite database bytes, ready to be written to disk.
    SqliteDb(Vec<u8>),
    /// Parsed package metadata from JSON format.
    Json(Vec<RemotePackage>),
}

/// Fetches the public key for package signature verification.
///
/// Downloads the minisign public key from the specified URL and saves it
/// to the repository path. If the public key file already exists, this
/// function returns immediately without re-downloading.
///
/// # Arguments
///
/// * `repo_path` - Directory where the public key will be stored as `minisign.pub`
/// * `pubkey_url` - URL to fetch the public key from
///
/// # Errors
///
/// Returns [`RegistryError`] if the download fails.
pub async fn fetch_public_key<P: AsRef<Path>>(repo_path: P, pubkey_url: &str) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let pubkey_file = repo_path.join("minisign.pub");

    if pubkey_file.exists() {
        return Ok(());
    }

    debug!("Fetching public key from {}", pubkey_url);

    Download::new(pubkey_url)
        .output(pubkey_file.to_string_lossy().to_string())
        .overwrite(OverwriteMode::Force)
        .execute()?;

    Ok(())
}

/// Fetches repository metadata from a remote source.
///
/// This function retrieves package metadata for a configured repository, handling
/// caching via ETags and respecting the repository's sync interval. It also
/// fetches the repository's public key if configured.
///
/// # Arguments
///
/// * `repo` - The repository configuration
/// * `force` - If `true`, bypasses cache validation and fetches fresh metadata
/// * `existing_etag` - Optional etag from a previous fetch, read from the database
///
/// # Returns
///
/// * `Ok(Some((etag, content)))` - New metadata was fetched successfully
/// * `Ok(None)` - Cached metadata is still valid (not modified)
/// * `Err(_)` - An error occurred during fetching or processing
///
/// # Errors
///
/// Returns [`RegistryError`] if:
/// - The repository URL is invalid
/// - Network request fails
/// - Server returns an error response
/// - Response is missing required ETag header
/// - Metadata content cannot be processed
/// - Public key fetch fails (if configured)
///
/// # Example
///
/// ```no_run
/// use soar_registry::{fetch_metadata, MetadataContent, write_metadata_db};
/// use soar_config::repository::Repository;
///
/// async fn sync(repo: &Repository, etag: Option<String>) -> soar_registry::Result<()> {
///     if let Some((new_etag, content)) = fetch_metadata(repo, false, etag).await? {
///         let db_path = repo.get_path().unwrap().join("metadata.db");
///         if let MetadataContent::SqliteDb(bytes) = content {
///             write_metadata_db(&bytes, &db_path)?;
///         }
///     }
///     Ok(())
/// }
/// ```
pub async fn fetch_metadata(
    repo: &Repository,
    force: bool,
    existing_etag: Option<String>,
) -> Result<Option<(String, MetadataContent)>> {
    let repo_path = repo.get_path().map_err(|e| {
        RegistryError::IoError {
            action: "getting repository path".to_string(),
            source: io::Error::other(e.to_string()),
        }
    })?;
    let metadata_db = repo_path.join("metadata.db");

    if !metadata_db.exists() {
        fs::create_dir_all(&repo_path)
            .with_context(|| format!("creating directory {}", repo_path.display()))?;
    }

    let sync_interval = repo.sync_interval();

    let etag = if metadata_db.exists() {
        let etag = existing_etag.unwrap_or_default();

        if !force && !etag.is_empty() {
            let file_info = metadata_db
                .metadata()
                .with_context(|| format!("reading file metadata from {}", metadata_db.display()))?;
            if let Ok(created) = file_info.created() {
                if sync_interval >= created.elapsed()?.as_millis() {
                    return Ok(None);
                }
            }
        }
        etag
    } else {
        String::new()
    };

    Url::parse(&repo.url).map_err(|err| RegistryError::InvalidUrl(err.to_string()))?;

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
        .map_err(|err| RegistryError::FailedToFetchRemote(err.to_string()))?;

    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let msg = format!("{} [{}]", repo.url, resp.status());
        return Err(RegistryError::FailedToFetchRemote(msg));
    }

    let etag = resp
        .headers()
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(String::from)
        .ok_or(RegistryError::MissingEtag)?;

    debug!("Fetching metadata from {}", repo.url);

    let content = resp.into_body().read_to_vec()?;
    let metadata_content = process_metadata_content(content, &metadata_db)?;

    Ok(Some((etag, metadata_content)))
}

/// Processes raw metadata content and determines its format.
///
/// This function inspects the magic bytes of the content to determine whether
/// it's a SQLite database, zstd-compressed data, or JSON. Compressed content
/// is automatically decompressed.
///
/// # Arguments
///
/// * `content` - Raw bytes fetched from the remote source
/// * `metadata_db_path` - Path used for creating temporary files during decompression
///
/// # Returns
///
/// Returns [`MetadataContent::SqliteDb`] if the content is (or decompresses to)
/// a SQLite database, or [`MetadataContent::Json`] if it's JSON data.
///
/// # Errors
///
/// Returns [`RegistryError`] if:
/// - Content is less than 4 bytes (too short to identify)
/// - Zstd decompression fails
/// - JSON parsing fails
/// - Temporary file operations fail
pub fn process_metadata_content(
    content: Vec<u8>,
    metadata_db_path: &Path,
) -> Result<MetadataContent> {
    if content.len() < 4 {
        return Err(RegistryError::MetadataTooShort);
    }

    if content[..4] == ZST_MAGIC_BYTES {
        let tmp_path = format!("{}.part", metadata_db_path.display());
        let mut tmp_file = File::create(&tmp_path)
            .with_context(|| format!("creating temporary file {tmp_path}"))?;

        let mut decoder = zstd::Decoder::new(content.as_slice())
            .map_err(|e| RegistryError::Custom(format!("creating zstd decoder: {e}")))?;
        io::copy(&mut decoder, &mut tmp_file)
            .with_context(|| format!("decoding zstd from {tmp_path}"))?;

        let magic_bytes = soar_utils::fs::read_file_signature(&tmp_path, 4).map_err(|e| {
            RegistryError::IoError {
                action: format!("reading signature from {tmp_path}"),
                source: io::Error::other(e.to_string()),
            }
        })?;

        if magic_bytes == SQLITE_MAGIC_BYTES {
            let db_content = fs::read(&tmp_path)
                .with_context(|| format!("reading temporary file {tmp_path}"))?;
            fs::remove_file(&tmp_path)
                .with_context(|| format!("removing temporary file {tmp_path}"))?;
            Ok(MetadataContent::SqliteDb(db_content))
        } else {
            let tmp_file = File::open(&tmp_path)
                .with_context(|| format!("opening temporary file {tmp_path}"))?;
            let reader = BufReader::new(tmp_file);
            let metadata: Vec<RemotePackage> = serde_json::from_reader(reader)?;
            fs::remove_file(&tmp_path)
                .with_context(|| format!("removing temporary file {tmp_path}"))?;
            Ok(MetadataContent::Json(metadata))
        }
    } else if content[..4] == SQLITE_MAGIC_BYTES {
        Ok(MetadataContent::SqliteDb(content))
    } else {
        let metadata: Vec<RemotePackage> = serde_json::from_slice(&content)?;
        Ok(MetadataContent::Json(metadata))
    }
}

/// Writes SQLite database content to a file.
///
/// This is a convenience function for writing [`MetadataContent::SqliteDb`]
/// bytes to disk using buffered I/O.
///
/// # Arguments
///
/// * `content` - Raw SQLite database bytes
/// * `path` - Destination file path
///
/// # Errors
///
/// Returns [`RegistryError::IoError`] if file creation or writing fails.
///
/// # Example
///
/// ```no_run
/// use soar_registry::write_metadata_db;
///
/// fn save_db(bytes: &[u8]) -> soar_registry::Result<()> {
///     write_metadata_db(bytes, "/path/to/metadata.db")
/// }
/// ```
pub fn write_metadata_db<P: AsRef<Path>>(content: &[u8], path: P) -> Result<()> {
    let path = path.as_ref();
    let mut writer = BufWriter::new(
        File::create(path).with_context(|| format!("creating metadata file {}", path.display()))?,
    );
    writer
        .write_all(content)
        .with_context(|| format!("writing to metadata file {}", path.display()))?;
    Ok(())
}
