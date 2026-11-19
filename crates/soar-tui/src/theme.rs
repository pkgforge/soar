//! The palette, text styles, and symbols every part of the interface draws
//! with.
//!
//! All colors come from the 16 named ANSI colors, so the interface follows
//! the terminal's theme. Without color, every style is built from modifiers
//! alone. The styles and glyphs no view draws yet carry their own allow
//! until their views arrive.

use ratatui::style::{Color, Modifier, Style};

/// The colors and glyphs one session draws with.
///
/// A session either renders colors and icons or it does not, so every style
/// and symbol comes from here instead of being written at the drawing site.
pub(crate) struct Theme {
    color: bool,
    icons: bool,
}

impl Theme {
    /// Builds the theme for one session.
    ///
    /// `color` turns the palette on; without it the styles fall back to
    /// bold, dim, italic, reversed, and underlined text. `icons` selects the
    /// unicode glyph set over its ASCII fallback.
    pub(crate) fn new(color: bool, icons: bool) -> Theme {
        Theme {
            color,
            icons,
        }
    }

    /// The brand and focus color: the header mark, the focused border, the
    /// active tab, and gauges.
    pub(crate) fn accent(&self) -> Style {
        self.styled(Color::Cyan, Modifier::BOLD)
    }

    /// Version numbers and package family names.
    pub(crate) fn accent2(&self) -> Style {
        self.styled(Color::Magenta, Modifier::ITALIC)
    }

    /// Normal text.
    pub(crate) fn text(&self) -> Style {
        self.styled(Color::Reset, Modifier::empty())
    }

    /// Secondary text: hints, descriptions, and unfocused borders.
    pub(crate) fn muted(&self) -> Style {
        self.styled(Color::DarkGray, Modifier::DIM)
    }

    /// Installed, healthy, done, and new versions.
    pub(crate) fn success(&self) -> Style {
        self.styled(Color::Green, Modifier::empty())
    }

    /// Notes, pinned packages, skipped items, and the system badge.
    pub(crate) fn warning(&self) -> Style {
        self.styled(Color::Yellow, Modifier::BOLD)
    }

    /// Errors, broken packages, and removal.
    pub(crate) fn danger(&self) -> Style {
        self.styled(Color::Red, Modifier::BOLD | Modifier::UNDERLINED)
    }

    /// Package names in summaries and repository names.
    pub(crate) fn info(&self) -> Style {
        self.styled(Color::Blue, Modifier::empty())
    }

    /// The row under the cursor.
    pub(crate) fn cursor_row(&self) -> Style {
        if self.color {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        }
    }

    /// A row the user selected for an action.
    pub(crate) fn marked(&self) -> Style {
        if self.color {
            self.accent().add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
    }

    /// The title of a list or pane.
    pub(crate) fn title(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    /// A key name in hints and help.
    pub(crate) fn key(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    /// The border of a pane: accent while it has focus, muted otherwise.
    pub(crate) fn border(&self, focused: bool) -> Style {
        if self.color {
            if focused { self.accent() } else { self.muted() }
        } else if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        }
    }

    /// The style of a package type badge, such as `appimage` or `archive`.
    pub(crate) fn type_badge(&self, pkg_type: &str) -> Style {
        match pkg_type {
            "appimage" => self.info(),
            "flatimage" => self.accent2(),
            "static" | "dynamic" => self.success(),
            "archive" => self.warning(),
            _ => self.muted(),
        }
    }

    /// The glyph set for this session: unicode when icons are on, ASCII
    /// otherwise.
    pub(crate) fn sym(&self) -> &'static Symbols {
        if self.icons { &UNICODE } else { &ASCII }
    }

    /// Whether this session draws the unicode glyph set.
    pub(crate) fn icons(&self) -> bool {
        self.icons
    }

    /// The spinner frame for the given tick counter, wrapping around.
    pub(crate) fn spinner(&self, frame: u64) -> &'static str {
        let symbols = self.sym();
        symbols.spinner[(frame % symbols.spinner.len() as u64) as usize]
    }

    /// Builds a style with the given foreground in color mode, or the given
    /// modifiers alone without it.
    fn styled(&self, color: Color, modifiers: Modifier) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new().add_modifier(modifiers)
        }
    }
}

/// The glyphs one session draws with, in one of two forms.
pub(crate) struct Symbols {
    /// The mark shown before the program name in the header.
    pub(crate) brand: &'static str,
    /// Marks an installed or finished item.
    pub(crate) installed: &'static str,
    /// Marks a package that is not installed yet.
    pub(crate) available: &'static str,
    /// Marks a row the user selected for an action.
    pub(crate) marked: &'static str,
    /// Marks the row under the cursor.
    pub(crate) cursor: &'static str,
    /// Marks a package with a newer version.
    pub(crate) update: &'static str,
    /// Marks a package the plan would take away again.
    pub(crate) remove: &'static str,
    /// Marks a note or a skipped item.
    pub(crate) warning: &'static str,
    /// Marks a failure.
    pub(crate) error: &'static str,
    /// Stands for one typed character of a hidden value, such as a
    /// password.
    pub(crate) mask: &'static str,
    /// Marks a package held on its current version.
    pub(crate) pinned: &'static str,
    /// Points from an old version to a new one.
    pub(crate) arrow: &'static str,
    /// Separates two short facts, such as profile and mode.
    pub(crate) dot: &'static str,
    /// The filled part of a progress bar.
    pub(crate) gauge_fill: &'static str,
    /// The empty part of a progress bar.
    pub(crate) gauge_empty: &'static str,
    /// Marks text cut short to fit its column.
    pub(crate) ellipsis: &'static str,
    /// The frames a spinner cycles through, one per tick.
    pub(crate) spinner: &'static [&'static str],
}

/// The unicode glyph set.
const UNICODE: Symbols = Symbols {
    brand: "◆",
    installed: "✓",
    available: "○",
    marked: "●",
    cursor: "▌",
    update: "↑",
    remove: "−",
    warning: "⚠",
    error: "✗",
    mask: "•",
    pinned: "⊙",
    arrow: "→",
    dot: "·",
    gauge_fill: "━",
    gauge_empty: "─",
    ellipsis: "…",
    spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
};

/// The ASCII fallback glyph set.
const ASCII: Symbols = Symbols {
    brand: "*",
    installed: "+",
    available: "-",
    marked: "*",
    cursor: ">",
    update: "^",
    remove: "-",
    warning: "!",
    error: "x",
    mask: "*",
    pinned: "p",
    arrow: "->",
    dot: "|",
    gauge_fill: "#",
    gauge_empty: "-",
    ellipsis: "...",
    spinner: &["-", "\\", "|", "/"],
};

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier};

    use super::{ASCII, Theme, UNICODE};

    fn colored() -> Theme {
        Theme::new(true, true)
    }

    fn plain() -> Theme {
        Theme::new(false, true)
    }

    #[test]
    fn no_color_leaves_every_style_colorless() {
        let theme = plain();
        let styles = [
            theme.accent(),
            theme.accent2(),
            theme.text(),
            theme.muted(),
            theme.success(),
            theme.warning(),
            theme.danger(),
            theme.info(),
            theme.cursor_row(),
            theme.marked(),
            theme.title(),
            theme.key(),
            theme.border(true),
            theme.border(false),
            theme.type_badge("appimage"),
            theme.type_badge("flatimage"),
            theme.type_badge("static"),
            theme.type_badge("dynamic"),
            theme.type_badge("archive"),
            theme.type_badge("other"),
        ];
        for style in styles {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
        }
    }

    #[test]
    fn tokens_use_the_palette_with_color() {
        let theme = colored();
        assert_eq!(theme.accent().fg, Some(Color::Cyan));
        assert_eq!(theme.accent2().fg, Some(Color::Magenta));
        assert_eq!(theme.text().fg, Some(Color::Reset));
        assert_eq!(theme.muted().fg, Some(Color::DarkGray));
        assert_eq!(theme.success().fg, Some(Color::Green));
        assert_eq!(theme.warning().fg, Some(Color::Yellow));
        assert_eq!(theme.danger().fg, Some(Color::Red));
        assert_eq!(theme.info().fg, Some(Color::Blue));
    }

    #[test]
    fn tokens_fall_back_to_modifiers_without_color() {
        let theme = plain();
        assert!(theme.accent().add_modifier.contains(Modifier::BOLD));
        assert!(theme.accent2().add_modifier.contains(Modifier::ITALIC));
        assert_eq!(theme.text().add_modifier, Modifier::empty());
        assert!(theme.muted().add_modifier.contains(Modifier::DIM));
        assert_eq!(theme.success().add_modifier, Modifier::empty());
        assert!(theme.warning().add_modifier.contains(Modifier::BOLD));
        assert!(theme.danger().add_modifier.contains(Modifier::BOLD));
        assert!(theme.danger().add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(theme.info().add_modifier, Modifier::empty());
    }

    #[test]
    fn the_cursor_row_reverses_without_color() {
        let style = plain().cursor_row();
        assert!(style.add_modifier.contains(Modifier::REVERSED));
        assert!(style.add_modifier.contains(Modifier::BOLD));

        let style = colored().cursor_row();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::Cyan));
    }

    #[test]
    fn a_marked_row_underlines_without_color() {
        let style = plain().marked();
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));

        let style = colored().marked();
        assert_eq!(style.fg, Some(Color::Cyan));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn borders_follow_focus() {
        assert_eq!(colored().border(true).fg, Some(Color::Cyan));
        assert_eq!(colored().border(false).fg, Some(Color::DarkGray));
        assert!(plain().border(true).add_modifier.contains(Modifier::BOLD));
        assert_eq!(plain().border(false).add_modifier, Modifier::empty());
    }

    #[test]
    fn the_ascii_glyphs_are_ascii() {
        let symbols = &ASCII;
        let glyphs = [
            symbols.brand,
            symbols.installed,
            symbols.available,
            symbols.marked,
            symbols.cursor,
            symbols.update,
            symbols.remove,
            symbols.warning,
            symbols.error,
            symbols.mask,
            symbols.pinned,
            symbols.arrow,
            symbols.dot,
            symbols.gauge_fill,
            symbols.gauge_empty,
            symbols.ellipsis,
        ];
        for glyph in glyphs {
            assert!(glyph.is_ascii(), "{glyph:?} is not ASCII");
        }
        for frame in ASCII.spinner {
            assert!(frame.is_ascii(), "{frame:?} is not ASCII");
        }
    }

    #[test]
    fn both_spinners_have_frames() {
        assert!(!UNICODE.spinner.is_empty());
        assert!(!ASCII.spinner.is_empty());
    }

    #[test]
    fn the_spinner_picks_its_set() {
        assert_eq!(colored().sym().brand, UNICODE.brand);
        assert_eq!(Theme::new(true, false).sym().brand, ASCII.brand);
    }

    #[test]
    fn the_spinner_wraps_around() {
        let theme = colored();
        let frames = theme.sym().spinner;
        let len = frames.len() as u64;
        assert_eq!(theme.spinner(0), frames[0]);
        assert_eq!(theme.spinner(len - 1), frames[(len - 1) as usize]);
        assert_eq!(theme.spinner(len), frames[0]);
        assert_eq!(theme.spinner(len + 2), frames[2]);
    }
}
