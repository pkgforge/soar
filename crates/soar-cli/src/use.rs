use nu_ansi_term::Color::{Blue, Cyan, Magenta, Red};
use soar_core::SoarResult;
use soar_operations::{switch, SoarContext};
use soar_utils::bytes::format_bytes;
use tracing::info;

use crate::utils::{get_valid_selection, Colored};

pub async fn use_alternate_package(ctx: &SoarContext, name: &str) -> SoarResult<()> {
    let variants = switch::list_variants(ctx, name)?;

    if variants.is_empty() {
        info!("Package is not installed");
        return Ok(());
    }

    for (idx, variant) in variants.iter().enumerate() {
        let package = &variant.package;
        info!(
            active = variant.is_active,
            pkg_name = package.pkg_name,
            pkg_id = package.pkg_id,
            repo_name = package.repo_name,
            pkg_type = package.pkg_type,
            version = package.version,
            size = package.size,
            "[{}] {}#{}:{} ({}-{}) ({}){}",
            idx + 1,
            Colored(Blue, &package.pkg_name),
            Colored(Cyan, &package.pkg_id),
            Colored(Cyan, &package.repo_name),
            package
                .pkg_type
                .as_ref()
                .map(|pkg_type| format!(":{}", Colored(Magenta, &pkg_type)))
                .unwrap_or_default(),
            Colored(Magenta, &package.version),
            Colored(Magenta, format_bytes(package.size, 2)),
            if variant.is_active {
                format!(" {}", Colored(Red, "*"))
            } else {
                String::new()
            }
        );
    }

    if variants.len() == 1 {
        return Ok(());
    }

    let selection = get_valid_selection(variants.len())?;
    switch::switch_variant(ctx, name, selection).await?;

    info!(
        "Switched to {}#{}",
        variants[selection].package.pkg_name, variants[selection].package.pkg_id
    );

    Ok(())
}
