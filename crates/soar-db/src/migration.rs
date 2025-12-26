use std::error::Error;

use diesel::{sql_query, RunQueryDsl, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub const CORE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/core");
pub const METADATA_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/metadata");
pub const NEST_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/nest");

#[derive(Clone, Copy, Debug)]
pub enum DbType {
    Core,
    Metadata,
    Nest,
}

fn get_migrations(db_type: &DbType) -> EmbeddedMigrations {
    match db_type {
        DbType::Core => CORE_MIGRATIONS,
        DbType::Metadata => METADATA_MIGRATIONS,
        DbType::Nest => NEST_MIGRATIONS,
    }
}

pub fn apply_migrations(
    conn: &mut SqliteConnection,
    db_type: &DbType,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    loop {
        match conn.run_pending_migrations(get_migrations(db_type)) {
            Ok(_) => break,
            Err(e) if e.to_string().contains("already exists") => {
                mark_first_pending(conn, db_type)?;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

fn mark_first_pending(
    conn: &mut SqliteConnection,
    db_type: &DbType,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let pending = conn.pending_migrations(get_migrations(db_type))?;
    if let Some(first) = pending.first() {
        sql_query("INSERT INTO __diesel_schema_migrations (version) VALUES (?1)")
            .bind::<diesel::sql_types::Text, _>(first.name().version())
            .execute(conn)?;
    }

    Ok(())
}

/// Migrate text JSON columns to JSONB binary format.
///
/// This is needed when migrating from rusqlite (which stores JSON as text)
/// to diesel (which uses SQLite's native JSONB format).
///
/// Handles both:
/// - Text type columns (typeof = 'text')
/// - Blob columns containing text JSON (starts with '[' or '{')
///
/// # Performance Note
///
/// This runs on every database open but is essentially a no-op after the first
/// successful migration. The WHERE clause only matches rows with text-based JSON,
/// so once all rows are converted to JSONB binary format, no rows will be updated.
///
/// TODO: Remove this migration in a future version (v0.10 or v1.0) once users
/// have had sufficient time to migrate their databases.
pub fn migrate_json_to_jsonb(
    conn: &mut SqliteConnection,
    db_type: DbType,
) -> Result<usize, Box<dyn Error + Send + Sync + 'static>> {
    // Check for text type OR blob containing text JSON (starts with '[' or '{')
    // Use hex comparison for blobs: 5B = '[', 7B = '{'
    let json_condition = |col: &str| {
        format!(
            "{col} IS NOT NULL AND (typeof({col}) = 'text' OR (typeof({col}) = 'blob' AND hex(substr({col}, 1, 1)) IN ('5B', '7B')))"
        )
    };

    let queries: Vec<String> = match db_type {
        DbType::Core => {
            vec![
                format!(
                    "UPDATE packages SET provides = jsonb(provides) WHERE {}",
                    json_condition("provides")
                ),
                format!(
                    "UPDATE packages SET install_patterns = jsonb(install_patterns) WHERE {}",
                    json_condition("install_patterns")
                ),
            ]
        }
        DbType::Metadata => {
            vec![
                format!(
                    "UPDATE packages SET licenses = jsonb(licenses) WHERE {}",
                    json_condition("licenses")
                ),
                format!(
                    "UPDATE packages SET homepages = jsonb(homepages) WHERE {}",
                    json_condition("homepages")
                ),
                format!(
                    "UPDATE packages SET notes = jsonb(notes) WHERE {}",
                    json_condition("notes")
                ),
                format!(
                    "UPDATE packages SET source_urls = jsonb(source_urls) WHERE {}",
                    json_condition("source_urls")
                ),
                format!(
                    "UPDATE packages SET tags = jsonb(tags) WHERE {}",
                    json_condition("tags")
                ),
                format!(
                    "UPDATE packages SET categories = jsonb(categories) WHERE {}",
                    json_condition("categories")
                ),
                format!(
                    "UPDATE packages SET provides = jsonb(provides) WHERE {}",
                    json_condition("provides")
                ),
                format!(
                    "UPDATE packages SET snapshots = jsonb(snapshots) WHERE {}",
                    json_condition("snapshots")
                ),
                format!(
                    "UPDATE packages SET replaces = jsonb(replaces) WHERE {}",
                    json_condition("replaces")
                ),
            ]
        }
        DbType::Nest => vec![],
    };

    let mut total = 0;
    for query in queries {
        total += sql_query(&query).execute(conn)?;
    }

    Ok(total)
}
