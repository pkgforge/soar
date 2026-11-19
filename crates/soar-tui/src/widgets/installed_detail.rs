//! The installed package detail pane: where a package lives and how it
//! was installed, drawn as a headline, a banner for a broken install, and
//! a label and value grid.
//!
//! Beside the list the pane clips its rows at the bottom; filling the
//! screen it scrolls with the view's own cursor.

use std::{path::Path, time::SystemTime};

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use soar_operations::InstalledEntry;

use crate::{app::App, format, theme::Theme, widgets::badge};

/// How many cells a grid label keeps before its value starts.
const LABEL_WIDTH: usize = 12;

/// How many cells a checksum keeps before an ellipsis ends it.
const CHECKSUM_WIDTH: usize = 16;

/// Draws the pane: a rounded border titled with the package name, then the
/// headline, the banner a broken install shows, and the label and value
/// grid. While the pane fills the screen its rows scroll; beside the list
/// they clip.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App, full: bool) {
    let theme = &app.theme;
    let title = app
        .installed
        .cursor_entry()
        .map_or_else(|| "Details".to_string(), pane_title);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border(true))
        .title(Line::styled(format!(" {title} "), theme.title()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if full {
        // The wheel scrolls this pane while it fills the screen.
        app.layout.detail = Some(inner);
    }
    let width = usize::from(inner.width);
    let height = usize::from(inner.height);
    let lines = match app.installed.cursor_entry() {
        Some(entry) => lines(entry, width, theme),
        None => return,
    };
    let rows = if full {
        app.installed.detail_rows = lines.len();
        app.installed.detail.window(lines.len(), height)
    } else {
        0..lines.len().min(height)
    };
    f.render_widget(Paragraph::new(lines[rows].to_vec()), inner);
}

/// The pane's rows: the headline, the banner a broken install shows, then
/// the label and value grid.
fn lines(entry: &InstalledEntry, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![headline(entry, theme)];
    if !entry.is_healthy {
        lines.push(broken_banner(theme));
    }
    lines.push(Line::raw(""));
    grid(entry, width, theme, &mut lines);
    lines
}

/// The pane title: `family/name` when a family is recorded, else the name.
fn pane_title(entry: &InstalledEntry) -> String {
    match entry.package.pkg_family.as_deref() {
        Some(family) => format!("{family}/{}", entry.package.pkg_name),
        None => entry.package.pkg_name.clone(),
    }
}

/// The first row: the version, the package type, and the repository.
fn headline(entry: &InstalledEntry, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(entry.package.version.clone(), theme.accent2())];
    if let Some(pkg_type) = entry.package.pkg_type.as_deref() {
        spans.push(Span::raw(" "));
        spans.push(badge::pkg_type(Some(pkg_type), theme));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(entry.package.repo_name.clone(), theme.info()));
    Line::from(spans)
}

/// The danger row under the headline when the installed files are gone
/// from disk.
fn broken_banner(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(theme.sym().error, theme.danger()),
        Span::styled(
            " Files are missing. Remove it, or use Health (6) to clean up.",
            theme.danger(),
        ),
    ])
}

/// The label and value grid. A fact the record omits leaves no row.
fn grid(entry: &InstalledEntry, width: usize, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    let budget = width.saturating_sub(LABEL_WIDTH);
    let ellipsis = theme.sym().ellipsis;
    let mut grid: Vec<(&'static str, String)> = Vec::new();
    let path = format::home_relative(Path::new(&entry.package.installed_path));
    if !path.is_empty() {
        grid.push(("Path", format::truncate_middle(&path, budget, ellipsis)));
    }
    if let Some(installed) = installed_line(entry) {
        grid.push(("Installed", installed));
    }
    grid.push(("Size", format::size(entry.disk_size)));
    if let Some(checksum) = entry.package.checksum.as_deref() {
        grid.push((
            "Checksum",
            format::truncate(checksum, CHECKSUM_WIDTH, ellipsis),
        ));
    }
    if !entry.package.profile.is_empty() {
        grid.push(("Profile", entry.package.profile.clone()));
    }
    portable_dirs(entry, &mut grid);
    provides(entry, &mut grid);
    if let Some(url) = entry
        .package
        .download_url
        .as_deref()
        .filter(|url| !url.is_empty())
    {
        grid.push(("Source", url.to_string()));
    }
    if let Some(flags) = flags_line(entry) {
        grid.push(("Flags", flags));
    }
    for (label, value) in grid {
        if value.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", label, width = LABEL_WIDTH),
                theme.muted(),
            ),
            Span::styled(format::truncate(&value, budget, ellipsis), theme.text()),
        ]));
    }
}

/// The install date as recorded, with the relative age in parens when it
/// parses.
fn installed_line(entry: &InstalledEntry) -> Option<String> {
    let date = &entry.package.installed_date;
    if date.is_empty() {
        return None;
    }
    let mut line = date.clone();
    if let Some(then) = format::parse_installed_date(date) {
        line.push_str(&format!(" ({})", format::relative(then, SystemTime::now())));
    }
    Some(line)
}

/// One grid row per portable directory the package keeps, as `home
/// ~/path` style, with the label on the first row only.
fn portable_dirs(entry: &InstalledEntry, grid: &mut Vec<(&'static str, String)>) {
    let dirs = [
        ("home", entry.package.portable_home.as_deref()),
        ("config", entry.package.portable_config.as_deref()),
        ("share", entry.package.portable_share.as_deref()),
        ("cache", entry.package.portable_cache.as_deref()),
    ];
    let mut first = true;
    for (name, path) in dirs {
        let Some(path) = path.filter(|path| !path.is_empty()) else {
            continue;
        };
        grid.push((
            if first { "Portable" } else { "" },
            format!("{name} {}", format::home_relative(Path::new(path))),
        ));
        first = false;
    }
}

/// The provides row: one entry per provide, `@name` when it links straight
/// into bin, `name=>target` style when it carries a target.
fn provides(entry: &InstalledEntry, grid: &mut Vec<(&'static str, String)>) {
    let Some(provides) = entry
        .package
        .provides
        .as_deref()
        .filter(|provides| !provides.is_empty())
    else {
        return;
    };
    let names = provides
        .iter()
        .map(|provide| {
            let mut text = if provide.symlink_to_bin {
                format!("@{}", provide.name)
            } else {
                provide.name.clone()
            };
            if let Some(target) = provide.target.as_deref() {
                if let Some(strategy) = provide.strategy.as_ref() {
                    text.push_str(&strategy.to_string());
                }
                text.push_str(target);
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ");
    grid.push(("Provides", names));
}

/// The flags row: the words for the record's pinned, detached, and not
/// active states, in that order, when any holds.
fn flags_line(entry: &InstalledEntry) -> Option<String> {
    let mut flags: Vec<&'static str> = Vec::new();
    if entry.package.pinned {
        flags.push("pinned");
    }
    if entry.package.detached {
        flags.push("detached");
    }
    if entry.package.unlinked {
        flags.push("inactive");
    }
    (!flags.is_empty()).then(|| flags.join(", "))
}
