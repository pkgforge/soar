use soar_config::config::get_config;
use soar_core::{
    database::nests::repository,
    utils::get_nests_db_conn,
    SoarResult,
};
use soar_registry::Nest;

pub async fn add_nest(name: &str, url: &str) -> SoarResult<()> {
    let name = format!("nest-{name}");
    let config = get_config();
    let mut conn = get_nests_db_conn(&config)?;
    let tx = conn.transaction()?;
    let nest = Nest {
        id: 0,
        name: name.to_string(),
        url: url.to_string(),
    };
    repository::add(&tx, &nest)?;
    tx.commit()?;
    println!("Added nest: {}", name);
    Ok(())
}

pub async fn remove_nest(name: &str) -> SoarResult<()> {
    let config = get_config();
    let mut conn = get_nests_db_conn(&config)?;
    let tx = conn.transaction()?;
    repository::remove(&tx, name)?;
    tx.commit()?;
    println!("Removed nest: {}", name);
    Ok(())
}

pub async fn list_nests() -> SoarResult<()> {
    let config = get_config();
    let mut conn = get_nests_db_conn(&config)?;
    let tx = conn.transaction()?;
    let nests = repository::list(&tx)?;
    for nest in nests {
        println!("{} - {}", nest.name, nest.url);
    }
    Ok(())
}
