use soar_registry::Nest;

use crate::database::models::FromRow;

impl FromRow for Nest {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Nest {
            id: row.get("id")?,
            name: row
                .get::<_, String>("name")?
                .strip_prefix("nest-")
                .unwrap()
                .to_string(),
            url: row.get("url")?,
        })
    }
}
