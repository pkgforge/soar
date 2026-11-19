//! The placeholders a view shows while its content loads, when it has
//! nothing to show, or when loading failed.
//!
//! The views draw these once they hold their own state.

#![allow(dead_code)]

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::{Paragraph, Wrap},
};

use crate::{format, theme::Theme};

/// Draws `{spinner} {text}` while the view's data loads.
pub(crate) fn loading(f: &mut Frame, area: Rect, theme: &Theme, frame: u64, text: &str) {
    let line = one_line(
        theme,
        &format!("{} {text}", theme.spinner(frame)),
        theme.muted(),
        area.width,
    );
    centered(f, area, vec![line], false);
}

/// Draws the text in muted when the view has nothing to show.
pub(crate) fn empty(f: &mut Frame, area: Rect, theme: &Theme, text: &str) {
    let line = one_line(theme, text, theme.muted(), area.width);
    centered(f, area, vec![line], false);
}

/// Draws the failure message in the danger color and the retry hint in
/// muted under it, wrapping a long message across the area.
pub(crate) fn error(f: &mut Frame, area: Rect, theme: &Theme, text: &str) {
    let lines = vec![
        Line::styled(format!("{} {text}", theme.sym().error), theme.danger()),
        Line::styled("ctrl-r to retry".to_string(), theme.muted()),
    ];
    centered(f, area, lines, true);
}

/// Draws `{installed} {text}` in the success color when an action finished.
pub(crate) fn success(f: &mut Frame, area: Rect, theme: &Theme, text: &str) {
    let line = one_line(
        theme,
        &format!("{} {text}", theme.sym().installed),
        theme.success(),
        area.width,
    );
    centered(f, area, vec![line], false);
}

/// One line cut to `width` columns, so long text never overflows the area.
fn one_line(theme: &Theme, text: &str, style: Style, width: u16) -> Line<'static> {
    Line::styled(
        format::truncate(text, usize::from(width), theme.sym().ellipsis),
        style,
    )
}

/// Draws the lines vertically and horizontally centered in the area,
/// wrapping them across the width when asked. Content taller than the area
/// starts at its top and clips at the bottom.
fn centered(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>, wrap: bool) {
    let rows = visible_rows(area, &lines, wrap).min(usize::from(area.height));
    let top = ((usize::from(area.height) - rows) / 2) as u16;
    let rect = Rect {
        y: area.y + top,
        height: area.height - top,
        ..area
    };
    let mut paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    if wrap {
        paragraph = paragraph.wrap(Wrap {
            trim: true,
        });
    }
    f.render_widget(paragraph, rect);
}

/// The rows the lines occupy, spreading each line across the area width
/// when it wraps.
fn visible_rows(area: Rect, lines: &[Line<'_>], wrap: bool) -> usize {
    let width = usize::from(area.width).max(1);
    lines
        .iter()
        .map(|line| {
            if wrap {
                line.width().div_ceil(width).max(1)
            } else {
                1
            }
        })
        .sum()
}
