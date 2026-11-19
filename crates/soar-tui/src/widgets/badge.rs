//! Small marks shown beside package rows and names.
//!
//! The views draw the badges once they hold their own rows.

#![allow(dead_code)]

use ratatui::text::Span;

use crate::theme::Theme;

/// The package type word in its own color, or an empty span when the row
/// has no package type.
pub(crate) fn pkg_type(pkg_type: Option<&str>, theme: &Theme) -> Span<'static> {
    match pkg_type {
        None => Span::raw(""),
        Some(pkg_type) => Span::styled(pkg_type.to_string(), theme.type_badge(pkg_type)),
    }
}

/// The mark showing a package is installed.
pub(crate) fn installed(theme: &Theme) -> Span<'static> {
    Span::styled(theme.sym().installed, theme.success())
}

#[cfg(test)]
mod tests {
    use super::{installed, pkg_type};
    use crate::theme::Theme;

    #[test]
    fn a_missing_package_type_renders_nothing() {
        let theme = Theme::new(true, true);
        assert_eq!(pkg_type(None, &theme).width(), 0);
    }

    #[test]
    fn the_package_type_takes_its_own_color() {
        let theme = Theme::new(true, true);
        let span = pkg_type(Some("appimage"), &theme);
        assert_eq!(span.content, "appimage");
        assert!(span.style.fg.is_some());

        let plain = Theme::new(false, true);
        assert_eq!(pkg_type(Some("appimage"), &plain).style.fg, None);
    }

    #[test]
    fn the_installed_mark_uses_the_installed_glyph() {
        let theme = Theme::new(true, true);
        let span = installed(&theme);
        assert_eq!(span.content, theme.sym().installed);
        assert!(span.style.fg.is_some());
    }
}
