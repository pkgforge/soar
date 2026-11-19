//! The live job strip above the status bar, showing what runs from any
//! view.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::App,
    format,
    jobs::{Job, JobKind, JobState},
    progress::{OpRow, Phase},
    theme::Theme,
    widgets::gauge,
};

/// How many operation lines the strip shows before folding the rest into a
/// count.
const MAX_OP_LINES: usize = 4;

/// How wide a progress gauge tries to be before it yields space to text.
const GAUGE_WIDTH: u16 = 24;

/// Draws the strip: a separating rule, the job's headline with its overall
/// progress, and up to four operation lines.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let mut rows = Rows::new(area);

    let rule = rows.next(area.width);
    f.render_widget(
        Paragraph::new(Span::styled(
            theme.sym().gauge_empty.repeat(area.width as usize),
            theme.muted(),
        )),
        rule,
    );

    if let Some(job) = app.running_job() {
        render_summary(f, rows.next(area.width), job, app);
        if job.kind == JobKind::Sync {
            let syncing: Vec<&(String, String)> = job.tracker.sync.iter().collect();
            for (repo, stage) in syncing.iter().take(MAX_OP_LINES) {
                render_sync(f, rows.next(area.width), repo, stage, theme);
            }
            if syncing.len() > MAX_OP_LINES {
                let text = format!("+{} more", syncing.len() - MAX_OP_LINES);
                f.render_widget(
                    Paragraph::new(text).style(theme.muted()),
                    rows.next(area.width),
                );
            }
        } else {
            let active: Vec<&OpRow> = job.tracker.active().collect();
            for op in active.iter().take(MAX_OP_LINES) {
                render_op(f, rows.next(area.width), op, theme, area.width);
            }
            if active.len() > MAX_OP_LINES {
                let text = format!("+{} more", active.len() - MAX_OP_LINES);
                f.render_widget(
                    Paragraph::new(text).style(theme.muted()),
                    rows.next(area.width),
                );
            }
        }
    } else if let Some((job, _)) = app
        .jobs
        .list
        .iter()
        .filter_map(|job| {
            match &job.state {
                JobState::Finished {
                    ended, ..
                } => Some((job, ended)),
                _ => None,
            }
        })
        .max_by_key(|(_, ended)| *ended)
    {
        render_finished(f, rows.next(area.width), job, theme);
    } else {
        render_background(f, &mut rows, app, area.width);
    }
}

/// One repository line while a sync job runs: the repository name, then
/// its stage, with the finished stages in the success color.
fn render_sync(f: &mut Frame, row: Rect, repo: &str, stage: &str, theme: &Theme) {
    let symbols = theme.sym();
    let name = format::truncate(repo, 12, symbols.ellipsis);
    let mut line = vec![
        Span::raw("   "),
        Span::styled(format!("{name:<12}"), theme.text()),
    ];
    if matches!(stage, "done" | "up to date") {
        line.push(Span::styled(stage.to_string(), theme.success()));
    } else {
        line.push(Span::styled(stage.to_string(), theme.muted()));
    }
    f.render_widget(Paragraph::new(Line::from(line)), row);
}

/// The background lines: one per repository still syncing, then one
/// summary line while other soar work runs outside the queue.
fn render_background(f: &mut Frame, rows: &mut Rows, app: &App, width: u16) {
    let theme = &app.theme;
    let syncing: Vec<&(String, String)> = app.background.active_sync().collect();
    for (repo, stage) in syncing.iter().take(MAX_OP_LINES) {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled("Syncing ", theme.muted()),
            Span::styled(repo.clone(), theme.info()),
            Span::raw(": "),
            Span::styled(stage.clone(), theme.muted()),
        ]);
        f.render_widget(Paragraph::new(line), rows.next(width));
    }
    if syncing.len() > MAX_OP_LINES {
        let text = format!("+{} more", syncing.len() - MAX_OP_LINES);
        f.render_widget(Paragraph::new(text).style(theme.muted()), rows.next(width));
    }
    if let Some(op) = app.background.active().next() {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(theme.spinner(app.frame), theme.accent()),
            Span::raw(" "),
            Span::styled("Preparing ", theme.muted()),
            Span::styled(op.name.clone(), theme.text()),
        ]);
        f.render_widget(Paragraph::new(line), rows.next(width));
    }
}

/// The summary line: spinner, title, overall gauge, counts, and the abort
/// hint on the right.
fn render_summary(f: &mut Frame, row: Rect, job: &Job, app: &App) {
    let theme = &app.theme;
    let JobState::Running {
        aborting, ..
    } = job.state
    else {
        return;
    };
    let symbols = theme.sym();

    let spinner_style = if aborting {
        theme.warning()
    } else {
        theme.accent()
    };
    let mut left = vec![
        Span::raw(" "),
        Span::styled(theme.spinner(app.frame), spinner_style),
        Span::raw(" "),
        Span::styled(job.title.clone(), theme.title()),
    ];

    let hint = if aborting {
        Some(Line::from(Span::styled(
            format!("aborting{}", symbols.ellipsis),
            theme.warning(),
        )))
    } else if job.kind != JobKind::Sync {
        Some(Line::from(Span::styled("ctrl-x abort", theme.muted())))
    } else {
        None
    };
    let hint_width = hint.as_ref().map_or(0, Line::width) as u16;

    let mut fits = row.width.saturating_sub(hint_width + 1) as usize;
    if let Some(ratio) = job_ratio(job) {
        let gauge_width = GAUGE_WIDTH.min(fits.saturating_sub(2) as u16);
        if gauge_width > 3 {
            left.push(Span::raw("  "));
            left.extend(gauge::line(ratio, gauge_width, theme).spans);
            fits = fits.saturating_sub(gauge_width as usize + 2);
        }
    }

    for (i, part) in job_counts(job, app, theme).into_iter().enumerate() {
        let separator = if i == 0 { "  " } else { " · " };
        let width = Span::raw(separator).width() + part.width();
        if width > fits {
            break;
        }
        left.push(Span::styled(separator, theme.muted()));
        left.push(part);
        fits -= width;
    }

    f.render_widget(Paragraph::new(Line::from(left)), row);
    if let Some(hint) = hint {
        f.render_widget(Paragraph::new(hint).alignment(Alignment::Right), row);
    }
}

/// The overall progress parts of the summary line: the counts when a batch
/// runs, then the queue depth. Trailing parts drop off a narrow screen
/// first.
fn job_counts(job: &Job, app: &App, theme: &Theme) -> Vec<Span<'static>> {
    let mut parts = Vec::new();
    if let Some((completed, total, failed)) = job.tracker.batch {
        parts.push(Span::styled(format!("{completed}/{total}"), theme.text()));
        parts.push(Span::styled(
            format!("{failed} failed"),
            if failed > 0 {
                theme.danger()
            } else {
                theme.muted()
            },
        ));
    }
    let queued = app.queued_count();
    if queued > 0 {
        parts.push(Span::styled(format!("{queued} queued"), theme.muted()));
    }
    parts
}

/// The overall fill ratio, from the batch counts or a lone download.
fn job_ratio(job: &Job) -> Option<f64> {
    if let Some((completed, total, _)) = job.tracker.batch {
        return (total > 0).then(|| f64::from(completed) / f64::from(total));
    }
    let mut active = job.tracker.active();
    let lone = active.next()?;
    if active.next().is_some() {
        return None;
    }
    let Phase::Downloading {
        current,
        total,
    } = lone.phase
    else {
        return None;
    };
    (total > 0).then_some(current as f64 / total as f64)
}

/// One operation line: the name, then where its work is. The activity
/// strip and the jobs detail share it.
pub(crate) fn render_op(f: &mut Frame, row: Rect, op: &OpRow, theme: &Theme, width: u16) {
    let symbols = theme.sym();
    let name = format::truncate(&op.name, 12, symbols.ellipsis);
    let mut line = vec![
        Span::raw("   "),
        Span::styled(format!("{name:<12}"), theme.text()),
    ];

    match &op.phase {
        Phase::Waiting => line.push(Span::styled("waiting", theme.muted())),
        Phase::Step(label) => {
            line.push(Span::styled(label.clone(), theme.muted()));
            if let Some(output) = &op.last_output {
                let room = width.saturating_sub(spans_width(&line) as u16 + 1) as usize;
                if room > 2 {
                    let text = format::truncate(output, room, symbols.ellipsis);
                    line.push(Span::raw(" "));
                    line.push(Span::styled(
                        text,
                        theme.muted().add_modifier(Modifier::DIM),
                    ));
                }
            }
        }
        Phase::Downloading {
            current,
            total,
        } => {
            let ratio = if *total > 0 {
                *current as f64 / *total as f64
            } else {
                0.0
            };
            let mut tail = format!(" {}/{}", format::size(*current), format::size(*total));
            if let Some(speed) = op.speed {
                tail.push_str(&format!(" {}", format::speed(speed)));
            }
            if let Some(eta) = op.eta() {
                tail.push_str(&format!(" {}", format::eta(eta)));
            }
            let room = width.saturating_sub(spans_width(&line) as u16) as usize;
            let tail_width = Span::raw(tail.as_str()).width();
            let gauge_width = 30u16.min(room.saturating_sub(tail_width + 1) as u16);
            if gauge_width > 3 {
                line.push(Span::raw(" "));
                line.extend(gauge::line(ratio, gauge_width, theme).spans);
            }
            line.push(Span::raw(tail));
        }
        Phase::Done => {
            line.push(Span::styled(symbols.installed, theme.success()));
            line.push(Span::styled(" done", theme.success()));
        }
        Phase::Failed(error) => {
            let room = width.saturating_sub(spans_width(&line) as u16 + 2) as usize;
            let text = format::truncate(error, room, symbols.ellipsis);
            line.push(Span::styled(symbols.error, theme.danger()));
            line.push(Span::raw(" "));
            line.push(Span::styled(text, theme.danger()));
        }
        Phase::Skipped => {
            line.push(Span::styled(symbols.available, theme.muted()));
            line.push(Span::styled(" skipped", theme.muted()));
        }
    }

    f.render_widget(Paragraph::new(Line::from(line)), row);
}

/// The one-line summary a finished job leaves behind for three seconds.
fn render_finished(f: &mut Frame, row: Rect, job: &Job, theme: &Theme) {
    let symbols = theme.sym();
    let JobState::Finished {
        started,
        ended,
        outcome,
    } = &job.state
    else {
        return;
    };
    let duration = ended.duration_since(*started);
    let line = if outcome.aborted {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(symbols.warning, theme.warning()),
            Span::styled(format!(" {}", outcome.headline), theme.warning()),
            Span::styled(format!(" in {}", format::clock(duration)), theme.muted()),
        ])
    } else if outcome.ok {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(symbols.installed, theme.success()),
            Span::styled(format!(" {}", outcome.headline), theme.success()),
            Span::styled(format!(" in {}", format::clock(duration)), theme.muted()),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(symbols.error, theme.danger()),
            Span::styled(format!(" {} (see Jobs)", outcome.headline), theme.danger()),
        ])
    };
    f.render_widget(Paragraph::new(line), row);
}

/// The display width of a run of spans.
fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Hands out one-row slices of the strip's area, top to bottom.
struct Rows {
    x: u16,
    next_y: u16,
    bottom: u16,
}

impl Rows {
    /// Starts above the first row of `area`.
    fn new(area: Rect) -> Rows {
        Rows {
            x: area.x,
            next_y: area.y,
            bottom: area.y + area.height,
        }
    }

    /// Returns the next row, clipped to nothing once the area is spent so
    /// extra renders draw nowhere.
    fn next(&mut self, width: u16) -> Rect {
        let y = self.next_y;
        self.next_y += 1;
        Rect {
            x: self.x,
            y,
            width: if y < self.bottom { width } else { 0 },
            height: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use soar_events::{OperationId, SoarEvent, SyncStage};

    use crate::{
        app::App,
        jobs::{Job, JobKind, JobOutcome, JobState},
        msg::Msg,
        progress::Tracker,
        test_support::{screen, test_app},
    };

    /// A running sync job whose tracker holds one stage line per
    /// repository.
    fn running_sync_app(repos: &[(&str, &str)]) -> App {
        let mut app = test_app();
        let mut tracker = Tracker::default();
        let now = Instant::now();
        for (repo, stage) in repos {
            let sync_stage = match *stage {
                "fetching" => SyncStage::Fetching,
                "done" => {
                    SyncStage::Complete {
                        package_count: None,
                    }
                }
                "up to date" => SyncStage::UpToDate,
                "decompressing" => SyncStage::Decompressing,
                "writing" => SyncStage::WritingDatabase,
                "validating" => SyncStage::Validating,
                other => panic!("unknown stage {other}"),
            };
            tracker.apply(
                &SoarEvent::SyncProgress {
                    repo_name: repo.to_string(),
                    stage: sync_stage,
                },
                now,
            );
        }
        app.jobs.list.push(Job {
            id: 0,
            kind: JobKind::Sync,
            title: "Sync repositories".to_string(),
            state: JobState::Running {
                started: now,
                aborting: false,
            },
            spec: None,
            tracker,
        });
        app
    }

    fn running_app_with_ops(count: usize) -> App {
        let mut app = test_app();
        let mut tracker = Tracker::default();
        let now = Instant::now();
        for i in 0..count as OperationId {
            tracker.apply(
                &SoarEvent::DownloadPreparing {
                    op_id: i,
                    pkg_name: format!("pkg{i}"),
                },
                now,
            );
        }
        app.jobs.list.push(Job {
            id: 0,
            kind: JobKind::Install,
            title: format!("Install {count} packages"),
            state: JobState::Running {
                started: now,
                aborting: false,
            },
            spec: None,
            tracker,
        });
        app
    }

    #[test]
    fn a_running_job_with_six_active_ops_folds_the_last_two() {
        let mut app = running_app_with_ops(6);
        assert_eq!(app.activity_height(), 7);
        assert!(screen(&mut app, 80, 24).contains("+2 more"));
    }

    #[test]
    fn an_idle_screen_shows_no_strip() {
        let app = test_app();
        assert_eq!(app.activity_height(), 0);
    }

    #[test]
    fn a_failed_job_shows_a_see_jobs_summary_for_three_seconds() {
        let mut app = test_app();
        let now = Instant::now();
        app.jobs.list.push(Job {
            id: 0,
            kind: JobKind::Install,
            title: "Install 1 package".into(),
            state: JobState::Finished {
                started: now,
                ended: now,
                outcome: JobOutcome {
                    ok: false,
                    aborted: false,
                    headline: "Install finished with 1 failure".into(),
                    lines: Vec::new(),
                },
            },
            spec: None,
            tracker: Tracker::default(),
        });
        app.activity_until = Some(now + Duration::from_secs(3));
        assert_eq!(app.activity_height(), 2);
        assert!(screen(&mut app, 80, 24).contains("(see Jobs)"));
        app.update(Msg::Tick, now + Duration::from_secs(4));
        assert_eq!(app.activity_height(), 0);
    }

    #[test]
    fn an_aborting_job_shows_the_warning_hint_on_the_strip() {
        let mut app = running_app_with_ops(1);
        if let Some(job) = app.running_job_mut()
            && let JobState::Running {
                aborting, ..
            } = &mut job.state
        {
            *aborting = true;
        }
        let hint = format!("aborting{}", app.theme.sym().ellipsis);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains(&hint), "screen is {screen:?}");
    }

    #[test]
    fn a_running_sync_job_shows_one_line_per_repository() {
        let mut app = running_sync_app(&[
            ("bincache", "fetching"),
            ("pkgforge", "done"),
            ("isolated", "up to date"),
        ]);
        let screen = screen(&mut app, 80, 24);
        for text in [
            "bincache    fetching",
            "pkgforge    done",
            "isolated    up to date",
        ] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_running_sync_job_folds_past_four_repositories() {
        let repos: Vec<(String, String)> = (0..6)
            .map(|i| (format!("repo{i}"), "fetching".to_string()))
            .collect();
        let refs: Vec<(&str, &str)> = repos
            .iter()
            .map(|(r, s)| (r.as_str(), s.as_str()))
            .collect();
        let mut app = running_sync_app(&refs);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("repo3"), "screen is {screen:?}");
        assert!(!screen.contains("repo4"), "screen is {screen:?}");
        assert!(screen.contains("+2 more"), "screen is {screen:?}");
    }

    #[test]
    fn a_background_sync_shows_one_line_per_repo() {
        let mut app = test_app();
        let now = Instant::now();
        for (repo, stage) in [
            ("bincache", SyncStage::Fetching),
            ("pkgforge", SyncStage::Decompressing),
        ] {
            app.update(
                Msg::Soar(SoarEvent::SyncProgress {
                    repo_name: repo.to_string(),
                    stage,
                }),
                now,
            );
        }
        assert_eq!(app.activity_height(), 3);
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Syncing bincache: fetching"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("Syncing pkgforge: decompressing"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_finished_sync_leaves_no_background_lines() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(
            Msg::Soar(SoarEvent::SyncProgress {
                repo_name: "bincache".to_string(),
                stage: SyncStage::Complete {
                    package_count: Some(1200),
                },
            }),
            now,
        );
        assert_eq!(app.activity_height(), 0);
    }

    #[test]
    fn a_background_sync_marks_the_screen_dirty_for_the_next_frame() {
        let mut app = test_app();
        app.dirty = false;
        app.update(
            Msg::Soar(SoarEvent::SyncProgress {
                repo_name: "bincache".to_string(),
                stage: SyncStage::Fetching,
            }),
            Instant::now(),
        );
        assert!(app.dirty);
    }
}
