//! Test-only helpers for the interface: rendering the state into text,
//! building key messages, and making database fixtures.

use std::path::PathBuf;

use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};
use soar_config::packages::ResolvedPackage;
use soar_core::{
    database::models::{InstalledPackage, Package},
    package::install::InstallTarget,
};
use soar_operations::{ApplyDiff, HealthReport, InstalledEntry, UpdateInfo};

use crate::{
    app::{App, AppEnv},
    msg::Msg,
    theme::Theme,
    ui,
    views::{
        apply::ApplyPlan,
        browse::{CatalogRow, PackageKey},
    },
};

/// Renders one frame into a fixed-size buffer and returns it as text rows.
/// Wide characters occupy two cells, the second of which is empty, so the
/// joined row reproduces the visible text.
pub(crate) fn render(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect()
}

/// Renders one frame and joins the rows into one screen of text.
pub(crate) fn screen(app: &mut App, width: u16, height: u16) -> String {
    render(app, width, height).join("\n")
}

/// Builds a plain character key press.
pub(crate) fn key(c: char) -> Msg {
    key_msg(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Builds a named key press, such as Tab or Enter.
pub(crate) fn code(code: KeyCode) -> Msg {
    key_msg(code, KeyModifiers::NONE)
}

/// Builds a character key press held with Ctrl.
pub(crate) fn ctrl(c: char) -> Msg {
    key_msg(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn key_msg(code: KeyCode, mods: KeyModifiers) -> Msg {
    Msg::Input(Event::Key(KeyEvent::new_with_kind(
        code,
        mods,
        KeyEventKind::Press,
    )))
}

/// Builds the state the interface starts in, with fixed session facts.
pub(crate) fn test_app() -> App {
    test_app_with(Theme::new(true, true))
}

/// Builds the interface around a given theme, for the sessions that draw
/// the ASCII glyph set.
pub(crate) fn test_app_with(theme: Theme) -> App {
    App::new(
        AppEnv {
            version: "0.0.0".to_string(),
            profile: "default".to_string(),
            system: false,
            config_path: PathBuf::from("/tmp/soar-test-config.toml"),
            packages_path: PathBuf::from("/tmp/soar-test-packages.toml"),
        },
        theme,
    )
}

/// Builds a catalog package with the given name, version, and repository.
pub(crate) fn package(name: &str, version: &str, repo: &str) -> Package {
    Package {
        pkg_name: name.to_string(),
        version: version.to_string(),
        repo_name: repo.to_string(),
        ..Default::default()
    }
}

/// Builds a catalog row with the given name, version, and repository.
pub(crate) fn catalog_row(name: &str, version: &str, repo: &str) -> CatalogRow {
    CatalogRow {
        key: PackageKey {
            repo_name: repo.to_string(),
            pkg_name: name.to_string(),
            pkg_family: None,
            pkg_id: None,
        },
        version: version.to_string(),
        pkg_type: None,
        description: String::new(),
        installed: false,
        other_versions: Vec::new(),
        haystack: name.to_lowercase(),
    }
}

/// Builds an installed package row with the given name, version, and
/// repository.
pub(crate) fn installed_package(name: &str, version: &str, repo: &str) -> InstalledPackage {
    InstalledPackage {
        repo_name: repo.to_string(),
        pkg_name: name.to_string(),
        version: version.to_string(),
        id: name_id(name),
        pkg_id: None,
        pkg_family: None,
        pkg_type: None,
        size: 0,
        checksum: None,
        installed_path: String::new(),
        installed_date: String::new(),
        profile: String::new(),
        pinned: false,
        is_installed: true,
        detached: false,
        unlinked: false,
        provides: None,
        portable_path: None,
        portable_home: None,
        portable_config: None,
        portable_share: None,
        portable_cache: None,
        install_patterns: None,
        download_url: None,
        update_info: None,
    }
}

/// Builds an installed entry with the given name, version, and repository,
/// healthy and with no size recorded yet.
pub(crate) fn installed_entry(name: &str, version: &str, repo: &str) -> InstalledEntry {
    InstalledEntry {
        package: installed_package(name, version, repo),
        disk_size: 0,
        is_healthy: true,
    }
}

/// Builds one checked update: the package named moving from `current` to
/// `new` in the given repository, with the new package behind the install
/// target.
pub(crate) fn update_info(name: &str, current: &str, new: &str, repo: &str) -> UpdateInfo {
    UpdateInfo {
        pkg_name: name.to_string(),
        repo_name: repo.to_string(),
        current_version: current.to_string(),
        new_version: new.to_string(),
        target: InstallTarget {
            package: package(name, new, repo),
            ..Default::default()
        },
        update_toml_url: None,
    }
}

/// Builds a health report with every check passing and nothing broken.
pub(crate) fn health_report() -> HealthReport {
    HealthReport {
        path_configured: true,
        bin_path: PathBuf::from("/home/user/.local/bin"),
        man_path: None,
        man_path_configured: true,
        broken_packages: Vec::new(),
        broken_symlinks: Vec::new(),
    }
}

/// One declared package with the given name.
pub(crate) fn resolved_package(name: &str) -> ResolvedPackage {
    ResolvedPackage {
        name: name.to_string(),
        ..Default::default()
    }
}

/// An install plan entry for a package with the given name and version.
pub(crate) fn install_entry(name: &str, version: &str) -> (ResolvedPackage, InstallTarget) {
    (
        resolved_package(name),
        InstallTarget {
            package: package(name, version, "bincache"),
            ..Default::default()
        },
    )
}

/// A computed plan for the packages file holding one install, one update,
/// and two in sync, at the usual path.
pub(crate) fn apply_plan() -> ApplyPlan {
    let update = InstallTarget {
        package: package("yq", "4.44.3", "bincache"),
        existing_install: Some(installed_package("yq", "4.44.2", "bincache")),
        ..Default::default()
    };
    let diff = ApplyDiff {
        to_install: vec![install_entry("bat", "0.24.0")],
        to_update: vec![(resolved_package("yq"), update)],
        in_sync: vec!["fd".to_string(), "jq".to_string()],
        ..Default::default()
    };
    ApplyPlan {
        path: PathBuf::from("/home/user/.config/soar/packages.toml"),
        diff: Some(diff),
        system: Vec::new(),
    }
}

/// Builds the paths list the health view shows beside its checks.
pub(crate) fn health_paths() -> Vec<(String, PathBuf)> {
    [
        ("Config", "/home/user/.config/soar/config.toml"),
        ("Packages config", "/home/user/.config/soar/packages.toml"),
        ("Bin", "/home/user/.local/share/soar/bin"),
        ("Database", "/home/user/.local/share/soar/db"),
        ("Cache", "/home/user/.local/share/soar/cache"),
        ("Packages", "/home/user/.local/share/soar/packages"),
        ("Repositories", "/home/user/.local/share/soar/repos"),
    ]
    .into_iter()
    .map(|(label, path)| (label.to_string(), PathBuf::from(path)))
    .collect()
}

/// A database id for a fixture name, unique enough for the names tests
/// use, so marks can tell fixtures apart.
fn name_id(name: &str) -> u64 {
    name.bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u64::from(byte))
    })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ratatui::crossterm::event::KeyCode;

    use super::{code, ctrl, installed_package, key, package, screen, test_app};
    use crate::views::ViewId;

    #[test]
    fn the_first_view_title_appears_on_screen() {
        assert!(screen(&mut test_app(), 80, 24).contains("Browse"));
    }

    #[test]
    fn a_character_key_reaches_the_app() {
        let mut app = test_app();
        app.update(key('q'), Instant::now());
        assert!(app.quit);
    }

    #[test]
    fn a_named_key_reaches_the_app() {
        let mut app = test_app();
        app.update(code(KeyCode::Tab), Instant::now());
        assert_eq!(app.view, ViewId::Installed);
    }

    #[test]
    fn a_ctrl_key_reaches_the_app() {
        let mut app = test_app();
        app.update(ctrl('c'), Instant::now());
        assert!(app.quit);
    }

    #[test]
    fn fixtures_carry_name_version_and_repo() {
        let pkg = package("fd", "10.2.0", "pkgforge");
        assert_eq!(pkg.pkg_name, "fd");
        assert_eq!(pkg.version, "10.2.0");
        assert_eq!(pkg.repo_name, "pkgforge");

        let installed = installed_package("fd", "10.2.0", "pkgforge");
        assert_eq!(installed.pkg_name, "fd");
        assert_eq!(installed.version, "10.2.0");
        assert_eq!(installed.repo_name, "pkgforge");
    }
}
