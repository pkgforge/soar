//! The jobs view: what the queue has run, the selected job's detail, and
//! the log of everything printed.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::{
    app::App,
    format,
    jobs::{Job, JobState},
    log::LogLevel,
    progress::OpRow,
    theme::Theme,
    views::ViewId,
    widgets::{activity, states},
};

/// Which pane of the jobs view holds the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum JobsFocus {
    /// The job list.
    #[default]
    List,
    /// The selected job's detail.
    Detail,
    /// The log.
    Log,
}

/// The jobs view's cursor state: the selected job, the detail scroll, and
/// which pane holds them.
#[derive(Debug, Default)]
pub(crate) struct JobsView {
    /// The selected row of the job list, which shows jobs newest first.
    pub(crate) cursor: crate::widgets::scroll::ListCursor,
    /// The first detail row shown when the detail is longer than its pane.
    pub(crate) detail: crate::widgets::scroll::ListCursor,
    /// How many rows the detail held at the last render, so scrolling
    /// moves over what the pane actually shows.
    pub(crate) detail_rows: usize,
    /// The pane that scrolling moves.
    pub(crate) focus: JobsFocus,
}

/// The narrowest terminal that shows the list beside the detail.
const SIDE_BY_SIDE_WIDTH: u16 = 100;

/// Draws the three panes: the job list and the selected job's detail on
/// top, the log below.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let [top, log_area] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    let (list_area, detail_area) = if area.width >= SIDE_BY_SIDE_WIDTH {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(top);
        (list, detail)
    } else {
        let [list, detail] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(top);
        (list, detail)
    };

    // Panes draw from shared state, so every scroll adjustment happens
    // here, before the panes borrow the jobs and the log.
    let list_len = app.jobs.list.len();
    let list_height = list_area.height.saturating_sub(2) as usize;
    let index = app
        .jobs_view
        .cursor
        .selected
        .min(list_len.saturating_sub(1));
    let selected = app.jobs.list.iter().rev().nth(index);
    let detail_lines = detail_lines(selected, &app.theme, detail_area.width as usize);
    let detail_here = match selected.map(|job| &job.state) {
        Some(JobState::Running {
            ..
        }) => {
            selected
                .map(|job| job.tracker.active().count())
                .unwrap_or(0)
        }
        _ => detail_lines.len(),
    };
    app.jobs_view.detail_rows = detail_here;
    let detail_height = detail_area.height.saturating_sub(2) as usize;
    let log_len = app.log.lines.len();
    let log_height = log_area.height.saturating_sub(2) as usize;

    let list_rows = app.jobs_view.cursor.window(list_len, list_height);
    let detail_rows = app.jobs_view.detail.window(detail_here, detail_height);
    let log_rows = if app.log.follow {
        log_len.saturating_sub(log_height)..log_len
    } else {
        app.log.cursor.window(log_len, log_height)
    };

    // The wheel scrolls the pane under the pointer, so the detail and
    // log panes record their rows areas beside the list's.
    app.layout.detail = Some(Block::bordered().inner(detail_area));
    app.layout.log = Some(Block::bordered().inner(log_area));
    render_list(f, list_area, app, list_rows);
    render_detail(f, detail_area, app, detail_rows, &detail_lines);
    render_log(f, log_area, app, log_rows);
}

/// Draws the job list, newest first, with each job's status and duration.
fn render_list(f: &mut Frame, area: Rect, app: &mut App, rows: std::ops::Range<usize>) {
    let theme = &app.theme;
    let focused = app.jobs_view.focus == JobsFocus::List && app.view == ViewId::Jobs;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title(pane_title("Jobs", theme));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let jobs: Vec<&Job> = app.jobs.list.iter().rev().collect();
    if jobs.is_empty() {
        states::empty(
            f,
            inner,
            theme,
            "No jobs yet. Install, update, or sync something and it shows up here.",
        );
        return;
    }
    app.layout.list = Some((inner, rows.start));

    let selected_index = app.jobs_view.cursor.selected;
    for (row, job) in jobs[rows.clone()].iter().enumerate() {
        let rect = Rect {
            y: inner.y + row as u16,
            height: 1,
            ..inner
        };
        let line = Line::from(vec![
            Span::styled(
                if rows.start + row == selected_index {
                    "▌"
                } else {
                    " "
                },
                theme.cursor_row(),
            ),
            Span::raw(" "),
            status_span(job, app, theme),
            Span::raw(" "),
            Span::styled(job.title.clone(), theme.text()),
        ]);
        f.render_widget(Paragraph::new(line), rect);
        f.render_widget(
            Paragraph::new(duration_span(job, app, theme)).alignment(Alignment::Right),
            rect,
        );
    }
}

/// The status glyph of one job row: a spinner while running, a dot while
/// queued or waiting for authorization, and the outcome symbols after.
fn status_span(job: &Job, app: &App, theme: &Theme) -> Span<'static> {
    let symbols = theme.sym();
    match &job.state {
        JobState::Running {
            aborting, ..
        } => {
            Span::styled(
                theme.spinner(app.frame),
                if *aborting {
                    theme.warning()
                } else {
                    theme.accent()
                },
            )
        }
        JobState::Queued | JobState::Authorizing => Span::styled(symbols.dot, theme.muted()),
        JobState::Finished {
            outcome, ..
        } => {
            if outcome.aborted {
                Span::styled(symbols.warning, theme.warning())
            } else if outcome.ok {
                Span::styled(symbols.installed, theme.success())
            } else {
                Span::styled(symbols.error, theme.danger())
            }
        }
    }
}

/// The right-aligned duration of one job row: elapsed while running, the
/// word queued while waiting, the word authorizing while the user has not
/// approved the administrator pass yet, and the total once finished.
fn duration_span(job: &Job, app: &App, theme: &Theme) -> Span<'static> {
    match &job.state {
        JobState::Running {
            started,
            aborting,
        } => {
            if *aborting {
                Span::styled("aborting", theme.warning())
            } else {
                Span::styled(
                    format::clock(app.now.duration_since(*started)),
                    theme.muted(),
                )
            }
        }
        JobState::Queued => Span::styled("queued", theme.muted()),
        JobState::Authorizing => Span::styled("authorizing", theme.muted()),
        JobState::Finished {
            started,
            ended,
            ..
        } => Span::styled(format::clock(ended.duration_since(*started)), theme.muted()),
    }
}

/// Draws the selected job: its operations while running, or its headline
/// and report once finished.
fn render_detail(
    f: &mut Frame,
    area: Rect,
    app: &App,
    rows: std::ops::Range<usize>,
    lines: &[Line<'static>],
) {
    let theme = &app.theme;
    let index = app
        .jobs_view
        .cursor
        .selected
        .min(app.jobs.list.len().saturating_sub(1));
    let job = app.jobs.list.iter().rev().nth(index);
    let title_text = match job {
        None => "Job".to_string(),
        Some(job) => job.title.clone(),
    };
    let focused = app.jobs_view.focus == JobsFocus::Detail && app.view == ViewId::Jobs;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title(pane_title(&title_text, theme));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(job) = job else {
        return;
    };
    match &job.state {
        JobState::Running {
            ..
        } => {
            let active: Vec<&OpRow> = job.tracker.active().collect();
            for (row, op) in active[rows].iter().enumerate() {
                let rect = Rect {
                    y: inner.y + row as u16,
                    height: 1,
                    ..inner
                };
                activity::render_op(f, rect, op, theme, inner.width);
            }
        }
        _ => {
            let rect = Rect {
                height: inner.height,
                ..inner
            };
            f.render_widget(Paragraph::new(lines[rows].to_vec()), rect);
        }
    }
}

/// The report rows of the selected job: its operations while running, or
/// its headline and outcome once finished.
fn detail_lines(job: Option<&Job>, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    match job {
        None => Vec::new(),
        Some(job) => {
            match &job.state {
                JobState::Finished {
                    outcome, ..
                } => {
                    let mut lines = vec![headline_line(outcome, theme)];
                    lines.extend(outcome_lines(outcome, theme, width));
                    lines
                }
                _ => Vec::new(),
            }
        }
    }
}

/// The first detail row of a finished job, colored by how it went.
fn headline_line(outcome: &crate::jobs::JobOutcome, theme: &Theme) -> Line<'static> {
    let style = if outcome.aborted {
        theme.warning()
    } else if outcome.ok {
        theme.success()
    } else {
        theme.danger()
    };
    Line::from(Span::styled(outcome.headline.clone(), style))
}

/// The report rows of a finished job: what succeeded, what was skipped,
/// what failed with its full error, and the notes and warnings around it.
fn outcome_lines(
    outcome: &crate::jobs::JobOutcome,
    theme: &Theme,
    width: usize,
) -> Vec<Line<'static>> {
    use crate::jobs::OutcomeLine;

    let symbols = theme.sym();
    let mut lines = Vec::new();
    let mut notes_started = false;
    for entry in &outcome.lines {
        match entry {
            OutcomeLine::Done(name) => {
                lines.push(Line::from(vec![
                    Span::styled(symbols.installed, theme.success()),
                    Span::raw(" "),
                    Span::styled(name.clone(), theme.text()),
                ]))
            }
            OutcomeLine::Skipped(name) => {
                lines.push(Line::from(vec![
                    Span::styled(symbols.available, theme.muted()),
                    Span::raw(" "),
                    Span::styled(format!("{name} skipped"), theme.muted()),
                ]))
            }
            OutcomeLine::Failed {
                name,
                error,
            } => {
                lines.push(Line::from(vec![
                    Span::styled(symbols.error, theme.danger()),
                    Span::raw(" "),
                    Span::styled(name.clone(), theme.danger()),
                ]));
                for text in format::wrap(error, width.saturating_sub(4)) {
                    lines.push(Line::from(Span::styled(
                        format!("   {text}"),
                        theme.danger(),
                    )));
                }
            }
            OutcomeLine::Note {
                name,
                text,
            } => {
                if !notes_started {
                    notes_started = true;
                    lines.push(Line::from(Span::styled("Notes", theme.warning())));
                }
                lines.push(Line::from(Span::styled(
                    format!("{name}: {text}"),
                    theme.text(),
                )));
            }
            OutcomeLine::Warning(text) => {
                lines.push(Line::from(vec![
                    Span::styled(symbols.warning, theme.warning()),
                    Span::raw(" "),
                    Span::styled(text.clone(), theme.warning()),
                ]))
            }
        }
    }
    lines
}

/// Draws the log pane: one row per line, following the newest line unless
/// the user scrolled away from it.
fn render_log(f: &mut Frame, area: Rect, app: &App, rows: std::ops::Range<usize>) {
    let theme = &app.theme;
    let focused = app.jobs_view.focus == JobsFocus::Log && app.view == ViewId::Jobs;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(focused))
        .title(pane_title("Log", theme));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let log = &app.log;
    if log.lines.is_empty() {
        states::empty(f, inner, theme, "Nothing logged yet.");
        return;
    }
    for (row, line) in log.lines.range(rows.clone()).enumerate() {
        let mut spans = vec![
            Span::styled(line.time.clone(), theme.muted()),
            Span::raw(" "),
        ];
        if line.level != LogLevel::Info {
            spans.push(Span::styled(
                level_tag(line.level),
                level_style(line.level, theme),
            ));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            line.text.clone(),
            text_style(line.level, theme),
        ));
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                y: inner.y + row as u16,
                height: 1,
                ..inner
            },
        );
    }
}

/// The level word shown in the log pane.
fn level_tag(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

/// The color of the level tag.
fn level_style(level: LogLevel, theme: &Theme) -> Style {
    match level {
        LogLevel::Debug | LogLevel::Info => theme.muted(),
        LogLevel::Warn => theme.warning(),
        LogLevel::Error => theme.danger(),
    }
}

/// The color of the message text.
fn text_style(level: LogLevel, theme: &Theme) -> Style {
    match level {
        LogLevel::Error => theme.danger(),
        _ => theme.text(),
    }
}

/// The title of a pane block.
fn pane_title(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled(text.to_string(), theme.title()),
        Span::raw(" "),
    ])
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::{
        app::App,
        jobs::{Job, JobKind, JobOutcome, JobState, OutcomeLine},
        progress::Tracker,
        test_support::{key, screen, test_app},
        views::ViewId,
    };

    fn finished_failed_app() -> App {
        let mut app = test_app();
        app.view = ViewId::Jobs;
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
                    lines: vec![OutcomeLine::Failed {
                        name: "obsidian".into(),
                        error: "checksum mismatch: expected abc, got def".into(),
                    }],
                },
            },
            spec: None,
            tracker: Tracker::default(),
        });
        app
    }

    #[test]
    fn a_finished_failed_job_shows_its_title_and_full_error() {
        let mut app = finished_failed_app();
        let screen = screen(&mut app, 110, 30);
        assert!(screen.contains("Install 1 package"), "screen is {screen:?}");
        assert!(
            screen.contains("Install finished with 1 failure"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("checksum mismatch: expected abc, got def"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_empty_state_names_what_lands_here() {
        let mut app = test_app();
        app.view = ViewId::Jobs;
        let screen = screen(&mut app, 110, 30);
        assert!(
            screen.contains("No jobs yet. Install, update, or"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn clearing_finished_jobs_empties_the_list() {
        let mut app = finished_failed_app();
        app.update(key('c'), Instant::now());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn an_aborting_job_lists_the_word_in_place_of_its_duration() {
        let mut app = test_app();
        app.view = ViewId::Jobs;
        app.jobs.list.push(Job {
            id: 0,
            kind: JobKind::Install,
            title: "Install 1 package".into(),
            state: JobState::Running {
                started: Instant::now(),
                aborting: true,
            },
            spec: None,
            tracker: Tracker::default(),
        });
        let screen = screen(&mut app, 110, 30);
        assert!(screen.contains("aborting"), "screen is {screen:?}");
    }

    #[test]
    fn an_authorizing_job_lists_like_a_queued_one_with_a_note() {
        let mut app = test_app();
        app.view = ViewId::Jobs;
        app.jobs.list.push(Job {
            id: 0,
            kind: JobKind::ApplySystem,
            title: "Apply 2 system packages".into(),
            state: JobState::Authorizing,
            spec: None,
            tracker: Tracker::default(),
        });
        let screen = screen(&mut app, 110, 30);
        assert!(
            screen.contains("Apply 2 system packages"),
            "screen is {screen:?}"
        );
        assert!(screen.contains("authorizing"), "screen is {screen:?}");
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_hard_splits_long_words() {
        use crate::format::wrap;
        assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
        assert_eq!(wrap("abcdef", 3), vec!["abc", "def"]);
    }
}
