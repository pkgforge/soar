//! The yes-or-no dialog that names everything about to change.

#![allow(dead_code)]

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyModifiers},
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph},
};

use crate::{
    app::App,
    theme::Theme,
    widgets::overlay::{OnAccept, Overlay, OverlayOutcome, buttons, centered, frame},
};

/// A dialog asking whether to carry out one action, naming everything that
/// will change. Destructive dialogs start focused on cancel so an
/// accidental enter cannot act.
#[derive(Debug)]
pub(crate) struct Confirm {
    title: String,
    body: Vec<Line<'static>>,
    action_label: String,
    danger: bool,
    focus_action: bool,
    on_accept: OnAccept,
}

impl Confirm {
    /// Builds a confirm dialog. Destructive dialogs start with the focus on
    /// the cancel button.
    pub(crate) fn new(
        title: impl Into<String>,
        body: Vec<Line<'static>>,
        action_label: impl Into<String>,
        danger: bool,
        on_accept: OnAccept,
    ) -> Confirm {
        Confirm {
            title: title.into(),
            body,
            action_label: action_label.into(),
            danger,
            focus_action: !danger,
            on_accept,
        }
    }

    /// What accepting this dialog carries out.
    pub(crate) fn action(&self) -> &OnAccept {
        &self.on_accept
    }

    /// Acts on one pressed key. `y` accepts, `n` and esc close, enter
    /// presses the focused button, and the movement keys switch it.
    pub(crate) fn handle(mut self, key: ratatui::crossterm::event::KeyEvent) -> OverlayOutcome {
        match (key.code, key.modifiers) {
            (KeyCode::Char('y'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                OverlayOutcome::Accept(self.on_accept, None)
            }
            (KeyCode::Char('n'), KeyModifiers::NONE | KeyModifiers::SHIFT) | (KeyCode::Esc, _) => {
                OverlayOutcome::Close
            }
            (
                KeyCode::Left
                | KeyCode::Right
                | KeyCode::Tab
                | KeyCode::Char('h')
                | KeyCode::Char('l'),
                _,
            ) => {
                self.focus_action = !self.focus_action;
                OverlayOutcome::Stay(Overlay::Confirm(self))
            }
            (KeyCode::Enter, _) if self.focus_action => {
                OverlayOutcome::Accept(self.on_accept, None)
            }
            (KeyCode::Enter, _) => OverlayOutcome::Close,
            _ => OverlayOutcome::Stay(Overlay::Confirm(self)),
        }
    }

    /// Draws the dialog centered in the body area, and returns the rect it
    /// drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(64);
        let rect = centered(area, width, self.body.len() as u16 + 6);
        f.render_widget(Clear, rect);

        let mut lines = vec![Line::raw("")];
        lines.extend(self.visible_body(theme, rect.height));
        lines.push(Line::raw(""));
        let buttons = buttons(
            &self.action_label,
            !self.focus_action,
            self.focus_action,
            self.danger,
            theme,
        )
        .right_aligned();
        lines.push(buttons);

        let block = frame(
            Line::styled(format!(" {} ", self.title), theme.title()),
            None,
            self.danger,
            theme,
        );
        f.render_widget(Paragraph::new(lines).block(block), rect);
        rect
    }

    /// The body lines that fit in `height` rows, ending with a count of the
    /// ones cut off.
    fn visible_body(&self, theme: &Theme, height: u16) -> Vec<Line<'static>> {
        let budget = usize::from(height).saturating_sub(5);
        if self.body.len() <= budget {
            return self.body.clone();
        }
        let shown = budget.saturating_sub(1);
        let mut lines: Vec<Line<'static>> = self.body.iter().take(shown).cloned().collect();
        let hidden = self.body.len() - shown;
        let more = format!("{} and {hidden} more", theme.sym().ellipsis);
        lines.push(Line::styled(more, theme.muted()));
        lines
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        text::Line,
    };

    use super::Confirm;
    use crate::{
        test_support::{screen, test_app},
        widgets::overlay::{OnAccept, Overlay, OverlayOutcome},
    };

    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers::NONE)
    }

    fn confirm(danger: bool) -> Confirm {
        Confirm::new(
            "Remove 2 packages",
            vec![Line::raw("obsidian 1.7.7"), Line::raw("zellij 0.41.2")],
            "Remove",
            danger,
            OnAccept::Test,
        )
    }

    fn stay(outcome: OverlayOutcome) -> Confirm {
        match outcome {
            OverlayOutcome::Stay(Overlay::Confirm(confirm)) => confirm,
            other => panic!("expected the dialog to stay open, got {other:?}"),
        }
    }

    #[test]
    fn enter_on_a_danger_confirm_closes_without_accepting() {
        assert!(matches!(
            confirm(true).handle(press(KeyCode::Enter)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn enter_on_a_plain_confirm_accepts() {
        assert!(matches!(
            confirm(false).handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, None)
        ));
    }

    #[test]
    fn y_accepts_whether_the_action_is_focused() {
        for danger in [true, false] {
            assert!(matches!(
                confirm(danger).handle(press(KeyCode::Char('y'))),
                OverlayOutcome::Accept(OnAccept::Test, None)
            ));
        }
    }

    #[test]
    fn n_and_esc_close() {
        assert!(matches!(
            confirm(true).handle(press(KeyCode::Char('n'))),
            OverlayOutcome::Close
        ));
        assert!(matches!(
            confirm(true).handle(press(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn movement_keys_switch_the_focused_button() {
        let confirm = stay(confirm(true).handle(press(KeyCode::Right)));
        assert!(matches!(
            confirm.handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, None)
        ));
    }

    #[test]
    fn the_dialog_shows_its_title_body_and_buttons() {
        let mut app = test_app();
        app.overlay = Some(Overlay::Confirm(confirm(true)));
        let screen = screen(&mut app, 80, 24);
        for text in [
            "Remove 2 packages",
            "obsidian 1.7.7",
            "[ Cancel ]",
            "[ Remove ]",
        ] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_tall_body_lists_what_does_not_fit() {
        let mut app = test_app();
        let body: Vec<Line<'static>> = (0..20).map(|n| Line::raw(format!("package {n}"))).collect();
        app.overlay = Some(Overlay::Confirm(Confirm::new(
            "Remove",
            body,
            "Remove",
            true,
            OnAccept::Test,
        )));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("package 0"), "screen is {screen:?}");
        assert!(screen.contains("and 4 more"), "screen is {screen:?}");
        assert!(!screen.contains("package 19"), "screen is {screen:?}");
    }
}
