use std::sync::atomic::Ordering;

use indicatif::{HumanBytes, ProgressBar, ProgressState, ProgressStyle};
use nu_ansi_term::Color::Red;
use soar_core::database::models::Package;
use soar_dl::downloader::DownloadState;

use crate::{install::InstallContext, utils::Colored};

pub fn create_progress_bar() -> ProgressBar {
    let progress_bar = ProgressBar::new(0);
    let style = ProgressStyle::with_template(
        "{prefix} [{wide_bar:.green/white}] {bytes_per_sec:14} {computed_bytes:22}",
    )
    .unwrap()
    .with_key("computed_bytes", format_bytes)
    .progress_chars("━━");
    progress_bar.set_style(style);
    progress_bar
}

fn format_bytes(state: &ProgressState, w: &mut dyn std::fmt::Write) {
    write!(
        w,
        "{}/{}",
        HumanBytes(state.pos()),
        HumanBytes(state.len().unwrap_or(state.pos()))
    )
    .unwrap();
}

pub fn handle_progress(state: DownloadState, progress_bar: &ProgressBar) {
    match state {
        DownloadState::Preparing(total) => {
            progress_bar.set_length(total);
        }
        DownloadState::Progress(progress) => {
            progress_bar.set_position(progress);
        }
        DownloadState::Complete => progress_bar.finish(),
        _ => {}
    }
}

pub fn handle_install_progress(
    state: DownloadState,
    progress_bar: &mut Option<ProgressBar>,
    ctx: &InstallContext,
    package: &Package,
    idx: usize,
    fixed_width: usize,
) {
    if progress_bar.is_none() {
        let pb = ctx
            .multi_progress
            .insert_from_back(1, create_progress_bar());

        let prefix = format!(
            "[{}/{}] {}#{}",
            idx + 1,
            ctx.total_packages,
            package.pkg_name,
            package.pkg_id
        );
        let prefix = if prefix.len() > fixed_width {
            format!("{:.width$}", prefix, width = fixed_width)
        } else {
            format!("{:<width$}", prefix, width = fixed_width)
        };
        pb.set_prefix(prefix);

        *progress_bar = Some(pb);
    }

    match state {
        DownloadState::Preparing(total) => {
            if let Some(pb) = progress_bar {
                pb.set_length(total);
            }
        }
        DownloadState::Progress(progress) => {
            if let Some(pb) = progress_bar {
                pb.set_position(progress);
            }
        }
        DownloadState::Complete => {
            if let Some(pb) = progress_bar.take() {
                pb.finish();
            }
        }
        DownloadState::Error => {
            let count = ctx.retrying.fetch_add(1, Ordering::Relaxed);
            let failed_count = ctx.failed.load(Ordering::Relaxed);
            ctx.total_progress_bar.set_message(format!(
                "(Retrying: {}){}",
                Colored(Red, count + 1),
                if failed_count > 0 {
                    format!(" (Failed: {})", Colored(Red, failed_count))
                } else {
                    String::new()
                },
            ));
        }
        DownloadState::Aborted => {
            let failed_count = ctx.failed.fetch_add(1, Ordering::Relaxed);
            if let Some(pb) = progress_bar {
                let prefix = format!(
                    "[{}/{}] {}#{}",
                    idx + 1,
                    ctx.total_packages,
                    package.pkg_name,
                    package.pkg_id
                );
                pb.set_style(ProgressStyle::with_template("{prefix} {msg}").unwrap());
                pb.set_prefix(prefix);
                pb.finish_with_message(format!(
                    "\n  {}",
                    Colored(Red, "└── Error: Too many failures. Aborted.")
                ));

                let count = ctx.retrying.fetch_sub(1, Ordering::Relaxed);
                if count > 1 {
                    ctx.total_progress_bar.set_message(format!(
                        "(Retrying: {}) (Failed: {})",
                        Colored(Red, count - 1),
                        Colored(Red, failed_count + 1)
                    ));
                } else {
                    ctx.total_progress_bar.set_message("");
                }
            }
        }
        DownloadState::Recovered => {
            let count = ctx.retrying.fetch_sub(1, Ordering::Relaxed);
            let failed_count = ctx.failed.load(Ordering::Relaxed);
            if count > 1 || failed_count > 0 {
                ctx.total_progress_bar.set_message(format!(
                    "(Retrying: {}){}",
                    Colored(Red, count - 1),
                    if failed_count > 0 {
                        format!(" (Failed: {})", Colored(Red, failed_count))
                    } else {
                        String::new()
                    },
                ));
            } else {
                ctx.total_progress_bar.set_message("");
            }
        }
    }
}
