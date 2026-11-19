//! The health view: whether PATH, MANPATH, packages, and symlinks are in
//! order, and where soar keeps its files.

use std::{ops::Range, path::PathBuf};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use soar_operations::{BrokenPackage, HealthReport};

use crate::{
    action::Action,
    app::App,
    format, keymap,
    loadable::Loadable,
    theme::Theme,
    views::ViewId,
    widgets::{scroll::ListCursor, states},
};

/// The width of a check's label column.
const LABEL_WIDTH: usize = 10;

/// The width of a path's label column.
const PATH_LABEL_WIDTH: usize = 16;

/// How far the check rows and the rows under them are indented: the
/// symbol, its space, the label, and its space.
const INDENT_WIDTH: usize = LABEL_WIDTH + 3;

/// How many broken symlink paths show before the rest fold into a count.
const VISIBLE_SYMLINKS: usize = 5;

/// The narrowest terminal that shows the checks beside the paths.
const SIDE_BY_SIDE_WIDTH: u16 = 100;

/// The state of the health view.
#[derive(Default)]
pub(crate) struct HealthView {
    /// The health report and the paths it names, loaded on the first visit
    /// and again after every job that changes what the checks report on.
    pub(crate) report: Loadable<(HealthReport, Vec<(String, PathBuf)>)>,
    /// The row of the paths pane the cursor sits on.
    pub(crate) cursor: ListCursor,
}

impl HealthView {
    /// Starts a fresh load. The last report goes when the new one arrives.
    pub(crate) fn start(&mut self) {
        self.report = Loadable::Loading;
    }

    /// Adopts a finished load. The cursor clamps to the paths list.
    pub(crate) fn loaded(
        &mut self,
        result: Result<(HealthReport, Vec<(String, PathBuf)>), String>,
    ) {
        self.report = match result {
            Ok(data) => Loadable::Ready(data),
            Err(error) => Loadable::Failed(error),
        };
        self.cursor.clamp(self.paths_len());
    }

    /// The number of rows in the paths list, zero while it loads.
    pub(crate) fn paths_len(&self) -> usize {
        self.report.ready().map_or(0, |(_, paths)| paths.len())
    }
}

/// Draws the health view: the checks and the paths side by side, stacked
/// under the width where they stop fitting beside each other.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let (health_area, paths_area) = if area.width >= SIDE_BY_SIDE_WIDTH {
        let [health, paths] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);
        (health, paths)
    } else {
        let [health, paths] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
        (health, paths)
    };
    let rows_height = usize::from(paths_area.height.saturating_sub(2));
    let window = app
        .health
        .cursor
        .window(app.health.paths_len(), rows_height);
    render_checks(f, health_area, app);
    render_paths(f, paths_area, app, window);
}

/// Draws the checks pane: its border, then the rows the report describes,
/// or the loading and error states while the report is not there.
fn render_checks(f: &mut Frame, area: Rect, app: &App) {
    let block = pane_block(ViewId::Health.title(), &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let theme = &app.theme;
    let Some((report, _)) = app.health.report.ready() else {
        match &app.health.report {
            Loadable::Loading => states::loading(f, inner, theme, app.frame, "Checking health"),
            Loadable::Failed(error) => states::error(f, inner, theme, error),
            Loadable::Idle | Loadable::Ready(_) => {}
        }
        return;
    };
    let width = usize::from(inner.width);
    f.render_widget(Paragraph::new(check_lines(report, width, theme)), inner);
}

/// Draws the paths pane: its border, then the window of paths the cursor
/// points into.
fn render_paths(f: &mut Frame, area: Rect, app: &mut App, window: Range<usize>) {
    let block = pane_block("Paths", &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some((_, paths)) = app.health.report.ready() else {
        return;
    };
    let width = usize::from(inner.width);
    let offset = window.start;
    let lines = path_lines(paths, window, width, &app.theme);
    app.layout.list = Some((inner, offset));
    f.render_widget(Paragraph::new(lines), inner);
}

/// A rounded, bordered pane titled with the given name.
fn pane_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title.to_string(), theme.title()),
            Span::raw(" "),
        ]))
}

/// The check rows for one report, plus the fixes line for what is broken.
fn check_lines(report: &HealthReport, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let symbols = theme.sym();
    let mut lines = Vec::new();
    let bin = format::home_relative(&report.bin_path);
    if report.path_configured {
        lines.push(check_line(
            symbols.installed,
            theme.success(),
            "PATH",
            &format!("{bin} is in PATH"),
            width,
            theme,
        ));
    } else {
        lines.push(check_line(
            symbols.warning,
            theme.warning(),
            "PATH",
            &format!("{bin} is not in PATH"),
            width,
            theme,
        ));
        lines.push(path_hint_line(&bin, width, theme));
    }
    if let Some(man_path) = &report.man_path {
        let man = format::home_relative(man_path);
        if report.man_path_configured {
            lines.push(check_line(
                symbols.installed,
                theme.success(),
                "MANPATH",
                "configured",
                width,
                theme,
            ));
        } else {
            lines.push(check_line(
                symbols.warning,
                theme.warning(),
                "MANPATH",
                &format!("{man} is not searched by man"),
                width,
                theme,
            ));
        }
    }
    let broken = &report.broken_packages;
    if broken.is_empty() {
        lines.push(check_line(
            symbols.installed,
            theme.success(),
            "Packages",
            "none broken",
            width,
            theme,
        ));
    } else {
        let count = broken.len();
        lines.push(check_line(
            symbols.error,
            theme.danger(),
            "Packages",
            &format!("{count} broken"),
            width,
            theme,
        ));
        for package in broken {
            lines.push(broken_package_line(package, width, theme));
        }
    }
    let symlinks = &report.broken_symlinks;
    if symlinks.is_empty() {
        lines.push(check_line(
            symbols.installed,
            theme.success(),
            "Symlinks",
            "none broken",
            width,
            theme,
        ));
    } else {
        let count = symlinks.len();
        lines.push(check_line(
            symbols.error,
            theme.danger(),
            "Symlinks",
            &format!("{count} broken"),
            width,
            theme,
        ));
        for path in symlinks.iter().take(VISIBLE_SYMLINKS) {
            lines.push(indented_line(&path.display().to_string(), width, theme));
        }
        let hidden = count.saturating_sub(VISIBLE_SYMLINKS);
        if hidden > 0 {
            lines.push(indented_line(&format!("+{hidden} more"), width, theme));
        }
    }
    if let Some(line) = fixes_line(report, theme) {
        lines.push(line);
    }
    lines
}

/// One check row: the status symbol, the bold label, and the message,
/// each cut so the row never overflows its pane.
fn check_line(
    symbol: &str,
    style: Style,
    label: &str,
    message: &str,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let message_width = width.saturating_sub(INDENT_WIDTH);
    Line::from(vec![
        Span::styled(symbol.to_string(), style),
        Span::raw(" "),
        Span::styled(
            format!("{:<LW$}", label, LW = LABEL_WIDTH),
            theme.text().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format::truncate(message, message_width, theme.sym().ellipsis),
            style,
        ),
    ])
}

/// The shell line that adds the bin directory to PATH, shown in muted
/// under a failing PATH check.
fn path_hint_line(bin: &str, width: usize, theme: &Theme) -> Line<'static> {
    let hint = format!("add to your shell: export PATH=\"{bin}:$PATH\"");
    let hint = format::truncate(
        &hint,
        width.saturating_sub(INDENT_WIDTH),
        theme.sym().ellipsis,
    );
    Line::styled(format!("{}{hint}", " ".repeat(INDENT_WIDTH)), theme.muted())
}

/// One broken package under its count: `name  path`, indented to the
/// message column, the path cut in the middle.
fn broken_package_line(package: &BrokenPackage, width: usize, theme: &Theme) -> Line<'static> {
    let room = width.saturating_sub(INDENT_WIDTH);
    let name = format::truncate(
        &package.pkg_name,
        room.saturating_sub(2),
        theme.sym().ellipsis,
    );
    let path_room = room
        .saturating_sub(Span::raw(name.as_str()).width())
        .saturating_sub(2);
    let path = format::truncate_middle(&package.installed_path, path_room, theme.sym().ellipsis);
    Line::from(vec![
        Span::raw(" ".repeat(INDENT_WIDTH)),
        Span::styled(name, theme.text()),
        Span::raw("  "),
        Span::styled(path, theme.muted()),
    ])
}

/// One indented muted line: a broken symlink path or the count of the
/// paths that folded away.
fn indented_line(text: &str, width: usize, theme: &Theme) -> Line<'static> {
    let text = format::truncate(
        text,
        width.saturating_sub(INDENT_WIDTH),
        theme.sym().ellipsis,
    );
    Line::from(vec![
        Span::raw(" ".repeat(INDENT_WIDTH)),
        Span::styled(text, theme.muted()),
    ])
}

/// The fixes worth running for what is broken, as `key label` pairs taken
/// from the health key table. Nothing broken leaves the line out.
fn fixes_line(report: &HealthReport, theme: &Theme) -> Option<Line<'static>> {
    let broken_packages = !report.broken_packages.is_empty();
    let broken_symlinks = !report.broken_symlinks.is_empty();
    let mut spans: Vec<Span<'static>> = Vec::new();
    for binding in keymap::bindings(ViewId::Health) {
        let label = match binding.action {
            Action::FixSymlinks if broken_symlinks => binding.label.to_string(),
            Action::RemoveBroken if broken_packages => format!("{} packages", binding.label),
            _ => continue,
        };
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            keymap::key_label(binding.keys[0]),
            theme.key(),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label, theme.muted()));
    }
    if spans.is_empty() {
        None
    } else {
        Some(Line::from(spans))
    }
}

/// The window of paths the pane shows, each label padded to its column
/// and the path shown relative to the home directory.
fn path_lines(
    paths: &[(String, PathBuf)],
    window: Range<usize>,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    window
        .map(|index| {
            let (label, path) = &paths[index];
            let value = format::home_relative(path);
            Line::from(vec![
                Span::styled(
                    format!("{:<PLW$}", label, PLW = PATH_LABEL_WIDTH),
                    theme.muted(),
                ),
                Span::styled(
                    format::truncate(
                        &value,
                        width.saturating_sub(PATH_LABEL_WIDTH),
                        theme.sym().ellipsis,
                    ),
                    theme.text(),
                ),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Instant};

    use soar_operations::{BrokenPackage, HealthReport};

    use crate::{
        effect::Effect,
        jobs::JobSpec,
        msg::{Loaded, Msg},
        test_support::{ctrl, health_paths, health_report, key, screen, test_app},
        views::ViewId,
    };

    /// A session on the health view with the given report loaded.
    fn health_app(report: HealthReport) -> (crate::app::App, Instant) {
        let mut app = test_app();
        app.view = ViewId::Health;
        app.update(loaded(report), Instant::now());
        (app, Instant::now())
    }

    /// A finished health load for the given report.
    fn loaded(report: HealthReport) -> Msg {
        Msg::Loaded(Loaded::Health(Ok((report, health_paths()))))
    }

    /// One broken package with the given name and install path.
    fn broken(name: &str, path: &str) -> BrokenPackage {
        BrokenPackage {
            pkg_name: name.to_string(),
            installed_path: path.to_string(),
        }
    }

    #[test]
    fn entering_health_the_first_time_loads_the_report() {
        let mut app = test_app();
        let effects = app.update(key('6'), Instant::now());
        let [Effect::LoadHealth] = &effects[..] else {
            panic!("expected one load on first entry");
        };
        assert!(app.health.report.is_loading());
    }

    #[test]
    fn reentering_health_starts_nothing() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(key('6'), now);
        app.update(loaded(health_report()), now);
        app.update(key('1'), now);
        let effects = app.update(key('6'), now);
        assert!(effects.is_empty());
        assert!(app.health.report.ready().is_some());
    }

    #[test]
    fn ctrl_r_reloads_the_report() {
        let (mut app, now) = health_app(health_report());
        let effects = app.update(ctrl('r'), now);
        let [Effect::LoadHealth] = &effects[..] else {
            panic!("expected one load on reload");
        };
        assert!(app.health.report.is_loading());
    }

    #[test]
    fn a_path_not_in_path_shows_the_export_hint() {
        let mut report = health_report();
        report.path_configured = false;
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("is not in PATH"), "screen is {screen:?}");
        assert!(screen.contains("export PATH="), "screen is {screen:?}");
    }

    #[test]
    fn a_configured_path_shows_no_hint() {
        let (mut app, _) = health_app(health_report());
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("is in PATH"), "screen is {screen:?}");
        assert!(!screen.contains("export PATH="), "screen is {screen:?}");
    }

    #[test]
    fn the_manpath_row_shows_only_with_a_man_path() {
        let (mut app, _) = health_app(health_report());
        let without = screen(&mut app, 100, 24);
        assert!(!without.contains("MANPATH"), "screen is {without:?}");

        let mut report = health_report();
        report.man_path = Some("/home/user/.local/share/soar/bin/share/man".into());
        let (mut app, _) = health_app(report);
        let with = screen(&mut app, 100, 24);
        assert!(with.contains("MANPATH"), "screen is {with:?}");
        assert!(with.contains("configured"), "screen is {with:?}");
    }

    #[test]
    fn an_unsearched_man_path_warns() {
        let mut report = health_report();
        report.man_path = Some("/home/user/man".into());
        report.man_path_configured = false;
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("is not searched by man"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn broken_packages_are_named_under_their_count() {
        let mut report = health_report();
        report.broken_packages = vec![
            broken("obsidian", "/home/user/.local/share/soar/packages/obsidian"),
            broken("zellij", "/home/user/.local/share/soar/packages/zellij"),
        ];
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("2 broken"), "screen is {screen:?}");
        assert!(screen.contains("obsidian"), "screen is {screen:?}");
        assert!(screen.contains("zellij"), "screen is {screen:?}");
    }

    #[test]
    fn a_long_broken_package_path_is_cut_in_the_middle() {
        let mut report = health_report();
        let long = "/home/user/.local/share/soar/packages/delta-shell-0.4.1-full-distribution";
        report.broken_packages = vec![broken("delta-shell", long)];
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("delta-shell"), "screen is {screen:?}");
        assert!(screen.contains('…'), "screen is {screen:?}");
        assert!(!screen.contains(long), "screen is {screen:?}");
    }

    #[test]
    fn broken_symlinks_fold_after_five_paths() {
        let mut report = health_report();
        report.broken_symlinks = (1..=7)
            .map(|n| Path::new("/home/user/.local/bin").join(format!("tool{n}")))
            .collect();
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("7 broken"), "screen is {screen:?}");
        assert!(screen.contains("tool5"), "screen is {screen:?}");
        assert!(screen.contains("+2 more"), "screen is {screen:?}");
        assert!(!screen.contains("tool6"), "screen is {screen:?}");
    }

    #[test]
    fn broken_symlinks_offer_their_fix() {
        let mut report = health_report();
        report.broken_symlinks = vec![Path::new("/home/user/.local/bin/tool").into()];
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("b fix symlinks"), "screen is {screen:?}");
    }

    #[test]
    fn broken_packages_offer_their_fix() {
        let mut report = health_report();
        report.broken_packages = vec![broken("obsidian", "/gone/obsidian")];
        let (mut app, _) = health_app(report);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("p remove broken packages"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_clean_report_lists_no_fixes() {
        let (mut app, _) = health_app(health_report());
        let screen = screen(&mut app, 100, 24);
        assert!(!screen.contains("broken packages"), "screen is {screen:?}");
    }

    #[test]
    fn the_paths_block_shows_a_home_relative_value() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        if home.is_empty() {
            return;
        }
        let report = health_report();
        let mut paths = health_paths();
        paths[5] = ("Packages".to_string(), Path::new(&home).join(".local/bin"));
        let mut app = test_app();
        app.view = ViewId::Health;
        app.update(
            Msg::Loaded(Loaded::Health(Ok((report, paths)))),
            Instant::now(),
        );
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("~/.local/bin"), "screen is {screen:?}");
    }

    #[test]
    fn the_paths_block_names_every_path() {
        let (mut app, _) = health_app(health_report());
        let screen = screen(&mut app, 120, 24);
        for label in [
            "Config",
            "Packages config",
            "Bin",
            "Database",
            "Cache",
            "Packages",
            "Repositories",
        ] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_failed_load_shows_the_error_state() {
        let mut app = test_app();
        app.view = ViewId::Health;
        app.update(
            Msg::Loaded(Loaded::Health(Err("database locked".to_string()))),
            Instant::now(),
        );
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("database locked"), "screen is {screen:?}");
        assert!(screen.contains("ctrl-r to retry"), "screen is {screen:?}");
    }

    #[test]
    fn j_moves_the_paths_cursor() {
        let (mut app, now) = health_app(health_report());
        app.update(key('j'), now);
        app.update(key('j'), now);
        assert_eq!(app.health.cursor.selected, 2);
    }

    #[test]
    fn a_finished_fix_reloads_the_report() {
        let (mut app, now) = health_app(health_report());
        app.enqueue(JobSpec::CleanCache, now);
        let effects = app.update(
            Msg::JobFinished {
                id: 0,
                outcome: crate::jobs::JobOutcome {
                    ok: true,
                    aborted: false,
                    headline: "Cache cleaned".to_string(),
                    lines: Vec::new(),
                },
            },
            now,
        );
        assert!(matches!(effects.as_slice(), [Effect::LoadHealth]));
        assert!(app.health.report.is_loading());
    }

    #[test]
    fn stacked_layouts_at_a_narrow_width() {
        let (mut app, _) = health_app(health_report());
        let screen = screen(&mut app, 80, 30);
        assert!(screen.contains("Paths"), "screen is {screen:?}");
        assert!(screen.contains("is in PATH"), "screen is {screen:?}");
    }
}
