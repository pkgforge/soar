use soar_core::{error::SoarError, utils::get_nests_db_conn, SoarResult};
use soar_db::{models::nest::NewNest, repository::nest::NestRepository};

pub async fn add_nest(name: &str, url: &str) -> SoarResult<()> {
    let full_name = format!("nest-{name}");
    let mut conn = get_nests_db_conn()?;

    let nest = NewNest {
        name: &full_name,
        url,
    };
    NestRepository::insert(conn.conn(), &nest)
        .map_err(|e| SoarError::Custom(format!("Failed to add nest: {}", e)))?;
    println!("Added nest: {}", name);
    Ok(())
}

pub async fn remove_nest(name: &str) -> SoarResult<()> {
    let full_name = format!("nest-{name}");
    let mut conn = get_nests_db_conn()?;

    let deleted = NestRepository::delete_by_name(conn.conn(), &full_name)
        .map_err(|e| SoarError::Custom(format!("Failed to remove nest: {}", e)))?;

    if deleted == 0 {
        return Err(SoarError::Custom(format!(
            "No nest found with name `{name}`"
        )));
    }
    println!("Removed nest: {}", name);
    Ok(())
}

pub async fn list_nests() -> SoarResult<()> {
    let mut conn = get_nests_db_conn()?;

    let nests = NestRepository::list_all(conn.conn())
        .map_err(|e| SoarError::Custom(format!("Failed to list nests: {}", e)))?;

    for nest in nests {
        let display_name = nest.name.strip_prefix("nest-").unwrap_or(&nest.name);
        println!("{} - {}", display_name, nest.url);
    }
    Ok(())
}
