use indicatif::{HumanBytes, ProgressBar, ProgressState, ProgressStyle};
use soar_dl::downloader::DownloadState;

pub fn create_progress_bar() -> ProgressBar {
    let progress_bar = ProgressBar::new(0);
    let style = ProgressStyle::with_template(
        "{msg}[{wide_bar:.green/white}] {speed:14} {computed_bytes:22}",
    )
    .unwrap()
    .with_key("computed_bytes", format_bytes)
    .with_key("speed", format_speed)
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

fn format_speed(state: &ProgressState, w: &mut dyn std::fmt::Write) {
    let speed = calculate_speed(state.pos(), state.elapsed().as_secs_f64());
    write!(w, "{}/s", HumanBytes(speed)).unwrap();
}

fn calculate_speed(pos: u64, elapsed: f64) -> u64 {
    if elapsed > 0.0 {
        (pos as f64 / elapsed) as u64
    } else {
        0
    }
}

pub fn handle_progress(state: DownloadState, progress_bar: &ProgressBar) {
    match state {
        DownloadState::Progress(progress) => {
            if let Some(total) = progress.total_bytes {
                progress_bar.set_length(total);
            }
            progress_bar.set_position(progress.bytes_downloaded);
        }
        DownloadState::Complete => progress_bar.finish(),
    }
}
