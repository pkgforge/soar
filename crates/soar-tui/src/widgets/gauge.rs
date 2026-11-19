//! The progress bar drawn as a row of gauge glyphs.
//!
//! The views draw it once they hold their own progress.

#![allow(dead_code)]

use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// One row of progress `width` columns wide, filled to `ratio` of its
/// length. The ratio clamps to 0..=1 and one that is not a number renders
/// the bar empty.
pub(crate) fn line(ratio: f64, width: u16, theme: &Theme) -> Line<'static> {
    let symbols = theme.sym();
    let filled = ((ratio * f64::from(width)).round() as usize).min(usize::from(width));
    let mut spans = Vec::with_capacity(2);
    if filled > 0 {
        spans.push(Span::styled(
            symbols.gauge_fill.repeat(filled),
            theme.accent(),
        ));
    }
    let rest = usize::from(width) - filled;
    if rest > 0 {
        spans.push(Span::styled(
            symbols.gauge_empty.repeat(rest),
            theme.muted(),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::line;
    use crate::theme::Theme;

    #[test]
    fn a_half_full_gauge_fills_half_the_width() {
        let theme = Theme::new(true, true);
        let symbols = theme.sym();
        let line = line(0.5, 10, &theme);
        assert_eq!(line.width(), 10);
        assert_eq!(
            line.spans[0].content.to_string(),
            symbols.gauge_fill.repeat(5)
        );
        assert_eq!(
            line.spans[1].content.to_string(),
            symbols.gauge_empty.repeat(5)
        );
        assert!(line.spans[0].style.fg.is_some());
        assert!(line.spans[1].style.fg.is_some());
    }

    #[test]
    fn a_ratio_above_one_fills_the_width() {
        let theme = Theme::new(true, true);
        let line = line(2.0, 10, &theme);
        assert_eq!(line.width(), 10);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].content.to_string(),
            theme.sym().gauge_fill.repeat(10)
        );
    }

    #[test]
    fn a_ratio_below_zero_renders_empty() {
        let theme = Theme::new(true, true);
        let line = line(-1.0, 10, &theme);
        assert_eq!(line.width(), 10);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].content.to_string(),
            theme.sym().gauge_empty.repeat(10)
        );
    }

    #[test]
    fn a_nan_ratio_renders_empty() {
        let theme = Theme::new(true, true);
        let line = line(f64::NAN, 10, &theme);
        assert_eq!(line.width(), 10);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].content.to_string(),
            theme.sym().gauge_empty.repeat(10)
        );
    }

    #[test]
    fn the_ascii_gauge_uses_the_ascii_glyphs() {
        let theme = Theme::new(true, false);
        let symbols = theme.sym();
        let line = line(0.5, 10, &theme);
        assert_eq!(
            line.spans[0].content.to_string(),
            symbols.gauge_fill.repeat(5)
        );
        assert_eq!(
            line.spans[1].content.to_string(),
            symbols.gauge_empty.repeat(5)
        );
    }

    #[test]
    fn the_gauge_keeps_its_modifiers_without_color() {
        let theme = Theme::new(false, true);
        let line = line(0.5, 10, &theme);
        assert_eq!(line.spans[0].style.fg, None);
        assert_eq!(line.spans[1].style.fg, None);
    }
}
