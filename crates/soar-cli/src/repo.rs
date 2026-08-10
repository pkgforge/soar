use soar_config::repository::Repository;
use soar_core::SoarResult;
use soar_operations::{repo::RepoUpdate, SoarContext};
use tracing::info;

use crate::{
    cli::RepoAction,
    json_output::{self, Listing, RepositoryJson},
    utils::json_enabled,
};

pub fn handle_repo_action(ctx: &SoarContext, action: RepoAction) -> SoarResult<()> {
    match action {
        RepoAction::Add {
            name,
            url,
            pubkey,
            enabled,
            desktop_integration,
            signature_verification,
            sync_interval,
        } => {
            ctx.add_repository(Repository {
                name: name.clone(),
                url,
                pubkey,
                enabled,
                desktop_integration,
                signature_verification,
                sync_interval,
            })?;
            info!("Repository '{}' added successfully.", name);
        }
        RepoAction::Update {
            name,
            url,
            pubkey,
            enabled,
            desktop_integration,
            signature_verification,
            sync_interval,
        } => {
            ctx.update_repository(
                &name,
                RepoUpdate {
                    url,
                    pubkey,
                    enabled,
                    desktop_integration,
                    signature_verification,
                    sync_interval,
                },
            )?;
            info!("Repository '{}' updated successfully.", name);
        }
        RepoAction::Remove {
            name,
        } => {
            ctx.remove_repository(&name)?;
            info!("Repository '{}' removed successfully.", name);
        }
        RepoAction::List => {
            let config = soar_config::config::get_config();

            if json_enabled() {
                let items: Vec<RepositoryJson> =
                    config.repositories.iter().map(Into::into).collect();
                json_output::emit(&Listing::new(items, config.repositories.len()));
                return Ok(());
            }

            if config.repositories.is_empty() {
                info!("No repositories configured.");
            } else {
                for repo in &config.repositories {
                    let status = if repo.is_enabled() {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    info!("{} ({}) - {}", repo.name, status, repo.url);
                }
            }
        }
    }
    Ok(())
}
