use beryl_model::{CasThreadId, CasTurnId};

use super::ThreadActiveFlags;

/// Closed loaded-thread status retained from one ordered `thread/status/changed` control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadedThreadStatus {
    Idle,
    SystemError,
    Active { active_flags: ThreadActiveFlags },
}

/// Compact exact-thread status observation from the foreground stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadStatusChanged {
    thread_id: CasThreadId,
    status: LoadedThreadStatus,
}

/// Compact exact identity from one ordered `thread/closed` notification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadClosed {
    thread_id: CasThreadId,
}

/// Compact exact identity from one ordered `turn/started` notification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnStarted {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl LoadedThreadStatus {
    #[must_use]
    pub const fn active(active_flags: ThreadActiveFlags) -> Self {
        Self::Active { active_flags }
    }

    #[must_use]
    pub const fn active_flags(self) -> Option<ThreadActiveFlags> {
        match self {
            Self::Active { active_flags } => Some(active_flags),
            Self::Idle | Self::SystemError => None,
        }
    }
}

impl ThreadStatusChanged {
    pub(crate) const fn decoded(thread_id: CasThreadId, status: LoadedThreadStatus) -> Self {
        Self { thread_id, status }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn status(&self) -> LoadedThreadStatus {
        self.status
    }
}

impl ThreadClosed {
    pub(crate) const fn decoded(thread_id: CasThreadId) -> Self {
        Self { thread_id }
    }

    /// Returns the validated exact CAS thread identity carried by the notification.
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }
}

impl TurnStarted {
    pub(crate) const fn decoded(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}
