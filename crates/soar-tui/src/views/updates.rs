//! The available updates view: its table, the marks, the keep-old flag,
//! and the skipped list.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, List, ListItem, Row, Table},
};
use soar_operations::UpdateInfo;

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    views::ViewId,
    widgets::{scroll::ListCursor, states},
};

/// The state of the available updates view.
#[derive(Default)]
pub(crate) struct UpdatesView {
    /// The checked updates, loaded on the first visit and again on every
    /// manual recheck.
    pub(crate) list: Loadable<Vec<UpdateInfo>>,
    /// How many packages the running check has reported on so far.
    pub(crate) checked: usize,
    /// Packages the running check passed over, each with why.
    pub(crate) skipped: Vec<(String, String)>,
    /// Whether the skipped list replaces the table.
    pub(crate) show_skipped: bool,
    /// The visible row the cursor sits on.
    pub(crate) cursor: ListCursor,
    /// The rows marked for the next update, by index into the ready list.
    /// The indexes move on every load, so the marks clear with it.
    pub(crate) marks: HashSet<usize>,
    /// Whether superseded versions stay on disk during an update.
    pub(crate) keep_old: bool,
    /// The packages a check is limited to, when it was started for a
    /// selection made in another view.
    pub(crate) scope: Option<Vec<String>>,
    /// The number of the check in flight. Every start numbers itself so a
    /// result from a check a newer one replaced can be dropped.
    pub(crate) check: u32,
    /// Whether the check in flight replaced one that was still running,
    /// so its unnumbered events could have landed in the live counters.
    /// The next load cannot tell those strays from the newest check's
    /// own progress and drops the lot.
    superseded: bool,
}

impl UpdatesView {
    /// Starts a fresh check: the count and the skipped list of the last
    /// one go, and the marks and cursor go when the result arrives.
    /// A check that replaces one still running marks the progress
    /// suspect, since the old check's events keep landing while both
    /// run.
    pub(crate) fn start_check(&mut self) {
        self.superseded = self.list.is_loading();
        self.list = Loadable::Loading;
        self.checked = 0;
        self.skipped.clear();
        self.check = self.check.wrapping_add(1);
    }

    /// Adopts a finished check. The marks and the cursor reset because
    /// the rows they pointed at have moved. The check's own skipped
    /// list stays for the S key, unless the check replaced one still
    /// running: the two checks' events were indistinguishable while
    /// they overlapped, so that progress cannot be trusted and goes.
    pub(crate) fn loaded(&mut self, result: Result<Vec<UpdateInfo>, String>) {
        self.list = match result {
            Ok(updates) => Loadable::Ready(updates),
            Err(error) => Loadable::Failed(error),
        };
        self.marks.clear();
        self.cursor = ListCursor::default();
        if self.superseded {
            self.checked = 0;
            self.skipped.clear();
            self.show_skipped = false;
            self.superseded = false;
        }
    }

    /// Marks or unmarks the row under the cursor, then moves one row down.
    pub(crate) fn toggle_mark(&mut self) {
        let len = self.ready_len();
        if len == 0 {
            return;
        }
        let index = self.cursor.selected;
        if !self.marks.remove(&index) {
            self.marks.insert(index);
        }
        self.cursor.down(1, len);
    }

    /// Marks every row, or clears the marks when every row is marked.
    pub(crate) fn mark_all(&mut self) {
        let len = self.ready_len();
        if len == 0 {
            return;
        }
        if self.marks.len() == len {
            self.marks.clear();
        } else {
            self.marks = (0..len).collect();
        }
    }

    /// The number of rows in the ready list, zero while it loads.
    pub(crate) fn ready_len(&self) -> usize {
        self.list.ready().map_or(0, Vec::len)
    }
}

/// Draws the updates view: its border with the count and the keep-old
/// flag in the title, then the table, the checking and skipped states, or
/// the up-to-date and error states.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    // The skipped-note row leaves one less row for the table, so the
    // window must shrink with it or the bottom row hides below it.
    let note = u16::from(!app.updates.skipped.is_empty());
    let rows_height = usize::from(area.height.saturating_sub(3 + note));
    let window = app
        .updates
        .cursor
        .window(app.updates.ready_len(), rows_height);
    let block = list_block(&app.updates, &app.theme, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let view = &app.updates;
    let theme = &app.theme;
    match &view.list {
        Loadable::Idle => {}
        Loadable::Loading => {
            let text = format!("Checking {}{}", view.checked, theme.sym().ellipsis);
            states::loading(f, inner, theme, app.frame, &text);
        }
        Loadable::Failed(error) => states::error(f, inner, theme, error),
        Loadable::Ready(list) if list.is_empty() => render_up_to_date(f, inner, view, theme),
        Loadable::Ready(_) if view.show_skipped && !view.skipped.is_empty() => {
            render_skipped(f, inner, view, theme);
        }
        Loadable::Ready(list) => {
            let offset = window.start;
            let rows = render_table(f, inner, view, list, window, theme);
            app.layout.list = Some((rows, offset));
        }
    }
}

/// Draws the success state saying nothing needs an update, with the
/// skipped count on a line under it when the check passed any over.
fn render_up_to_date(f: &mut Frame, area: Rect, view: &UpdatesView, theme: &Theme) {
    if view.skipped.is_empty() {
        states::success(f, area, theme, "Everything is up to date");
        return;
    }
    let [state, note] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    states::success(f, state, theme, "Everything is up to date");
    f.render_widget(skipped_note(view, theme), note);
}

/// Draws the checked updates table, with the skipped count on a line
/// under it when the check passed any over, and returns the rect the
/// rows drew into.
fn render_table(
    f: &mut Frame,
    area: Rect,
    view: &UpdatesView,
    list: &[UpdateInfo],
    window: std::ops::Range<usize>,
    theme: &Theme,
) -> Rect {
    let (table_area, note_area) = if view.skipped.is_empty() {
        (area, None)
    } else {
        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
        (top, Some(bottom))
    };
    let constraints = [
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(14),
        Constraint::Length(4),
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Length(10),
    ];
    let name_width = name_column_width(table_area.width, &constraints);
    let table = Table::new(
        table_rows(view, list, window, name_width, theme),
        constraints,
    )
    .header(header_row(theme));
    f.render_widget(table, table_area);
    if let Some(note) = note_area {
        f.render_widget(skipped_note(view, theme), note);
    }
    Rect {
        y: table_area.y.saturating_add(1),
        height: table_area.height.saturating_sub(1),
        ..table_area
    }
}

/// The width the name column gets: everything the fixed columns and the
/// spacing between them leave over.
fn name_column_width(width: u16, constraints: &[Constraint; 7]) -> usize {
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

/// The column labels in muted bold. The arrow column stays unlabeled.
fn header_row(theme: &Theme) -> Row<'static> {
    Row::new(["", "NAME", "CURRENT", "", "NEW", "REPO", "SIZE"])
        .style(theme.muted().add_modifier(Modifier::BOLD))
}

/// Builds the rows the window shows: the mark, the name, the two
/// versions with the arrow between them, the repository, and the size.
fn table_rows(
    view: &UpdatesView,
    list: &[UpdateInfo],
    window: std::ops::Range<usize>,
    name_width: usize,
    theme: &Theme,
) -> Vec<Row<'static>> {
    window
        .map(|index| {
            update_row(
                &list[index],
                view.marks.contains(&index),
                index == view.cursor.selected,
                name_width,
                theme,
            )
        })
        .collect()
}

/// One table row. The cursor row takes the cursor highlight across its
/// whole width and starts with the cursor symbol; a marked row starts with
/// the marked symbol.
fn update_row(
    info: &UpdateInfo,
    marked: bool,
    cursor_here: bool,
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
    let name = format::truncate(&info.pkg_name, name_width, symbols.ellipsis);
    let size = info
        .target
        .package
        .ghcr_size
        .or(info.target.package.size)
        .unwrap_or(0);
    let style = if cursor_here {
        theme.cursor_row()
    } else {
        Style::new()
    };
    Row::new(vec![
        Cell::from(Line::from(vec![lead, Span::raw(" ")])),
        Cell::from(Span::styled(name, theme.text())),
        Cell::from(Span::styled(info.current_version.clone(), theme.danger())),
        Cell::from(
            Line::from(Span::styled(symbols.arrow.to_string(), theme.muted()))
                .alignment(Alignment::Center),
        ),
        Cell::from(Span::styled(info.new_version.clone(), theme.success())),
        Cell::from(Span::styled(info.repo_name.clone(), theme.info())),
        Cell::from(Line::from(Span::raw(format::size(size))).alignment(Alignment::Right)),
    ])
    .style(style)
}

/// Draws the skipped packages as `name  reason` rows, one per line.
fn render_skipped(f: &mut Frame, area: Rect, view: &UpdatesView, theme: &Theme) {
    let rows: Vec<ListItem<'static>> = view
        .skipped
        .iter()
        .map(|(name, reason)| {
            ListItem::new(Line::from(vec![
                Span::styled(name.clone(), theme.text()),
                Span::raw("  "),
                Span::styled(reason.clone(), theme.muted()),
            ]))
        })
        .collect();
    f.render_widget(List::new(rows), area);
}

/// The line telling how many packages the check passed over and that `S`
/// shows them.
fn skipped_note(view: &UpdatesView, theme: &Theme) -> Line<'static> {
    Line::styled(
        format!("  {} skipped  (press S to show)", view.skipped.len()),
        theme.muted(),
    )
}

/// The pane border: the update count and the scoped-selection count on
/// the left of the top edge, and the keep-old flag on its right.
fn list_block(view: &UpdatesView, theme: &Theme, width: u16) -> Block<'static> {
    let left = match (&view.list, &view.scope) {
        (Loadable::Ready(list), Some(scope)) => {
            Line::styled(
                format!(
                    " Updates {} {} {} selected ",
                    list.len(),
                    theme.sym().dot,
                    scope.len()
                ),
                theme.title(),
            )
        }
        (Loadable::Ready(list), None) => {
            Line::styled(format!(" Updates {} ", list.len()), theme.title())
        }
        _ => Line::styled(format!(" {} ", ViewId::Updates.title()), theme.title()),
    };
    let budget = usize::from(width).saturating_sub(2 + left.width());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(left);
    if budget > 0 {
        block.title_top(keep_old_line(view, theme, budget).right_aligned())
    } else {
        block
    }
}

/// The right end of the top border: whether updates keep the old versions
/// on disk, dropped when the title leaves no room for it.
fn keep_old_line(view: &UpdatesView, theme: &Theme, budget: usize) -> Line<'static> {
    let state = if view.keep_old { "on" } else { "off" };
    let mut line = Line::styled(format!(" keep old versions: {state} "), theme.muted());
    if line.width() > budget {
        line.spans.clear();
    }
    line
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use soar_events::{SoarEvent, UpdateCheckStatus};
    use soar_operations::UpdateInfo;

    use crate::{
        app::App,
        effect::Effect,
        msg::{Loaded, Msg},
        test_support::{ctrl, key, screen, test_app, update_info},
    };

    /// A ready list holding the given updates, answering the current
    /// check.
    fn updates(app: &App, list: Vec<UpdateInfo>) -> Msg {
        Msg::Loaded(Loaded::Updates {
            check: app.updates.check,
            result: Ok(list),
        })
    }

    fn failed(app: &App, message: &str) -> Msg {
        Msg::Loaded(Loaded::Updates {
            check: app.updates.check,
            result: Err(message.to_string()),
        })
    }

    /// One `UpdateCheck` event for the named package.
    fn checked(name: &str, status: UpdateCheckStatus) -> Msg {
        Msg::Soar(SoarEvent::UpdateCheck {
            pkg_name: name.to_string(),
            status,
        })
    }

    /// Two checked updates, the usual fixtures, the first one with a
    /// recorded download size.
    fn two_updates() -> Vec<UpdateInfo> {
        let mut list = vec![
            update_info("bat", "0.23.0", "0.24.0", "bincache"),
            update_info("fd", "10.1.0", "10.2.0", "bincache"),
        ];
        list[0].target.package.ghcr_size = Some(5_452_595);
        list
    }

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn the_last_row_stays_visible_alongside_the_skipped_note() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(key('3'), now);
        let many: Vec<UpdateInfo> = (0..30)
            .map(|n| update_info(&format!("pkg{n}"), "1.0.0", "2.0.0", "bincache"))
            .collect();
        app.update(
            checked(
                "pinned",
                UpdateCheckStatus::Skipped {
                    reason: "pinned".to_string(),
                },
            ),
            now,
        );
        app.update(updates(&app, many), now);
        app.update(key('G'), now);
        let open = screen(&mut app, 80, 24);
        assert!(
            open.contains("pkg29"),
            "the selected last row must be on screen, screen is {open:?}"
        );
        assert!(open.contains("skipped"), "the note row is present");
    }

    #[test]
    fn list_keys_do_not_reach_the_table_while_the_skipped_list_shows() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(key('3'), now);
        app.update(
            checked(
                "pinned",
                UpdateCheckStatus::Skipped {
                    reason: "pinned".to_string(),
                },
            ),
            now,
        );
        app.update(updates(&app, two_updates()), now);
        app.update(key('S'), now);
        app.update(key('j'), now);
        app.update(key(' '), now);
        assert_eq!(app.updates.cursor.selected, 0, "the hidden table stays put");
        assert!(
            app.updates.marks.is_empty(),
            "marks cannot land on hidden rows"
        );
        app.update(key('u'), now);
        assert!(
            app.overlay.is_none(),
            "an update cannot be driven from off screen"
        );
    }

    #[test]
    fn entering_updates_the_first_time_starts_a_check() {
        let mut app = test_app();
        let effects = app.update(key('3'), now());
        let [
            Effect::CheckUpdates {
                names: scope, ..
            },
        ] = &effects[..]
        else {
            panic!("expected one check on first entry");
        };
        assert_eq!(scope, &None);
        assert!(app.updates.list.is_loading());
        assert_eq!(app.updates.checked, 0);
        assert!(app.updates.skipped.is_empty());
    }

    #[test]
    fn reentering_updates_starts_nothing() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(key('1'), now());
        let effects = app.update(key('3'), now());
        assert!(effects.is_empty());
    }

    #[test]
    fn three_check_events_raise_the_count_to_three() {
        let mut app = test_app();
        app.update(key('3'), now());
        for _ in 0..3 {
            app.update(
                checked(
                    "bat",
                    UpdateCheckStatus::UpToDate {
                        version: "0.24.0".to_string(),
                    },
                ),
                now(),
            );
        }
        assert_eq!(app.updates.checked, 3);
    }

    #[test]
    fn check_events_stop_counting_once_the_result_arrived() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, Vec::new()), now());
        app.update(
            checked(
                "bat",
                UpdateCheckStatus::UpToDate {
                    version: "0.24.0".to_string(),
                },
            ),
            now(),
        );
        assert_eq!(app.updates.checked, 0);
    }

    #[test]
    fn a_skipped_event_records_the_reason() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(
            checked(
                "yq",
                UpdateCheckStatus::Skipped {
                    reason: "pinned".to_string(),
                },
            ),
            now(),
        );
        assert_eq!(
            app.updates.skipped,
            vec![("yq".to_string(), "pinned".to_string())]
        );
    }

    #[test]
    fn a_finished_check_clears_the_marks_and_the_cursor() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        app.update(key(' '), now());
        app.update(key('G'), now());
        assert!(!app.updates.marks.is_empty());
        app.update(updates(&app, two_updates()), now());
        assert!(app.updates.marks.is_empty());
        assert_eq!(app.updates.cursor.selected, 0);
    }

    #[test]
    fn a_load_that_replaced_a_running_check_drops_its_progress() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(
            checked(
                "pinned",
                UpdateCheckStatus::Skipped {
                    reason: "pinned".to_string(),
                },
            ),
            now(),
        );
        // The reload replaces a check still in flight, whose events keep
        // landing in the live counters with no number to tell them by.
        app.update(ctrl('r'), now());
        app.update(
            checked(
                "pinned",
                UpdateCheckStatus::Skipped {
                    reason: "pinned".to_string(),
                },
            ),
            now(),
        );
        assert_eq!(app.updates.checked, 1);
        assert!(!app.updates.skipped.is_empty());
        app.updates.show_skipped = true;
        app.update(updates(&app, two_updates()), now());
        assert_eq!(app.updates.checked, 0);
        assert!(app.updates.skipped.is_empty());
        assert!(!app.updates.show_skipped);
    }

    #[test]
    fn a_failed_check_shows_the_error_state() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(failed(&app, "metadata missing"), now());
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("metadata missing"), "screen is {screen:?}");
        assert!(screen.contains("ctrl-r to retry"), "screen is {screen:?}");
    }

    #[test]
    fn ctrl_r_checks_again_with_the_current_scope() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        app.updates.scope = Some(vec!["bat".to_string()]);
        let effects = app.update(ctrl('r'), now());
        let [
            Effect::CheckUpdates {
                names, ..
            },
        ] = &effects[..]
        else {
            panic!("expected one check on reload");
        };
        assert_eq!(names, &Some(vec!["bat".to_string()]));
        assert!(app.updates.list.is_loading());
    }

    #[test]
    fn the_badge_counts_the_ready_updates() {
        let mut app = test_app();
        assert_eq!(app.updates_badge(), None);
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        assert_eq!(app.updates_badge(), Some(2));
    }

    #[test]
    fn space_marks_the_row_under_the_cursor_by_index() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        app.update(key(' '), now());
        assert_eq!(app.updates.marks, [0].into_iter().collect());
        assert_eq!(app.updates.cursor.selected, 1);
    }

    #[test]
    fn mark_all_marks_every_row_and_a_second_press_clears_them() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        app.update(key('a'), now());
        assert_eq!(app.updates.marks, [0, 1].into_iter().collect());
        app.update(key('a'), now());
        assert!(app.updates.marks.is_empty());
    }

    #[test]
    fn o_toggles_the_keep_old_flag_in_the_border() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        assert!(screen(&mut app, 100, 24).contains("keep old versions: off"));
        app.update(key('o'), now());
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("keep old versions: on"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn two_updates_render_both_new_versions_and_the_arrow() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("0.24.0"), "screen is {screen:?}");
        assert!(screen.contains("10.2.0"), "screen is {screen:?}");
        assert!(
            screen.contains(app.theme.sym().arrow),
            "screen is {screen:?}"
        );
        assert!(screen.contains("Updates 2"), "screen is {screen:?}");
        assert!(screen.contains("5.2 MiB"), "screen is {screen:?}");
    }

    #[test]
    fn the_table_names_its_columns() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        let screen = screen(&mut app, 120, 24);
        for label in ["NAME", "CURRENT", "NEW", "REPO", "SIZE"] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn the_title_shows_the_scoped_selection_count() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, two_updates()), now());
        app.updates.scope = Some(vec!["bat".to_string(), "fd".to_string()]);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("2 selected"), "screen is {screen:?}");
    }

    #[test]
    fn an_empty_check_shows_the_up_to_date_state() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(updates(&app, Vec::new()), now());
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Everything is up to date"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_loading_state_shows_the_running_count() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(
            checked(
                "bat",
                UpdateCheckStatus::UpToDate {
                    version: "0.24.0".to_string(),
                },
            ),
            now(),
        );
        app.update(
            checked(
                "fd",
                UpdateCheckStatus::Available {
                    current_version: "10.1.0".to_string(),
                    new_version: "10.2.0".to_string(),
                },
            ),
            now(),
        );
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("Checking 2"), "screen is {screen:?}");
        assert!(!screen.contains("0.24.0"), "screen is {screen:?}");
    }

    #[test]
    fn the_skipped_count_sits_under_the_table_until_s_shows_the_list() {
        let mut app = test_app();
        app.update(key('3'), now());
        app.update(
            checked(
                "pinned-pkg",
                UpdateCheckStatus::Skipped {
                    reason: "held back".to_string(),
                },
            ),
            now(),
        );
        let mut list = two_updates();
        list.push(update_info("yq", "4.44.2", "4.44.3", "bincache"));
        app.update(updates(&app, list), now());
        let rendered = screen(&mut app, 100, 24);
        assert!(
            rendered.contains("1 skipped  (press S to show)"),
            "screen is {rendered:?}"
        );
        assert!(!rendered.contains("held back"), "screen is {rendered:?}");
        app.update(key('S'), now());
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("pinned-pkg"), "screen is {rendered:?}");
        assert!(rendered.contains("held back"), "screen is {rendered:?}");
        app.update(key('S'), now());
        let rendered = screen(&mut app, 100, 24);
        assert!(!rendered.contains("held back"), "screen is {rendered:?}");
    }

    #[test]
    fn more_rows_than_screen_lines_render_only_the_visible_ones() {
        let mut app = test_app();
        app.update(key('3'), now());
        let list: Vec<_> = (0..50)
            .map(|n| update_info(&format!("pkg{n:03}"), "1.0.0", "1.1.0", "bincache"))
            .collect();
        app.update(updates(&app, list), now());
        for _ in 0..40 {
            app.update(key('j'), now());
        }
        let screen = screen(&mut app, 100, 30);
        assert!(screen.contains("pkg040"), "screen is {screen:?}");
        assert!(!screen.contains("pkg000"), "screen is {screen:?}");
        assert!(!screen.contains("pkg049"), "screen is {screen:?}");
    }
}
