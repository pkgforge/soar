//! The apply view: what applying `packages.toml` would change, and what
//! it would leave alone.

use std::path::PathBuf;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use soar_operations::ApplyDiff;

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    widgets::{scroll::ListCursor, states},
};

/// How far the rows under a section header are indented.
const INDENT_WIDTH: usize = 4;

/// What applying the packages file would do, and what it leaves alone.
pub(crate) struct ApplyPlan {
    /// The packages file the plan was computed from.
    pub(crate) path: PathBuf,
    /// The changes applying would make, or `None` when the file does not
    /// exist.
    pub(crate) diff: Option<ApplyDiff>,
    /// Names of `system = true` packages left for the system pass.
    /// Always empty in system mode.
    pub(crate) system: Vec<String>,
}

/// The state of the apply view.
#[derive(Default)]
pub(crate) struct ApplyView {
    /// The computed plan, loaded on the first visit and again after every
    /// change to the file, the installation, or the prune flag.
    pub(crate) plan: Loadable<ApplyPlan>,
    /// Whether applying also removes packages the file does not declare.
    pub(crate) prune: bool,
    /// The number of the computation in flight. Every start numbers
    /// itself so a result from a computation a newer one replaced can be
    /// dropped, whatever it found.
    pub(crate) check: u32,
    /// The body line the cursor sits on.
    pub(crate) cursor: ListCursor,
    /// How many lines the body drew last frame, so the cursor knows its
    /// ends between renders.
    pub(crate) body_rows: usize,
}

impl ApplyView {
    /// Starts a fresh computation. The last plan goes when the new one
    /// arrives, and the start numbers itself so a result from a
    /// computation a newer one replaced is dropped.
    pub(crate) fn start(&mut self) {
        self.check = self.check.wrapping_add(1);
        self.plan = Loadable::Loading;
    }

    /// Adopts a finished computation. The cursor returns to the top
    /// because the body it pointed into has been rebuilt.
    pub(crate) fn loaded(&mut self, result: Result<ApplyPlan, String>) {
        self.plan = match result {
            Ok(plan) => Loadable::Ready(plan),
            Err(error) => Loadable::Failed(error),
        };
        self.cursor = ListCursor::default();
    }
}

/// Draws the apply view: its border with the file's path and the prune
/// flag in the title, then the plan body, or the loading, missing-file,
/// and error states.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let block = apply_block(&app.apply, theme, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let height = usize::from(inner.height);
    match &app.apply.plan {
        Loadable::Idle => {}
        Loadable::Loading => {
            states::loading(f, inner, theme, app.frame, "Computing changes");
        }
        Loadable::Failed(error) => states::error(f, inner, theme, error),
        Loadable::Ready(plan) => {
            match &plan.diff {
                None => {
                    let text = format!(
                        "No packages.toml at {}. Press n to create one.",
                        format::home_relative(&plan.path)
                    );
                    states::empty(f, inner, theme, &text);
                }
                Some(diff) => {
                    let lines = plan_lines(plan, diff, usize::from(inner.width), theme);
                    app.apply.body_rows = lines.len();
                    app.apply.cursor.clamp(lines.len());
                    let window = app.apply.cursor.window(lines.len(), height);
                    let visible = lines[window.clone()].to_vec();
                    app.layout.list = Some((inner, window.start));
                    f.render_widget(Paragraph::new(visible), inner);
                }
            }
        }
    }
}

/// The pane border: the file name in bold with its home-relative path
/// beside it, and the prune flag on the right of the top edge.
fn apply_block(view: &ApplyView, theme: &Theme, width: u16) -> Block<'static> {
    let mut spans = vec![Span::raw(" "), Span::styled("packages.toml", theme.title())];
    if let Some(plan) = view.plan.ready() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format::home_relative(&plan.path),
            theme.muted(),
        ));
    }
    let left = Line::from(spans);
    let budget = usize::from(width).saturating_sub(2 + left.width());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(left);
    if budget > 0 {
        block.title_top(prune_line(view.prune, theme, budget).right_aligned())
    } else {
        block
    }
}

/// The right end of the top border: whether applying also prunes, dropped
/// when the title leaves no room for it.
fn prune_line(prune: bool, theme: &Theme, budget: usize) -> Line<'static> {
    let state = if prune { "on" } else { "off" };
    let mut line = Line::styled(format!(" prune: {state} "), theme.muted());
    if line.width() > budget {
        line.spans.clear();
    }
    line
}

/// The body lines for one computed plan, in section order. Install,
/// update, and remove always show their count so a zero is visible; the
/// other sections fold away when they have nothing to list.
fn plan_lines(
    plan: &ApplyPlan,
    diff: &ApplyDiff,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let symbols = theme.sym();
    let mut lines = Vec::new();
    section_header(
        &mut lines,
        "+",
        theme.success(),
        "Install",
        diff.to_install.len(),
    );
    for (_resolved, target) in &diff.to_install {
        let package = &target.package;
        lines.push(row_line(
            name_version_repo_row(
                &package.pkg_name,
                &package.version,
                &package.repo_name,
                theme,
            ),
            width,
            theme,
        ));
    }
    section_header(
        &mut lines,
        symbols.update,
        theme.warning(),
        "Update",
        diff.to_update.len(),
    );
    for (_resolved, target) in &diff.to_update {
        let old = target
            .existing_install
            .as_ref()
            .map_or_else(|| "?".to_string(), |installed| installed.version.clone());
        let package = &target.package;
        lines.push(row_line(
            vec![
                Span::styled(package.pkg_name.clone(), theme.text()),
                Span::raw("  "),
                Span::styled(old, theme.danger()),
                Span::raw(" "),
                Span::styled(symbols.arrow, theme.muted()),
                Span::raw(" "),
                Span::styled(package.version.clone(), theme.success()),
            ],
            width,
            theme,
        ));
    }
    section_header(
        &mut lines,
        symbols.remove,
        theme.danger(),
        "Remove",
        diff.to_remove.len(),
    );
    for package in &diff.to_remove {
        lines.push(row_line(
            name_version_repo_row(
                &package.pkg_name,
                &package.version,
                &package.repo_name,
                theme,
            ),
            width,
            theme,
        ));
    }
    if !diff.not_found.is_empty() {
        section_header(
            &mut lines,
            symbols.error,
            theme.danger(),
            "Not found",
            diff.not_found.len(),
        );
        for name in &diff.not_found {
            lines.push(row_line(
                vec![Span::styled(name.clone(), theme.warning())],
                width,
                theme,
            ));
        }
    }
    if !diff.has_changes() && !diff.has_toml_updates() {
        lines.push(Line::styled(
            format!("{} Everything matches packages.toml", symbols.installed),
            theme.success(),
        ));
    }
    if !diff.in_sync.is_empty() {
        section_header(
            &mut lines,
            symbols.installed,
            theme.muted(),
            "In sync",
            diff.in_sync.len(),
        );
    }
    if !diff.pending_version_updates.is_empty() {
        section_header(
            &mut lines,
            symbols.pinned,
            theme.info(),
            "Version pins",
            diff.pending_version_updates.len(),
        );
        for (name, version) in &diff.pending_version_updates {
            lines.push(row_line(
                vec![
                    Span::styled(name.clone(), theme.text()),
                    Span::raw("  "),
                    Span::styled(version.clone(), theme.accent2()),
                ],
                width,
                theme,
            ));
        }
        lines.push(row_line(
            vec![Span::styled("written back to packages.toml", theme.muted())],
            width,
            theme,
        ));
    }
    if !plan.system.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} System {}", symbols.dot, plan.system.len()),
                theme.warning().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("(asks for administrator rights)", theme.muted()),
        ]));
        let room = width.saturating_sub(INDENT_WIDTH);
        let names = plan.system.join(" ");
        for row in format::wrap(&names, room) {
            lines.push(row_line(
                vec![Span::styled(row, theme.text())],
                width,
                theme,
            ));
        }
    }
    lines
}

/// One bold section header in its own color: `{symbol} {label} {count}`.
fn section_header(
    lines: &mut Vec<Line<'static>>,
    symbol: &str,
    style: Style,
    label: &str,
    count: usize,
) {
    lines.push(Line::styled(
        format!("{symbol} {label} {count}"),
        style.add_modifier(Modifier::BOLD),
    ));
}

/// The `name  version  repo` spans shared by the install and remove rows.
fn name_version_repo_row(
    name: &str,
    version: &str,
    repo: &str,
    theme: &Theme,
) -> Vec<Span<'static>> {
    vec![
        Span::styled(name.to_string(), theme.text()),
        Span::raw("  "),
        Span::styled(version.to_string(), theme.accent2()),
        Span::raw("  "),
        Span::styled(repo.to_string(), theme.info()),
    ]
}

/// One indented row, cut to the pane width when its spans run past it.
fn row_line(spans: Vec<Span<'static>>, width: usize, theme: &Theme) -> Line<'static> {
    let mut all = vec![Span::raw(" ".repeat(INDENT_WIDTH))];
    all.extend(spans);
    let total: usize = all.iter().map(|span| span.width()).sum();
    if total <= width {
        return Line::from(all);
    }
    let text = all
        .iter()
        .map(|span| span.content.to_string())
        .collect::<String>();
    Line::styled(
        format::truncate(&text, width, theme.sym().ellipsis),
        theme.text(),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::Instant,
    };

    use soar_operations::ApplyDiff;

    use super::{ApplyPlan, ApplyView};
    use crate::{
        effect::Effect,
        msg::{Loaded, Msg},
        test_support::{apply_plan, ctrl, install_entry, key, screen, test_app},
        views::ViewId,
    };

    /// The apply view with the given plan loaded.
    fn apply_app(plan: ApplyPlan) -> (crate::app::App, Instant) {
        let mut app = test_app();
        app.view = ViewId::Apply;
        app.update(loaded(plan), Instant::now());
        (app, Instant::now())
    }

    /// A finished computation for the given plan, answering the check the
    /// view still expects before any recompute has started.
    fn loaded(plan: ApplyPlan) -> Msg {
        Msg::Loaded(Loaded::ApplyDiff {
            check: 0,
            result: Ok(plan),
        })
    }

    #[test]
    fn entering_apply_the_first_time_computes_the_plan() {
        let mut app = test_app();
        let effects = app.update(key('4'), Instant::now());
        let [
            Effect::ComputeApplyDiff {
                prune, ..
            },
        ] = &effects[..]
        else {
            panic!("expected one computation on first entry");
        };
        assert!(!*prune);
        assert!(app.apply.plan.is_loading());
    }

    #[test]
    fn reentering_apply_starts_nothing() {
        let mut app = test_app();
        let now = Instant::now();
        app.update(key('4'), now);
        app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: app.apply.check,
                result: Ok(apply_plan()),
            }),
            now,
        );
        app.update(key('1'), now);
        let effects = app.update(key('4'), now);
        assert!(effects.is_empty());
        assert!(app.apply.plan.ready().is_some());
    }

    #[test]
    fn ctrl_r_recomputes_with_the_current_prune() {
        let (mut app, now) = apply_app(apply_plan());
        app.apply.prune = true;
        let effects = app.update(ctrl('r'), now);
        let [
            Effect::ComputeApplyDiff {
                prune, ..
            },
        ] = &effects[..]
        else {
            panic!("expected one computation on reload");
        };
        assert!(*prune);
        assert!(app.apply.plan.is_loading());
    }

    #[test]
    fn the_sections_show_their_counts() {
        let (mut app, _) = apply_app(apply_plan());
        let screen = screen(&mut app, 100, 24);
        for label in ["Install 1", "Update 1", "Remove 0", "In sync 2"] {
            assert!(screen.contains(label), "screen is {screen:?}");
        }
    }

    #[test]
    fn rows_show_the_packages_the_plan_touches() {
        let (mut app, _) = apply_app(apply_plan());
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("bat"), "screen is {screen:?}");
        assert!(screen.contains("0.24.0"), "screen is {screen:?}");
        assert!(screen.contains("bincache"), "screen is {screen:?}");
        assert!(screen.contains("4.44.2"), "screen is {screen:?}");
        assert!(screen.contains("4.44.3"), "screen is {screen:?}");
        assert!(
            screen.contains(app.theme.sym().arrow),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn nothing_to_do_says_so_above_the_in_sync_count() {
        let mut settled = apply_plan();
        if let Some(diff) = settled.diff.as_mut() {
            diff.to_install.clear();
            diff.to_update.clear();
            diff.in_sync = vec!["fd".to_string()];
        }
        let (mut app, _) = apply_app(settled);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("Everything matches packages.toml"),
            "screen is {screen:?}"
        );
        assert!(screen.contains("In sync 1"), "screen is {screen:?}");
    }

    #[test]
    fn a_missing_file_shows_how_to_create_one() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        if home.is_empty() {
            return;
        }
        let missing = ApplyPlan {
            path: Path::new(&home).join(".config/soar/packages.toml"),
            diff: None,
            system: Vec::new(),
        };
        let (mut app, _) = apply_app(missing);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("No packages.toml at"),
            "screen is {screen:?}"
        );
        assert!(
            screen.contains("Press n to create one."),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_failed_computation_shows_the_error_state() {
        let mut app = test_app();
        app.view = ViewId::Apply;
        app.update(
            Msg::Loaded(Loaded::ApplyDiff {
                check: 0,
                result: Err("packages file unreadable".to_string()),
            }),
            Instant::now(),
        );
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("packages file unreadable"),
            "screen is {screen:?}"
        );
        assert!(screen.contains("ctrl-r to retry"), "screen is {screen:?}");
    }

    #[test]
    fn the_system_section_lists_the_deferred_names() {
        let mut deferred = apply_plan();
        deferred.system = vec!["docker-compose".to_string(), "tailscale".to_string()];
        let (mut app, _) = apply_app(deferred);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("System 2"), "screen is {screen:?}");
        assert!(
            screen.contains("asks for administrator rights"),
            "screen is {screen:?}"
        );
        assert!(screen.contains("docker-compose"), "screen is {screen:?}");
        assert!(screen.contains("tailscale"), "screen is {screen:?}");
    }

    #[test]
    fn the_system_section_is_left_out_when_empty() {
        let (mut app, _) = apply_app(apply_plan());
        let screen = screen(&mut app, 100, 24);
        assert!(!screen.contains("System"), "screen is {screen:?}");
    }

    #[test]
    fn version_pins_note_the_write_back() {
        let mut pinned = apply_plan();
        if let Some(diff) = pinned.diff.as_mut() {
            diff.pending_version_updates = vec![("bat".to_string(), "0.24.0".to_string())];
        }
        let (mut app, _) = apply_app(pinned);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("Version pins 1"), "screen is {screen:?}");
        assert!(
            screen.contains("written back to packages.toml"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_border_shows_the_prune_flag() {
        let (mut app, _) = apply_app(apply_plan());
        assert!(screen(&mut app, 100, 24).contains("prune: off"));
        app.apply.prune = true;
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("prune: on"), "screen is {screen:?}");
    }

    #[test]
    fn the_title_shows_the_home_relative_path() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        if home.is_empty() {
            return;
        }
        let mut titled = apply_plan();
        titled.path = Path::new(&home).join(".config/soar/packages.toml");
        let (mut app, _) = apply_app(titled);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("~/.config/soar/packages.toml"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_tall_body_scrolls_with_j_to_the_last_row() {
        let diff = ApplyDiff {
            to_install: (0..40)
                .map(|n| install_entry(&format!("pkg{n:03}"), "1.0.0"))
                .collect(),
            ..Default::default()
        };
        let tall = ApplyPlan {
            path: PathBuf::from("/home/user/.config/soar/packages.toml"),
            diff: Some(diff),
            system: Vec::new(),
        };
        let (mut app, now) = apply_app(tall);
        screen(&mut app, 100, 20);
        for _ in 0..40 {
            app.update(key('j'), now);
        }
        let screen = screen(&mut app, 100, 20);
        assert!(screen.contains("pkg039"), "screen is {screen:?}");
        assert!(!screen.contains("pkg000"), "screen is {screen:?}");
    }

    #[test]
    fn a_loaded_plan_puts_the_cursor_back_at_the_top() {
        let (mut app, now) = apply_app(apply_plan());
        screen(&mut app, 100, 24);
        app.update(key('j'), now);
        app.update(key('j'), now);
        assert_eq!(app.apply.cursor.selected, 2);
        app.update(loaded(apply_plan()), now);
        assert_eq!(app.apply.cursor.selected, 0);
    }

    #[test]
    fn the_view_state_starts_empty() {
        let view = ApplyView::default();
        assert!(view.plan.is_idle());
        assert!(!view.prune);
        assert_eq!(view.cursor.selected, 0);
    }
}
