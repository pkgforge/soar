//! The header line: the brand, the view tabs, and the profile with the
//! session mode.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{app::App, format, theme::Theme, views::ViewId};

/// Which extra parts of the header survive as the terminal narrows, fullest
/// first: (version, tab digits, full tab titles).
const DEGRADATIONS: [(bool, bool, bool); 4] = [
    (true, true, true),
    (false, true, true),
    (false, false, true),
    (false, false, false),
];

/// One candidate header: its left and right spans, and where its tabs sit
/// relative to the row start.
struct Parts {
    left: Vec<Span<'static>>,
    tabs: Vec<(u16, u16, ViewId)>,
    right: Vec<Span<'static>>,
}

/// Draws the header row and records where each tab landed, so pointer
/// clicks can find the tab they hit.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let row = Rect {
        height: 1,
        ..area
    };
    let header = DEGRADATIONS
        .iter()
        .find_map(|&(version, digits, full_titles)| {
            let candidate = parts(app, version, digits, full_titles, row.width);
            let width = spans_width(&candidate.left) + spans_width(&candidate.right);
            (width <= row.width as usize).then_some(candidate)
        })
        .unwrap_or_else(|| parts(app, false, false, true, row.width));
    app.layout.tabs = header
        .tabs
        .into_iter()
        .map(|(offset, width, view)| {
            (
                Rect {
                    x: row.x + offset,
                    y: row.y,
                    width,
                    height: 1,
                },
                view,
            )
        })
        .collect();
    f.render_widget(Line::from(header.left), row);
    f.render_widget(Line::from(header.right).right_aligned(), row);
}

/// Builds the header at one degradation level. The right side truncates its
/// profile to whatever the left side leaves of `width`.
fn parts(app: &App, with_version: bool, with_digits: bool, full_titles: bool, width: u16) -> Parts {
    let theme = &app.theme;
    let symbols = theme.sym();
    let mut left = Vec::new();
    let mut x: usize = 0;
    let mut tabs = Vec::with_capacity(ViewId::ALL.len());

    left.push(Span::raw(" "));
    x += 1;
    let brand = format!("{} soar ", symbols.brand);
    x += Span::raw(brand.as_str()).width();
    left.push(Span::styled(
        brand,
        theme.accent().add_modifier(Modifier::BOLD),
    ));
    if with_version {
        x += Span::raw(app.env.version.as_str()).width();
        left.push(Span::styled(app.env.version.clone(), theme.muted()));
    }
    for (position, view) in ViewId::ALL.iter().enumerate() {
        let title = if full_titles {
            view.title().to_string()
        } else {
            view.title().chars().take(3).collect()
        };
        let label = if with_digits {
            let number = position + 1;
            format!("{number} {title}")
        } else {
            title
        };
        left.push(Span::raw("  "));
        x += 2;
        let start = x as u16;
        let mut tab_width = Span::raw(label.as_str()).width() as u16;
        x += tab_width as usize;
        left.push(Span::styled(label, tab_style(theme, *view, app.view)));
        if *view == ViewId::Updates
            && let Some(count) = app.updates_badge()
            && count > 0
        {
            let badge = Span::styled(format!(" {count}"), theme.warning());
            tab_width += badge.width() as u16;
            x += badge.width();
            left.push(badge);
        }
        if *view == ViewId::Jobs && app.jobs_attention() {
            let mark = Span::styled(" !", theme.danger());
            tab_width += mark.width() as u16;
            x += mark.width();
            left.push(mark);
        }
        if *view == ViewId::Jobs && app.running_job().is_some() {
            let mark = Span::styled(format!(" {}", theme.spinner(app.frame)), theme.accent());
            tab_width += mark.width() as u16;
            x += mark.width();
            left.push(mark);
        }
        tabs.push((start, tab_width, *view));
    }

    let mode = if app.env.system { "SYSTEM" } else { "user" };
    let mode_style = if app.env.system {
        theme.warning().add_modifier(Modifier::BOLD)
    } else {
        theme.muted()
    };
    let reserved = 2 + Span::raw(symbols.dot).width() + Span::raw(mode).width();
    let profile_budget = (width as usize).saturating_sub(reserved);
    let profile = format::truncate(&app.env.profile, profile_budget, symbols.ellipsis);
    let right = vec![
        Span::styled(profile, theme.text()),
        Span::styled(format!(" {} ", symbols.dot), theme.muted()),
        Span::styled(mode.to_string(), mode_style),
    ];
    Parts {
        left,
        tabs,
        right,
    }
}

/// The style of one tab: the active one stands out.
fn tab_style(theme: &Theme, view: ViewId, active: ViewId) -> Style {
    if view == active {
        theme
            .accent()
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        theme.muted()
    }
}

/// The display width of a row of spans.
fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

#[cfg(test)]
mod tests {
    use crate::{
        test_support::{render, test_app},
        views::ViewId,
    };

    #[test]
    fn the_header_shows_tabs_profile_and_mode_at_full_width() {
        let mut app = test_app();
        let lines = render(&mut app, 100, 16);
        for text in ["1 Browse", "7 Jobs", "default", "user"] {
            assert!(lines[0].contains(text), "header is {:?}", lines[0]);
        }
        assert_eq!(app.layout.tabs.len(), 7);
        assert_eq!(app.layout.tabs[0].1, ViewId::Browse);
    }

    #[test]
    fn system_mode_shows_the_system_badge() {
        let mut app = test_app();
        app.env.system = true;
        let lines = render(&mut app, 100, 16);
        assert!(lines[0].contains("SYSTEM"), "header is {:?}", lines[0]);
    }

    #[test]
    fn a_narrow_header_drops_the_version_first() {
        let mut app = test_app();
        let lines = render(&mut app, 96, 16);
        assert!(lines[0].contains("1 Browse"), "header is {:?}", lines[0]);
        assert!(!lines[0].contains("0.0.0"), "header is {:?}", lines[0]);
    }

    #[test]
    fn a_narrower_header_drops_the_tab_digits() {
        let mut app = test_app();
        let lines = render(&mut app, 80, 16);
        assert!(lines[0].contains("Browse"), "header is {:?}", lines[0]);
        assert!(!lines[0].contains("1 Browse"), "header is {:?}", lines[0]);
    }

    #[test]
    fn the_narrowest_header_shortens_the_titles() {
        let mut app = test_app();
        let lines = render(&mut app, 70, 16);
        assert!(lines[0].contains("Job"), "header is {:?}", lines[0]);
        assert!(!lines[0].contains("Jobs"), "header is {:?}", lines[0]);
        assert!(lines[0].contains("default"), "header is {:?}", lines[0]);
    }
}
