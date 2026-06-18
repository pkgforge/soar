//! Metadata fetching and processing for package repositories.
//!
//! This module provides functions for fetching package metadata from remote
//! repositories, handling both SQLite database and JSON formats.

use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};

use minisign_verify::{PublicKey, Signature};
use soar_config::repository::Repository;
use soar_dl::http_client::SHARED_AGENT;
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

/// Maximum size, in bytes, allowed for metadata in either form.
///
/// This bounds both the downloaded body (which would otherwise be capped at
/// ureq's 10 MB default, truncating a large catalog) and the zstd-decompressed
/// output (so a decompression bomb cannot exhaust the disk). 256 MB leaves ample
/// headroom for catalog growth while keeping a malicious response bounded.
pub const MAX_METADATA_SIZE: u64 = 256 * 1024 * 1024;

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

/// Fetches repository metadata from a remote source.
///
/// This function retrieves package metadata for a configured repository, handling
/// caching via ETags and respecting the repository's sync interval.
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

    if metadata_db.exists() && !force {
        if sync_interval == u128::MAX {
            return Ok(None);
        }

        let file_info = metadata_db
            .metadata()
            .with_context(|| format!("reading file metadata from {}", metadata_db.display()))?;
        if let Ok(modified) = file_info.modified() {
            if sync_interval >= modified.elapsed()?.as_millis() {
                return Ok(None);
            }
        }
    }

    let etag = if metadata_db.exists() {
        existing_etag.unwrap_or_default()
    } else {
        String::new()
    };

    let parsed_url =
        Url::parse(&repo.url).map_err(|err| RegistryError::InvalidUrl(err.to_string()))?;
    if parsed_url.scheme() != "https" {
        return Err(RegistryError::InsecureUrl(repo.url.clone()));
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

    let content = resp
        .into_body()
        .into_with_config()
        .limit(MAX_METADATA_SIZE)
        .read_to_vec()?;

    verify_metadata_signature(repo, &content)?;

    let metadata_content = process_metadata_content(content, &metadata_db)?;

    Ok(Some((etag, metadata_content)))
}

/// Verifies the authenticity of fetched metadata against the repository pubkey.
///
/// When the repository has signature verification enabled, this fetches the
/// detached minisign signature published next to the metadata (`<url>.sig`)
/// and verifies it over the raw fetched bytes, before the metadata is
/// decompressed, parsed, or persisted. A missing or invalid signature is a hard
/// error so a tampered metadata source cannot supply both the package
/// `download_url` and its expected checksum.
fn verify_metadata_signature(repo: &Repository, content: &[u8]) -> Result<()> {
    if !repo.signature_verification() {
        return Ok(());
    }

    let pubkey = repo.pubkey.as_deref().ok_or_else(|| {
        RegistryError::MetadataSignatureInvalid {
            repo: repo.name.clone(),
            reason: "signature verification is enabled but no public key is configured".to_string(),
        }
    })?;

    let sig_url = format!("{}.sig", repo.url);
    let sig_text = fetch_signature_text(&sig_url).map_err(|reason| {
        RegistryError::MetadataSignatureMissing {
            repo: repo.name.clone(),
            reason,
        }
    })?;

    let public_key = PublicKey::from_base64(pubkey.trim()).map_err(|err| {
        RegistryError::MetadataSignatureInvalid {
            repo: repo.name.clone(),
            reason: format!("invalid public key: {err}"),
        }
    })?;
    let signature = Signature::decode(&sig_text).map_err(|err| {
        RegistryError::MetadataSignatureInvalid {
            repo: repo.name.clone(),
            reason: format!("malformed signature: {err}"),
        }
    })?;

    public_key
        .verify(content, &signature, true)
        .map_err(|err| {
            RegistryError::MetadataSignatureInvalid {
                repo: repo.name.clone(),
                reason: err.to_string(),
            }
        })?;

    debug!("Verified metadata signature for {}", repo.name);
    Ok(())
}

/// Fetches the textual contents of a detached minisign signature.
fn fetch_signature_text(url: &str) -> std::result::Result<String, String> {
    let resp = SHARED_AGENT
        .get(url)
        .header(CACHE_CONTROL, "no-cache")
        .header(PRAGMA, "no-cache")
        .call()
        .map_err(|err| err.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("{} [{}]", url, resp.status()));
    }

    resp.into_body()
        .read_to_string()
        .map_err(|err| err.to_string())
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

        let decoder = zstd::Decoder::new(content.as_slice())
            .map_err(|e| RegistryError::Custom(format!("creating zstd decoder: {e}")))?;
        let mut limited = io::Read::take(decoder, MAX_METADATA_SIZE + 1);
        let written = io::copy(&mut limited, &mut tmp_file)
            .with_context(|| format!("decoding zstd from {tmp_path}"))?;
        if written > MAX_METADATA_SIZE {
            drop(tmp_file);
            let _ = fs::remove_file(&tmp_path);
            return Err(RegistryError::MetadataTooLarge {
                limit: MAX_METADATA_SIZE,
            });
        }

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
