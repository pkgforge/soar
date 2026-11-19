//! The brief notifications stacked in the top-right corner of the body,
//! reporting what the interface just did.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::{app::App, format, theme::Theme};

/// How many toasts may show at once.
const MAX_TOASTS: usize = 3;

/// The widest a toast box gets, whatever its text says.
const MAX_WIDTH: usize = 48;

/// The rows one toast box occupies: two borders around one message.
const HEIGHT: u16 = 3;

/// The kinds of toast, deciding the icon, the border color, and how long
/// the toast stays. The views raise toasts as their actions run, so until
/// then only tests construct the kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ToastKind {
    /// Something finished well.
    Success,
    /// Neutral information.
    Info,
    /// A note worth reading.
    Warning,
    /// Something failed.
    Error,
}

impl ToastKind {
    /// The glyph marking this kind of toast.
    fn symbol(self, theme: &Theme) -> &'static str {
        let symbols = theme.sym();
        match self {
            ToastKind::Success => symbols.installed,
            ToastKind::Info => symbols.dot,
            ToastKind::Warning => symbols.warning,
            ToastKind::Error => symbols.error,
        }
    }

    /// The style of the icon and border for this kind.
    fn style(self, theme: &Theme) -> Style {
        match self {
            ToastKind::Success => theme.success(),
            ToastKind::Info => theme.info(),
            ToastKind::Warning => theme.warning(),
            ToastKind::Error => theme.danger(),
        }
    }

    /// How long the toast stays on screen.
    fn lifetime(self) -> Duration {
        match self {
            ToastKind::Error => Duration::from_secs(8),
            ToastKind::Success | ToastKind::Info | ToastKind::Warning => Duration::from_secs(4),
        }
    }
}

/// One brief notification.
#[derive(Debug)]
pub(crate) struct Toast {
    kind: ToastKind,
    text: String,
    /// The time the toast stops showing.
    until: Instant,
}

/// The toasts on screen, newest first, at most three.
#[derive(Debug, Default)]
pub(crate) struct Toasts(VecDeque<Toast>);

impl Toasts {
    /// Shows a toast on top, dropping the oldest one beyond three. Toasts
    /// stay four seconds, or eight when they report a failure.
    pub(crate) fn push(&mut self, kind: ToastKind, text: impl Into<String>, now: Instant) {
        if self.0.len() >= MAX_TOASTS {
            self.0.pop_back();
        }
        self.0.push_front(Toast {
            kind,
            text: text.into(),
            until: now + kind.lifetime(),
        });
    }

    /// Removes the toasts past their lifetime and reports whether any went.
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let before = self.0.len();
        self.0.retain(|toast| toast.until > now);
        self.0.len() != before
    }

    /// The toasts from newest to oldest.
    pub(crate) fn iter(&self) -> std::collections::vec_deque::Iter<'_, Toast> {
        self.0.iter()
    }
}

/// Draws the toasts stacked from the top-right corner of the body area,
/// newest on top, each a rounded box sized to its message and colored by
/// its kind. Toasts never take input.
pub(crate) fn render(f: &mut Frame, body: Rect, app: &App) {
    let theme = &app.theme;
    let mut used: u16 = 0;
    for toast in app.toasts.iter() {
        if used + HEIGHT > body.height {
            break;
        }
        let text_width = Span::raw(toast.text.as_str()).width();
        let width = (text_width + 4).min(MAX_WIDTH).min(usize::from(body.width)) as u16;
        let rect = Rect {
            x: body.x + body.width - width,
            y: body.y + used,
            width,
            height: HEIGHT,
        };
        used += HEIGHT;

        let symbol = toast.kind.symbol(theme);
        let inner = usize::from(width).saturating_sub(2);
        let budget = inner.saturating_sub(Span::raw(symbol).width() + 1);
        let text = format::truncate(&toast.text, budget, theme.sym().ellipsis);
        let line = Line::from(vec![
            Span::styled(symbol, toast.kind.style(theme)),
            Span::raw(" "),
            Span::raw(text),
        ]);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(toast.kind.style(theme));
        f.render_widget(Paragraph::new(line).block(block), rect);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ToastKind, Toasts};
    use crate::test_support::{render, test_app};

    fn texts(toasts: &Toasts) -> Vec<String> {
        toasts.iter().map(|toast| toast.text.clone()).collect()
    }

    #[test]
    fn pushing_past_three_drops_the_oldest() {
        let mut toasts = Toasts::default();
        let now = Instant::now();
        toasts.push(ToastKind::Info, "first", now);
        toasts.push(ToastKind::Info, "second", now);
        toasts.push(ToastKind::Info, "third", now);
        toasts.push(ToastKind::Info, "fourth", now);
        assert_eq!(texts(&toasts), ["fourth", "third", "second"]);
    }

    #[test]
    fn an_error_outlives_other_toasts() {
        let mut toasts = Toasts::default();
        let now = Instant::now();
        toasts.push(ToastKind::Warning, "a note", now);
        toasts.push(ToastKind::Error, "the sync failed", now);
        toasts.expire(now + Duration::from_secs(5));
        assert_eq!(texts(&toasts), ["the sync failed"]);
    }

    #[test]
    fn a_toast_leaves_after_its_lifetime() {
        let mut toasts = Toasts::default();
        let now = Instant::now();
        toasts.push(ToastKind::Info, "installed fd", now);
        assert!(!toasts.expire(now + Duration::from_secs(3)));
        assert!(toasts.expire(now + Duration::from_secs(5)));
        assert!(texts(&toasts).is_empty());
    }

    #[test]
    fn the_newest_toast_draws_on_top_at_the_right_edge() {
        let mut app = test_app();
        let now = Instant::now();
        app.toast(ToastKind::Info, "older toast", now);
        app.toast(ToastKind::Success, "Installed fd 10.2.0", now);
        let lines = render(&mut app, 80, 24);
        assert!(
            lines[2].contains("Installed fd 10.2.0"),
            "top toast row is {:?}",
            lines[2]
        );
        assert!(
            lines[5].contains("older toast"),
            "second toast row is {:?}",
            lines[5]
        );
        assert!(lines[2].ends_with('│'), "top toast row is {:?}", lines[2]);
    }

    #[test]
    fn a_long_message_fits_the_widest_toast() {
        let mut app = test_app();
        app.toast(
            ToastKind::Error,
            "very long failure ".repeat(6),
            Instant::now(),
        );
        let lines = render(&mut app, 80, 24);
        let row = &lines[2];
        assert!(row.ends_with("…│"), "top toast row is {row:?}");
        let start = row.chars().count() - 48;
        let box_row: String = row.chars().skip(start).collect();
        assert_eq!(box_row.chars().count(), 48, "top toast row is {row:?}");
        assert!(box_row.starts_with('│'), "top toast row is {row:?}");
    }
}
