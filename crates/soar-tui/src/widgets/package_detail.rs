//! The package detail pane: what the repositories know about the package
//! under the catalog cursor, drawn as a headline, warning banners, a
//! wrapped description, a label and value grid, and notes.
//!
//! Beside the list the pane clips its rows at the bottom; filling the
//! screen it scrolls with the view's own cursor.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use soar_core::database::models::Package;

use crate::{
    app::App,
    format,
    loadable::Loadable,
    theme::Theme,
    views::browse::CatalogRow,
    widgets::{badge, states},
};

/// How many cells a grid label keeps before its value starts.
const LABEL_WIDTH: usize = 12;

/// How many versions the grid lists before it counts the rest.
const VERSIONS_SHOWN: usize = 5;

/// Draws the pane: a rounded border titled with the package name, then the
/// detail rows, or the state a failed load shows. While the pane fills the
/// screen its rows scroll; beside the list they clip.
pub(crate) fn render(f: &mut Frame, area: Rect, app: &mut App, full: bool) {
    let theme = &app.theme;
    let row = app.browse.cursor_row().cloned();
    let title = row
        .as_ref()
        .map(|row| row.key.pkg_name.clone())
        .unwrap_or_else(|| "Details".to_string());
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
    let Some(row) = row else {
        return;
    };
    let entry = app.browse.details.get(&row.key);
    if let Some(Loadable::Failed(error)) = entry {
        states::error(f, inner, theme, error);
        return;
    }
    let lines = lines(&row, entry, usize::from(inner.width), app.frame, theme);
    let height = usize::from(inner.height);
    let rows = if full {
        app.browse.detail_rows = lines.len();
        app.browse.detail.window(lines.len(), height)
    } else {
        0..lines.len().min(height)
    };
    f.render_widget(Paragraph::new(lines[rows].to_vec()), inner);
}

/// The pane's rows: the headline, the warning banners, the wrapped
/// description, then the package's facts and notes, or the line a pending
/// or empty load shows.
fn lines(
    row: &CatalogRow,
    entry: Option<&Loadable<Option<Package>>>,
    width: usize,
    frame: u64,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![headline(row, theme)];
    if let Some(Loadable::Ready(Some(package))) = entry {
        banners(package, theme, &mut lines);
    }
    lines.push(Line::raw(""));
    let before = lines.len();
    lines.extend(
        format::wrap(&row.description, width)
            .into_iter()
            .map(|piece| Line::styled(piece, theme.text())),
    );
    if lines.len() > before {
        lines.push(Line::raw(""));
    }
    match entry {
        Some(Loadable::Ready(Some(package))) => facts(row, package, width, theme, &mut lines),
        Some(Loadable::Ready(None)) => {
            lines.push(Line::styled("not found".to_string(), theme.muted()));
        }
        Some(Loadable::Loading) => {
            lines.push(Line::styled(
                format!("{} loading details", theme.spinner(frame)),
                theme.muted(),
            ))
        }
        _ => {}
    }
    lines
}

/// The first row: the version, the package type, and the installed mark.
fn headline(row: &CatalogRow, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(row.version.clone(), theme.accent2())];
    if let Some(pkg_type) = row.pkg_type.as_deref() {
        spans.push(Span::raw(" "));
        spans.push(badge::pkg_type(Some(pkg_type), theme));
    }
    if row.installed {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} installed", theme.sym().installed),
            theme.success(),
        ));
    }
    Line::from(spans)
}

/// The warning rows above the description: a deprecated package, and a
/// disabled one with its reason.
fn banners(package: &Package, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    let symbol = theme.sym().warning;
    if package.deprecated {
        lines.push(Line::from(vec![
            Span::styled(symbol, theme.warning()),
            Span::styled(" Deprecated", theme.warning()),
        ]));
    }
    if package.disabled == Some(true) {
        let mut text = "Disabled".to_string();
        if let Some(reason) = package.disabled_reason.as_deref().filter(|r| !r.is_empty()) {
            text.push_str(": ");
            text.push_str(reason);
        }
        lines.push(Line::from(vec![
            Span::styled(symbol, theme.warning()),
            Span::styled(format!(" {text}"), theme.warning()),
        ]));
    }
}

/// The label and value grid, then the notes. A fact the package omits
/// leaves no row.
fn facts(
    row: &CatalogRow,
    package: &Package,
    width: usize,
    theme: &Theme,
    lines: &mut Vec<Line<'static>>,
) {
    let mut grid: Vec<(&'static str, String)> = Vec::new();
    grid.push(("Repository", row.key.repo_name.clone()));
    if let Some(family) = row.key.pkg_family.as_deref() {
        grid.push(("Family", family.to_string()));
    }
    if let Some(bytes) = package.ghcr_size.or(package.size) {
        grid.push(("Size", format::size(bytes)));
    }
    joined(&mut grid, "License", package.licenses.as_deref());
    if let Some(homepages) = package.homepages.as_deref().and_then(non_empty) {
        let rest = homepages.len().saturating_sub(1);
        let value = if rest > 0 {
            format!("{} +{rest}", homepages[0])
        } else {
            homepages[0].clone()
        };
        grid.push(("Homepage", value));
    }
    if let Some(sources) = package.source_urls.as_deref().and_then(non_empty) {
        grid.push(("Source", sources[0].clone()));
    }
    if let Some(maintainers) = package.maintainers.as_deref().and_then(non_empty) {
        let names = maintainers
            .iter()
            .map(|maintainer| maintainer.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        grid.push(("Maintainer", names));
    }
    joined(&mut grid, "Categories", package.categories.as_deref());
    if let Some(provides) = package.provides.as_deref().and_then(non_empty) {
        let names = provides
            .iter()
            .map(|provide| provide.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        grid.push(("Provides", names));
    }
    if let Some(date) = package
        .build_date
        .as_deref()
        .filter(|date| !date.is_empty())
    {
        let day = date.split([' ', 'T']).next().unwrap_or(date);
        grid.push(("Built", day.to_string()));
    }
    if let Some(versions) = versions_line(row) {
        grid.push(("Versions", versions));
    }
    let budget = width.saturating_sub(LABEL_WIDTH);
    for (label, value) in grid {
        if value.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", label, width = LABEL_WIDTH),
                theme.muted(),
            ),
            Span::styled(
                format::truncate(&value, budget, theme.sym().ellipsis),
                theme.text(),
            ),
        ]));
    }
    if let Some(notes) = package.notes.as_deref().and_then(non_empty) {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Notes", theme.warning()));
        for note in notes {
            lines.push(Line::from(vec![
                Span::styled(theme.sym().warning, theme.warning()),
                Span::raw(" "),
                Span::styled(note.clone(), theme.text()),
            ]));
        }
    }
}

/// The versions row: the first few, then a count of the rest. `None` when
/// the row has no other versions.
fn versions_line(row: &CatalogRow) -> Option<String> {
    let versions = &row.other_versions;
    if versions.is_empty() {
        return None;
    }
    let shown = versions
        .iter()
        .take(VERSIONS_SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let rest = versions.len().saturating_sub(VERSIONS_SHOWN);
    Some(if rest > 0 {
        format!("{shown} +{rest}")
    } else {
        shown
    })
}

/// Adds a grid row that joins a list of values with commas, when there is
/// at least one.
fn joined(grid: &mut Vec<(&'static str, String)>, label: &'static str, values: Option<&[String]>) {
    let Some(values) = values.and_then(non_empty) else {
        return;
    };
    grid.push((label, values.join(", ")));
}

/// The slice itself when it holds at least one entry.
fn non_empty<T>(values: &[T]) -> Option<&[T]> {
    (!values.is_empty()).then_some(values)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use soar_core::database::models::{Maintainer, Package};

    use crate::{
        loadable::Loadable,
        msg::{Loaded, Msg},
        test_support::{catalog_row, package, screen, test_app},
        views::browse::{CatalogRow, PackageKey},
    };

    fn loaded(rows: Vec<CatalogRow>) -> Msg {
        Msg::Loaded(Loaded::Catalog(Ok(rows)))
    }

    /// Builds the interface with the catalog loaded, the cursor resting on
    /// `bat`, and the given package stored as its loaded details.
    fn app_with_detail(description: &str, package: &Package) -> crate::app::App {
        let mut app = test_app();
        let mut rows = vec![catalog_row("bat", "0.24.0", "bincache")];
        rows[0].description = description.to_string();
        app.update(loaded(rows), Instant::now());
        let key = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "bat".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        app.browse
            .details
            .insert(key, Loadable::Ready(Some(package.clone())));
        app
    }

    #[test]
    fn the_pane_lists_licenses_sizes_and_notes() {
        let mut bat = package("bat", "0.24.0", "bincache");
        bat.licenses = Some(vec!["MIT".to_string(), "Apache-2.0".to_string()]);
        bat.ghcr_size = Some(5_452_595);
        bat.homepages = Some(vec!["https://github.com/sharkdp/bat".to_string()]);
        bat.build_date = Some("2026-11-02 13:45:07".to_string());
        bat.notes = Some(vec!["Config lives in ~/.config/bat".to_string()]);
        let mut app = app_with_detail("A cat clone with syntax highlighting.", &bat);
        let screen = screen(&mut app, 100, 30);
        assert!(screen.contains("MIT, Apache-2.0"), "screen is {screen:?}");
        assert!(
            screen.contains("Config lives in ~/.config/bat"),
            "screen is {screen:?}"
        );
        assert!(screen.contains("5.2 MiB"), "screen is {screen:?}");
        assert!(screen.contains("2026-11-02"), "screen is {screen:?}");
        assert!(screen.contains("bincache"), "screen is {screen:?}");
    }

    #[test]
    fn a_deprecated_and_disabled_package_warns_above_the_description() {
        let mut bat = package("bat", "0.24.0", "bincache");
        bat.deprecated = true;
        bat.disabled = Some(true);
        bat.disabled_reason = Some("upstream is gone".to_string());
        let mut app = app_with_detail("A cat clone.", &bat);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("Deprecated"), "screen is {screen:?}");
        assert!(
            screen.contains("Disabled: upstream is gone"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn a_pending_detail_shows_the_known_fields_and_a_spinner() {
        let mut app = test_app();
        let mut rows = vec![catalog_row("bat", "0.24.0", "bincache")];
        rows[0].description = "A cat clone.".to_string();
        rows[0].installed = true;
        rows[0].pkg_type = Some("static".to_string());
        app.update(loaded(rows), Instant::now());
        let key = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "bat".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        app.browse.details.insert(key, Loadable::Loading);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("loading details"), "screen is {screen:?}");
        assert!(screen.contains("A cat clone."), "screen is {screen:?}");
        assert!(screen.contains("0.24.0"), "screen is {screen:?}");
        assert!(screen.contains("installed"), "screen is {screen:?}");
    }

    #[test]
    fn an_empty_detail_shows_not_found() {
        let mut app = test_app();
        app.update(
            loaded(vec![catalog_row("bat", "0.24.0", "bincache")]),
            Instant::now(),
        );
        let key = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "bat".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        app.browse.details.insert(key, Loadable::Ready(None));
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("not found"), "screen is {screen:?}");
    }

    #[test]
    fn the_grid_lists_five_versions_and_counts_the_rest() {
        let mut app = test_app();
        let mut rows = vec![catalog_row("bat", "0.24.0", "bincache")];
        rows[0].other_versions = (1..=7).map(|n| format!("0.{n}")).collect();
        app.update(loaded(rows), Instant::now());
        let key = PackageKey {
            repo_name: "bincache".to_string(),
            pkg_name: "bat".to_string(),
            pkg_family: None,
            pkg_id: None,
        };
        app.browse.details.insert(
            key,
            Loadable::Ready(Some(package("bat", "0.24.0", "bincache"))),
        );
        let screen = screen(&mut app, 100, 30);
        assert!(
            screen.contains("0.1, 0.2, 0.3, 0.4, 0.5 +2"),
            "screen is {screen:?}"
        );
    }

    #[test]
    fn maintainers_show_their_names_without_contacts() {
        let mut bat = package("bat", "0.24.0", "bincache");
        bat.maintainers = Some(vec![Maintainer {
            name: "Jane Doe".to_string(),
            contact: "jane@example.com".to_string(),
        }]);
        let mut app = app_with_detail("A cat clone.", &bat);
        let screen = screen(&mut app, 100, 24);
        assert!(screen.contains("Jane Doe"), "screen is {screen:?}");
        assert!(!screen.contains("jane@example.com"), "screen is {screen:?}");
    }

    #[test]
    fn several_homepages_count_the_ones_beyond_the_first() {
        let mut bat = package("bat", "0.24.0", "bincache");
        bat.homepages = Some(vec![
            "https://bat.example".to_string(),
            "https://bat-docs.example".to_string(),
            "https://bat-chat.example".to_string(),
        ]);
        let mut app = app_with_detail("A cat clone.", &bat);
        let screen = screen(&mut app, 100, 24);
        assert!(
            screen.contains("https://bat.example +2"),
            "screen is {screen:?}"
        );
        assert!(!screen.contains("bat-chat.example"), "screen is {screen:?}");
    }

    #[test]
    fn below_100_columns_the_pane_stays_hidden() {
        let mut app = app_with_detail(
            "uniquely hidden pane text",
            &package("bat", "0.24.0", "bincache"),
        );
        let narrow = screen(&mut app, 99, 24);
        assert!(
            !narrow.contains("uniquely hidden pane text"),
            "screen is {narrow:?}"
        );
        let wide = screen(&mut app, 100, 24);
        assert!(
            wide.contains("uniquely hidden pane text"),
            "screen is {wide:?}"
        );
    }
}
