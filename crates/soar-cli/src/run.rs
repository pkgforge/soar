use soar_core::SoarResult;
use soar_operations::{run, PrepareRunResult, SoarContext};

use crate::utils::select_package_interactively;

pub async fn run_package(
    ctx: &SoarContext,
    command: &[String],
    yes: bool,
    no_verify: bool,
    repo_name: Option<&str>,
    pkg_id: Option<&str>,
) -> SoarResult<i32> {
    let package_name = &command[0];
    let args = if command.len() > 1 {
        &command[1..]
    } else {
        &[]
    };

    let result = run::prepare_run(ctx, package_name, repo_name, pkg_id, no_verify).await?;

    let downloaded;
    let output_path = match result {
        PrepareRunResult::Ready {
            path,
            downloaded: d,
        } => {
            downloaded = d;
            path
        }
        PrepareRunResult::Ambiguous(amb) => {
            let pkg = if yes {
                amb.candidates.into_iter().next()
            } else {
                select_package_interactively(amb.candidates, &amb.query)?
            };

            let Some(pkg) = pkg else {
                return Ok(0);
            };

            // Run what was chosen. Resolving it by name again would pose
            // the same ambiguous question the choice just answered.
            let query = match pkg.pkg_family {
                Some(ref family) => {
                    format!(
                        "{}/{}@{}:{}",
                        family, pkg.pkg_name, pkg.version, pkg.repo_name
                    )
                }
                None => format!("{}@{}:{}", pkg.pkg_name, pkg.version, pkg.repo_name),
            };
            let result = run::prepare_run(
                ctx,
                &query,
                Some(&pkg.repo_name),
                pkg.pkg_id.as_deref(),
                no_verify,
            )
            .await?;

            match result {
                PrepareRunResult::Ready {
                    path,
                    downloaded: d,
                } => {
                    downloaded = d;
                    path
                }
                _ => return Ok(0),
            }
        }
    };

    // The progress bar leaves the cursor mid-line, so a program that writes
    // straight to stdout would start where the bar stopped.
    if downloaded {
        eprintln!();
    }

    let run_result = run::execute_binary(&output_path, args)?;

    Ok(run_result.exit_code)
}
