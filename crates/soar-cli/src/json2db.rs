use std::fs;

use soar_core::{
    error::{ErrorContext, SoarError},
    SoarResult,
};
use soar_db::{
    connection::DbConnection, migration::DbType, repository::metadata::MetadataRepository,
};
use soar_registry::RemotePackage;
use tracing::info;

/// Converts JSON metadata file to SQLite database.
pub fn json_to_db(input_path: &str, output_path: &str, repo_name: Option<&str>) -> SoarResult<()> {
    info!(
        input = input_path,
        output = output_path,
        "Converting JSON metadata to SQLite database"
    );

    let repo_name = repo_name.unwrap_or("custom");

    let json_content = fs::read_to_string(input_path)
        .with_context(|| format!("reading JSON metadata from {}", input_path))?;

    let packages: Vec<RemotePackage> = serde_json::from_str(&json_content)
        .map_err(|e| SoarError::Custom(format!("parsing JSON from {}: {}", input_path, e)))?;

    info!(count = packages.len(), "Parsed JSON metadata");

    if packages.is_empty() {
        info!("No packages found in JSON file");
        return Ok(());
    }

    let output_path = std::path::Path::new(output_path);
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating output directory {}", parent.display()))?;
        }
    }

    if output_path.exists() {
        fs::remove_file(output_path)
            .with_context(|| format!("removing existing database {}", output_path.display()))?;
    }

    let mut conn = DbConnection::open(output_path, DbType::Metadata)
        .map_err(|e| SoarError::Custom(format!("opening database: {}", e)))?;

    MetadataRepository::import_packages(conn.conn(), &packages, repo_name)
        .map_err(|e| SoarError::Custom(format!("importing packages: {}", e)))?;

    info!(
        count = packages.len(),
        output = %output_path.display(),
        "Successfully converted JSON to SQLite database"
    );

    Ok(())
}
