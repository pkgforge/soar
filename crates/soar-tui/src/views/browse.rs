//! The package catalog view: its table, the filter, and the marks.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    time::{Duration, Instant},
};

use nucleo_matcher::{
    Matcher, Utf32String,
    pattern::{CaseMatching, Normalization, Pattern},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row, Table},
};
use soar_core::database::models::Package;

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    views::ViewId,
    widgets::{badge, package_detail, scroll::ListCursor, states, text_input::TextInput},
};

/// Identifies one catalog row across reloads, marks, and the filter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackageKey {
    /// The repository the package comes from.
    pub(crate) repo_name: String,
    /// The package's name inside its repository.
    pub(crate) pkg_name: String,
    /// The family grouping the package's variants, when it has one.
    pub(crate) pkg_family: Option<String>,
    /// The package id the repository publishes, when it does.
    pub(crate) pkg_id: Option<String>,
}

impl PackageKey {
    /// The exact install query naming this package at `version`, as
    /// `family/name@version:repo`, without the family when there is none.
    pub(crate) fn query(&self, version: &str) -> String {
        match self.pkg_family.as_deref() {
            Some(family) => format!("{family}/{}@{version}:{}", self.pkg_name, self.repo_name),
            None => format!("{}@{version}:{}", self.pkg_name, self.repo_name),
        }
    }
}

/// One package of the catalog, with what the list shows and the filter
/// searches.
#[derive(Clone, Debug)]
pub(crate) struct CatalogRow {
    /// What identifies this row across reloads.
    pub(crate) key: PackageKey,
    /// The newest version the repository publishes.
    pub(crate) version: String,
    /// The package's kind, such as `static` or `appimage`.
    pub(crate) pkg_type: Option<String>,
    /// The short summary shown in the detail pane and searched by the
    /// filter.
    pub(crate) description: String,
    /// Whether a version of this package is installed.
    pub(crate) installed: bool,
    /// The other versions the repository publishes, newest first. The
    /// detail pane lists them; the version picker reads them once it
    /// lands.
    pub(crate) other_versions: Vec<String>,
    /// `family/name` or `name`, lowercased once for matching.
    pub(crate) haystack: String,
}

/// How long the cursor must rest on a row before its details load.
const DETAIL_DELAY: Duration = Duration::from_millis(120);

/// How many packages the detail cache keeps before it starts over.
const DETAIL_CACHE_MAX: usize = 256;

/// The narrowest terminal that shows the list beside the detail pane.
const SPLIT_WIDTH: u16 = 100;

/// The state of the catalog view.
#[derive(Debug, Default)]
pub(crate) struct BrowseView {
    /// The whole package catalog, loaded at startup and reloaded on
    /// request.
    pub(crate) catalog: Loadable<Vec<CatalogRow>>,
    /// The indexes into the catalog of the rows shown, in display order.
    pub(crate) visible: Vec<usize>,
    /// The visible row the cursor sits on.
    pub(crate) cursor: ListCursor,
    /// The text the filter matches rows against.
    pub(crate) filter: TextInput,
    /// Whether the filter holds the keyboard.
    pub(crate) filter_focused: bool,
    /// The repository the list is narrowed to, when it is.
    pub(crate) repo_filter: Option<String>,
    /// The rows marked for the next bulk action.
    pub(crate) marks: HashSet<PackageKey>,
    /// The details already fetched, keyed by package. The value stays
    /// `None` when the repositories hold no package with that query.
    pub(crate) details: HashMap<PackageKey, Loadable<Option<Package>>>,
    /// The row waiting for its details to load, with when the cursor
    /// arrived on it.
    pub(crate) wanted: Option<(PackageKey, Instant)>,
    /// The scroll of the detail pane while it fills the screen.
    pub(crate) detail: ListCursor,
    /// How many rows the detail held at the last render, so scrolling
    /// moves over what the pane actually shows.
    pub(crate) detail_rows: usize,
    /// Whether the detail fills the whole body instead of sitting beside
    /// the list.
    pub(crate) detail_open: bool,
    /// The fuzzy matcher scoring the filter against rows. It allocates on
    /// creation, so one instance is reused for every keystroke.
    matcher: Matcher,
}

impl BrowseView {
    /// The catalog row the cursor points at, when the list has one. The
    /// indexes can be stale right after a reload replaced the catalog, so
    /// the lookups tolerate a missing row.
    pub(crate) fn cursor_row(&self) -> Option<&CatalogRow> {
        let rows = self.catalog.ready()?;
        let index = *self.visible.get(self.cursor.selected)?;
        rows.get(index)
    }

    /// The key of the row the cursor points at, when the list has one.
    fn cursor_key(&self) -> Option<PackageKey> {
        self.cursor_row().map(|row| row.key.clone())
    }

    /// The version of the row a key names, so the detail query can ask
    /// for that exact variant.
    pub(crate) fn row_version(&self, key: &PackageKey) -> Option<String> {
        let rows = self.catalog.ready()?;
        rows.iter()
            .find(|row| &row.key == key)
            .map(|row| row.version.clone())
    }

    /// The catalog row a key names, when the catalog holds it.
    pub(crate) fn row(&self, key: &PackageKey) -> Option<&CatalogRow> {
        self.catalog.ready()?.iter().find(|row| &row.key == key)
    }

    /// Records the row the cursor rests on, so its details start loading
    /// once the cursor stays put long enough.
    pub(crate) fn note_cursor(&mut self, now: Instant) {
        self.wanted = self.cursor_key().map(|key| (key, now));
    }

    /// The detail request whose row the cursor has rested on long enough,
    /// if any, marking it loading so it is requested only once. The row's
    /// version is resolved here, because the catalog can be mid-reload
    /// while the cursor rests: asking then would park the row on a
    /// Loading entry no request ever answers, so the want stays put and
    /// the next tick retries once the reload has landed.
    pub(crate) fn due_detail(&mut self, now: Instant) -> Option<PackageKey> {
        let (key, rested_at) = self.wanted.clone()?;
        if now.duration_since(rested_at) < DETAIL_DELAY {
            return None;
        }
        // The version must resolve now, or the request would go out
        // for a variant the mid-reload catalog cannot name yet.
        self.row_version(&key)?;
        self.wanted = None;
        if self.details.contains_key(&key) {
            return None;
        }
        self.cache_detail(key.clone(), Loadable::Loading);
        Some(key)
    }

    /// Stores one package's details, emptying the cache first when it
    /// grew past its bound.
    pub(crate) fn cache_detail(&mut self, key: PackageKey, state: Loadable<Option<Package>>) {
        if self.details.len() > DETAIL_CACHE_MAX {
            self.details.clear();
        }
        self.details.insert(key, state);
    }

    /// Rebuilds the visible rows from the filter text and the repository
    /// filter. A query matches names first, then descriptions once it is
    /// long enough to be worth scanning them; name matches rank above
    /// description matches, and ties order by name. The cursor stays on
    /// its package while that row survives the new filter.
    pub(crate) fn refilter(&mut self, now: Instant) {
        let previous = self.cursor_key();
        let mut ranked: Vec<((u8, i64), usize)> = Vec::new();
        if let Loadable::Ready(rows) = &self.catalog {
            let query = self.filter.value().trim();
            let repo = self.repo_filter.as_deref();
            let pattern = (!query.is_empty())
                .then(|| Pattern::parse(query, CaseMatching::Smart, Normalization::Smart));
            match pattern {
                None => {
                    for (index, row) in rows.iter().enumerate() {
                        if repo.is_none_or(|name| row.key.repo_name == name) {
                            ranked.push(((0, 0), index));
                        }
                    }
                }
                Some(pattern) => {
                    let matcher = &mut self.matcher;
                    let scan_descriptions = query.len() >= 3;
                    for (index, row) in rows.iter().enumerate() {
                        if !repo.is_none_or(|name| row.key.repo_name == name) {
                            continue;
                        }
                        let mut rank = pattern
                            .score(Utf32String::from(row.haystack.as_str()).slice(..), matcher)
                            .map(|score| (0u8, -i64::from(score)));
                        if rank.is_none() && scan_descriptions {
                            rank = pattern
                                .score(
                                    Utf32String::from(row.description.as_str()).slice(..),
                                    matcher,
                                )
                                .map(|score| (1u8, -i64::from(score)));
                        }
                        if let Some(rank) = rank {
                            ranked.push((rank, index));
                        }
                    }
                    ranked.sort_by(|a, b| {
                        a.0.cmp(&b.0)
                            .then_with(|| rows[a.1].key.pkg_name.cmp(&rows[b.1].key.pkg_name))
                    });
                }
            }
        }
        self.visible = ranked.into_iter().map(|(_, index)| index).collect();
        self.cursor = ListCursor::default();
        if let Some(previous) = previous
            && let Loadable::Ready(rows) = &self.catalog
            && let Some(position) = self
                .visible
                .iter()
                .position(|&index| rows[index].key == previous)
        {
            self.cursor.selected = position;
        }
        self.note_cursor(now);
    }

    /// Marks or unmarks the row under the cursor, then moves one row down.
    pub(crate) fn toggle_mark(&mut self, now: Instant) {
        let Some(index) = self.visible.get(self.cursor.selected).copied() else {
            return;
        };
        let Some(rows) = self.catalog.ready() else {
            return;
        };
        let Some(row) = rows.get(index) else {
            return;
        };
        let key = row.key.clone();
        if !self.marks.remove(&key) {
            self.marks.insert(key);
        }
        self.cursor.down(1, self.visible.len());
        self.note_cursor(now);
    }

    /// Moves the repository filter to its next value: no filter first, then
    /// each repository in the catalog in name order, then back to no
    /// filter. The visible rows are rebuilt with the new value.
    pub(crate) fn cycle_repo(&mut self, now: Instant) {
        let Loadable::Ready(rows) = &self.catalog else {
            return;
        };
        let mut repos: Vec<&str> = rows.iter().map(|row| row.key.repo_name.as_str()).collect();
        repos.sort_unstable();
        repos.dedup();
        self.repo_filter = match self.repo_filter.as_deref() {
            None => repos.first().map(|name| (*name).to_string()),
            Some(current) => {
                match repos.iter().position(|name| *name == current) {
                    Some(position) if position + 1 < repos.len() => {
                        Some(repos[position + 1].to_string())
                    }
                    _ => None,
                }
            }
        };
        self.refilter(now);
    }

    /// The keys the next bulk action works on: every marked row, or the row
    /// under the cursor when nothing is marked. Rows marked but hidden by
    /// the filters stay in the list, and the install confirm lists them all.
    pub(crate) fn selected_keys(&self) -> Vec<PackageKey> {
        if !self.marks.is_empty() {
            let mut keys: Vec<PackageKey> = self.marks.iter().cloned().collect();
            keys.sort();
            return keys;
        }
        self.cursor_key().into_iter().collect()
    }

    /// The selected rows as `(key, version)` pairs, each pinning the
    /// version its catalog row shows.
    pub(crate) fn selected_versions(&self) -> Vec<(PackageKey, String)> {
        self.selected_keys()
            .into_iter()
            .filter_map(|key| {
                let version = self.row_version(&key)?;
                Some((key, version))
            })
            .collect()
    }
}

/// Draws the catalog view: the list pane, and beside it the detail pane
/// of the row under the cursor, or the detail alone while it fills the
/// screen.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.browse.detail_open {
        package_detail::render(f, area, app, true);
        return;
    }
    if area.width >= SPLIT_WIDTH {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);
        render_list(f, list, app, area.width);
        package_detail::render(f, detail, app, false);
    } else {
        render_list(f, area, app, area.width);
    }
}

/// Draws the catalog pane: its border with the counts and the filter in the
/// title, then the table of visible rows or the loading, error, and empty
/// states. The column layout follows the full terminal width, whatever the
/// list shares it with.
fn render_list(f: &mut Frame, area: Rect, app: &mut App, full_width: u16) {
    let theme = &app.theme;
    let rows_height = usize::from(area.height.saturating_sub(3));
    let window = match &app.browse.catalog {
        Loadable::Ready(_) => {
            app.browse
                .cursor
                .window(app.browse.visible.len(), rows_height)
        }
        _ => 0..0,
    };
    let block = list_block(&app.browse, theme, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = &app.browse;
    match &view.catalog {
        Loadable::Idle => {}
        Loadable::Loading => states::loading(f, inner, theme, app.frame, "Loading packages"),
        Loadable::Failed(error) => states::error(f, inner, theme, error),
        Loadable::Ready(rows) if rows.is_empty() => {
            states::empty(
                f,
                inner,
                theme,
                "No packages. Add or enable a repository in Repos (5).",
            )
        }
        Loadable::Ready(rows) if view.visible.is_empty() => {
            states::empty(
                f,
                inner,
                theme,
                &format!("No packages match {}", view.filter.value()),
            )
        }
        Loadable::Ready(rows) => {
            let (columns, constraints) = columns(full_width);
            let name_width = usize::from(name_column_width(area.width, &constraints));
            let offset = window.start;
            let table = Table::new(
                table_rows(view, rows, window, &columns, name_width, theme),
                constraints,
            )
            .header(header_row(&columns, theme));
            app.layout.list = Some((rows_area(inner), offset));
            f.render_widget(table, inner);
        }
    }
}

/// The rows area of a table pane: its header row takes the first line of
/// the pane's inner area, and the window range maps onto what is left.
fn rows_area(inner: Rect) -> Rect {
    Rect {
        y: inner.y.saturating_add(1),
        height: inner.height.saturating_sub(1),
        ..inner
    }
}

/// The columns the table shows at one terminal width. The type badge
/// leaves on narrow terminals and the repository column on very narrow
/// ones; the full terminal width decides, whatever the list shares it
/// with.
fn columns(width: u16) -> (Columns, Vec<Constraint>) {
    let every = (
        Columns {
            repo: true,
            kind: true,
        },
        vec![
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    );
    let no_kind = (
        Columns {
            repo: true,
            kind: false,
        },
        vec![
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Length(12),
        ],
    );
    let no_repo = (
        Columns {
            repo: false,
            kind: false,
        },
        vec![
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(14),
        ],
    );
    if width < 70 {
        no_repo
    } else if width < 80 {
        no_kind
    } else {
        every
    }
}

/// Which optional columns the table shows.
#[derive(Clone, Copy)]
struct Columns {
    /// The repository column.
    repo: bool,
    /// The package type column.
    kind: bool,
}

/// The width the name column gets: everything the fixed columns and the
/// spacing between them leave over.
fn name_column_width(width: u16, constraints: &[Constraint]) -> u16 {
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
    width.saturating_sub(2).saturating_sub(fixed + spacing)
}

/// The column labels in muted bold.
fn header_row(columns: &Columns, theme: &Theme) -> Row<'static> {
    let mut labels = vec!["", "NAME", "VERSION"];
    if columns.repo {
        labels.push("REPO");
    }
    if columns.kind {
        labels.push("TYPE");
    }
    Row::new(labels).style(theme.muted().add_modifier(Modifier::BOLD))
}

/// Builds the rows the window shows: the mark, the family and name, the
/// version, and the repository and type badge when their columns exist.
fn table_rows(
    view: &BrowseView,
    rows: &[CatalogRow],
    window: Range<usize>,
    columns: &Columns,
    name_width: usize,
    theme: &Theme,
) -> Vec<Row<'static>> {
    let shown = &view.visible[window.clone()];
    window
        .zip(shown.iter())
        .map(|(position, &index)| {
            let row = &rows[index];
            list_row(
                row,
                view.marks.contains(&row.key),
                position == view.cursor.selected,
                columns,
                name_width,
                theme,
            )
        })
        .collect()
}

/// One table row. The cursor row takes the cursor highlight across its
/// whole width and starts with the cursor symbol; a marked row starts with
/// the marked symbol.
fn list_row(
    row: &CatalogRow,
    marked: bool,
    cursor_here: bool,
    columns: &Columns,
    name_width: usize,
    theme: &Theme,
) -> Row<'static> {
    let symbols = theme.sym();
    let lead = if cursor_here {
        Span::styled(symbols.cursor.to_string(), theme.cursor_row())
    } else if marked {
        Span::styled(symbols.marked.to_string(), theme.marked())
    } else {
        Span::raw(" ")
    };
    let mut cells = vec![
        Cell::from(Line::from(vec![lead, Span::raw(" ")])),
        name_cell(row, theme, name_width),
        Cell::from(Span::styled(row.version.clone(), theme.accent2())),
    ];
    if columns.repo {
        cells.push(Cell::from(Span::styled(
            row.key.repo_name.clone(),
            theme.info(),
        )));
    }
    if columns.kind {
        cells.push(Cell::from(badge::pkg_type(row.pkg_type.as_deref(), theme)));
    }
    let style = if cursor_here {
        theme.cursor_row()
    } else {
        Style::new()
    };
    Row::new(cells).style(style)
}

/// The name cell: `family/` before the name in the family color, with the
/// installed mark at the right edge of the cell for installed packages.
fn name_cell(row: &CatalogRow, theme: &Theme, width: usize) -> Cell<'static> {
    let mut full = String::new();
    let mut family_chars = 0;
    if let Some(family) = row.key.pkg_family.as_deref() {
        full.push_str(family);
        full.push('/');
        family_chars = full.chars().count();
    }
    full.push_str(&row.key.pkg_name);
    let symbols = theme.sym();
    let mark_width = Span::raw(symbols.installed).width();
    let reserved = usize::from(row.installed) * (1 + mark_width);
    let text = format::truncate(&full, width.saturating_sub(reserved), symbols.ellipsis);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if family_chars > 0 {
        let boundary = text
            .char_indices()
            .nth(family_chars)
            .map_or(text.len(), |(at, _)| at);
        spans.push(Span::styled(text[..boundary].to_string(), theme.accent2()));
        spans.push(Span::styled(text[boundary..].to_string(), theme.text()));
    } else {
        spans.push(Span::styled(text, theme.text()));
    }
    if row.installed {
        let used = spans.iter().map(Span::width).sum::<usize>();
        let pad = width.saturating_sub(used + mark_width);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(badge::installed(theme));
    }
    Cell::from(Line::from(spans))
}

/// The pane border: the package count on the left of the top edge and the
/// filter and repository filter on its right.
fn list_block(view: &BrowseView, theme: &Theme, width: u16) -> Block<'static> {
    let left = match view.catalog.ready() {
        Some(rows) => {
            let filtered = !view.filter.value().trim().is_empty() || view.repo_filter.is_some();
            let count = if filtered {
                format!(
                    "Packages {} of {}",
                    format::thousands(view.visible.len()),
                    format::thousands(rows.len())
                )
            } else {
                format!("Packages {}", format::thousands(rows.len()))
            };
            Line::styled(format!(" {count} "), theme.title())
        }
        None => Line::styled(format!(" {} ", ViewId::Browse.title()), theme.title()),
    };
    let budget = usize::from(width).saturating_sub(2 + left.width());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(left);
    if budget > 0 {
        block.title_top(right_title(view, theme, budget).right_aligned())
    } else {
        block
    }
}

/// The right end of the top border: the filter text while it is being
/// typed or holds one, then the repository filter, cut to `budget` cells
/// so it never reaches the title on the left.
fn right_title(view: &BrowseView, theme: &Theme, budget: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let show_filter = !view.filter.is_empty() || view.filter_focused;
    let repo = view
        .repo_filter
        .as_deref()
        .map(|name| format!("repo: {name}"));
    let repo_width = repo.as_deref().map_or(0, |text| Span::raw(text).width());
    let gap = usize::from(show_filter && repo.is_some());
    if show_filter {
        let room = budget.saturating_sub(2).saturating_sub(gap + repo_width);
        spans.push(Span::styled("/ ".to_string(), theme.muted()));
        spans.extend(view.filter.line(view.filter_focused, room, theme).spans);
    }
    if let Some(text) = repo {
        if show_filter {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(text, theme.muted()));
    }
    if !spans.is_empty() {
        spans.insert(0, Span::raw(" "));
        spans.push(Span::raw(" "));
    }
    let mut shown: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for span in spans {
        let width = span.width();
        if used + width > budget {
            break;
        }
        used += width;
        shown.push(span);
    }
    Line::from(shown)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::KeyCode;
    use soar_core::database::models::Package;

    use crate::{
        action::Action,
        effect::Effect,
        jobs::{InstallChoices, JobOutcome, JobSpec},
        loadable::Loadable,
        msg::{Loaded, Msg},
        test_support::{catalog_row, code, key, package, screen, test_app},
        views::browse::{BrowseView, CatalogRow, PackageKey},
    };

    /// A catalog reply for the load the app still expects before any
    /// reload has started.
    fn loaded(rows: Vec<CatalogRow>) -> Msg {
        Msg::Loaded(Loaded::Catalog {
            check: 0,
            result: Ok(rows),
        })
    }

    fn catalog() -> Vec<CatalogRow> {
        vec![
            catalog_row("bat", "0.24.0", "bincache"),
            catalog_row("fd", "10.2.0", "pkgforge"),
            catalog_row("jq", "1.7.1", "bincache"),
        ]
    }

    fn key_for(name: &str, repo: &str) -> PackageKey {
        PackageKey {
            repo_name: repo.to_string(),
            pkg_name: name.to_string(),
            pkg_family: None,
            pkg_id: None,
        }
    }

    fn bat_key() -> PackageKey {
        key_for("bat", "bincache")
    }

    #[test]
    fn the_exact_query_names_family_name_version_and_repo() {
        let mut key = key_for("ffmpeg", "bincache");
        key.pkg_family = Some("jellyfin".to_string());
        assert_eq!(key.query("7.1"), "jellyfin/ffmpeg@7.1:bincache");
        assert_eq!(
            key_for("bat", "bincache").query("0.24.0"),
            "bat@0.24.0:bincache"
        );
    }

    fn detail(key: PackageKey, result: Result<Option<Package>, String>) -> Msg {
        Msg::Loaded(Loaded::Detail {
            key,
            result: Box::new(result),
        })
    }

    #[test]
    fn a_loaded_catalog_renders_every_name() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("bat"), "screen is {screen:?}");
        assert!(screen.contains("fd"), "screen is {screen:?}");
        assert!(screen.contains("jq"), "screen is {screen:?}");
    }

    #[test]
    fn a_failed_catalog_shows_the_error() {
        let mut app = test_app();
        app.update(
            Msg::Loaded(Loaded::Catalog {
                check: 0,
                result: Err("metadata unreadable".to_string()),
            }),
            Instant::now(),
        );
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("metadata unreadable"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn an_empty_catalog_says_where_to_add_repositories() {
        let mut app = test_app();
        app.update(loaded(Vec::new()), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("No packages. Add or enable a repository in Repos (5)."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_loading_catalog_shows_the_spinner_line() {
        let mut app = test_app();
        app.startup();
        assert!(screen(&mut app, 80, 24).contains("Loading packages"));
    }

    #[test]
    fn more_rows_than_screen_lines_render_only_the_visible_ones() {
        let mut app = test_app();
        let rows: Vec<_> = (0..50)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        app.update(loaded(rows), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("pkg0"), "screen is {screen:?}");
        assert!(screen.contains("pkg15"), "screen is {screen:?}");
        assert!(!screen.contains("pkg40"), "screen is {screen:?}");
    }

    #[test]
    fn the_header_names_its_columns() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let screen = screen(&mut app, 80, 24);
        for label in ["NAME", "VERSION", "REPO", "TYPE"] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn moving_the_cursor_down_scrolls_the_window_with_it() {
        let mut app = test_app();
        let rows: Vec<_> = (0..50)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        for _ in 0..40 {
            app.update(key('j'), now);
        }
        let screen = screen(&mut app, 100, 30);
        assert!(screen.contains("pkg40"), "screen is {screen:?}");
        assert!(!screen.contains("pkg0"), "screen is {screen:?}");
    }

    #[test]
    fn the_title_shows_the_total_count() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        assert!(screen(&mut app, 80, 24).contains("Packages 3"));
    }

    #[test]
    fn the_title_shows_the_filtered_count() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        app.update(key('b'), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("Packages 1 of 3"), "screen is {screen:?}");
        assert!(screen.contains("/ b"), "screen is {screen:?}");
    }

    #[test]
    fn narrow_terminals_drop_the_type_then_the_repo_column() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let mid = screen(&mut app, 79, 20);
        assert!(mid.contains("REPO"), "screen is {mid:?}");
        assert!(!mid.contains("TYPE"), "screen is {mid:?}");
        let narrow = screen(&mut app, 69, 20);
        assert!(!narrow.contains("REPO"), "screen is {narrow:?}");
        assert!(!narrow.contains("TYPE"), "screen is {narrow:?}");
    }

    #[test]
    fn marked_and_installed_rows_show_their_symbols() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[1].installed = true;
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("●"), "screen is {screen:?}");
        assert!(screen.contains("✓"), "screen is {screen:?}");
        assert!(screen.contains("1 marked"), "screen is {screen:?}");
    }

    #[test]
    fn the_type_badge_renders() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[0].pkg_type = Some("appimage".to_string());
        app.update(loaded(rows), Instant::now());
        assert!(screen(&mut app, 80, 24).contains("appimage"));
    }

    #[test]
    fn typing_while_the_filter_holds_focus_reaches_the_filter_only() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        app.update(key('q'), now);
        assert!(!app.quit);
        assert!(app.browse.filter_focused);
        assert_eq!(app.browse.filter.value(), "q");
    }

    #[test]
    fn enter_keeps_the_filter_and_esc_clears_it() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['b', 'a'] {
            app.update(key(c), now);
        }
        assert_eq!(app.browse.visible, vec![0]);
        app.update(code(KeyCode::Enter), now);
        assert!(!app.browse.filter_focused);
        assert_eq!(app.browse.filter.value(), "ba");
        assert_eq!(app.browse.visible, vec![0]);
        app.update(code(KeyCode::Esc), now);
        assert!(!app.browse.filter_focused);
        assert!(app.browse.filter.value().is_empty());
        assert_eq!(app.browse.visible, vec![0, 1, 2]);
    }

    #[test]
    fn a_three_character_query_ranks_name_matches_above_description_matches() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[1].key.pkg_name = "mole".to_string();
        rows[1].haystack = "mole".to_string();
        rows[1].description = "a bat with poor eyesight".to_string();
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['b', 'a', 't'] {
            app.update(key(c), now);
        }
        assert_eq!(app.browse.visible, vec![0, 1]);
    }

    #[test]
    fn a_two_character_query_skips_the_descriptions() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[1].key.pkg_name = "mole".to_string();
        rows[1].haystack = "mole".to_string();
        rows[1].description = "a bat with poor eyesight".to_string();
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['b', 't'] {
            app.update(key(c), now);
        }
        assert_eq!(app.browse.visible, vec![0]);
    }

    #[test]
    fn a_filter_matching_nothing_shows_the_empty_text() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['x', 'y', 'z'] {
            app.update(key(c), now);
        }
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("No packages match xyz"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn refiltering_keeps_the_cursor_on_its_package_while_it_is_visible() {
        let mut app = test_app();
        let rows = vec![
            catalog_row("ant", "1.0", "bincache"),
            catalog_row("bat", "1.0", "bincache"),
            catalog_row("cat", "1.0", "bincache"),
        ];
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key('j'), now);
        assert_eq!(app.browse.cursor.selected, 1);
        app.update(key('/'), now);
        for c in ['b', 'a'] {
            app.update(key(c), now);
        }
        assert_eq!(app.browse.visible, vec![1]);
        assert_eq!(app.browse.cursor.selected, 0);
        for c in ['x', 'y'] {
            app.update(key(c), now);
        }
        assert!(app.browse.visible.is_empty());
        assert_eq!(app.browse.cursor.selected, 0);
    }

    #[test]
    fn marking_two_rows_then_hiding_one_keeps_both_selected() {
        let mut app = test_app();
        let rows = vec![
            catalog_row("red", "1.0", "bincache"),
            catalog_row("green", "1.0", "bincache"),
            catalog_row("blue", "1.0", "bincache"),
        ];
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        app.update(key(' '), now);
        assert_eq!(app.browse.marks.len(), 2);
        app.update(key('/'), now);
        for c in ['b', 'l'] {
            app.update(key(c), now);
        }
        assert_eq!(app.browse.visible, vec![2]);
        let keys = app.browse.selected_keys();
        assert_eq!(keys.len(), 2);
        let red = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "red".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        let green = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "green".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        assert!(keys.contains(&red));
        assert!(keys.contains(&green));
    }

    #[test]
    fn a_reload_onto_a_smaller_catalog_keeps_the_state_valid() {
        let mut app = test_app();
        let rows: Vec<_> = (0..50)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        for _ in 0..40 {
            app.update(key('j'), now);
        }
        app.update(key(' '), now);
        app.update(loaded(vec![catalog_row("bat", "1.0", "bincache")]), now);
        assert_eq!(app.browse.visible, vec![0]);
        assert_eq!(app.browse.cursor.selected, 0);
        assert_eq!(app.browse.marks.len(), 1);
        assert_eq!(app.browse.selected_keys().len(), 1);
        app.update(key(' '), now);
        assert_eq!(app.browse.marks.len(), 2);
    }

    #[test]
    fn a_second_esc_clears_the_marks() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        assert_eq!(app.browse.marks.len(), 1);
        app.update(key('/'), now);
        app.update(key('b'), now);
        app.update(code(KeyCode::Esc), now);
        assert!(app.browse.filter.value().is_empty());
        assert_eq!(app.browse.marks.len(), 1);
        app.update(code(KeyCode::Esc), now);
        assert!(app.browse.marks.is_empty());
    }

    #[test]
    fn cycling_the_repo_filter_walks_every_repository_and_back() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        assert_eq!(
            app.hint_suffix(&Action::CycleRepo),
            Some(": all".to_string())
        );
        assert!(screen(&mut app, 100, 24).contains("repo: all"));
        app.update(key('R'), now);
        assert_eq!(app.browse.repo_filter.as_deref(), Some("bincache"));
        assert_eq!(app.browse.visible, vec![0, 2]);
        assert_eq!(
            app.hint_suffix(&Action::CycleRepo),
            Some(": bincache".to_string())
        );
        app.update(key('R'), now);
        assert_eq!(app.browse.repo_filter.as_deref(), Some("pkgforge"));
        assert_eq!(app.browse.visible, vec![1]);
        app.update(key('R'), now);
        assert_eq!(app.browse.repo_filter, None);
        assert_eq!(app.browse.visible, vec![0, 1, 2]);
    }

    #[test]
    fn escape_clears_the_repo_filter() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('R'), now);
        assert_eq!(app.browse.repo_filter.as_deref(), Some("bincache"));
        app.update(code(KeyCode::Esc), now);
        assert_eq!(app.browse.repo_filter, None);
        assert_eq!(app.browse.visible, vec![0, 1, 2]);
    }

    #[test]
    fn the_filter_and_the_repo_filter_both_narrow_the_list() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('R'), now);
        app.update(key('R'), now);
        assert_eq!(app.browse.repo_filter.as_deref(), Some("pkgforge"));
        app.update(key('/'), now);
        app.update(key('f'), now);
        assert_eq!(app.browse.visible, vec![1]);
        assert_eq!(
            app.hint_suffix(&Action::CycleRepo),
            Some(": pkgforge".to_string())
        );
    }

    /// Times 20 filter passes over a catalog of 10,000 rows and asserts the
    /// average stays under 15 ms, so matching on every keystroke never
    /// lags. The numbers are only meaningful without debug assertions, so
    /// run it with `cargo test --release -- --ignored`.
    #[test]
    #[ignore]
    fn refiltering_ten_thousand_rows_stays_fast() {
        let mut view = BrowseView::default();
        let rows: Vec<_> = (0..10_000)
            .map(|n| {
                let mut row = catalog_row(&format!("package-{n}"), "1.0.0", "bincache");
                row.description = format!("a test package with the number {n} in it");
                row
            })
            .collect();
        view.catalog = Loadable::Ready(rows);
        view.refilter(Instant::now());
        view.filter.set("ab".to_string());
        let start = Instant::now();
        for _ in 0..20 {
            view.refilter(Instant::now());
        }
        let average = start.elapsed() / 20;
        assert!(
            average < Duration::from_millis(15),
            "average refilter took {average:?}"
        );
    }

    #[test]
    fn a_resting_cursor_requests_its_details_after_the_delay() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('j'), now);
        let effects = app.update(Msg::Tick, now + Duration::from_millis(50));
        assert!(effects.is_empty());
        assert!(app.browse.details.is_empty());
        let effects = app.update(Msg::Tick, now + Duration::from_millis(130));
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            &effects[0],
            Effect::LoadDetail {
                key,
                version,
            } if key.pkg_name == "fd" && version == "10.2.0"
        ));
        assert!(matches!(
            app.browse.details.get(&key_for("fd", "pkgforge")),
            Some(Loadable::Loading)
        ));
    }

    #[test]
    fn a_detail_request_waits_for_a_mid_reload_catalog() {
        let mut app = test_app();
        app.startup();
        let now = Instant::now();
        let key = bat_key();
        app.browse.wanted = Some((key.clone(), now));
        let effects = app.update(Msg::Tick, now + Duration::from_millis(200));
        assert!(effects.is_empty());
        assert!(app.browse.details.is_empty(), "nothing is cached yet");
        assert!(app.browse.wanted.is_some(), "the want stays for a retry");
        app.update(
            Msg::Loaded(Loaded::Catalog {
                check: app.catalog_load,
                result: Ok(catalog()),
            }),
            now,
        );
        let effects = app.update(Msg::Tick, now + Duration::from_millis(200));
        assert!(matches!(
            &effects[..],
            [Effect::LoadDetail {
                key,
                version,
            }] if key.pkg_name == "bat" && version == "0.24.0"
        ));
        assert!(matches!(
            app.browse.details.get(&bat_key()),
            Some(Loadable::Loading)
        ));
    }

    #[test]
    fn an_arrived_detail_is_cached_and_rendered_in_the_pane() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[1].description = "A descendant of the unix fortune program".to_string();
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(key('j'), now);
        let key = key_for("fd", "pkgforge");
        let mut fd = package("fd", "10.2.0", "pkgforge");
        fd.homepages = Some(vec!["https://github.com/sharkdp/fd".to_string()]);
        app.update(detail(key.clone(), Ok(Some(fd))), now);
        assert!(matches!(
            app.browse.details.get(&key),
            Some(Loadable::Ready(Some(_)))
        ));
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("A descendant of the unix fortune program"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("https://github.com/sharkdp/fd"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_failed_detail_shows_the_error_state() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        app.update(
            detail(bat_key(), Err("metadata locked".to_string())),
            Instant::now(),
        );
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("metadata locked"), "screen is {screen:?}");
    }

    #[test]
    fn a_detail_query_that_finds_nothing_renders_not_found() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        app.update(detail(bat_key(), Ok(None)), Instant::now());
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("not found"), "screen is {screen:?}");
    }

    #[test]
    fn an_install_job_clears_the_cached_details_but_a_sync_keeps_them() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        app.update(
            detail(bat_key(), Ok(Some(package("bat", "0.24.0", "bincache")))),
            Instant::now(),
        );
        assert_eq!(app.browse.details.len(), 1);
        let now = Instant::now();
        app.enqueue(
            JobSpec::Install {
                targets: Vec::new(),
                picked: vec![package("bat", "0.24.0", "bincache")],
                options: InstallChoices::default(),
            },
            now,
        );
        app.update(
            Msg::JobFinished {
                id: 0,
                outcome: JobOutcome {
                    ok: true,
                    aborted: false,
                    headline: "Installed bat 0.24.0".to_string(),
                    lines: Vec::new(),
                },
            },
            now,
        );
        assert!(app.browse.details.is_empty());
        app.update(
            detail(bat_key(), Ok(Some(package("bat", "0.24.0", "bincache")))),
            now,
        );
        app.enqueue(JobSpec::Sync, now);
        app.update(
            Msg::JobFinished {
                id: 1,
                outcome: JobOutcome {
                    ok: true,
                    aborted: false,
                    headline: "Repositories synced".to_string(),
                    lines: Vec::new(),
                },
            },
            now,
        );
        assert_eq!(app.browse.details.len(), 1);
    }

    #[test]
    fn the_cache_clears_itself_once_it_exceeds_its_bound() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        for n in 0..257 {
            let key = PackageKey {
                repo_name: "bincache".to_string(),
                pkg_name: format!("pkg{n}"),
                pkg_family: None,
                pkg_id: None,
            };
            app.browse.details.insert(key, Loadable::Ready(None));
        }
        assert_eq!(app.browse.details.len(), 257);
        app.update(
            detail(bat_key(), Ok(Some(package("bat", "0.24.0", "bincache")))),
            Instant::now(),
        );
        assert_eq!(app.browse.details.len(), 1);
    }

    #[test]
    fn enter_opens_the_full_screen_detail_and_esc_closes_it() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        assert!(app.browse.detail_open);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("bat"), "screen is {open:?}");
        assert!(!open.contains("NAME"), "screen is {open:?}");
        app.update(code(KeyCode::Esc), now);
        assert!(!app.browse.detail_open);
        let closed = screen(&mut app, 80, 24);
        assert!(closed.contains("NAME"), "screen is {closed:?}");
    }

    #[test]
    fn marking_still_works_while_the_detail_is_full_screen() {
        let mut app = test_app();
        app.update(loaded(catalog()), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        app.update(key(' '), now);
        assert!(app.browse.detail_open);
        assert_eq!(app.browse.marks.len(), 1);
    }

    #[test]
    fn the_full_screen_detail_scrolls_with_the_cursor_keys() {
        let mut app = test_app();
        let mut rows = catalog();
        rows[0].description = "lorem ipsum dolor sit amet ".repeat(80);
        app.update(loaded(rows), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        assert!(screen(&mut app, 80, 24).contains("0.24.0"));
        assert!(app.browse.detail_rows > 20);
        app.update(key('G'), now);
        assert_eq!(app.browse.detail.selected, app.browse.detail_rows - 1);
        assert!(!screen(&mut app, 80, 24).contains("0.24.0"));
        app.update(key('g'), now);
        assert!(screen(&mut app, 80, 24).contains("0.24.0"));
    }

    /// A catalog ten thousand rows long must still draw instantly: the
    /// window builds only the visible rows, so the frame cost cannot grow
    /// with the catalog. Timings are only meaningful in release mode, so
    /// the test carries `ignore` and runs through
    /// `cargo test -p soar-tui --release -- --ignored`.
    #[test]
    #[ignore]
    fn a_ten_thousand_row_catalog_draws_under_five_milliseconds() {
        let rows: Vec<CatalogRow> = (0..10_000)
            .map(|n| catalog_row(&format!("pkg{n}"), "1.0.0", "bincache"))
            .collect();
        let mut app = test_app();
        app.update(loaded(rows), Instant::now());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 60)).unwrap();
        terminal.draw(|f| crate::ui::render(f, &mut app)).unwrap();
        let start = Instant::now();
        for _ in 0..100 {
            terminal.draw(|f| crate::ui::render(f, &mut app)).unwrap();
        }
        let average = start.elapsed() / 100;
        println!("average frame: {average:?}");
        assert!(
            average < Duration::from_millis(5),
            "average frame was {average:?}"
        );
    }
}
