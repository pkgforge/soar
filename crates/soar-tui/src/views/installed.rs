//! The installed packages view: its table, the filter, the sort, the
//! marks, and the detail pane.

use std::{collections::HashSet, ops::Range, time::SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Row, Table},
};
use soar_operations::{InstalledEntry, InstalledListResult};

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    views::ViewId,
    widgets::{installed_detail, scroll::ListCursor, states, text_input::TextInput},
};

/// The narrowest terminal that shows the list beside the detail pane.
const SPLIT_WIDTH: u16 = 100;

/// The order the installed list sorts its rows in.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstalledSort {
    /// By package name.
    #[default]
    Name,
    /// By size on disk, largest first.
    Size,
    /// By install date, newest first.
    Date,
}

impl InstalledSort {
    /// The sort this one cycles to: name, then size, then date, then back
    /// to name.
    pub(crate) fn next(self) -> InstalledSort {
        match self {
            InstalledSort::Name => InstalledSort::Size,
            InstalledSort::Size => InstalledSort::Date,
            InstalledSort::Date => InstalledSort::Name,
        }
    }

    /// The word the sort hint shows.
    pub(crate) fn label(self) -> &'static str {
        match self {
            InstalledSort::Name => "name",
            InstalledSort::Size => "size",
            InstalledSort::Date => "date",
        }
    }
}

/// The state of the installed packages view.
#[derive(Default)]
pub(crate) struct InstalledView {
    /// The whole installed list, loaded at startup and reloaded after any
    /// job that changes installs.
    pub(crate) list: Loadable<InstalledListResult>,
    /// The indexes into the list of the rows shown, in display order.
    pub(crate) visible: Vec<usize>,
    /// The visible row the cursor sits on.
    pub(crate) cursor: ListCursor,
    /// The text the filter matches rows against.
    pub(crate) filter: TextInput,
    /// Whether the filter holds the keyboard.
    pub(crate) filter_focused: bool,
    /// The order the rows sort in.
    pub(crate) sort: InstalledSort,
    /// The rows marked for the next bulk action, by package id.
    pub(crate) marks: HashSet<u64>,
    /// The scroll of the detail pane while it fills the screen.
    pub(crate) detail: ListCursor,
    /// How many rows the detail held at the last render, so scrolling
    /// moves over what the pane actually shows.
    pub(crate) detail_rows: usize,
    /// Whether the detail fills the whole body instead of sitting beside
    /// the list.
    pub(crate) detail_open: bool,
}

impl InstalledView {
    /// The entry the cursor points at, when the list has one. The indexes
    /// can be stale right after a reload replaced the list, so the lookup
    /// tolerates a missing row.
    pub(crate) fn cursor_entry(&self) -> Option<&InstalledEntry> {
        let listing = self.list.ready()?;
        let index = *self.visible.get(self.cursor.selected)?;
        listing.packages.get(index)
    }

    /// The package id of the row the cursor points at, when there is one.
    fn cursor_id(&self) -> Option<u64> {
        self.cursor_entry().map(|entry| entry.package.id)
    }

    /// Rebuilds the visible rows from the filter text and the sort. The
    /// cursor stays on its package while that row survives the new order.
    pub(crate) fn refilter(&mut self) {
        let previous = self.cursor_id();
        let query = self.filter.value().trim().to_lowercase();
        let mut shown: Vec<usize> = Vec::new();
        if let Loadable::Ready(listing) = &self.list {
            for (index, entry) in listing.packages.iter().enumerate() {
                if !is_finished(entry) {
                    continue;
                }
                if !query.is_empty() && !haystack(entry).contains(&query) {
                    continue;
                }
                shown.push(index);
            }
            match self.sort {
                InstalledSort::Name => {
                    shown.sort_by(|&a, &b| {
                        let (x, y) = (&listing.packages[a], &listing.packages[b]);
                        x.package
                            .pkg_name
                            .cmp(&y.package.pkg_name)
                            .then(x.package.pkg_family.cmp(&y.package.pkg_family))
                            .then(x.package.repo_name.cmp(&y.package.repo_name))
                    })
                }
                InstalledSort::Size => {
                    shown.sort_by(|&a, &b| {
                        listing.packages[b]
                            .disk_size
                            .cmp(&listing.packages[a].disk_size)
                    })
                }
                InstalledSort::Date => {
                    shown.sort_by(|&a, &b| {
                        let (x, y) = (&listing.packages[a], &listing.packages[b]);
                        date_of(y).cmp(&date_of(x))
                    })
                }
            }
        }
        self.visible = shown;
        self.cursor = ListCursor::default();
        if let Some(previous) = previous
            && let Loadable::Ready(listing) = &self.list
            && let Some(position) = self
                .visible
                .iter()
                .position(|&index| listing.packages[index].package.id == previous)
        {
            self.cursor.selected = position;
        }
    }

    /// Marks or unmarks the row under the cursor, then moves one row down.
    pub(crate) fn toggle_mark(&mut self) {
        let Some(&index) = self.visible.get(self.cursor.selected) else {
            return;
        };
        let Loadable::Ready(listing) = &self.list else {
            return;
        };
        let Some(entry) = listing.packages.get(index) else {
            return;
        };
        let id = entry.package.id;
        if !self.marks.remove(&id) {
            self.marks.insert(id);
        }
        self.cursor.down(1, self.visible.len());
    }

    /// The rows the next bulk action works on: every marked row, or the
    /// row under the cursor when nothing is marked. The records are
    /// copied so the removal confirm keeps them until it is accepted.
    /// Marks whose rows a reload dropped are skipped.
    pub(crate) fn selected_entries(&self) -> Vec<InstalledEntry> {
        let Some(listing) = self.list.ready() else {
            return Vec::new();
        };
        if self.marks.is_empty() {
            return self.cursor_entry().map(cloned_entry).into_iter().collect();
        }
        listing
            .packages
            .iter()
            .filter(|entry| self.marks.contains(&entry.package.id))
            .map(cloned_entry)
            .collect()
    }

    /// Moves the sort to its next order and rebuilds the rows.
    pub(crate) fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.refilter();
    }

    /// Opens the full-screen detail on the row under the cursor, or
    /// closes it back to the list.
    pub(crate) fn toggle_detail(&mut self) {
        if self.detail_open {
            self.detail_open = false;
        } else if self.cursor_entry().is_some() {
            self.detail_open = true;
            self.detail.top();
        }
    }
}

/// Whether an entry represents a finished install. Rows recorded with
/// `is_installed == false` are leftovers of interrupted installs, which
/// Health handles, so the list skips them.
fn is_finished(entry: &InstalledEntry) -> bool {
    entry.package.is_installed
}

/// A copy of one entry: the record cloned, the computed size and health
/// kept as they are.
fn cloned_entry(entry: &InstalledEntry) -> InstalledEntry {
    InstalledEntry {
        package: entry.package.clone(),
        disk_size: entry.disk_size,
        is_healthy: entry.is_healthy,
    }
}

/// The text the filter matches against: `family/name` when a family is
/// recorded, else the name.
fn haystack(entry: &InstalledEntry) -> String {
    match entry.package.pkg_family.as_deref() {
        Some(family) => format!("{family}/{}", entry.package.pkg_name),
        None => entry.package.pkg_name.clone(),
    }
    .to_lowercase()
}

/// The install date an entry was recorded with, when it parses.
fn date_of(entry: &InstalledEntry) -> Option<SystemTime> {
    format::parse_installed_date(&entry.package.installed_date)
}

/// Draws the installed packages view: the list pane, and beside it the
/// detail pane of the row under the cursor, or the detail alone while it
/// fills the screen.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.installed.detail_open {
        installed_detail::render(f, area, app, true);
        return;
    }
    if area.width >= SPLIT_WIDTH {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);
        render_list(f, list, app);
        installed_detail::render(f, detail, app, false);
    } else {
        render_list(f, area, app);
    }
}

/// Draws the list pane: its border with the count, the total size, and the
/// filter in the title, then the table of visible rows or the loading,
/// error, and empty states.
fn render_list(f: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let rows_height = usize::from(area.height.saturating_sub(3));
    let window = match &app.installed.list {
        Loadable::Ready(_) => {
            app.installed
                .cursor
                .window(app.installed.visible.len(), rows_height)
        }
        _ => 0..0,
    };
    let block = list_block(&app.installed, theme, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = &app.installed;
    match &view.list {
        Loadable::Idle => {}
        Loadable::Loading => {
            states::loading(f, inner, theme, app.frame, "Loading installed packages")
        }
        Loadable::Failed(error) => states::error(f, inner, theme, error),
        Loadable::Ready(listing) if listing.packages.is_empty() => {
            states::empty(
                f,
                inner,
                theme,
                "Nothing installed yet. Find something in Browse (1).",
            )
        }
        Loadable::Ready(_) if view.visible.is_empty() => {
            states::empty(
                f,
                inner,
                theme,
                &format!("No packages match {}", view.filter.value()),
            )
        }
        Loadable::Ready(listing) => {
            let (columns, constraints) = columns(area.width);
            let name_width = usize::from(name_column_width(area.width, &constraints));
            let offset = window.start;
            let table = Table::new(
                table_rows(view, listing, window, &columns, name_width, theme),
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

/// Which optional columns the table shows at one terminal width.
#[derive(Clone, Copy)]
struct Columns {
    /// The repository column.
    repo: bool,
    /// The install date column.
    installed: bool,
    /// The profile column.
    profile: bool,
}

/// The columns the table shows at one terminal width. The profile column
/// leaves on narrow terminals, the install date on very narrow ones, and
/// the repository column on the narrowest.
fn columns(width: u16) -> (Columns, Vec<Constraint>) {
    let shown = Columns {
        repo: width >= 70,
        installed: width >= 80,
        profile: width >= 100,
    };
    let mut constraints = vec![
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(14),
    ];
    if shown.repo {
        constraints.push(Constraint::Length(12));
    }
    constraints.push(Constraint::Length(10));
    if shown.installed {
        constraints.push(Constraint::Length(12));
    }
    if shown.profile {
        constraints.push(Constraint::Length(10));
    }
    constraints.push(Constraint::Length(8));
    (shown, constraints)
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
    labels.push("SIZE");
    if columns.installed {
        labels.push("INSTALLED");
    }
    if columns.profile {
        labels.push("PROFILE");
    }
    labels.push("FLAG");
    Row::new(labels).style(theme.muted().add_modifier(Modifier::BOLD))
}

/// Builds the rows the window shows: the mark, the name, and the columns
/// that exist at this width.
fn table_rows(
    view: &InstalledView,
    listing: &InstalledListResult,
    window: Range<usize>,
    columns: &Columns,
    name_width: usize,
    theme: &Theme,
) -> Vec<Row<'static>> {
    let shown = &view.visible[window.clone()];
    window
        .zip(shown.iter())
        .map(|(position, &index)| {
            let entry = &listing.packages[index];
            list_row(
                entry,
                view.marks.contains(&entry.package.id),
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
    entry: &InstalledEntry,
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
        name_cell(entry, theme, name_width),
        Cell::from(Span::styled(entry.package.version.clone(), theme.accent2())),
    ];
    if columns.repo {
        cells.push(Cell::from(Span::styled(
            entry.package.repo_name.clone(),
            theme.info(),
        )));
    }
    cells.push(Cell::from(
        Line::from(Span::raw(format::size(entry.disk_size))).alignment(Alignment::Right),
    ));
    if columns.installed {
        cells.push(Cell::from(Span::styled(age_of(entry), theme.text())));
    }
    if columns.profile {
        cells.push(Cell::from(Span::styled(
            entry.package.profile.clone(),
            theme.muted(),
        )));
    }
    cells.push(Cell::from(
        flag_span(entry, theme).unwrap_or_else(|| Span::raw("")),
    ));
    let style = if cursor_here {
        theme.cursor_row()
    } else {
        Style::new()
    };
    Row::new(cells).style(style)
}

/// How long ago an entry was installed, such as `3 days ago`, or nothing
/// when its date is missing or unreadable.
fn age_of(entry: &InstalledEntry) -> String {
    date_of(entry).map_or_else(String::new, |then| {
        format::relative(then, SystemTime::now())
    })
}

/// The name cell: the state symbol first, then `family/` before the name
/// in the family color.
fn name_cell(entry: &InstalledEntry, theme: &Theme, width: usize) -> Cell<'static> {
    let symbols = theme.sym();
    let symbol = status_span(entry, theme);
    let mut full = String::new();
    let mut family_chars = 0;
    if let Some(family) = entry.package.pkg_family.as_deref() {
        full.push_str(family);
        full.push('/');
        family_chars = full.chars().count();
    }
    full.push_str(&entry.package.pkg_name);
    let reserved = symbol.width() + 1;
    let text = format::truncate(&full, width.saturating_sub(reserved), symbols.ellipsis);
    let mut spans = vec![symbol, Span::raw(" ")];
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
    Cell::from(Line::from(spans))
}

/// The state span starting the name cell: broken first, then not active,
/// then pinned, then the plain installed mark, each in its own color.
fn status_span(entry: &InstalledEntry, theme: &Theme) -> Span<'static> {
    let symbols = theme.sym();
    let (symbol, style) = if !entry.is_healthy {
        (symbols.warning, theme.danger())
    } else if entry.package.unlinked {
        (symbols.available, theme.muted())
    } else if entry.package.pinned {
        (symbols.pinned, theme.warning())
    } else {
        (symbols.installed, theme.success())
    };
    Span::styled(symbol.to_string(), style)
}

/// The word the flag column shows, first match wins: broken, then
/// inactive, then pinned.
fn flag_span(entry: &InstalledEntry, theme: &Theme) -> Option<Span<'static>> {
    if !entry.is_healthy {
        Some(Span::styled("broken", theme.danger()))
    } else if entry.package.unlinked {
        Some(Span::styled("inactive", theme.muted()))
    } else if entry.package.pinned {
        Some(Span::styled("pinned", theme.warning()))
    } else {
        None
    }
}

/// The pane border: the shown count and the total size on the left of the
/// top edge, and the filter on its right.
fn list_block(view: &InstalledView, theme: &Theme, width: u16) -> Block<'static> {
    let left = match view.list.ready() {
        Some(listing) => {
            Line::styled(
                format!(
                    " Installed {} {} {} ",
                    view.visible.len(),
                    theme.sym().dot,
                    format::size(listing.total_size)
                ),
                theme.title(),
            )
        }
        None => Line::styled(format!(" {} ", ViewId::Installed.title()), theme.title()),
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
/// typed or holds one, cut to `budget` cells so it never reaches the
/// title on the left.
fn right_title(view: &InstalledView, theme: &Theme, budget: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if !view.filter.is_empty() || view.filter_focused {
        let room = budget.saturating_sub(2);
        spans.push(Span::styled("/ ".to_string(), theme.muted()));
        spans.extend(view.filter.line(view.filter_focused, room, theme).spans);
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
    use std::time::Instant;

    use ratatui::crossterm::event::KeyCode;
    use soar_operations::{InstalledEntry, InstalledListResult};

    use crate::{
        action::Action,
        app::App,
        effect::Effect,
        format,
        jobs::JobSpec,
        msg::{Loaded, Msg},
        test_support::{code, installed_entry, key, screen, test_app},
        views::{ViewId, installed::InstalledSort},
        widgets::overlay::Overlay,
    };

    /// A loaded installed list holding the given entries, with the total
    /// size and count summed from them, answering the load the app still
    /// expects before any reload has started.
    fn installed(entries: Vec<InstalledEntry>) -> Msg {
        let total_size = entries.iter().map(|entry| entry.disk_size).sum();
        let total_count = entries.len();
        Msg::Loaded(Loaded::Installed {
            check: 0,
            result: Ok(InstalledListResult {
                packages: entries,
                total_count,
                total_size,
            }),
        })
    }

    fn catalog() -> Vec<InstalledEntry> {
        vec![
            installed_entry("bat", "0.24.0", "bincache"),
            installed_entry("fd", "10.2.0", "bincache"),
            installed_entry("jq", "1.7.1", "pkgcache"),
        ]
    }

    fn failed(message: &str) -> Msg {
        Msg::Loaded(Loaded::Installed {
            check: 0,
            result: Err(message.to_string()),
        })
    }

    /// The interface state on the installed view, where its keys and its
    /// pane are live.
    fn installed_app() -> App {
        let mut app = test_app();
        app.view = ViewId::Installed;
        app
    }

    #[test]
    fn a_loaded_list_renders_every_name() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let screen = screen(&mut app, 100, 24);
        for name in ["bat", "fd", "jq"] {
            assert!(screen.contains(name), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_failed_list_shows_the_error() {
        let mut app = installed_app();
        app.update(failed("database locked"), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("database locked"), "screen is {screen:?}");
    }

    #[test]
    fn an_empty_list_says_where_to_find_packages() {
        let mut app = installed_app();
        app.update(installed(Vec::new()), Instant::now());
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Nothing installed yet. Find something in Browse (1)."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_loading_list_shows_the_spinner_line() {
        let mut app = installed_app();
        app.startup();
        assert!(screen(&mut app, 80, 24).contains("Loading installed packages"));
    }

    #[test]
    fn the_title_shows_the_shown_count_and_the_total_size() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].disk_size = 5_452_595;
        entries[1].disk_size = 3_250_585;
        entries[2].disk_size = 300;
        app.update(installed(entries), Instant::now());
        let expected = format!(
            "Installed 3 {} {}",
            app.theme.sym().dot,
            format::size(5_452_595 + 3_250_585 + 300)
        );
        assert!(screen(&mut app, 100, 24).contains(&expected));
    }

    #[test]
    fn the_header_names_its_columns() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let screen = screen(&mut app, 200, 24);
        for label in [
            "NAME",
            "VERSION",
            "REPO",
            "SIZE",
            "INSTALLED",
            "PROFILE",
            "FLAG",
        ] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn narrow_terminals_drop_the_profile_then_the_date_then_the_repo_column() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let full = screen(&mut app, 200, 20);
        assert!(full.contains("PROFILE"), "screen is {full:?}");
        let no_profile = screen(&mut app, 99, 20);
        assert!(!no_profile.contains("PROFILE"), "screen is {no_profile:?}");
        assert!(no_profile.contains("INSTALLED"), "screen is {no_profile:?}");
        let no_date = screen(&mut app, 79, 20);
        assert!(!no_date.contains("INSTALLED"), "screen is {no_date:?}");
        assert!(no_date.contains("REPO"), "screen is {no_date:?}");
        let no_repo = screen(&mut app, 69, 20);
        assert!(!no_repo.contains("REPO"), "screen is {no_repo:?}");
    }

    #[test]
    fn rows_sorted_by_size_put_the_biggest_first() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].disk_size = 300;
        entries[1].disk_size = 5_452_595;
        entries[2].disk_size = 1_048_576;
        app.update(installed(entries), Instant::now());
        app.update(key('o'), Instant::now());
        assert_eq!(app.installed.sort, InstalledSort::Size);
        assert_eq!(app.installed.visible, vec![1, 2, 0]);
    }

    #[test]
    fn date_sort_puts_the_newest_first() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.installed_date = "2026-01-01 10:00:00".to_string();
        entries[1].package.installed_date = "2026-06-01 10:00:00".to_string();
        entries[2].package.installed_date = "2024-03-05 09:30:00".to_string();
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(key('o'), now);
        app.update(key('o'), now);
        assert_eq!(app.installed.sort, InstalledSort::Date);
        assert_eq!(app.installed.visible, vec![1, 0, 2]);
    }

    #[test]
    fn a_row_left_by_an_interrupted_install_is_skipped() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[1].package.is_installed = false;
        app.update(installed(entries), Instant::now());
        assert_eq!(app.installed.visible, vec![0, 2]);
        let screen = screen(&mut app, 100, 24);
        assert!(!screen.contains("fd"), "screen is {screen:?}");
    }

    #[test]
    fn filter_ba_keeps_bat_and_hides_fd() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['b', 'a'] {
            app.update(key(c), now);
        }
        assert_eq!(app.installed.visible, vec![0]);
        app.update(code(KeyCode::Esc), now);
        assert_eq!(app.installed.visible, vec![0, 1, 2]);
    }

    #[test]
    fn the_filter_matches_the_family_before_the_name() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.pkg_family = Some("sharkdp".to_string());
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['s', 'h', 'a', 'r', 'k', 'd', 'p', '/', 'b'] {
            app.update(key(c), now);
        }
        assert_eq!(app.installed.visible, vec![0]);
    }

    #[test]
    fn a_filter_matching_nothing_shows_the_empty_text() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
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
    fn typing_while_the_filter_holds_focus_reaches_the_filter_only() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        app.update(key('q'), now);
        assert!(!app.quit);
        assert!(app.installed.filter_focused);
        assert_eq!(app.installed.filter.value(), "q");
    }

    #[test]
    fn enter_keeps_the_filter_and_esc_clears_it() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['b', 'a'] {
            app.update(key(c), now);
        }
        app.update(code(KeyCode::Enter), now);
        assert!(!app.installed.filter_focused);
        assert_eq!(app.installed.filter.value(), "ba");
        assert_eq!(app.installed.visible, vec![0]);
        app.update(code(KeyCode::Esc), now);
        assert!(!app.installed.filter_focused);
        assert!(app.installed.filter.value().is_empty());
        assert_eq!(app.installed.visible, vec![0, 1, 2]);
    }

    #[test]
    fn a_second_esc_clears_the_marks() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        assert_eq!(app.installed.marks.len(), 1);
        app.update(key('/'), now);
        app.update(key('b'), now);
        app.update(code(KeyCode::Esc), now);
        assert!(app.installed.filter.value().is_empty());
        assert_eq!(app.installed.marks.len(), 1);
        app.update(code(KeyCode::Esc), now);
        assert!(app.installed.marks.is_empty());
    }

    #[test]
    fn space_marks_the_row_its_package_is_recorded_under() {
        let mut app = installed_app();
        let entries = catalog();
        let first = entries[0].package.id;
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        assert_eq!(app.installed.marks, [first].into_iter().collect());
        assert_eq!(app.installed.cursor.selected, 1);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("1 marked"), "screen is {screen:?}");
    }

    #[test]
    fn cycling_the_sort_shows_its_name_in_the_hint_suffix() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        assert_eq!(
            app.hint_suffix(&Action::CycleSort),
            Some(": name".to_string())
        );
        assert!(screen(&mut app, 100, 24).contains("sort: name"));
        app.update(key('o'), now);
        assert_eq!(
            app.hint_suffix(&Action::CycleSort),
            Some(": size".to_string())
        );
        app.update(key('o'), now);
        assert_eq!(
            app.hint_suffix(&Action::CycleSort),
            Some(": date".to_string())
        );
        app.update(key('o'), now);
        assert_eq!(
            app.hint_suffix(&Action::CycleSort),
            Some(": name".to_string())
        );
    }

    #[test]
    fn broken_inactive_and_pinned_rows_show_their_symbols_and_flags() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].is_healthy = false;
        entries[1].package.unlinked = true;
        entries[2].package.pinned = true;
        app.update(installed(entries), Instant::now());
        let screen = screen(&mut app, 110, 24);
        for text in ["⚠", "broken", "○", "inactive", "⊙", "pinned"] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn enter_toggles_the_detail_flag() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        assert!(app.installed.detail_open);
        app.update(code(KeyCode::Esc), now);
        assert!(!app.installed.detail_open);
    }

    #[test]
    fn enter_without_a_row_opens_nothing() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['x', 'y', 'z'] {
            app.update(key(c), now);
        }
        app.update(code(KeyCode::Enter), now);
        assert!(!app.installed.detail_open);
    }

    #[test]
    fn refiltering_keeps_the_cursor_on_its_package_while_it_is_visible() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[1].package.pkg_name = "ant".to_string();
        entries[2].package.pkg_name = "cat".to_string();
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(key('j'), now);
        assert_eq!(app.installed.cursor.selected, 1);
        app.update(key('/'), now);
        for c in ['b', 'a'] {
            app.update(key(c), now);
        }
        assert_eq!(app.installed.visible, vec![0]);
        assert_eq!(app.installed.cursor.selected, 0);
        app.update(code(KeyCode::Esc), now);
        assert_eq!(app.installed.cursor.selected, 1);
    }

    #[test]
    fn more_rows_than_screen_lines_render_only_the_visible_ones() {
        let mut app = installed_app();
        let entries: Vec<_> = (0..50)
            .map(|n| installed_entry(&format!("pkg{n:03}"), "1.0.0", "bincache"))
            .collect();
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        for _ in 0..40 {
            app.update(key('j'), now);
        }
        let screen = screen(&mut app, 100, 30);
        assert!(screen.contains("pkg040"), "screen is {screen:?}");
        assert!(!screen.contains("pkg000"), "screen is {screen:?}");
        assert!(!screen.contains("pkg049"), "screen is {screen:?}");
    }

    #[test]
    fn an_install_date_renders_as_a_relative_age() {
        let mut app = installed_app();
        let mut entries = catalog();
        let three_days = chrono::Utc::now() - chrono::Duration::days(3);
        entries[0].package.installed_date = three_days.format("%Y-%m-%d %H:%M:%S").to_string();
        app.update(installed(entries), Instant::now());
        let screen = screen(&mut app, 99, 24);
        assert!(screen.contains("3 days ago"), "screen is {screen:?}");
    }

    #[test]
    fn enter_opens_the_full_screen_detail_and_esc_closes_it_at_narrow_width() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        assert!(app.installed.detail_open);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("0.24.0"), "screen is {open:?}");
        assert!(!open.contains("NAME"), "screen is {open:?}");
        app.update(code(KeyCode::Esc), now);
        assert!(!app.installed.detail_open);
        let closed = screen(&mut app, 80, 24);
        assert!(closed.contains("NAME"), "screen is {closed:?}");
    }

    #[test]
    fn the_side_pane_shows_the_cursor_rows_details_at_full_width() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.installed_path = "/opt/unique-path/bat".to_string();
        app.update(installed(entries), Instant::now());
        let wide = screen(&mut app, 110, 24);
        assert!(wide.contains("/opt/unique-path/bat"), "screen is {wide:?}");
        assert!(wide.contains("NAME"), "screen is {wide:?}");
        let narrow = screen(&mut app, 99, 24);
        assert!(
            !narrow.contains("/opt/unique-path/bat"),
            "screen is {narrow:?}"
        );
    }

    #[test]
    fn the_detail_pane_titles_and_heads_itself_like_the_catalog_pane() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.pkg_family = Some("sharkdp".to_string());
        entries[0].package.pkg_type = Some("static".to_string());
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("sharkdp/bat"), "screen is {screen:?}");
        assert!(screen.contains("0.24.0"), "screen is {screen:?}");
        assert!(screen.contains("static"), "screen is {screen:?}");
        assert!(screen.contains("bincache"), "screen is {screen:?}");
    }

    #[test]
    fn a_broken_entry_shows_the_missing_files_banner_when_the_detail_is_open() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].is_healthy = false;
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains(app.theme.sym().error));
        assert!(
            screen.contains("Files are missing. Remove it, or use Health (6) to clean up."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_healthy_entry_shows_no_missing_files_banner() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(
            !screen.contains("Files are missing"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_detail_shows_the_recorded_date_and_the_relative_age() {
        let mut app = installed_app();
        let mut entries = catalog();
        let three_days = chrono::Utc::now() - chrono::Duration::days(3);
        let date = three_days.format("%Y-%m-%d %H:%M:%S").to_string();
        entries[0].package.installed_date = date.clone();
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("(3 days ago)"), "screen is {screen:?}");
        assert!(screen.contains(&date), "screen is {screen:?}");
    }

    #[test]
    fn the_checksum_is_shortened_with_an_ellipsis() {
        let mut app = installed_app();
        let mut entries = catalog();
        let checksum = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        entries[0].package.checksum = Some(checksum.to_string());
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let expected = format::truncate(checksum, 16, app.theme.sym().ellipsis);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains(&expected), "screen is {screen:?}");
        assert!(!screen.contains(checksum), "screen is {screen:?}");
    }

    #[test]
    fn portable_directories_show_one_line_each() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.portable_home = Some("~/.local/share/bat".to_string());
        entries[0].package.portable_cache = Some("~/.cache/bat".to_string());
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("home ~/.local/share/bat"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("cache ~/.cache/bat"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_flags_row_joins_the_recorded_states() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.pinned = true;
        entries[0].package.detached = true;
        entries[0].package.unlinked = true;
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("pinned, detached, inactive"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_detail_shows_the_profile_the_size_and_the_source() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].package.profile = "default".to_string();
        entries[0].disk_size = 5_452_595;
        entries[0].package.download_url = Some("https://example.com/bat-0.24.0.tar.gz".to_string());
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(code(KeyCode::Enter), now);
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("default"), "screen is {screen:?}");
        assert!(screen.contains("5.2 MiB"), "screen is {screen:?}");
        assert!(
            screen.contains("https://example.com/bat-0.24.0.tar.gz"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn d_on_a_row_opens_a_danger_confirm_naming_the_package() {
        let mut app = installed_app();
        let mut entries = catalog();
        entries[0].disk_size = 5_452_595;
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        let effects = app.update(key('d'), now);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Remove bat"), "screen is {open:?}");
        assert!(
            open.contains("bat  0.24.0  bincache  5.2 MiB"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("5.2 MiB will be freed."),
            "screen is {open:?}"
        );
    }

    #[test]
    fn y_on_the_remove_confirm_enqueues_the_job_and_clears_the_marks() {
        let mut app = installed_app();
        let entries = catalog();
        let first = entries[0].package.id;
        app.update(installed(entries), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        assert_eq!(app.installed.marks, [first].into_iter().collect());
        app.update(key('d'), now);
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Remove {
                    packages,
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the removal to start");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].pkg_name, "bat");
        assert!(app.installed.marks.is_empty());
        assert!(app.overlay.is_none());
    }

    #[test]
    fn two_marked_rows_show_the_plural_title_and_both_names() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key(' '), now);
        app.update(key(' '), now);
        app.update(key('d'), now);
        let open = screen(&mut app, 80, 24);
        assert!(open.contains("Remove 2 packages"), "screen is {open:?}");
        assert!(
            open.contains("bat  0.24.0  bincache  0.0 B"),
            "screen is {open:?}"
        );
        assert!(
            open.contains("fd  10.2.0  bincache  0.0 B"),
            "screen is {open:?}"
        );
        let effects = app.update(key('y'), now);
        let [
            Effect::StartJob {
                spec: JobSpec::Remove {
                    packages,
                },
                ..
            },
        ] = &effects[..]
        else {
            panic!("expected the removal to start");
        };
        assert_eq!(packages.len(), 2);
    }

    #[test]
    fn esc_on_the_remove_confirm_enqueues_nothing() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('d'), now);
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        let effects = app.update(code(KeyCode::Esc), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.jobs.list.is_empty());
    }

    #[test]
    fn d_without_a_row_opens_nothing() {
        let mut app = installed_app();
        app.update(installed(catalog()), Instant::now());
        let now = Instant::now();
        app.update(key('/'), now);
        for c in ['x', 'y', 'z'] {
            app.update(key(c), now);
        }
        app.update(code(KeyCode::Enter), now);
        let effects = app.update(key('d'), now);
        assert!(effects.is_empty());
        assert!(app.overlay.is_none());
        assert!(app.jobs.list.is_empty());
    }
}
