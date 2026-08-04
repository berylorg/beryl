use beryl_model::SyndicTurnId;

/// Immutable parent descriptor for one submitted turn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConversationParent {
    /// The submitted turn begins a root conversation path.
    Root,
    /// The submitted turn continues one exact historical turn.
    Turn(SyndicTurnId),
}

impl ConversationParent {
    /// Converts an optional persisted parent identity into the closed descriptor.
    #[must_use]
    pub const fn from_turn(parent: Option<SyndicTurnId>) -> Self {
        match parent {
            Some(parent) => Self::Turn(parent),
            None => Self::Root,
        }
    }

    /// Returns the exact parent turn, or `None` for a root.
    #[must_use]
    pub const fn turn(self) -> Option<SyndicTurnId> {
        match self {
            Self::Root => None,
            Self::Turn(parent) => Some(parent),
        }
    }
}
