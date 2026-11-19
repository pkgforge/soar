//! The views the tab bar switches between.

pub(crate) mod apply;
pub(crate) mod browse;
pub(crate) mod health;
pub(crate) mod installed;
pub(crate) mod jobs;
pub(crate) mod repos;
pub(crate) mod updates;

/// One screen of the interface, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ViewId {
    /// The package catalog with its filter and detail pane.
    Browse,
    /// The packages installed in this profile.
    Installed,
    /// The packages with a newer version available.
    Updates,
    /// The declarative packages.toml and the changes it would make.
    Apply,
    /// The enabled repositories and their sync state.
    Repos,
    /// The checks for a healthy installation.
    Health,
    /// The running and finished jobs, with their log.
    Jobs,
}

impl ViewId {
    /// Every view in tab order.
    pub(crate) const ALL: [ViewId; 7] = [
        ViewId::Browse,
        ViewId::Installed,
        ViewId::Updates,
        ViewId::Apply,
        ViewId::Repos,
        ViewId::Health,
        ViewId::Jobs,
    ];

    /// The name shown on the tab.
    pub(crate) fn title(self) -> &'static str {
        match self {
            ViewId::Browse => "Browse",
            ViewId::Installed => "Installed",
            ViewId::Updates => "Updates",
            ViewId::Apply => "Apply",
            ViewId::Repos => "Repos",
            ViewId::Health => "Health",
            ViewId::Jobs => "Jobs",
        }
    }

    /// The position of this view in [`Self::ALL`].
    pub(crate) fn index(self) -> usize {
        match self {
            ViewId::Browse => 0,
            ViewId::Installed => 1,
            ViewId::Updates => 2,
            ViewId::Apply => 3,
            ViewId::Repos => 4,
            ViewId::Health => 5,
            ViewId::Jobs => 6,
        }
    }

    /// The view after this one, wrapping around to the first.
    pub(crate) fn next(self) -> ViewId {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// The view before this one, wrapping around to the last.
    pub(crate) fn prev(self) -> ViewId {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::ViewId;

    #[test]
    fn next_from_jobs_wraps_to_browse() {
        assert_eq!(ViewId::Jobs.next(), ViewId::Browse);
    }

    #[test]
    fn prev_from_browse_wraps_to_jobs() {
        assert_eq!(ViewId::Browse.prev(), ViewId::Jobs);
    }

    #[test]
    fn index_matches_position_in_all() {
        for (position, view) in ViewId::ALL.iter().enumerate() {
            assert_eq!(view.index(), position);
        }
    }
}
