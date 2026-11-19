//! The dialog that picks one row out of a list.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    app::App,
    widgets::{
        overlay::{OnAccept, Overlay, OverlayOutcome, Value, centered, frame},
        scroll::ListCursor,
    },
};

/// A dialog listing the choices for one decision, accepted by index.
#[derive(Debug)]
pub(crate) struct Picker {
    title: String,
    rows: Vec<Line<'static>>,
    cursor: ListCursor,
    on_accept: OnAccept,
}

impl Picker {
    /// Builds a picker over `rows`, starting at the first one. The views
    /// open pickers once they gain their actions; until then only tests
    /// build one.
    #[allow(dead_code)]
    pub(crate) fn new(
        title: impl Into<String>,
        rows: Vec<Line<'static>>,
        on_accept: OnAccept,
    ) -> Picker {
        Picker {
            title: title.into(),
            rows,
            cursor: ListCursor::default(),
            on_accept,
        }
    }

    /// Acts on one pressed key. The movement keys move the cursor, enter
    /// accepts it, and esc closes.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        let len = self.rows.len();
        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.cursor.down(1, len);
            }
            (KeyCode::Char('k') | KeyCode::Up, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.cursor.up(1);
            }
            (KeyCode::Char('g') | KeyCode::Home, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.cursor.top();
            }
            (KeyCode::Char('G') | KeyCode::End, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.cursor.bottom(len);
            }
            (KeyCode::Enter, _) if len > 0 => {
                return OverlayOutcome::Accept(
                    self.on_accept,
                    Some(Value::Index(self.cursor.selected)),
                );
            }
            (KeyCode::Esc, _) => return OverlayOutcome::Close,
            _ => {}
        }
        OverlayOutcome::Stay(Overlay::Picker(self))
    }

    /// Draws the picker centered in the body area, scrolling its rows with
    /// the cursor, and returns the rect it drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(72);
        let height = (self.rows.len() as u16 + 2).min(area.height.saturating_sub(4));
        let rect = centered(area, width, height);
        f.render_widget(Clear, rect);

        let visible = usize::from(rect.height.saturating_sub(2));
        // The window derives from the cursor every frame, so its offset
        // needs no persistence between frames.
        let mut cursor = self.cursor;
        let window = cursor.window(self.rows.len(), visible);

        let symbols = theme.sym();
        let mut lines = Vec::new();
        for (index, row) in self
            .rows
            .iter()
            .enumerate()
            .skip(window.start)
            .take(window.end - window.start)
        {
            let selected = index == self.cursor.selected;
            let marker = if selected { symbols.cursor } else { " " };
            let mut spans = Vec::with_capacity(row.spans.len() + 2);
            spans.push(Span::raw(marker));
            spans.push(Span::raw(" "));
            spans.extend(row.spans.iter().cloned());
            let mut line = Line::from(spans);
            if selected {
                line = line.style(theme.cursor_row());
            }
            lines.push(line);
        }

        let footer = format!(" enter pick {} esc ", symbols.dot);
        let block = frame(
            Line::styled(format!(" {} ", self.title), theme.title()),
            Some(Line::styled(footer, theme.muted())),
            false,
            theme,
        );
        f.render_widget(Paragraph::new(lines).block(block), rect);
        rect
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        text::Line,
    };

    use super::Picker;
    use crate::{
        test_support::{screen, test_app},
        widgets::overlay::{OnAccept, Overlay, OverlayOutcome, Value},
    };

    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers::NONE)
    }

    fn picker() -> Picker {
        Picker::new(
            "Choose a variant of \"ffmpeg\"",
            vec![
                Line::raw("ffmpeg 7.1 bincache static"),
                Line::raw("ffmpeg 7.0.2 pkgcache appimage"),
                Line::raw("jellyfin/ffmpeg 7.0.2 pkgcache appimage"),
            ],
            OnAccept::Test,
        )
    }

    fn stay(outcome: OverlayOutcome) -> Picker {
        match outcome {
            OverlayOutcome::Stay(Overlay::Picker(picker)) => picker,
            other => panic!("expected the picker to stay open, got {other:?}"),
        }
    }

    #[test]
    fn moving_past_the_end_clamps_the_cursor() {
        let mut picker = picker();
        for _ in 0..4 {
            picker = stay(picker.handle(press(KeyCode::Char('j'))));
        }
        assert_eq!(picker.cursor.selected, 2);
    }

    #[test]
    fn moving_past_the_start_clamps_the_cursor() {
        let picker = stay(picker().handle(press(KeyCode::Char('k'))));
        assert_eq!(picker.cursor.selected, 0);
    }

    #[test]
    fn enter_accepts_the_cursor_index() {
        let picker = stay(picker().handle(press(KeyCode::Char('j'))));
        let picker = stay(picker.handle(press(KeyCode::Char('j'))));
        assert!(matches!(
            picker.handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Index(2)))
        ));
    }

    #[test]
    fn jump_keys_go_to_the_ends() {
        let picker = stay(picker().handle(press(KeyCode::Char('G'))));
        assert_eq!(picker.cursor.selected, 2);
        let picker = stay(picker.handle(press(KeyCode::Char('g'))));
        assert_eq!(picker.cursor.selected, 0);
    }

    #[test]
    fn esc_closes() {
        assert!(matches!(
            picker().handle(press(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn the_picker_shows_its_title_rows_and_footer() {
        let mut app = test_app();
        app.overlay = Some(Overlay::Picker(picker()));
        let screen = screen(&mut app, 80, 24);
        for text in ["Choose a variant", "ffmpeg 7.1", "enter pick · esc"] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn the_window_scrolls_a_long_picker_with_the_cursor() {
        let rows: Vec<Line<'static>> = (0..30).map(|i| Line::raw(format!("row {i}"))).collect();
        let mut picker = Picker::new("Pick", rows, OnAccept::Test);
        picker = stay(picker.handle(press(KeyCode::Char('G'))));
        let mut app = test_app();
        app.overlay = Some(Overlay::Picker(picker));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("row 29"), "screen is {screen:?}");
        assert!(!screen.contains("row 0"), "screen is {screen:?}");
    }
}
