// `QueryableByName` expands to `Type { field: field }`, which clippy
// reports against the field it was generated from.
#![allow(clippy::redundant_field_names)]

use std::error::Error;

use diesel::{
    sql_query, sql_types::Integer, Connection, QueryResult, QueryableByName, RunQueryDsl,
    SqliteConnection,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::trace;

pub const CORE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/core");
pub const METADATA_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/metadata");

#[derive(Clone, Copy, Debug)]
pub enum DbType {
    Core,
    Metadata,
}

fn get_migrations(db_type: &DbType) -> EmbeddedMigrations {
    match db_type {
        DbType::Core => CORE_MIGRATIONS,
        DbType::Metadata => METADATA_MIGRATIONS,
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

/// The JSON columns of a published metadata database.
const METADATA_JSON_COLUMNS: [&str; 8] = [
    "licenses",
    "homepages",
    "notes",
    "source_urls",
    "categories",
    "provides",
    "snapshots",
    "replaces",
];

/// The table soar writes into a metadata database once its JSON columns hold
/// JSONB, so the conversion runs once per fetched database instead of on every
/// open.
///
/// `user_version` is where a mark like this would otherwise go, but a published
/// database is generated elsewhere and that field belongs to whoever generated
/// it: writing to it would overwrite whatever it meant to them, and a value
/// that happened to match would leave text JSON in place with nothing to say
/// so. A table of soar's own can mean only what soar means by it.
const JSONB_MARKER_TABLE: &str = "soar_jsonb_converted";

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = Integer)]
    n: i32,
}

/// Whether this database has already had its JSON columns converted.
///
/// Read from `sqlite_master`, which answers without writing anything, so a
/// database this process cannot write is still one it can open.
fn holds_jsonb(conn: &mut SqliteConnection) -> QueryResult<bool> {
    let found = sql_query(format!(
        "SELECT count(*) AS n FROM sqlite_master WHERE type = 'table' AND name = '{JSONB_MARKER_TABLE}'"
    ))
    .get_result::<Count>(conn)?;

    Ok(found.n > 0)
}

/// Whether a column still holds JSON as text rather than JSONB.
///
/// `json_valid` with flag 8 answers "is this definitely JSONB", and with flag 1
/// "is this text that parses as JSON", which a blob holding text JSON also
/// satisfies. The first byte cannot tell the two apart: `5B` and `7B` open a
/// text array and object, but are equally the header of a JSONB array with a 5
/// or 7 byte payload, so `["htop"]` re-encoded as JSONB still looks unconverted
/// to that test. Anything that is neither is left alone, since `jsonb()` would
/// only fail on it.
fn is_text_json(column: &str) -> String {
    format!("{column} IS NOT NULL AND json_valid({column}, 1) AND NOT json_valid({column}, 8)")
}

/// Convert a metadata database's text JSON columns to JSONB binary format.
///
/// Published metadata is generated elsewhere and an older generator stores JSON
/// as text, which the queries here cannot read. Only this path still needs the
/// conversion: the core database has been written as JSONB for several releases.
///
/// The database records that it has been converted, so one opened again is a
/// single read of `sqlite_master` away from being left alone.
///
/// Every column and the mark itself land together, so a pass that fails partway
/// leaves the database as it was rather than converted in part, and the mark can
/// never outlast the conversion it stands for.
pub fn migrate_metadata_json_to_jsonb(
    conn: &mut SqliteConnection,
) -> Result<usize, Box<dyn Error + Send + Sync + 'static>> {
    if holds_jsonb(conn)? {
        trace!("metadata JSON columns already hold JSONB");
        return Ok(0);
    }

    let total = conn.transaction(|conn| {
        let mut total = 0;
        for column in METADATA_JSON_COLUMNS {
            trace!(column, "converting JSON column to JSONB");
            let query = format!(
                "UPDATE packages SET {column} = jsonb({column}) WHERE {}",
                is_text_json(column)
            );
            total += sql_query(&query).execute(conn)?;
        }

        sql_query(format!(
            "CREATE TABLE IF NOT EXISTS {JSONB_MARKER_TABLE} (converted_at TEXT NOT NULL)"
        ))
        .execute(conn)?;
        sql_query(format!(
            "INSERT INTO {JSONB_MARKER_TABLE} (converted_at) VALUES (datetime('now'))"
        ))
        .execute(conn)?;

        QueryResult::Ok(total)
    })?;

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(QueryableByName)]
    struct UserVersion {
        #[diesel(sql_type = Integer)]
        user_version: i32,
    }

    fn packages_db() -> SqliteConnection {
        db_with_columns(&METADATA_JSON_COLUMNS)
    }

    fn db_with_columns(columns: &[&str]) -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        sql_query(format!("CREATE TABLE packages ({});", columns.join(", ")))
            .execute(&mut conn)
            .unwrap();
        conn
    }

    fn user_version(conn: &mut SqliteConnection) -> i32 {
        sql_query("PRAGMA user_version;")
            .get_result::<UserVersion>(conn)
            .unwrap()
            .user_version
    }

    fn set_user_version(conn: &mut SqliteConnection, version: i32) {
        sql_query(format!("PRAGMA user_version = {version};"))
            .execute(conn)
            .unwrap();
    }

    fn insert(conn: &mut SqliteConnection, licenses: &str) {
        sql_query(format!(
            "INSERT INTO packages (licenses) VALUES ({licenses});"
        ))
        .execute(conn)
        .unwrap();
    }

    fn count(conn: &mut SqliteConnection, predicate: &str) -> i32 {
        sql_query(format!(
            "SELECT count(*) AS n FROM packages WHERE {predicate};"
        ))
        .get_result::<Count>(conn)
        .unwrap()
        .n
    }

    #[test]
    fn text_json_becomes_jsonb() {
        let mut conn = packages_db();
        insert(&mut conn, "'[\"MIT\"]'");

        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 1);
        assert_eq!(count(&mut conn, "json_valid(licenses, 8)"), 1);
    }

    #[test]
    fn a_jsonb_array_is_not_mistaken_for_text_json() {
        // `jsonb('["htop"]')` is a five byte payload, giving it the header byte
        // 5B that also opens a text array.
        let mut conn = packages_db();
        insert(&mut conn, "jsonb('[\"htop\"]')");
        insert(&mut conn, "jsonb('[\"abcdef\"]')");

        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 0);
    }

    #[test]
    fn text_that_is_not_json_is_left_alone() {
        let mut conn = packages_db();
        insert(&mut conn, "'not json at all'");

        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 0);
        assert_eq!(count(&mut conn, "typeof(licenses) = 'text'"), 1);
    }

    #[test]
    fn a_pass_that_fails_partway_converts_nothing() {
        // A database missing the column converted last: the columns before it
        // convert, and then the pass fails with seven updates to undo.
        let (last, rest) = METADATA_JSON_COLUMNS.split_last().unwrap();
        let mut conn = db_with_columns(rest);
        insert(&mut conn, "'[\"MIT\"]'");

        let err = migrate_metadata_json_to_jsonb(&mut conn).unwrap_err();
        assert!(err.to_string().contains(last), "unexpected error: {err}");
        assert_eq!(count(&mut conn, "typeof(licenses) = 'text'"), 1);
        assert!(!holds_jsonb(&mut conn).unwrap());
    }

    #[test]
    fn a_converted_database_is_left_alone() {
        let mut conn = packages_db();
        insert(&mut conn, "'[\"MIT\"]'");

        migrate_metadata_json_to_jsonb(&mut conn).unwrap();
        insert(&mut conn, "'[\"GPL-3.0\"]'");

        // The mark says the database has been converted, so a row added
        // afterwards is the writer's business rather than this migration's.
        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 0);
    }

    #[test]
    fn the_publishers_user_version_is_left_alone() {
        let mut conn = packages_db();
        set_user_version(&mut conn, 7);
        insert(&mut conn, "'[\"MIT\"]'");

        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 1);
        assert_eq!(user_version(&mut conn), 7);
    }

    #[test]
    fn a_user_version_that_looks_like_a_mark_is_not_one() {
        // 20260817 marked a converted database while the mark lived in
        // `user_version`. A publisher is free to use that number for something
        // else, and doing so must not pass for a conversion that never ran.
        let mut conn = packages_db();
        set_user_version(&mut conn, 20260817);
        insert(&mut conn, "'[\"MIT\"]'");

        assert_eq!(migrate_metadata_json_to_jsonb(&mut conn).unwrap(), 1);
        assert_eq!(count(&mut conn, "json_valid(licenses, 8)"), 1);
        assert_eq!(user_version(&mut conn), 20260817);
    }
}
