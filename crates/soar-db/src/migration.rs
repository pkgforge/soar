use std::error::Error;

use diesel::{sql_query, RunQueryDsl, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub const CORE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/core");
pub const METADATA_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/metadata");
pub const NEST_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/nest");

pub enum DbType {
    Core,
    Metadata,
    Nest,
}

pub fn apply_migrations(
    conn: &mut SqliteConnection,
    db_type: DbType,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    loop {
        let source = match db_type {
            DbType::Core => CORE_MIGRATIONS,
            DbType::Metadata => METADATA_MIGRATIONS,
            DbType::Nest => NEST_MIGRATIONS,
        };
        match conn.run_pending_migrations(source) {
            Ok(_) => break,
            Err(e) if e.to_string().contains("already exists") => {
                mark_first_pending(conn)?;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

fn mark_first_pending(
    conn: &mut SqliteConnection,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let pending = conn.pending_migrations(CORE_MIGRATIONS)?;
    if let Some(first) = pending.first() {
        sql_query("INSERT INTO __diesel_schema_migrations (version) VALUES (?1)")
            .bind::<diesel::sql_types::Text, _>(first.name().version())
            .execute(conn)?;
    }

    Ok(())
}
