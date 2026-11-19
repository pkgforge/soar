//! The dialog that collects several values at once, accepted as a whole.

#![allow(dead_code)]

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    app::App,
    theme::Theme,
    widgets::{
        overlay::{OnAccept, Overlay, OverlayOutcome, Value, buttons, centered, frame},
        text_input::TextInput,
    },
};

/// One row of a form: a checkbox or a single line of text.
#[derive(Debug)]
pub(crate) enum Field {
    /// A toggle, shown as `[x]` or `[ ]`.
    Check {
        /// The name shown beside the box.
        label: &'static str,
        /// Whether the box is ticked.
        checked: bool,
    },
    /// A single line of text.
    Text {
        /// The name shown beside the value.
        label: &'static str,
        /// The editable value.
        input: TextInput,
    },
    /// A single line of text the form refuses to accept while it is
    /// blank or only whitespace.
    RequiredText {
        /// The name shown beside the value.
        label: &'static str,
        /// The editable value.
        input: TextInput,
    },
}

/// The values of every field of an accepted form, in field order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FormValues(pub(crate) Vec<FieldValue>);

/// The value of one form field.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FieldValue {
    /// The text of a text field.
    Text(String),
    /// The state of a checkbox.
    Check(bool),
}

/// A dialog with checkboxes and text fields, accepted with its two buttons.
/// The focus walks over the fields and then the buttons.
#[derive(Debug)]
pub(crate) struct Form {
    title: String,
    fields: Vec<Field>,
    focus: usize,
    action_label: String,
    on_accept: OnAccept,
    /// Why the last accept was refused, shown in danger above the
    /// buttons. Editing any text field drops it again.
    error: Option<String>,
}

impl Form {
    /// Builds a form with the focus on its first field.
    pub(crate) fn new(
        title: impl Into<String>,
        fields: Vec<Field>,
        action_label: impl Into<String>,
        on_accept: OnAccept,
    ) -> Form {
        Form {
            title: title.into(),
            fields,
            focus: 0,
            action_label: action_label.into(),
            on_accept,
            error: None,
        }
    }

    /// The focus position of the action button, after the fields and the
    /// cancel button.
    fn last_focus(&self) -> usize {
        self.fields.len() + 1
    }

    /// Acts on one pressed key. Tab and the arrows move the focus, space
    /// toggles a checkbox, other keys edit a focused text field, enter
    /// presses a button or moves to the next field, and esc closes.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => return OverlayOutcome::Close,
            (KeyCode::Tab | KeyCode::Down, _) => {
                self.focus = (self.focus + 1).min(self.last_focus());
            }
            (KeyCode::BackTab | KeyCode::Up, _) => self.focus = self.focus.saturating_sub(1),
            (KeyCode::Enter, _) => {
                if self.focus == self.last_focus() {
                    if let Some((position, message)) = self.missing_required() {
                        self.focus = position;
                        self.error = Some(message);
                        return OverlayOutcome::Stay(Overlay::Form(self));
                    }
                    let values = self.values();
                    return OverlayOutcome::Accept(self.on_accept, Some(Value::Form(values)));
                }
                if self.focus == self.fields.len() {
                    return OverlayOutcome::Close;
                }
                self.focus = (self.focus + 1).min(self.last_focus());
            }
            (KeyCode::Char(' '), KeyModifiers::NONE | KeyModifiers::SHIFT)
                if matches!(self.fields.get(self.focus), Some(Field::Check { .. })) =>
            {
                if let Some(Field::Check {
                    checked, ..
                }) = self.fields.get_mut(self.focus)
                {
                    *checked = !*checked;
                }
            }
            _ => {
                let edited = match self.fields.get_mut(self.focus) {
                    Some(
                        Field::Text {
                            input, ..
                        }
                        | Field::RequiredText {
                            input, ..
                        },
                    ) => input.handle(key),
                    _ => false,
                };
                if edited {
                    self.error = None;
                }
            }
        }
        OverlayOutcome::Stay(Overlay::Form(self))
    }

    /// The first required field left blank, with the message naming it.
    fn missing_required(&self) -> Option<(usize, String)> {
        self.fields
            .iter()
            .enumerate()
            .find_map(|(position, field)| {
                match field {
                    Field::RequiredText {
                        label,
                        input,
                    } if input.value().trim().is_empty() => {
                        Some((position, format!("{label} is required")))
                    }
                    _ => None,
                }
            })
    }

    /// The values of every field, in field order.
    fn values(&self) -> FormValues {
        FormValues(
            self.fields
                .iter()
                .map(|field| {
                    match field {
                        Field::Text {
                            input, ..
                        }
                        | Field::RequiredText {
                            input, ..
                        } => FieldValue::Text(input.value().to_owned()),
                        Field::Check {
                            checked, ..
                        } => FieldValue::Check(*checked),
                    }
                })
                .collect(),
        )
    }

    /// Draws the form centered in the body area, and returns the rect it
    /// drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(64);
        let rows = self.fields.len() + usize::from(self.error.is_some());
        let rect = centered(area, width, rows as u16 + 4);
        f.render_widget(Clear, rect);

        // The block's border and padding take two columns on each side,
        // so the field rows are budgeted the body that is left.
        let content = usize::from(rect.width).saturating_sub(4);
        let mut lines = Vec::with_capacity(rows + 2);
        for (position, field) in self.fields.iter().enumerate() {
            lines.push(self.field_line(field, position == self.focus, content, theme));
        }
        lines.push(Line::raw(""));
        if let Some(error) = &self.error {
            lines.push(Line::styled(error.clone(), theme.danger()));
        }
        let buttons = buttons(
            &self.action_label,
            self.focus == self.fields.len(),
            self.focus == self.last_focus(),
            false,
            theme,
        )
        .right_aligned();
        lines.push(buttons);

        let block = frame(
            Line::styled(format!(" {} ", self.title), theme.title()),
            None,
            false,
            theme,
        );
        f.render_widget(Paragraph::new(lines).block(block), rect);
        rect
    }

    /// One field as a row, the focused one highlighted. `content` is the
    /// dialog body's width in columns, which the row's value is budgeted
    /// after the row's own furniture.
    fn field_line(
        &self,
        field: &Field,
        focused: bool,
        content: usize,
        theme: &Theme,
    ) -> Line<'static> {
        let mut line = match field {
            Field::Check {
                label,
                checked,
            } => {
                let mark = if *checked { "[x]" } else { "[ ]" };
                Line::from(vec![
                    Span::raw("  "),
                    Span::raw(mark),
                    Span::raw(" "),
                    Span::raw(*label),
                ])
            }
            Field::Text {
                label,
                input,
            }
            | Field::RequiredText {
                label,
                input,
            } => {
                let mut spans = vec![Span::raw("      "), Span::raw(*label), Span::raw("  ")];
                // The indent, label, and gap take their columns of the
                // dialog body first, so a long value keeps its tail,
                // where the cursor is, in view.
                let used = 6 + Span::raw(*label).width() + 2;
                let room = content.saturating_sub(used);
                spans.extend(input.line(focused, room, theme).spans);
                Line::from(spans)
            }
        };
        if focused {
            line = line.style(theme.cursor_row());
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Field, FieldValue, Form, FormValues};
    use crate::{
        test_support::{screen, test_app},
        widgets::{
            overlay::{OnAccept, Overlay, OverlayOutcome, Value},
            text_input::TextInput,
        },
    };

    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers::NONE)
    }

    fn form() -> Form {
        Form::new(
            "Install bat with options",
            vec![
                Field::Check {
                    label: "Force reinstall",
                    checked: false,
                },
                Field::Check {
                    label: "Binary only",
                    checked: false,
                },
                Field::Text {
                    label: "Portable path",
                    input: TextInput::default(),
                },
            ],
            "Install",
            OnAccept::Test,
        )
    }

    fn stay(outcome: OverlayOutcome) -> Form {
        match outcome {
            OverlayOutcome::Stay(Overlay::Form(form)) => form,
            other => panic!("expected the form to stay open, got {other:?}"),
        }
    }

    #[test]
    fn toggling_a_checkbox_then_accepting_returns_its_value() {
        let mut form = stay(form().handle(press(KeyCode::Char(' '))));
        for _ in 0..4 {
            form = stay(form.handle(press(KeyCode::Tab)));
        }
        assert!(matches!(
            form.handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Form(FormValues(values))))
                if values == vec![
                    FieldValue::Check(true),
                    FieldValue::Check(false),
                    FieldValue::Text(String::new()),
                ]
        ));
    }

    #[test]
    fn enter_on_a_field_moves_to_the_next_one() {
        let form = stay(form().handle(press(KeyCode::Enter)));
        assert_eq!(form.focus, 1);
        let form = stay(form.handle(press(KeyCode::Enter)));
        assert_eq!(form.focus, 2);
    }

    #[test]
    fn esc_closes() {
        assert!(matches!(
            form().handle(press(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn text_keys_reach_the_focused_text_field() {
        let form = stay(form().handle(press(KeyCode::Tab)));
        let form = stay(form.handle(press(KeyCode::Tab)));
        let form = stay(form.handle(press(KeyCode::Char('x'))));
        assert_eq!(form.focus, 2);
        let form = stay(form.handle(press(KeyCode::Enter)));
        let form = stay(form.handle(press(KeyCode::Tab)));
        assert!(matches!(
            form.handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Form(FormValues(values))))
                if values == vec![
                    FieldValue::Check(false),
                    FieldValue::Check(false),
                    FieldValue::Text("x".to_string()),
                ]
        ));
    }

    #[test]
    fn the_form_shows_its_fields_and_buttons() {
        let mut app = test_app();
        let form = stay(form().handle(press(KeyCode::Char(' '))));
        app.overlay = Some(Overlay::Form(form));
        let screen = screen(&mut app, 80, 24);
        for text in [
            "Install bat with options",
            "[x]",
            "[ ]",
            "[ Cancel ]",
            "[ Install ]",
        ] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }

    #[test]
    fn a_field_row_stays_within_the_body_width_it_is_given() {
        let theme = crate::theme::Theme::new(true, true);
        let mut input = TextInput::default();
        input.set("x".repeat(200));
        let form = Form::new(
            "Install bat with options",
            vec![Field::Text {
                label: "Portable path",
                input,
            }],
            "Install",
            OnAccept::Test,
        );
        let line = form.field_line(&form.fields[0], false, 60, &theme);
        let width: usize = line.spans.iter().map(|span| span.width()).sum();
        assert!(width <= 60, "row is {width} columns, body is 60");
        let line = form.field_line(&form.fields[0], false, 40, &theme);
        let width: usize = line.spans.iter().map(|span| span.width()).sum();
        assert!(width <= 40, "row is {width} columns, body is 40");
    }

    /// A form with two required text fields and one checkbox.
    fn required_form() -> Form {
        Form::new(
            "Add repository",
            vec![
                Field::RequiredText {
                    label: "Name",
                    input: TextInput::default(),
                },
                Field::RequiredText {
                    label: "URL",
                    input: TextInput::default(),
                },
                Field::Check {
                    label: "Enabled",
                    checked: true,
                },
            ],
            "Add",
            OnAccept::Test,
        )
    }

    #[test]
    fn accepting_with_an_empty_required_field_stays_and_sets_the_error() {
        let mut form = required_form();
        for _ in 0..4 {
            form = stay(form.handle(press(KeyCode::Tab)));
        }
        assert_eq!(form.focus, 4);
        let form = stay(form.handle(press(KeyCode::Enter)));
        assert_eq!(form.focus, 0);
        assert_eq!(form.error.as_deref(), Some("Name is required"));
    }

    #[test]
    fn filling_the_required_fields_then_accepting_returns_their_values() {
        let mut form = required_form();
        for c in "x".chars() {
            form = stay(form.handle(press(KeyCode::Char(c))));
        }
        form = stay(form.handle(press(KeyCode::Tab)));
        for c in "https://example.com/m.json".chars() {
            form = stay(form.handle(press(KeyCode::Char(c))));
        }
        for _ in 0..3 {
            form = stay(form.handle(press(KeyCode::Tab)));
        }
        assert_eq!(form.focus, 4);
        assert!(matches!(
            form.handle(press(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Form(FormValues(values))))
                if values == vec![
                    FieldValue::Text("x".to_string()),
                    FieldValue::Text("https://example.com/m.json".to_string()),
                    FieldValue::Check(true),
                ]
        ));
    }

    #[test]
    fn editing_a_field_clears_the_error_again() {
        let mut form = required_form();
        for _ in 0..4 {
            form = stay(form.handle(press(KeyCode::Tab)));
        }
        let form = stay(form.handle(press(KeyCode::Enter)));
        assert!(form.error.is_some());
        let form = stay(form.handle(press(KeyCode::Char('a'))));
        assert!(form.error.is_none());
    }

    #[test]
    fn the_form_shows_the_error_in_danger_above_the_buttons() {
        let mut app = test_app();
        let mut form = required_form();
        for _ in 0..4 {
            form = stay(form.handle(press(KeyCode::Tab)));
        }
        let form = stay(form.handle(press(KeyCode::Enter)));
        app.overlay = Some(Overlay::Form(form));
        let screen = screen(&mut app, 80, 24);
        assert!(screen.contains("Name is required"), "screen is {screen:?}");
    }
}
