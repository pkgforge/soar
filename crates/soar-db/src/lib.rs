pub mod expr;
pub mod helpers;
pub mod macros;
pub mod query;
pub mod traits;

pub use helpers::*;
pub use query::*;
pub use traits::FromRow;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::{Connection, Row};

    use super::*;
    use crate::traits::Expression as _;

    #[derive(Debug, Clone)]
    struct Package {
        pub id: u64,
        pub name: String,
        pub version: String,
        pub downloads: u64,
        pub description: Option<String>,
        pub maintainers: Option<Vec<String>>,
    }

    impl FromRow for Package {
        fn from_row(row: &Row) -> rusqlite::Result<Self> {
            Ok(Self {
                id: row.get("id")?,
                name: row.get("name")?,
                version: row.get("version")?,
                downloads: row.get("downloads")?,
                description: row.get("description")?,
                maintainers: from_optional_json(row.get("maintainers")),
            })
        }
    }

    #[derive(Debug, Clone)]
    struct PackageWithName {
        pub name: String,
    }

    impl FromRow for PackageWithName {
        fn from_row(row: &Row) -> rusqlite::Result<Self> {
            Ok(Self {
                name: row.get("name")?,
            })
        }
    }

    define_entity!(
        packages {
            table: "packages",
            columns: {
                ID: i64 => "id",
                NAME: String => "name",
                VERSION: String => "version",
                DOWNLOADS: u64 => "downloads",
                DESCRIPTION: Option<String> => "description",
                MAINTAINERS: Option<Vec<String>> => "maintainers"
            }
        }
    );

    fn setup_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE packages (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                downloads INTEGER NOT NULL DEFAULT 0,
                maintainers JSONB,
                description TEXT
            )",
            [],
        )
        .unwrap();

        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn test_insert() {
        let db = setup_db();

        let maintainers = vec!["John Doe", "Jane Smith"]
            .into_iter()
            .map(|v| v.to_string())
            .collect();

        let id = InsertQuery::into(db.clone(), packages::TABLE)
            .set(packages::NAME, "soar".to_string())
            .set(packages::VERSION, "1.0.0".to_string())
            .set(packages::DOWNLOADS, 100000)
            .set(packages::DESCRIPTION, "Test description".to_string())
            .set(packages::MAINTAINERS, to_json(&maintainers))
            .execute()
            .unwrap();

        assert!(id > 0);

        let pkg = SelectQuery::<Package>::from(db, packages::TABLE)
            .filter(packages::ID.eq(id))
            .fetch_one()
            .unwrap()
            .unwrap();

        assert_eq!(pkg.name, "soar");
        assert_eq!(pkg.version, "1.0.0");
        assert_eq!(pkg.downloads, 100000);
        assert_eq!(pkg.description, Some("Test description".into()));
        assert_eq!(pkg.maintainers, Some(maintainers));
    }

    #[test]
    fn test_select_with_like() {
        let db = setup_db();

        InsertQuery::into(db.clone(), packages::TABLE)
            .set(packages::NAME, "zls".to_string())
            .set(packages::VERSION, "0.15.1".to_string())
            .set(packages::DESCRIPTION, "Zig Language Server".to_string())
            .execute()
            .unwrap();

        InsertQuery::into(db.clone(), packages::TABLE)
            .set(packages::NAME, "rust-analyzer".to_string())
            .set(packages::VERSION, "1.92.0-nightly".to_string())
            .set(packages::DESCRIPTION, "Rusty Language Server".to_string())
            .execute()
            .unwrap();

        let pkgs = SelectQuery::<PackageWithName>::from(db, packages::TABLE)
            .select(&[packages::NAME])
            .filter(packages::NAME.like("rust"))
            .fetch()
            .unwrap();

        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "rust-analyzer");
    }
}
