use soar_core::SoarResult;
use soar_operations::{run, PrepareRunResult, SoarContext};

use crate::utils::select_package_interactively;

pub async fn run_package(
    ctx: &SoarContext,
    command: &[String],
    yes: bool,
    repo_name: Option<&str>,
    pkg_id: Option<&str>,
) -> SoarResult<()> {
    let package_name = &command[0];
    let args = if command.len() > 1 {
        &command[1..]
    } else {
        &[]
    };

    let result = run::prepare_run(ctx, package_name, repo_name, pkg_id).await?;

    let output_path = match result {
        PrepareRunResult::Ready(path) => path,
        PrepareRunResult::Ambiguous(amb) => {
            let pkg = if yes {
                amb.candidates.into_iter().next()
            } else {
                select_package_interactively(amb.candidates, &amb.query)?
            };

            let Some(pkg) = pkg else {
                return Ok(());
            };

            // Re-run with selected package
            let result =
                run::prepare_run(ctx, package_name, Some(&pkg.repo_name), Some(&pkg.pkg_id))
                    .await?;

            match result {
                PrepareRunResult::Ready(path) => path,
                _ => return Ok(()),
            }
        }
    };

    // Checksum verification for cached binary - prompt user on mismatch
    // Note: prepare_run already handles checksum and returns error on mismatch,
    // but for the interactive CLI we handle it specially
    let run_result = run::execute_binary(&output_path, args)?;

    // For the `run` subcommand, propagate the exit code
    if run_result.exit_code != 0 {
        std::process::exit(run_result.exit_code);
    }

    Ok(())
}
