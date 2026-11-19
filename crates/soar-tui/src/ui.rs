//! The frame layout: the too-small screen, the header, the view body, the
//! activity strip, and the status bar.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Clear, Paragraph},
};

use crate::{
    app::{App, LayoutCache},
    views::{self, ViewId},
    widgets::{activity, header, status_bar, toast},
};

/// The narrowest terminal the interface renders at.
pub(crate) const MIN_WIDTH: u16 = 60;

/// The shortest terminal the interface renders at.
pub(crate) const MIN_HEIGHT: u16 = 16;

/// Draws one frame: the chrome around the active view, or the too-small
/// message below the minimum size. The frame's hit areas are collected
/// from scratch, so a pointer click always reads the last drawn frame.
pub(crate) fn render(f: &mut Frame, app: &mut App) {
    app.layout = LayoutCache::default();
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(f, area, app);
        return;
    }
    let [header_row, body, activity_area, status_row] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(app.activity_height()),
        Constraint::Length(1),
    ])
    .areas(area);
    header::render(f, header_row, app);
    render_body(f, body, app);
    if activity_area.height > 0 {
        activity::render(f, activity_area, app);
    }
    status_bar::render(f, status_row, app);
    toast::render(f, body, app);
    if let Some(overlay) = &app.overlay {
        let rect = overlay.render(f, body, app);
        app.layout.overlay = Some(rect);
    }
}

/// Draws the message a terminal below the minimum size shows instead of the
/// interface.
fn render_too_small(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(Clear, area);
    let times = if app.theme.icons() { "×" } else { "x" };
    let message = format!(
        "Terminal too small: need {MIN_WIDTH}{times}{MIN_HEIGHT}, have {}{times}{}",
        area.width, area.height,
    );
    let row = Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
    };
    f.render_widget(Paragraph::new(message).alignment(Alignment::Center), row);
}

/// Draws the active view in the body.
fn render_body(f: &mut Frame, area: Rect, app: &mut App) {
    match app.view {
        ViewId::Browse => views::browse::render(f, area, app),
        ViewId::Installed => views::installed::render(f, area, app),
        ViewId::Updates => views::updates::render(f, area, app),
        ViewId::Apply => views::apply::render(f, area, app),
        ViewId::Repos => views::repos::render(f, area, app),
        ViewId::Health => views::health::render(f, area, app),
        ViewId::Jobs => views::jobs::render(f, area, app),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{render, screen, test_app};

    #[test]
    fn a_too_small_terminal_shows_the_message() {
        let mut app = test_app();
        assert!(screen(&mut app, 59, 20).contains("Terminal too small"));
        assert!(screen(&mut app, 60, 15).contains("Terminal too small"));
    }

    #[test]
    fn a_full_terminal_shows_the_view_pane() {
        let mut app = test_app();
        let lines = render(&mut app, 80, 24);
        let screen = lines.join("\n");
        assert!(!screen.contains("Terminal too small"));
        assert!(lines[1].contains("Browse"), "body top is {:?}", lines[1]);
    }

    #[test]
    fn the_ascii_glyph_set_draws_through_the_interface() {
        use crate::{
            msg::{Loaded, Msg},
            test_support::{catalog_row, test_app_with},
            theme::Theme,
        };
        let mut app = test_app_with(Theme::new(true, false));
        let mut long = catalog_row(
            "a-very-long-package-name-that-cannot-fit-the-column",
            "1.0.0",
            "bincache",
        );
        long.description = "shaped by the glyph set".to_string();
        app.update(
            Msg::Loaded(Loaded::Catalog {
                check: 0,
                result: Ok(vec![catalog_row("bat", "0.24.0", "bincache"), long]),
            }),
            std::time::Instant::now(),
        );
        let screen = screen(&mut app, 60, 20);
        assert!(
            screen.contains('>'),
            "the ascii cursor, screen is {screen:?}"
        );
        assert!(
            screen.contains("..."),
            "the ascii ellipsis cuts a long name, screen is {screen:?}"
        );
        assert!(!screen.contains('▌'), "screen is {screen:?}");
        assert!(!screen.contains('…'), "screen is {screen:?}");
    }
}
