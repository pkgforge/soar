use include_dir::Dir;
use rusqlite::{params, Connection};

use crate::{error::SoarError, SoarResult};

pub struct Migration {
    version: i32,
    description: String,
    sql: String,
}

pub struct MigrationManager {
    conn: Connection,
}

impl MigrationManager {
    pub fn new(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        ",
            [],
        )?;

        Ok(Self { conn })
    }

    fn get_current_version(&self) -> rusqlite::Result<i32> {
        let version = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        );
        version
    }

    fn run_migration(&mut self, migration: &Migration) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;

        match tx.execute_batch(&migration.sql) {
            Ok(_) => {
                tx.execute(
                    "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
                    params![&migration.version, &migration.description],
                )?;
                tx.commit()?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn load_migrations_from_dir(dir: Dir) -> SoarResult<Vec<Migration>> {
        let mut migrations = Vec::new();

        for entry in dir.files() {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("sql") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| SoarError::Custom("Invalid filename".into()))?;

                if !filename.starts_with('V') {
                    continue;
                }

                let parts: Vec<&str> = filename[1..].splitn(2, '_').collect();
                if parts.len() != 2 {
                    continue;
                }

                let version = parts[0].parse::<i32>().map_err(|_| {
                    SoarError::Custom(format!("Invalid version number in filename: {}", filename))
                })?;

                let description = parts[1].replace('_', " ");
                let sql = entry.contents_utf8().unwrap().to_string();

                migrations.push(Migration {
                    version,
                    description,
                    sql,
                });
            }
        }

        migrations.sort_by_key(|m| m.version);

        let mut expected_version = 1;
        for migration in &migrations {
            if migration.version != expected_version {
                return Err(SoarError::Custom(format!(
                    "Invalid migration sequence. Expected version {}, found {}",
                    expected_version, migration.version
                )));
            }
            expected_version += 1;
        }

        Ok(migrations)
    }

    pub fn migrate_from_dir(&mut self, dir: Dir) -> SoarResult<()> {
        let migrations = Self::load_migrations_from_dir(dir)?;
        let current_version = self.get_current_version()?;

        let pending: Vec<&Migration> = migrations
            .iter()
            .filter(|m| m.version > current_version)
            .collect();

        for migration in pending {
            self.run_migration(migration)?;
        }

        Ok(())
    }
}
