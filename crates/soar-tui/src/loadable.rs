//! Data that background work fills in over time.

/// The lifecycle of one piece of data a view shows.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) enum Loadable<T> {
    /// Nothing has been requested yet.
    #[default]
    Idle,
    /// A request is running.
    Loading,
    /// The request succeeded.
    Ready(T),
    /// The request failed.
    Failed(String),
}

#[allow(dead_code)]
impl<T> Loadable<T> {
    /// Whether nothing has been requested yet.
    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Whether a request is running.
    pub(crate) fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    /// The loaded data, when the request succeeded.
    pub(crate) fn ready(&self) -> Option<&T> {
        if let Self::Ready(data) = self {
            Some(data)
        } else {
            None
        }
    }

    /// The loaded data, when the request succeeded.
    pub(crate) fn ready_mut(&mut self) -> Option<&mut T> {
        if let Self::Ready(data) = self {
            Some(data)
        } else {
            None
        }
    }
}
