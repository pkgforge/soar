//! The administrator password dialog for the system pass.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    app::App,
    format,
    privileged::Secret,
    theme::Theme,
    widgets::{
        overlay::{Overlay, OverlayOutcome, buttons, centered, frame},
        text_input::TextInput,
    },
};

/// The focus positions: the password field, then cancel, then authorize.
const PASSWORD: usize = 0;
const CANCEL: usize = 1;
const AUTHORIZE: usize = 2;
const LAST_FOCUS: usize = AUTHORIZE;

/// Asks for the administrator password before the system pass runs. The
/// value never shows: each typed character draws as the theme's mask, and
/// the only way out of the widget is through a [`Secret`] handed straight
/// to the runner, or closing the dialog. The debug output hides the
/// password, because the input keeps it out of its own debug output.
#[derive(Debug)]
pub(crate) struct Authorize {
    /// The declared packages the pass applies, named in the body.
    names: Vec<String>,
    /// The password field, its value hidden.
    input: TextInput,
    /// Why the last password was refused, shown until the next one is
    /// typed.
    error: Option<String>,
    /// Which of the password field and the two buttons holds the focus.
    focus: usize,
    /// Whether a password is being checked: a spinner replaces the
    /// buttons and every key except esc is ignored.
    busy: bool,
}

impl Authorize {
    /// Builds the dialog for the named packages, focus in the password
    /// field.
    pub(crate) fn new(names: Vec<String>) -> Authorize {
        Authorize {
            names,
            input: TextInput::default().masked(),
            error: None,
            focus: PASSWORD,
            busy: false,
        }
    }

    /// Whether the dialog waits for a password check to answer. Only a
    /// waiting dialog can receive an answer.
    pub(crate) fn is_busy(&self) -> bool {
        self.busy
    }

    /// Builds the dialog already waiting for the password check to come
    /// back.
    pub(crate) fn waiting(names: Vec<String>) -> Authorize {
        let mut dialog = Authorize::new(names);
        dialog.busy = true;
        dialog
    }

    /// Acts on one pressed key. Typing edits the password while the field
    /// holds the focus, and tab and up and down walk the focus across the
    /// field and the two buttons, where left and right step the focus as
    /// well. Enter submits or presses a button, and esc closes.
    /// Keystrokes the focused control does not use are ignored, so the
    /// hidden password edits only from the field in view. While busy
    /// only esc is heard.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        if self.busy {
            return match key.code {
                KeyCode::Esc => OverlayOutcome::Close,
                _ => OverlayOutcome::Stay(Overlay::Authorize(self)),
            };
        }
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => OverlayOutcome::Close,
            (KeyCode::Tab | KeyCode::Down, _) => {
                self.focus = (self.focus + 1).min(LAST_FOCUS);
                OverlayOutcome::Stay(Overlay::Authorize(self))
            }
            (KeyCode::BackTab | KeyCode::Up, _) => {
                self.focus = self.focus.saturating_sub(1);
                OverlayOutcome::Stay(Overlay::Authorize(self))
            }
            (KeyCode::Left, _) if self.focus != PASSWORD => {
                self.focus = self.focus.saturating_sub(1);
                OverlayOutcome::Stay(Overlay::Authorize(self))
            }
            (KeyCode::Right, _) if self.focus != PASSWORD => {
                self.focus = (self.focus + 1).min(LAST_FOCUS);
                OverlayOutcome::Stay(Overlay::Authorize(self))
            }
            (KeyCode::Enter, _) => {
                match self.focus {
                    CANCEL => OverlayOutcome::Close,
                    _ => self.submit(),
                }
            }
            _ => {
                if self.focus == PASSWORD && self.input.handle(key) {
                    self.error = None;
                }
                OverlayOutcome::Stay(Overlay::Authorize(self))
            }
        }
    }

    /// Moves the typed value into a secret for the runner and waits for
    /// its answer. An empty password does nothing.
    fn submit(mut self) -> OverlayOutcome {
        if self.input.is_empty() {
            return OverlayOutcome::Stay(Overlay::Authorize(self));
        }
        let secret = Secret::new(self.input.take().into_bytes());
        self.busy = true;
        OverlayOutcome::Authenticate(secret)
    }

    /// Shows why the password was refused and takes the focus back for
    /// another try.
    pub(crate) fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.busy = false;
        self.focus = PASSWORD;
    }

    /// Draws the dialog centered in the body area, and returns the rect it
    /// drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(64);
        // The content is what the borders and the one-column padding on
        // each side leave, so every body line sizes against what it
        // actually draws into.
        let content = usize::from(width.saturating_sub(4));
        let lines = self.body_lines(theme, app.frame, content);
        let rect = centered(area, width, lines.len() as u16 + 2);
        f.render_widget(Clear, rect);

        let block = frame(
            Line::styled(" Administrator rights needed ", theme.title()),
            None,
            false,
            theme,
        );
        f.render_widget(Paragraph::new(lines).block(block), rect);
        rect
    }

    /// The body: what the pass applies, what runs it, the password field,
    /// the refusal of the last password if there was one, and the buttons
    /// or the waiting spinner. `content` is the dialog's inner width, so
    /// every row is cut to the width it really draws into.
    fn body_lines(&self, theme: &Theme, frame_tick: u64, content: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::raw("")];
        lines.push(Line::raw(format!(
            "  Apply {} system packages from packages.toml:",
            self.names.len()
        )));
        let names = self.names.join(", ");
        for row in format::wrap(&names, content.saturating_sub(2)) {
            lines.push(Line::raw(format!("  {row}")));
        }
        lines.push(Line::raw(""));
        lines.push(Line::raw("  sudo will run:"));
        lines.push(Line::styled(
            "  soar --system apply --only-system",
            theme.muted(),
        ));
        lines.push(Line::raw(""));
        let mut field = vec![Span::raw("  "), Span::raw("Password"), Span::raw("  ")];
        // The field label takes twelve of the dialog's columns and the
        // focused cursor cell draws one past the value's window, so the
        // value reserves both and keeps its tail in view.
        field.extend(
            self.input
                .line(self.focus == PASSWORD, content.saturating_sub(13), theme)
                .spans,
        );
        lines.push(Line::from(field));
        if let Some(error) = &self.error {
            let symbol = theme.sym().error;
            // The refusal's lead takes four of the content's columns, so
            // the text wraps to the rest and no row overflows the dialog.
            for (index, row) in format::wrap(error, content.saturating_sub(4))
                .into_iter()
                .enumerate()
            {
                let lead = if index == 0 {
                    format!("  {symbol} ")
                } else {
                    "    ".to_string()
                };
                lines.push(Line::styled(format!("{lead}{row}"), theme.danger()));
            }
        }
        lines.push(Line::raw(""));
        if self.busy {
            lines.push(
                Line::from(vec![
                    Span::styled(theme.spinner(frame_tick), theme.accent()),
                    Span::raw(" Checking password"),
                ])
                .right_aligned(),
            );
        } else {
            lines.push(
                buttons(
                    "Authorize",
                    self.focus == CANCEL,
                    self.focus == AUTHORIZE,
                    false,
                    theme,
                )
                .right_aligned(),
            );
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::Authorize;
    use crate::{
        test_support::{screen, test_app},
        widgets::overlay::{Overlay, OverlayOutcome},
    };

    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn press_key(key: KeyCode) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers::NONE)
    }

    fn dialog() -> Authorize {
        Authorize::new(vec!["docker-compose".to_string(), "tailscale".to_string()])
    }

    fn stay(outcome: OverlayOutcome) -> Authorize {
        match outcome {
            OverlayOutcome::Stay(Overlay::Authorize(dialog)) => dialog,
            other => panic!("expected the dialog to stay open, got {other:?}"),
        }
    }

    #[test]
    fn the_dialog_names_the_packages_and_the_pass() {
        let mut app = test_app();
        app.overlay = Some(Overlay::Authorize(dialog()));
        let screen = screen(&mut app, 80, 24);
        for text in [
            "Administrator rights needed",
            "Apply 2 system packages from packages.toml:",
            "docker-compose, tailscale",
            "sudo will run:",
            "soar --system apply --only-system",
            "Password",
            "[ Cancel ]",
            "[ Authorize ]",
        ] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn typed_characters_draw_as_masks_and_the_value_stays_hidden() {
        let mut app = test_app();
        let mut dialog = dialog();
        for c in ['a', 'b', 'c'] {
            dialog = stay(dialog.handle(press(c)));
        }
        app.overlay = Some(Overlay::Authorize(dialog));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("•••"), "screen is {screen:?}");
        assert!(!screen.contains("abc"), "screen is {screen:?}");
    }

    #[test]
    fn enter_moves_the_value_into_a_secret_and_waits() {
        let mut dialog = dialog();
        for c in ['a', 'b', 'c'] {
            dialog = stay(dialog.handle(press(c)));
        }
        let outcome = dialog.handle(press_key(KeyCode::Enter));
        match outcome {
            OverlayOutcome::Authenticate(secret) => {
                assert_eq!(secret.bytes(), b"abc");
            }
            other => panic!("expected the password to submit, got {other:?}"),
        }
    }

    #[test]
    fn enter_on_an_empty_password_does_nothing() {
        let outcome = dialog().handle(press_key(KeyCode::Enter));
        stay(outcome);
    }

    #[test]
    fn esc_and_the_cancel_button_close() {
        assert!(matches!(
            dialog().handle(press_key(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
        let dialog = stay(dialog().handle(press_key(KeyCode::Tab)));
        assert!(matches!(
            dialog.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn tab_walks_the_focus_between_the_field_and_the_buttons() {
        // One tab reaches cancel, whose enter closes.
        let shifted = stay(dialog().handle(press_key(KeyCode::Tab)));
        assert!(matches!(
            shifted.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Close
        ));
        // Two tabs reach authorize, whose enter submits.
        let mut dialog = dialog();
        for c in ['a', 'b'] {
            dialog = stay(dialog.handle(press(c)));
        }
        dialog = stay(dialog.handle(press_key(KeyCode::Tab)));
        let shifted = stay(dialog.handle(press_key(KeyCode::Tab)));
        assert!(matches!(
            shifted.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Authenticate(_)
        ));
    }

    #[test]
    fn left_and_right_stay_in_the_password_field() {
        let mut dialog = dialog();
        for c in ['a', 'b'] {
            dialog = stay(dialog.handle(press(c)));
        }
        dialog = stay(dialog.handle(press_key(KeyCode::Left)));
        dialog = stay(dialog.handle(press('c')));
        match dialog.handle(press_key(KeyCode::Enter)) {
            OverlayOutcome::Authenticate(secret) => {
                assert_eq!(secret.bytes(), b"acb");
            }
            other => panic!("the field kept the focus and submitted, got {other:?}"),
        }
    }

    #[test]
    fn typing_while_a_button_holds_the_focus_edits_nothing() {
        let mut dialog = dialog();
        for c in ['a', 'b'] {
            dialog = stay(dialog.handle(press(c)));
        }
        // One tab reaches cancel, one right the authorize button.
        dialog = stay(dialog.handle(press_key(KeyCode::Tab)));
        dialog = stay(dialog.handle(press_key(KeyCode::Right)));
        dialog = stay(dialog.handle(press('x')));
        match dialog.handle(press_key(KeyCode::Enter)) {
            OverlayOutcome::Authenticate(secret) => {
                assert_eq!(secret.bytes(), b"ab");
            }
            other => panic!("the button submitted the typed password, got {other:?}"),
        }
    }

    #[test]
    fn a_long_refusal_wraps_inside_the_dialog() {
        let mut app = test_app();
        let mut dialog = dialog();
        dialog.fail(
            "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen"
                .to_string(),
        );
        app.overlay = Some(Overlay::Authorize(dialog));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("eight nine ten"), "screen is {screen:?}");
        assert!(screen.contains("twelve thirteen"), "screen is {screen:?}");
        assert!(
            !screen.contains("eleven twelve"),
            "the refusal must wrap, screen is {screen:?}"
        );
    }

    #[test]
    fn the_password_line_fits_the_dialog_content_at_full_width() {
        let app = test_app();
        let mut dialog = dialog();
        // Longer than the value room at the dialog's full width, so the
        // line draws a full window of masks.
        for c in "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .chars()
        {
            dialog = stay(dialog.handle(press(c)));
        }
        let lines = dialog.body_lines(&app.theme, 0, 60);
        let Some(field) = lines.iter().find(|line| {
            line.spans
                .get(1)
                .is_some_and(|span| span.content == "Password")
        }) else {
            panic!("the password line is in the body");
        };
        assert!(
            field.width() <= 60,
            "the field must fit the content width, got {}",
            field.width()
        );
    }

    #[test]
    fn a_refused_password_shows_in_danger_until_the_next_keystroke() {
        let mut app = test_app();
        let mut dialog = dialog();
        dialog.fail("Incorrect password, try again".to_string());
        app.overlay = Some(Overlay::Authorize(dialog));
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("Incorrect password, try again"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn waiting_ignores_every_key_except_esc() {
        let dialog = Authorize::waiting(Vec::new());
        let outcome = dialog.handle(press('x'));
        stay(outcome);
        assert!(matches!(
            Authorize::waiting(Vec::new()).handle(press_key(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn waiting_draws_a_spinner_instead_of_the_buttons() {
        let mut app = test_app();
        app.overlay = Some(Overlay::Authorize(Authorize::waiting(Vec::new())));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("Checking password"), "screen is {screen:?}");
        assert!(!screen.contains("[ Authorize ]"), "screen is {screen:?}");
    }

    #[test]
    fn debugging_the_dialog_never_carries_the_password() {
        let mut dialog = dialog();
        for c in ['s', 'e', 'c', 'r', 'e', 't'] {
            dialog = stay(dialog.handle(press(c)));
        }
        let printed = format!("{dialog:?}");
        assert!(!printed.contains("secret"), "debug output is {printed:?}");
    }
}
