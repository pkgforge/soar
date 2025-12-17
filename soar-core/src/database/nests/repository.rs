use rusqlite::{params, Result};
use soar_registry::Nest;

use crate::{database::models::FromRow, error::SoarError, SoarResult};

pub fn add(tx: &rusqlite::Transaction, nest: &Nest) -> Result<()> {
    tx.execute(
        "INSERT INTO nests (name, url) VALUES (?1, ?2)",
        params![nest.name, nest.url],
    )?;
    Ok(())
}

pub fn list(tx: &rusqlite::Transaction) -> Result<Vec<Nest>> {
    let mut stmt = tx.prepare("SELECT id, name, url FROM nests")?;
    let nests = stmt
        .query_map([], Nest::from_row)?
        .filter_map(|n| {
            match n {
                Ok(nest) => Some(nest),
                Err(err) => {
                    eprintln!("Nest map error: {err:#?}");
                    None
                }
            }
        })
        .collect();
    Ok(nests)
}

pub fn remove(tx: &rusqlite::Transaction, name: &str) -> SoarResult<()> {
    let full_name = format!("nest-{name}");
    let result = tx.execute("DELETE FROM nests WHERE name = ?1", params![full_name])?;
    if result == 0 {
        return Err(SoarError::Custom(format!(
            "No nest found with name `{name}`",
        )));
    }
    Ok(())
}
