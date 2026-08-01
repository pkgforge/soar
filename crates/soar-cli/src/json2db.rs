use std::fs;

use soar_core::{
    error::{ErrorContext, SoarError},
    SoarResult,
};
use soar_db::{
    connection::DbConnection, migration::DbType, repository::metadata::MetadataRepository,
};
use soar_registry::RemotePackage;
use tracing::{info, warn};

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

    let packages: Vec<RemotePackage> = soar_registry::parse_index(json_content.as_bytes())
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

    // Built beside the target and swapped in only once it holds something.
    // Writing in place would destroy a working database whenever an import
    // turned out to be entirely rejected.
    // Named for this process, so a second import running at the same time
    // cannot delete the database this one is still building.
    let mut tmp_name = output_path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp_path = output_path.with_file_name(tmp_name);
    for stale in [
        &tmp_path,
        &tmp_path.with_extension("tmp-wal"),
        &tmp_path.with_extension("tmp-shm"),
    ] {
        fs::remove_file(stale).ok();
    }

    let mut conn = DbConnection::open(&tmp_path, DbType::Metadata)
        .map_err(|e| SoarError::Custom(format!("opening database: {}", e)))?;

    // Packages with an unsafe pkg_name/pkg_id are skipped during import.
    // Reporting success while writing nothing hides that entirely, which is
    // how an empty pkg_id silently produced an empty database.
    let result = MetadataRepository::import_packages(conn.conn(), &packages, repo_name)
        .map_err(|e| SoarError::Custom(format!("importing packages: {}", e)));
    let imported = match result {
        Ok(n) if n > 0 => n,
        other => {
            drop(conn);
            fs::remove_file(&tmp_path).ok();
            return match other {
                Err(e) => Err(e),
                Ok(_) => {
                    Err(SoarError::Custom(format!(
                        "imported 0 of {} packages; every entry was rejected, most likely \
                     an unsafe or empty pkg_name/pkg_id",
                        packages.len()
                    )))
                }
            };
        }
    };

    let skipped = packages.len() - imported;
    if skipped > 0 {
        warn!(skipped, "some packages were rejected during import");
    }

    drop(conn);
    fs::rename(&tmp_path, output_path).with_context(|| {
        format!(
            "replacing {} with the imported database",
            output_path.display()
        )
    })?;

    info!(
        count = imported,
        output = %output_path.display(),
        "Successfully converted JSON to SQLite database"
    );

    Ok(())
}
