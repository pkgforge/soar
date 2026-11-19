//! The single-line text input and the prompt dialog built on it.

#![allow(dead_code)]

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    app::App,
    theme::Theme,
    widgets::overlay::{OnAccept, Overlay, OverlayOutcome, Value, centered, frame},
};

/// One editable line of text with the cursor as a character index.
///
/// The value never appears in debug output, so an input holding a
/// password cannot leak it through a log or a panic message.
#[derive(Default)]
pub(crate) struct TextInput {
    value: String,
    cursor: usize,
    /// Whether every character draws as the theme's mask symbol.
    masked: bool,
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("value", &"[hidden]")
            .field("cursor", &self.cursor)
            .field("masked", &self.masked)
            .finish()
    }
}

impl Drop for TextInput {
    /// Zeroes the value before it returns to the allocator, so a typed
    /// password leaves memory with the secret it became. Bytes past the
    /// length and copies made by earlier reallocations can still outlive
    /// this, so the zeroing is best effort.
    fn drop(&mut self) {
        scrub(std::mem::take(&mut self.value));
    }
}

impl TextInput {
    /// Feeds one pressed key into the value and reports whether the input
    /// used it.
    pub(crate) fn handle(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char(c), mods)
                if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(c);
            }
            (KeyCode::Backspace, _) => self.backspace(),
            (KeyCode::Delete, _) => self.delete(),
            (KeyCode::Left, _) => self.cursor = self.cursor.saturating_sub(1),
            (KeyCode::Right, _) => self.cursor = (self.cursor + 1).min(self.char_len()),
            (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => self.cursor = 0,
            (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.cursor = self.char_len();
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => self.clear_to_start(),
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => self.delete_previous_word(),
            _ => return false,
        }
        true
    }

    /// The text collected so far.
    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    /// Replaces the text and puts the cursor after it.
    pub(crate) fn set(&mut self, value: String) {
        self.cursor = value.chars().count();
        scrub(std::mem::replace(&mut self.value, value));
    }

    /// Removes and returns the whole text, leaving the input empty.
    pub(crate) fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.value)
    }

    /// Empties the text.
    pub(crate) fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Whether nothing is entered yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Draws every character as the mask symbol instead of the value,
    /// for passwords.
    pub(crate) fn masked(mut self) -> TextInput {
        self.masked = true;
        self
    }

    /// The value as one line cut to `room` columns, with a reversed cell
    /// marking the cursor while focused. When the value overflows, the
    /// window keeps its tail and the cursor in view.
    pub(crate) fn line(&self, focused: bool, room: usize, theme: &Theme) -> Line<'static> {
        let chars: Vec<char> = if self.masked {
            self.value
                .chars()
                .flat_map(|_| theme.sym().mask.chars())
                .collect()
        } else {
            self.value.chars().collect()
        };
        let (start, end) = visible_window(&chars, self.cursor.min(chars.len()), room);
        let cursor = self.cursor.min(chars.len()) - start;
        let visible = &chars[start..end];
        let mut spans = Vec::new();
        if cursor > 0 {
            spans.push(Span::raw(visible[..cursor].iter().collect::<String>()));
        }
        if focused {
            match visible.get(cursor) {
                Some(&c) => {
                    let cell = Style::new().add_modifier(Modifier::REVERSED);
                    spans.push(Span::styled(c.to_string(), cell));
                    if cursor + 1 < visible.len() {
                        spans.push(Span::raw(visible[cursor + 1..].iter().collect::<String>()));
                    }
                }
                None => {
                    let cell = Style::new().add_modifier(Modifier::REVERSED);
                    spans.push(Span::styled(" ", cell));
                }
            }
        } else if cursor < visible.len() {
            spans.push(Span::raw(visible[cursor..].iter().collect::<String>()));
        }
        Line::from(spans).style(theme.text())
    }

    /// The number of characters in the value.
    fn char_len(&self) -> usize {
        self.value.chars().count()
    }

    /// Puts one character at the cursor.
    fn insert(&mut self, c: char) {
        let byte = byte_index(&self.value, self.cursor);
        self.value.insert(byte, c);
        self.cursor += 1;
    }

    /// Removes the character before the cursor.
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = byte_index(&self.value, self.cursor - 1);
        let end = byte_index(&self.value, self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Removes the character at the cursor.
    fn delete(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }
        let start = byte_index(&self.value, self.cursor);
        let end = byte_index(&self.value, self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    /// Removes everything before the cursor.
    fn clear_to_start(&mut self) {
        let byte = byte_index(&self.value, self.cursor);
        self.value.replace_range(..byte, "");
        self.cursor = 0;
    }

    /// Removes the word before the cursor, stopping at the space that
    /// separates it.
    fn delete_previous_word(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut end = self.cursor.min(chars.len());
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        if start == end {
            return;
        }
        let kept: String = chars[..start].iter().chain(chars[end..].iter()).collect();
        scrub(std::mem::replace(&mut self.value, kept));
        self.cursor = start;
    }
}

/// Zeroes a discarded value's bytes and black-boxes them, so password
/// bytes do not return to the allocator readable.
fn scrub(value: String) {
    let mut bytes = value.into_bytes();
    bytes.fill(0);
    std::hint::black_box(&bytes);
}

/// The byte offset of the character `chars` characters into `text`.
fn byte_index(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

/// The range of characters that fits `room` display columns, chosen so
/// the cursor stays in view: the tail while the cursor sits at the end,
/// and a stretch from the cursor once it moves back into the cut part.
fn visible_window(chars: &[char], cursor: usize, room: usize) -> (usize, usize) {
    let width = |index: usize| Span::raw(chars[index].to_string()).width();
    let total: usize = (0..chars.len()).map(width).sum();
    if total <= room {
        return (0, chars.len());
    }
    let mut end = chars.len();
    let mut used = 0;
    while end > 0 {
        let columns = width(end - 1);
        if used + columns > room {
            break;
        }
        used += columns;
        end -= 1;
    }
    if cursor < end {
        let mut window_end = cursor;
        let mut used = 0;
        while window_end < chars.len() {
            let columns = width(window_end);
            if used + columns > room {
                break;
            }
            used += columns;
            window_end += 1;
        }
        return (cursor, window_end);
    }
    (end, chars.len())
}

/// A dialog asking for one line of text, such as a name or a path.
#[derive(Debug)]
pub(crate) struct Prompt {
    title: String,
    input: TextInput,
    on_accept: OnAccept,
    /// Whether enter accepts an empty input instead of keeping the
    /// prompt open.
    allow_empty: bool,
}

impl Prompt {
    /// Builds a prompt starting from an empty input.
    pub(crate) fn new(title: impl Into<String>, on_accept: OnAccept) -> Prompt {
        Prompt {
            title: title.into(),
            input: TextInput::default(),
            on_accept,
            allow_empty: false,
        }
    }

    /// Lets enter accept an empty input, whose meaning the caller
    /// defines. The footer tells the user how the text is interpreted.
    pub(crate) fn allow_empty(mut self) -> Prompt {
        self.allow_empty = true;
        self
    }

    /// Acts on one pressed key. Enter accepts the trimmed text and stays
    /// open while it is empty, unless empty input is allowed, esc closes,
    /// and everything else edits the text.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        match key.code {
            KeyCode::Enter => {
                let text = self.input.value().trim();
                if text.is_empty() && !self.allow_empty {
                    OverlayOutcome::Stay(Overlay::Prompt(self))
                } else {
                    OverlayOutcome::Accept(self.on_accept, Some(Value::Text(text.to_owned())))
                }
            }
            KeyCode::Esc => OverlayOutcome::Close,
            _ => {
                self.input.handle(key);
                OverlayOutcome::Stay(Overlay::Prompt(self))
            }
        }
    }

    /// Draws the prompt centered in the body area, and returns the rect
    /// it drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(64);
        let rect = centered(area, width, 3);
        f.render_widget(Clear, rect);

        let mut spans = vec![Span::styled("> ", theme.muted())];
        // The frame's border and padding take four columns and the
        // marker two, so the value gets what is left, keeping its tail,
        // where the cursor is, in view.
        let room = usize::from(width).saturating_sub(6);
        spans.extend(self.input.line(true, room, theme).spans);
        let footer = if self.allow_empty {
            format!(
                " enter ok {} esc {} args are split on spaces ",
                theme.sym().dot,
                theme.sym().dot
            )
        } else {
            format!(" enter ok {} esc ", theme.sym().dot)
        };
        let block = frame(
            Line::styled(format!(" {} ", self.title), theme.title()),
            Some(Line::styled(footer, theme.muted())),
            false,
            theme,
        );
        f.render_widget(Paragraph::new(vec![Line::from(spans)]).block(block), rect);
        rect
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Prompt, TextInput};
    use crate::{
        test_support::{screen, test_app},
        widgets::overlay::{OnAccept, Overlay, OverlayOutcome, Value},
    };

    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn press_key(key: KeyCode) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn typed(input: &TextInput) -> String {
        input.value().to_string()
    }

    #[test]
    fn typing_then_moving_back_inserts_in_place() {
        let mut input = TextInput::default();
        for c in ['a', 'b', 'c'] {
            assert!(input.handle(press(c)));
        }
        assert!(input.handle(press_key(KeyCode::Left)));
        assert!(input.handle(press('x')));
        assert_eq!(typed(&input), "abxc");
    }

    #[test]
    fn ctrl_w_removes_the_previous_word() {
        let mut input = TextInput::default();
        input.set("foo bar".to_string());
        assert!(input.handle(ctrl('w')));
        assert_eq!(typed(&input), "foo ");
    }

    #[test]
    fn backspace_removes_a_multibyte_character() {
        let mut input = TextInput::default();
        assert!(input.handle(press('é')));
        assert_eq!(typed(&input), "é");
        assert!(input.handle(press_key(KeyCode::Backspace)));
        assert_eq!(typed(&input), "");
        assert!(input.is_empty());
    }

    #[test]
    fn ctrl_u_clears_to_the_start() {
        let mut input = TextInput::default();
        input.set("hello world".to_string());
        assert!(input.handle(ctrl('u')));
        assert_eq!(typed(&input), "");
    }

    #[test]
    fn home_and_end_reach_the_ends() {
        let mut input = TextInput::default();
        input.set("hello".to_string());
        assert!(input.handle(press_key(KeyCode::Home)));
        assert!(input.handle(press('x')));
        assert_eq!(typed(&input), "xhello");
        assert!(input.handle(ctrl('e')));
        assert!(input.handle(press('y')));
        assert_eq!(typed(&input), "xhelloy");
    }

    #[test]
    fn keys_outside_the_input_are_not_consumed() {
        let mut input = TextInput::default();
        assert!(!input.handle(press_key(KeyCode::PageDown)));
    }

    #[test]
    fn a_masked_input_draws_one_mask_per_character() {
        let theme = crate::theme::Theme::new(true, true);
        let mut input = TextInput::default().masked();
        for c in ['a', 'b', 'c'] {
            input.handle(press(c));
        }
        let line = input.line(true, 10, &theme);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "••• ");
        assert!(!text.contains("abc"));
        let line = input.line(false, 10, &theme);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "•••");
    }

    #[test]
    fn an_unmasked_input_still_shows_its_value() {
        let mut input = TextInput::default();
        input.set("abc".to_string());
        let theme = crate::theme::Theme::new(true, true);
        let line = input.line(false, 10, &theme);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "abc");
    }

    #[test]
    fn a_value_wider_than_the_room_shows_its_tail() {
        let mut input = TextInput::default();
        input.set("0123456789".to_string());
        let theme = crate::theme::Theme::new(true, true);
        let line = input.line(false, 5, &theme);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "56789");
    }

    #[test]
    fn a_cursor_moved_back_keeps_its_own_stretch_in_view() {
        let mut input = TextInput::default();
        input.set("abcdefghij".to_string());
        input.handle(press_key(KeyCode::Home));
        let theme = crate::theme::Theme::new(true, true);
        let line = input.line(false, 4, &theme);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "abcd");
    }

    #[test]
    fn debugging_an_input_never_carries_the_value() {
        let mut input = TextInput::default().masked();
        input.set("hunter2".to_string());
        let printed = format!("{input:?}");
        assert!(!printed.contains("hunter2"), "debug output is {printed:?}");
    }

    #[test]
    fn a_masked_input_drops_with_multibyte_text() {
        // Observing freed memory cannot be done safely, so this only
        // guards Drop against panics with multibyte content.
        let mut input = TextInput::default().masked();
        input.set("välu€".to_string());
        drop(input);
    }

    #[test]
    fn take_empties_the_input_and_returns_the_text() {
        let mut input = TextInput::default();
        input.set("abc".to_string());
        assert_eq!(input.take(), "abc");
        assert!(input.is_empty());
    }

    #[test]
    fn enter_accepts_the_trimmed_text() {
        let mut prompt = Prompt::new("Install by name, URL, or path", OnAccept::Test);
        prompt.input.set("  fd  ".to_string());
        assert!(matches!(
            prompt.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Text(text))) if text == "fd"
        ));
    }

    #[test]
    fn enter_on_empty_text_stays_open() {
        let prompt = Prompt::new("Install by name, URL, or path", OnAccept::Test);
        assert!(matches!(
            prompt.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Stay(Overlay::Prompt(_))
        ));
    }

    #[test]
    fn esc_closes_the_prompt() {
        let prompt = Prompt::new("Install by name, URL, or path", OnAccept::Test);
        assert!(matches!(
            prompt.handle(press_key(KeyCode::Esc)),
            OverlayOutcome::Close
        ));
    }

    #[test]
    fn enter_accepts_empty_input_when_allowed() {
        let prompt = Prompt::new("Run jq with arguments", OnAccept::Test).allow_empty();
        assert!(matches!(
            prompt.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Text(text))) if text.is_empty()
        ));
    }

    #[test]
    fn enter_accepts_whitespace_only_text_as_empty_when_allowed() {
        let mut prompt = Prompt::new("Run jq with arguments", OnAccept::Test).allow_empty();
        prompt.input.set("   ".to_string());
        assert!(matches!(
            prompt.handle(press_key(KeyCode::Enter)),
            OverlayOutcome::Accept(OnAccept::Test, Some(Value::Text(text))) if text.is_empty()
        ));
    }

    #[test]
    fn the_allowed_empty_prompt_hints_the_split_in_its_footer() {
        let mut app = test_app();
        let prompt = Prompt::new("Run jq with arguments", OnAccept::Test).allow_empty();
        app.overlay = Some(Overlay::Prompt(prompt));
        let screen = screen(&mut app, 80, 24);
        assert!(
            screen.contains("args are split on spaces"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn the_prompt_shows_its_title_input_line_and_footer() {
        let mut app = test_app();
        let mut prompt = Prompt::new("Install by name, URL, or path", OnAccept::Test);
        prompt.input.set("https://github.com/user/app".to_string());
        app.overlay = Some(Overlay::Prompt(prompt));
        let screen = screen(&mut app, 80, 24);
        for text in ["Install by name", "> https://github.com", "enter ok · esc"] {
            assert!(screen.contains(text), "screen is {screen:?}");
        }
    }
}
