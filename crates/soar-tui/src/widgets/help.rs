//! The overlay listing every key that works while a view is open.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    action::Action,
    app::App,
    keymap,
    theme::Theme,
    views::ViewId,
    widgets::overlay::{Overlay, OverlayOutcome, centered, frame},
};

/// The key reference of the active view, opened with `?`.
#[derive(Debug)]
pub(crate) struct Help {
    view: ViewId,
    scroll: u16,
}

impl Help {
    /// Builds the reference for one view, starting at its top.
    pub(crate) fn new(view: ViewId) -> Help {
        Help {
            view,
            scroll: 0,
        }
    }

    /// Acts on one pressed key. `j` and `k` scroll, and esc, `?`, and `q`
    /// close.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        match (key.code, key.modifiers) {
            (KeyCode::Char('j'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.scroll = self.scroll.saturating_add(1);
            }
            (KeyCode::Char('k'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            (KeyCode::Esc, _)
            | (KeyCode::Char('?') | KeyCode::Char('q'), KeyModifiers::NONE | KeyModifiers::SHIFT) =>
            {
                return OverlayOutcome::Close;
            }
            _ => {}
        }
        OverlayOutcome::Stay(Overlay::Help(self))
    }

    /// Draws the reference centered in the body area, taking most of it,
    /// and returns the rect it drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = u32::from(area.width) * 70 / 100;
        let height = u32::from(area.height) * 80 / 100;
        let rect = centered(area, width as u16, height as u16);
        f.render_widget(Clear, rect);

        let lines = self.lines(app);
        let visible = usize::from(rect.height.saturating_sub(2));
        let scroll = usize::from(self.scroll).min(lines.len().saturating_sub(visible));
        let shown: Vec<Line<'static>> = lines.into_iter().skip(scroll).take(visible).collect();

        let block = frame(Line::styled(" Keys ", theme.title()), None, false, theme);
        f.render_widget(Paragraph::new(shown).block(block), rect);
        rect
    }

    /// Every line of the reference: the global keys, then the view's own,
    /// and the marks note where marks exist.
    fn lines(&self, app: &App) -> Vec<Line<'static>> {
        let theme = &app.theme;
        let view_bindings = keymap::bindings(self.view);
        let mut lines = vec![section_line("Global", theme)];
        lines.extend(binding_lines(keymap::GLOBAL, theme));
        lines.push(section_line(self.view.title(), theme));
        lines.extend(binding_lines(view_bindings, theme));
        if view_bindings
            .iter()
            .any(|binding| matches!(binding.action, Action::ToggleMark))
        {
            lines.push(Line::styled(
                "Marks select rows for i, d, and u.",
                theme.muted(),
            ));
            lines.push(Line::styled(
                "Without marks, the current row is used.",
                theme.muted(),
            ));
        }
        lines
    }
}

/// One section heading of the reference.
fn section_line(name: &str, theme: &Theme) -> Line<'static> {
    Line::styled(name.to_owned(), theme.title())
}

/// The rows of one key table: the keys padded to a fixed width in the key
/// style, then the label.
fn binding_lines(bindings: &[keymap::Binding], theme: &Theme) -> Vec<Line<'static>> {
    bindings
        .iter()
        .map(|binding| {
            let keys = binding
                .keys
                .iter()
                .map(|key| keymap::key_label(*key))
                .collect::<Vec<_>>()
                .join("/");
            Line::from(vec![
                Span::styled(format!("{keys:<16}"), theme.key()),
                Span::styled(binding.label, theme.muted()),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::Help;
    use crate::{
        test_support::{key, screen, test_app},
        views::ViewId,
        widgets::overlay::{Overlay, OverlayOutcome},
    };

    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn stay(outcome: OverlayOutcome) -> Help {
        match outcome {
            OverlayOutcome::Stay(Overlay::Help(help)) => help,
            other => panic!("expected the help to stay open, got {other:?}"),
        }
    }

    #[test]
    fn opening_help_lists_the_global_and_view_keys() {
        let mut app = test_app();
        app.update(key('?'), Instant::now());
        assert!(matches!(app.overlay, Some(Overlay::Help(_))));
        let screen = screen(&mut app, 100, 40);
        assert!(screen.contains("Global"), "screen is {screen:?}");
        assert!(screen.contains("install"), "screen is {screen:?}");
    }

    #[test]
    fn the_marks_note_shows_only_where_marks_exist() {
        let mut app = test_app();
        app.update(key('?'), Instant::now());
        assert!(screen(&mut app, 100, 48).contains("Marks select rows"));

        app.overlay = None;
        app.view = ViewId::Jobs;
        app.update(key('?'), Instant::now());
        assert!(!screen(&mut app, 100, 48).contains("Marks select rows"));
    }

    #[test]
    fn every_binding_of_the_view_is_listed() {
        let mut app = test_app();
        app.overlay = Some(crate::widgets::overlay::Overlay::Help(Help::new(
            ViewId::Browse,
        )));
        let screen = screen(&mut app, 100, 40);
        assert!(screen.contains("install options"), "screen is {screen:?}");
        assert!(screen.contains("page down"), "screen is {screen:?}");
    }

    #[test]
    fn j_and_k_scroll() {
        let help = stay(Help::new(ViewId::Browse).handle(press('j')));
        assert_eq!(help.scroll, 1);
        let help = stay(help.handle(press('k')));
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn question_mark_esc_and_q_close() {
        for key in ['?', 'q'] {
            assert!(matches!(
                Help::new(ViewId::Browse).handle(press(key)),
                OverlayOutcome::Close
            ));
        }
        assert!(matches!(
            Help::new(ViewId::Browse).handle(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            OverlayOutcome::Close
        ));
    }
}
