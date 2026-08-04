use beryl_model::{CasItemId, CasThreadId, CasTurnId, SyndicItemId, SyndicTurnId};

use crate::{
    ProviderObservationIssue, SealedProviderFrameReference, SourceEventSequence, SyndicRecordError,
    TurnEndStatus,
};

/// Exact external source tuple carried by a normalized event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasTurnSource {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl CasTurnSource {
    #[must_use]
    pub const fn new(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
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

/// Exact external source tuple carried by one canonical item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasItemSource {
    turn: CasTurnSource,
    item_id: CasItemId,
}

impl CasItemSource {
    #[must_use]
    pub const fn new(turn: CasTurnSource, item_id: CasItemId) -> Self {
        Self { turn, item_id }
    }
    #[must_use]
    pub const fn turn(&self) -> &CasTurnSource {
        &self.turn
    }
    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }
}

/// Closed normalized effect retained by one source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceEventPayload {
    TurnActivated,
    /// One exact sealed start, delta, or completion frame for a provider item.
    ItemFrame {
        item_id: SyndicItemId,
        frame: Box<SealedProviderFrameReference>,
    },
    /// One exact-route sealed observation that conflicts with durable item lifecycle.
    ProviderObservationIssue(Box<ProviderObservationIssue>),
    TurnEnded(TurnEndStatus),
}

impl SourceEventPayload {
    #[must_use]
    pub const fn item_id(&self) -> Option<SyndicItemId> {
        match self {
            Self::ItemFrame { item_id, .. } => Some(*item_id),
            Self::TurnActivated | Self::ProviderObservationIssue(_) | Self::TurnEnded(_) => None,
        }
    }

    #[must_use]
    pub const fn cas_item_id(&self) -> Option<&CasItemId> {
        match self {
            Self::ItemFrame { frame, .. } => Some(frame.frame().item_id()),
            Self::ProviderObservationIssue(issue) => Some(issue.item_id()),
            Self::TurnActivated | Self::TurnEnded(_) => None,
        }
    }

    #[must_use]
    pub const fn requires_external_source(&self) -> bool {
        matches!(
            self,
            Self::TurnActivated | Self::ItemFrame { .. } | Self::ProviderObservationIssue(_)
        )
    }
}

/// One normalized bounded live-source event in exact per-turn sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEventRecord {
    turn_id: SyndicTurnId,
    sequence: SourceEventSequence,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
}

impl SourceEventRecord {
    pub fn new(
        turn_id: SyndicTurnId,
        sequence: SourceEventSequence,
        source: Option<CasTurnSource>,
        payload: SourceEventPayload,
    ) -> Result<Self, SyndicRecordError> {
        if payload.requires_external_source() && source.is_none() {
            return Err(SyndicRecordError::SourceIdentityMismatch);
        }
        Ok(Self {
            turn_id,
            sequence,
            source,
            payload,
        })
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn sequence(&self) -> SourceEventSequence {
        self.sequence
    }
    #[must_use]
    pub const fn source(&self) -> Option<&CasTurnSource> {
        self.source.as_ref()
    }
    #[must_use]
    pub const fn payload(&self) -> &SourceEventPayload {
        &self.payload
    }
}
