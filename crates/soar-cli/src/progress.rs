use std::sync::atomic::Ordering;

use indicatif::{HumanBytes, ProgressBar, ProgressState, ProgressStyle};
use nu_ansi_term::Color::Red;
use soar_config::display::ProgressStyle as ConfigProgressStyle;
use soar_dl::types::Progress;

use crate::{
    install::InstallContext,
    utils::{display_settings, progress_enabled, Colored},
};

const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

pub fn create_progress_bar() -> ProgressBar {
    let progress_bar = ProgressBar::new(0);
    if !progress_enabled() {
        progress_bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        return progress_bar;
    }
    let style = get_progress_style();
    progress_bar.set_style(style);
    progress_bar
}

fn get_progress_style() -> ProgressStyle {
    let settings = display_settings();

    match settings.progress_style() {
        ConfigProgressStyle::Modern => {
            ProgressStyle::with_template(
                "{spinner:.cyan} {prefix} [{wide_bar:.green/dim}] {bytes_per_sec:>12} {computed_bytes:>22} ETA: {eta}",
            )
            .unwrap()
            .with_key("computed_bytes", format_bytes)
            .tick_chars(SPINNER_CHARS)
            .progress_chars("━━─")
        }
        ConfigProgressStyle::Classic => {
            ProgressStyle::with_template(
                "{prefix} [{wide_bar}] {bytes_per_sec:>12} {computed_bytes:>22}",
            )
            .unwrap()
            .with_key("computed_bytes", format_bytes)
            .progress_chars("=>-")
        }
        ConfigProgressStyle::Minimal => {
            ProgressStyle::with_template(
                "{prefix} {percent:>3}% ({computed_bytes})",
            )
            .unwrap()
            .with_key("computed_bytes", format_bytes)
        }
    }
}

pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();

    if !progress_enabled() {
        spinner.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        return spinner;
    }

    let settings = display_settings();
    if settings.spinners() {
        spinner.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars(SPINNER_CHARS),
        );
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    } else {
        spinner.set_style(ProgressStyle::with_template("{msg}").unwrap());
    }

    spinner.set_message(message.to_string());
    spinner
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

pub fn handle_progress(state: Progress, progress_bar: &ProgressBar) {
    match state {
        Progress::Starting {
            total,
        } => {
            progress_bar.set_length(total);
        }
        Progress::Resuming {
            current,
            total,
        } => {
            progress_bar.set_length(total);
            progress_bar.set_position(current);
        }
        Progress::Chunk {
            current, ..
        } => {
            progress_bar.set_position(current);
        }
        Progress::Complete {
            ..
        } => progress_bar.finish(),
        _ => {}
    }
}

pub fn handle_install_progress(
    state: Progress,
    progress_bar: &mut Option<ProgressBar>,
    ctx: &InstallContext,
    prefix: &str,
) {
    if progress_bar.is_none() {
        let pb = ctx
            .multi_progress
            .insert_from_back(1, create_progress_bar());
        pb.set_prefix(prefix.to_string());
        *progress_bar = Some(pb);
    }

    match state {
        Progress::Starting {
            total,
        } => {
            if let Some(pb) = progress_bar {
                pb.set_length(total);
            }
        }
        Progress::Resuming {
            current,
            total,
        } => {
            if let Some(pb) = progress_bar {
                pb.set_length(total);
                pb.set_position(current);
            }
        }
        Progress::Chunk {
            current, ..
        } => {
            if let Some(pb) = progress_bar {
                pb.set_position(current);
            }
        }
        Progress::Complete {
            ..
        } => {
            if let Some(pb) = progress_bar.take() {
                pb.finish();
            }
        }
        Progress::Error => {
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
        Progress::Aborted => {
            let failed_count = ctx.failed.fetch_add(1, Ordering::Relaxed);
            if let Some(pb) = progress_bar {
                pb.set_style(ProgressStyle::with_template("{prefix} {msg}").unwrap());
                pb.set_prefix(prefix.to_string());
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
        Progress::Recovered => {
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
