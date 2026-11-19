//! The repositories view: every configured repository, its state, and
//! when its metadata was last written to disk.

use std::{ops::Range, time::SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row, Table},
};

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    views::ViewId,
    widgets::{scroll::ListCursor, states},
};

/// The width of the name column.
const NAME_WIDTH: usize = 16;

/// The width of the signed column.
const SIGNED_WIDTH: u16 = 8;

/// The width of the desktop column.
const DESKTOP_WIDTH: u16 = 8;

/// The width of the synced column.
const SYNCED_WIDTH: u16 = 14;

/// One configured repository as the repositories view keeps it.
pub(crate) struct RepoRow {
    /// The repository's unique name.
    pub(crate) name: String,
    /// The URL its metadata is fetched from.
    pub(crate) url: String,
    /// Whether the repository takes part in searches and installs.
    pub(crate) enabled: bool,
    /// Whether the repository's metadata is checked against its key.
    pub(crate) signed: bool,
    /// Whether packages from this repository get desktop entries.
    pub(crate) desktop: bool,
    /// The interval between syncs as configured, or the default.
    #[allow(dead_code)]
    pub(crate) sync_interval: String,
    /// When the repository's metadata database was last written to disk.
    pub(crate) synced: Option<SystemTime>,
}

/// The state of the repositories view.
#[derive(Default)]
pub(crate) struct ReposView {
    /// The configured repositories, loaded on the first visit and again
    /// after every job that changes them or their metadata.
    pub(crate) list: Loadable<Vec<RepoRow>>,
    /// The visible row the cursor sits on.
    pub(crate) cursor: ListCursor,
}

impl ReposView {
    /// Starts a fresh load. The last one's rows go when the result
    /// arrives.
    pub(crate) fn start(&mut self) {
        self.list = Loadable::Loading;
    }

    /// Adopts a finished load. The cursor stays on its row while the new
    /// list still has one.
    pub(crate) fn loaded(&mut self, result: Result<Vec<RepoRow>, String>) {
        self.list = match result {
            Ok(rows) => Loadable::Ready(rows),
            Err(error) => Loadable::Failed(error),
        };
        self.cursor.clamp(self.ready_len());
    }

    /// The number of rows in the ready list, zero while it loads.
    pub(crate) fn ready_len(&self) -> usize {
        self.list.ready().map_or(0, Vec::len)
    }

    /// The row the cursor points at, when the list has one.
    pub(crate) fn cursor_row(&self) -> Option<&RepoRow> {
        self.list
            .ready()
            .and_then(|rows| rows.get(self.cursor.selected))
    }
}

/// Draws the repositories view: its border with the count in the title,
/// then the table of repositories or the loading, empty, and error
/// states.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let rows_height = usize::from(area.height.saturating_sub(3));
    let window = app.repos.cursor.window(app.repos.ready_len(), rows_height);
    let block = list_block(&app.repos, &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = &app.repos;
    let theme = &app.theme;
    match &view.list {
        Loadable::Idle => {}
        Loadable::Loading => states::loading(f, inner, theme, app.frame, "Loading repositories"),
        Loadable::Failed(error) => states::error(f, inner, theme, error),
        Loadable::Ready(rows) if rows.is_empty() => {
            states::empty(f, inner, theme, "No repositories. Press a to add one.")
        }
        Loadable::Ready(rows) => {
            let offset = window.start;
            let rows_area = render_table(f, inner, rows, window, view.cursor.selected, theme);
            app.layout.list = Some((rows_area, offset));
        }
    }
}

/// Draws the table of repositories, one row each, and returns the rect
/// the rows drew into.
fn render_table(
    f: &mut Frame,
    area: Rect,
    rows: &[RepoRow],
    window: Range<usize>,
    selected: usize,
    theme: &Theme,
) -> Rect {
    let constraints = [
        Constraint::Length(2),
        Constraint::Length(NAME_WIDTH as u16),
        Constraint::Fill(1),
        Constraint::Length(SIGNED_WIDTH),
        Constraint::Length(DESKTOP_WIDTH),
        Constraint::Length(SYNCED_WIDTH),
    ];
    let url_width = url_column_width(area.width, &constraints);
    let table = Table::new(
        table_rows(rows, window, selected, url_width, theme),
        constraints,
    )
    .header(header_row(theme));
    f.render_widget(table, area);
    Rect {
        y: area.y.saturating_add(1),
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// The width the url column gets: everything the fixed columns and the
/// spacing between them leave over.
fn url_column_width(width: u16, constraints: &[Constraint; 6]) -> usize {
    let fixed: u16 = constraints
        .iter()
        .map(|constraint| {
            match constraint {
                Constraint::Length(n) => *n,
                _ => 0,
            }
        })
        .sum();
    let spacing = constraints.len().saturating_sub(1) as u16;
    usize::from(width.saturating_sub(2).saturating_sub(fixed + spacing))
}

/// The column labels in muted bold. The enabled column stays unlabeled.
fn header_row(theme: &Theme) -> Row<'static> {
    Row::new(["", "NAME", "URL", "SIGNED", "DESKTOP", "SYNCED"])
        .style(theme.muted().add_modifier(Modifier::BOLD))
}

/// Builds the rows the window shows.
fn table_rows(
    rows: &[RepoRow],
    window: Range<usize>,
    selected: usize,
    url_width: usize,
    theme: &Theme,
) -> Vec<Row<'static>> {
    window
        .map(|index| repo_row(&rows[index], index == selected, url_width, theme))
        .collect()
}

/// One table row. The cursor row takes the cursor highlight across its
/// whole width and starts with the cursor symbol; other rows lead with
/// the enabled state, the marked symbol for enabled repositories and
/// the available symbol for disabled ones, whose name is also muted.
fn repo_row(repo: &RepoRow, cursor_here: bool, url_width: usize, theme: &Theme) -> Row<'static> {
    let symbols = theme.sym();
    let lead = if cursor_here {
        Span::styled(symbols.cursor.to_string(), theme.cursor_row())
    } else if repo.enabled {
        Span::styled(symbols.marked.to_string(), theme.success())
    } else {
        Span::styled(symbols.available.to_string(), theme.muted())
    };
    let name_style = if repo.enabled {
        theme.text()
    } else {
        theme.muted()
    };
    let style = if cursor_here {
        theme.cursor_row()
    } else {
        Style::new()
    };
    Row::new(vec![
        Cell::from(Line::from(vec![lead, Span::raw(" ")])),
        Cell::from(Span::styled(
            format::truncate(&repo.name, NAME_WIDTH, symbols.ellipsis),
            name_style,
        )),
        Cell::from(Span::styled(
            format::truncate_middle(&repo.url, url_width, symbols.ellipsis),
            theme.muted(),
        )),
        bool_cell(repo.signed, theme),
        bool_cell(repo.desktop, theme),
        Cell::from(Span::styled(synced_text(repo.synced), theme.text())),
    ])
    .style(style)
}

/// The centered mark a boolean column shows: the installed symbol in the
/// success color when true, the error symbol muted when not.
fn bool_cell(value: bool, theme: &Theme) -> Cell<'static> {
    let symbols = theme.sym();
    let (symbol, style) = if value {
        (symbols.installed, theme.success())
    } else {
        (symbols.error, theme.muted())
    };
    Cell::from(Line::from(Span::styled(symbol.to_string(), style)).alignment(Alignment::Center))
}

/// When the metadata was last written to disk, as a relative age, or
/// `never` when the repository has not synced yet.
fn synced_text(synced: Option<SystemTime>) -> String {
    synced.map_or_else(
        || "never".to_string(),
        |then| format::relative(then, SystemTime::now()),
    )
}

/// The pane border: the repository count on the left of the top edge.
fn list_block(view: &ReposView, theme: &Theme) -> Block<'static> {
    let left = match view.list.ready() {
        Some(rows) => Line::styled(format!(" Repositories {} ", rows.len()), theme.title()),
        None => Line::styled(format!(" {} ", ViewId::Repos.title()), theme.title()),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(left)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use crate::{
        effect::Effect,
        jobs::{JobOutcome, JobSpec, RepoChange},
        msg::{Loaded, Msg},
        test_support::{ctrl, key, screen, test_app},
        views::{ViewId, repos::RepoRow},
    };

    /// A loaded repositories list holding the given rows.
    fn repos(rows: Vec<RepoRow>) -> Msg {
        Msg::Loaded(Loaded::Repos(Ok(rows)))
    }

    fn failed(message: &str) -> Msg {
        Msg::Loaded(Loaded::Repos(Err(message.to_string())))
    }

    /// One repository row with the given name, url, and enabled flag,
    /// signed and desktop-integrated, synced two hours ago.
    fn repo_row(name: &str, url: &str, enabled: bool) -> RepoRow {
        repo_row_synced(
            name,
            url,
            enabled,
            Some(SystemTime::now() - Duration::from_secs(2 * 3600 + 30)),
        )
    }

    /// One repository row with an explicit last-sync time.
    fn repo_row_synced(
        name: &str,
        url: &str,
        enabled: bool,
        synced: Option<SystemTime>,
    ) -> RepoRow {
        RepoRow {
            name: name.to_string(),
            url: url.to_string(),
            enabled,
            signed: true,
            desktop: true,
            sync_interval: "3h".to_string(),
            synced,
        }
    }

    /// Three configured repositories, the usual fixtures, the last one
    /// disabled.
    fn catalog() -> Vec<RepoRow> {
        vec![
            repo_row(
                "bincache",
                "https://meta.pkgforge.dev/bincache/metadata.db.zstd",
                true,
            ),
            repo_row(
                "pkgcache",
                "https://meta.pkgforge.dev/pkgcache/metadata.db.zstd",
                true,
            ),
            repo_row(
                "ivan-hc-am",
                "https://meta.pkgforge.dev/external/AM/metadata.db.zstd",
                false,
            ),
        ]
    }

    /// The repositories view with its rows loaded, where its keys and its
    /// pane are live.
    fn repos_app(rows: Vec<RepoRow>) -> (crate::app::App, Instant) {
        let mut app = test_app();
        app.view = ViewId::Repos;
        app.update(repos(rows), Instant::now());
        (app, Instant::now())
    }

    fn finished(id: u64, headline: &str) -> Msg {
        Msg::JobFinished {
            id,
            outcome: JobOutcome {
                ok: true,
                aborted: false,
                headline: headline.to_string(),
                lines: Vec::new(),
            },
        }
    }

    #[test]
    fn entering_repos_the_first_time_loads_the_list() {
        let mut app = test_app();
        let effects = app.update(key('5'), Instant::now());
        let [Effect::LoadRepos] = &effects[..] else {
            panic!("expected one load on first entry");
        };
        assert!(app.repos.list.is_loading());
    }

    #[test]
    fn reentering_repos_starts_nothing() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(key('5'), now);
        app.update(repos(catalog()), now);
        app.update(key('1'), now);
        let effects = app.update(key('5'), now);
        assert!(effects.is_empty());
        assert!(app.repos.list.ready().is_some());
    }

    #[test]
    fn a_finished_sync_reloads_the_repositories() {
        let mut app = test_app();
        let now = Instant::now();
        app.enqueue(JobSpec::Sync, now);
        let effects = app.update(finished(0, "Repositories synced"), now);
        let [
            Effect::RebuildContext,
            Effect::LoadCatalog,
            Effect::LoadInstalled,
            Effect::LoadRepos,
        ] = &effects[..]
        else {
            panic!("expected the reloads a sync causes");
        };
        assert!(app.repos.list.is_loading());
    }

    #[test]
    fn a_finished_repository_change_reloads_the_repositories() {
        let mut app = test_app();
        let now = Instant::now();
        app.enqueue(JobSpec::Repo(RepoChange::Remove("old".to_string())), now);
        let effects = app.update(finished(0, "Removed repository old"), now);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::RebuildContext,
                Effect::LoadRepos,
                Effect::LoadCatalog,
                Effect::LoadInstalled
            ]
        ));
    }

    #[test]
    fn ctrl_r_reloads_the_repositories() {
        let (mut app, now) = repos_app(catalog());
        let effects = app.update(ctrl('r'), now);
        let [Effect::LoadRepos] = &effects[..] else {
            panic!("expected one load on reload");
        };
        assert!(app.repos.list.is_loading());
    }

    #[test]
    fn the_title_shows_the_repository_count() {
        let (mut app, _) = repos_app(catalog());
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("Repositories 3"), "screen is {screen:?}");
    }

    #[test]
    fn an_empty_list_says_how_to_add_one() {
        let (mut app, _) = repos_app(Vec::new());
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("No repositories. Press a to add one."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_disabled_row_shows_the_available_symbol() {
        let (mut app, _) = repos_app(catalog());
        let screen = screen(&mut app, 110, 24);
        assert!(screen.contains('●'), "screen is {screen:?}");
        assert!(
            screen.contains(app.theme.sym().available),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_repository_never_synced_shows_never() {
        let rows = vec![repo_row_synced(
            "fresh",
            "https://example.com/fresh.db",
            true,
            None,
        )];
        let (mut app, _) = repos_app(rows);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("never"), "screen is {screen:?}");
    }

    #[test]
    fn a_synced_row_shows_a_relative_age() {
        let (mut app, _) = repos_app(catalog());
        let screen = screen(&mut app, 110, 24);
        assert!(screen.contains("2 hours ago"), "screen is {screen:?}");
    }

    #[test]
    fn a_failed_load_shows_the_error_state() {
        let mut app = test_app();
        app.view = ViewId::Repos;
        app.update(failed("config unreadable"), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("config unreadable"), "screen is {screen:?}");
        assert!(screen.contains("ctrl-r to retry"), "screen is {screen:?}");
    }

    #[test]
    fn the_table_names_its_columns() {
        let (mut app, _) = repos_app(catalog());
        let screen = screen(&mut app, 120, 24);
        for label in ["NAME", "URL", "SIGNED", "DESKTOP", "SYNCED"] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_long_url_is_cut_in_the_middle() {
        let long = "https://meta.pkgforge.dev/bincache/x86_64-linux/metadata.db.zstd";
        let rows = vec![repo_row("bincache", long, true)];
        let (mut app, _) = repos_app(rows);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("https://me"), "screen is {screen:?}");
        assert!(screen.contains("db.zstd"), "screen is {screen:?}");
        assert!(!screen.contains(long), "screen is {screen:?}");
    }

    #[test]
    fn more_rows_than_screen_lines_render_only_the_visible_ones() {
        let rows: Vec<_> = (0..50)
            .map(|n| repo_row(&format!("repo{n:03}"), "https://example.com/db", true))
            .collect();
        let (mut app, now) = repos_app(rows);
        for _ in 0..40 {
            app.update(key('j'), now);
        }
        let screen = screen(&mut app, 100, 30);
        assert!(screen.contains("repo040"), "screen is {screen:?}");
        assert!(!screen.contains("repo000"), "screen is {screen:?}");
        assert!(!screen.contains("repo049"), "screen is {screen:?}");
    }

    #[test]
    fn a_reload_keeps_the_cursor_on_its_row_while_the_list_shrinks() {
        let rows: Vec<_> = (0..10)
            .map(|n| repo_row(&format!("repo{n}"), "https://example.com/db", true))
            .collect();
        let (mut app, now) = repos_app(rows);
        app.update(key('G'), now);
        assert_eq!(app.repos.cursor.selected, 9);
        app.update(repos(catalog()), now);
        assert_eq!(app.repos.cursor.selected, 2);
    }
}
