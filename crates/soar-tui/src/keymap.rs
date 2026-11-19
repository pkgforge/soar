//! The table of keys and the actions they trigger.
//!
//! The status bar hints and the help overlay read from this table, so the
//! interface never shows a hint for a key that does nothing.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{action::Action, views::ViewId};

/// One key, as the terminal reports it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Key {
    /// The key itself, such as a character or a named key.
    pub(crate) code: KeyCode,
    /// The modifier keys held down.
    pub(crate) mods: KeyModifiers,
}

/// A key bound to an action, with how it is described to the user.
pub(crate) struct Binding {
    /// Every key that triggers [`Binding::action`].
    pub(crate) keys: &'static [Key],
    /// What pressing one of the keys does.
    pub(crate) action: Action,
    /// Short label for the status bar and help, like "install".
    pub(crate) label: &'static str,
    /// Whether the status bar shows this binding.
    pub(crate) bar: bool,
}

/// A plain character key without modifiers.
const fn ch(c: char) -> Key {
    Key {
        code: KeyCode::Char(c),
        mods: KeyModifiers::NONE,
    }
}

/// A character key held with Ctrl.
const fn ctrl(c: char) -> Key {
    Key {
        code: KeyCode::Char(c),
        mods: KeyModifiers::CONTROL,
    }
}

/// A named key without modifiers, such as an arrow or Enter.
const fn code(c: KeyCode) -> Key {
    Key {
        code: c,
        mods: KeyModifiers::NONE,
    }
}

/// One row of a key table.
const fn bind(keys: &'static [Key], action: Action, label: &'static str, bar: bool) -> Binding {
    Binding {
        keys,
        action,
        label,
        bar,
    }
}

/// Keys that work the same in every view while no overlay or text input is
/// open.
#[rustfmt::skip]
pub(crate) const GLOBAL: &[Binding] = &[
    bind(&[ch('1')], Action::View(ViewId::Browse), "browse", false),
    bind(&[ch('2')], Action::View(ViewId::Installed), "installed", false),
    bind(&[ch('3')], Action::View(ViewId::Updates), "updates", false),
    bind(&[ch('4')], Action::View(ViewId::Apply), "apply", false),
    bind(&[ch('5')], Action::View(ViewId::Repos), "repos", false),
    bind(&[ch('6')], Action::View(ViewId::Health), "health", false),
    bind(&[ch('7')], Action::View(ViewId::Jobs), "jobs", false),
    bind(&[code(KeyCode::Tab)], Action::NextView, "next view", false),
    bind(&[code(KeyCode::BackTab)], Action::PrevView, "previous view", false),
    bind(&[ch('?')], Action::Help, "help", true),
    bind(&[ch('q'), ctrl('c')], Action::Quit, "quit", false),
    bind(&[ctrl('x')], Action::AbortJob, "abort", true),
    bind(&[ctrl('r')], Action::Reload, "reload", false),
    bind(&[ch('j'), code(KeyCode::Down)], Action::Down, "down", false),
    bind(&[ch('k'), code(KeyCode::Up)], Action::Up, "up", false),
    bind(&[ch('g'), code(KeyCode::Home)], Action::Top, "top", false),
    bind(&[ch('G'), code(KeyCode::End)], Action::Bottom, "bottom", false),
    bind(&[ctrl('d'), code(KeyCode::PageDown)], Action::PageDown, "page down", false),
    bind(&[ctrl('u'), code(KeyCode::PageUp)], Action::PageUp, "page up", false),
    bind(&[code(KeyCode::Esc)], Action::Escape, "clear", false),
];

/// Keys of the package catalog view.
#[rustfmt::skip]
const BROWSE: &[Binding] = &[
    bind(&[ch('/')], Action::Filter, "filter", true),
    bind(&[ch(' ')], Action::ToggleMark, "mark", true),
    bind(&[ch('i')], Action::Install, "install", true),
    bind(&[ch('I')], Action::InstallWithOptions, "install options", false),
    bind(&[ch('v')], Action::PickVersion, "version", true),
    bind(&[ch('x')], Action::RunPackage, "run", true),
    bind(&[ch('+')], Action::InstallByQuery, "add", true),
    bind(&[ch('R')], Action::CycleRepo, "repo", true),
    bind(&[code(KeyCode::Enter)], Action::ToggleDetail, "details", false),
];

/// Keys of the installed packages view.
#[rustfmt::skip]
const INSTALLED: &[Binding] = &[
    bind(&[ch('/')], Action::Filter, "filter", true),
    bind(&[ch(' ')], Action::ToggleMark, "mark", true),
    bind(&[ch('d')], Action::Remove, "remove", true),
    bind(&[ch('s')], Action::SwitchVariant, "switch", true),
    bind(&[ch('u')], Action::UpdateSelected, "update", true),
    bind(&[ch('o')], Action::CycleSort, "sort", true),
    bind(&[code(KeyCode::Enter)], Action::ToggleDetail, "details", true),
];

/// Keys of the updates view.
#[rustfmt::skip]
const UPDATES: &[Binding] = &[
    bind(&[ch(' ')], Action::ToggleMark, "mark", true),
    bind(&[ch('a')], Action::MarkAll, "all", true),
    bind(&[ch('u')], Action::Update, "update", true),
    bind(&[ch('o')], Action::ToggleKeepOld, "keep old", true),
    bind(&[ch('S')], Action::ToggleSkipped, "skipped", true),
];

/// Keys of the apply view.
#[rustfmt::skip]
const APPLY: &[Binding] = &[
    bind(&[code(KeyCode::Enter)], Action::Apply, "apply", true),
    bind(&[ch('p')], Action::TogglePrune, "prune", true),
    bind(&[ch('e')], Action::EditPackages, "edit", true),
    bind(&[ch('n')], Action::CreatePackages, "create", true),
];

/// Keys of the repositories view.
#[rustfmt::skip]
const REPOS: &[Binding] = &[
    bind(&[ch('s')], Action::Sync, "sync", true),
    bind(&[ch('e')], Action::ToggleEnabled, "enable/disable", true),
    bind(&[ch('a')], Action::AddRepo, "add", true),
    bind(&[ch('d')], Action::RemoveRepo, "remove", true),
    bind(&[ch('E')], Action::EditConfig, "edit config", true),
];

/// Keys of the health view.
#[rustfmt::skip]
const HEALTH: &[Binding] = &[
    bind(&[ch('c')], Action::CleanCache, "clean cache", true),
    bind(&[ch('b')], Action::FixSymlinks, "fix symlinks", true),
    bind(&[ch('p')], Action::RemoveBroken, "remove broken", true),
];

/// Keys of the jobs view.
#[rustfmt::skip]
const JOBS: &[Binding] = &[
    bind(&[code(KeyCode::Enter)], Action::FocusDetail, "details", true),
    bind(&[ch('l')], Action::FocusLog, "focus log", true),
    bind(&[ch('x')], Action::AbortSelected, "abort", true),
    bind(&[ch('c')], Action::ClearFinished, "clear finished", true),
];

/// Every binding of one view, without the global keys.
pub(crate) fn bindings(view: ViewId) -> &'static [Binding] {
    match view {
        ViewId::Browse => BROWSE,
        ViewId::Installed => INSTALLED,
        ViewId::Updates => UPDATES,
        ViewId::Apply => APPLY,
        ViewId::Repos => REPOS,
        ViewId::Health => HEALTH,
        ViewId::Jobs => JOBS,
    }
}

/// The action one key press stands for in a view, or `None` when the key is
/// not bound. The view's own table wins over the global table.
pub(crate) fn lookup(view: ViewId, event: KeyEvent) -> Option<Action> {
    let key = normalise(event);
    bindings(view)
        .iter()
        .chain(GLOBAL.iter())
        .find(|binding| binding.keys.contains(&key))
        .map(|binding| binding.action)
}

/// Prepares a key event for comparison with the tables: shift alone on a
/// character key is dropped, since terminals report capital letters that way.
fn normalise(event: KeyEvent) -> Key {
    let mut mods = event.modifiers;
    if mods == KeyModifiers::SHIFT && matches!(event.code, KeyCode::Char(_)) {
        mods = KeyModifiers::NONE;
    }
    Key {
        code: event.code,
        mods,
    }
}

/// Writes one key the way hints and help show it, like "ctrl-r" or "pgdn".
pub(crate) fn key_label(key: Key) -> String {
    match (key.code, key.mods) {
        (KeyCode::Char(c), KeyModifiers::CONTROL) => format!("ctrl-{}", c.to_ascii_lowercase()),
        (KeyCode::Char(' '), _) => "space".to_string(),
        (KeyCode::Char(c), _) => c.to_string(),
        (KeyCode::Enter, _) => "enter".to_string(),
        (KeyCode::Esc, _) => "esc".to_string(),
        (KeyCode::Tab, _) => "tab".to_string(),
        (KeyCode::BackTab, _) => "backtab".to_string(),
        (KeyCode::Up, _) => "↑".to_string(),
        (KeyCode::Down, _) => "↓".to_string(),
        (KeyCode::Left, _) => "←".to_string(),
        (KeyCode::Right, _) => "→".to_string(),
        (KeyCode::Home, _) => "home".to_string(),
        (KeyCode::End, _) => "end".to_string(),
        (KeyCode::PageUp, _) => "pgup".to_string(),
        (KeyCode::PageDown, _) => "pgdn".to_string(),
        (KeyCode::Backspace, _) => "backspace".to_string(),
        (KeyCode::Delete, _) => "del".to_string(),
        (KeyCode::Insert, _) => "ins".to_string(),
        (KeyCode::F(n), _) => format!("f{n}"),
        _ => format!("{:?}", key.code).to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{GLOBAL, Key, bindings, lookup};
    use crate::{action::Action, views::ViewId};

    #[test]
    fn a_shifted_character_matches_its_binding() {
        let shifted = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(lookup(ViewId::Browse, shifted), Some(Action::Bottom));
    }

    #[test]
    fn keymap_has_no_duplicate_keys() {
        for view in ViewId::ALL {
            let mut seen: Vec<Key> = Vec::new();
            for binding in bindings(view).iter().chain(GLOBAL.iter()) {
                for key in binding.keys {
                    assert!(
                        !seen.contains(key),
                        "{key:?} is bound more than once for the {view:?} view",
                    );
                    seen.push(*key);
                }
            }
        }
    }

    #[test]
    fn every_view_has_bar_hints() {
        for view in ViewId::ALL {
            assert!(
                bindings(view).iter().any(|binding| binding.bar),
                "the {view:?} view has no binding for the status bar",
            );
        }
    }
}
