//! The status bar: key hints on the left, the view's message on the right.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};

use crate::{action::Action, app::App, format, keymap};

/// Draws the hints for the keys that work in the active view, then the
/// message the view has set, right-aligned in what the hints left.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &App) {
    let row = Rect {
        height: 1,
        ..area
    };
    let message = app.status_right();
    // The hints hold back room for the message, though at most a third
    // of the bar, so a long message cannot hide every hint.
    let reserve = if message.is_empty() {
        0
    } else {
        (Span::raw(&message).width() + 1).min(usize::from(row.width) / 3)
    };
    let hints = hint_spans(app, usize::from(row.width), reserve);
    let used = Line::from(hints.clone()).width();
    f.render_widget(Line::from(hints), row);
    if !message.is_empty() {
        // The message takes what the hints left, so the two sides never
        // overwrite each other.
        let room = usize::from(row.width).saturating_sub(used + 1);
        let message = format::truncate(&message, room, app.theme.sym().ellipsis);
        if !message.is_empty() {
            f.render_widget(
                Line::from(Span::styled(message, app.theme.muted())).right_aligned(),
                row,
            );
        }
    }
}

/// The left side of the bar: one hint per key binding the view shows on the
/// bar, followed by help and quit. Hints stop once the next one would not
/// fit, `reserve` columns included.
fn hint_spans(app: &App, width: usize, reserve: usize) -> Vec<Span<'static>> {
    let theme = &app.theme;
    let bindings = keymap::bindings(app.view)
        .iter()
        .filter(|binding| binding.bar && app.hint_visible(&binding.action))
        .chain(
            keymap::GLOBAL
                .iter()
                .filter(|binding| matches!(binding.action, Action::Help | Action::Quit)),
        );
    let mut spans = Vec::new();
    let mut used = 1 + reserve;
    let mut first = true;
    for binding in bindings {
        let key = keymap::key_label(binding.keys[0]);
        let suffix = app.hint_suffix(&binding.action);
        let suffix_width = suffix.as_deref().map_or(0, |text| Span::raw(text).width());
        let separator = if first { 0 } else { 2 };
        let cost = separator
            + Span::raw(key.as_str()).width()
            + 1
            + Span::raw(binding.label).width()
            + suffix_width;
        if used + cost > width && !first {
            break;
        }
        spans.push(Span::raw(if first { " " } else { "  " }));
        spans.push(Span::styled(key, theme.key()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(binding.label, theme.muted()));
        if let Some(suffix) = suffix {
            spans.push(Span::styled(suffix, theme.muted()));
        }
        used += cost;
        first = false;
    }
    spans
}

#[cfg(test)]
mod tests {
    use crate::test_support::{render, test_app};

    #[test]
    fn the_browse_bar_shows_filter_and_help() {
        let mut app = test_app();
        let lines = render(&mut app, 100, 16);
        let bar = lines.last().unwrap();
        assert!(bar.contains("/ filter"), "bar is {bar:?}");
        assert!(bar.contains("? help"), "bar is {bar:?}");
    }

    #[test]
    fn hints_stop_when_the_row_is_full() {
        let mut app = test_app();
        let lines = render(&mut app, 60, 16);
        let bar = lines.last().unwrap();
        assert!(bar.contains("/ filter"), "bar is {bar:?}");
        assert!(bar.contains("+ add"), "bar is {bar:?}");
        assert!(!bar.contains("R repo"), "bar is {bar:?}");
    }

    #[test]
    fn the_message_and_the_hints_share_the_row() {
        use crate::{
            msg::{Loaded, Msg},
            test_support::{catalog_row, key},
        };
        let mut app = test_app();
        let now = std::time::Instant::now();
        let rows = vec![
            catalog_row("bat", "0.24.0", "bincache"),
            catalog_row("fd", "10.2.0", "pkgforge"),
        ];
        app.update(Msg::Loaded(Loaded::Catalog(Ok(rows))), now);
        app.update(key(' '), now);
        let lines = render(&mut app, 73, 20);
        let bar = lines.last().unwrap();
        assert!(bar.contains("+ add"), "bar is {bar:?}");
        assert!(bar.contains("1 marked"), "bar is {bar:?}");
        assert!(
            !bar.contains("R repo"),
            "a hint the message would run over is dropped, bar is {bar:?}"
        );
    }
}
