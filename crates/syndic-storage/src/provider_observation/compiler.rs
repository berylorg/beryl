//! Constant-resident sealed-observation to ProviderItemV1 compilation.

mod encode;
mod replay;
mod stage;

use beryl_model::{CasItemId, RevisionError, SyndicContentId, SyndicItemId, SyndicTurnId};

use crate::{
    CasItemSource, CasTurnSource, ProviderFrameObservationSummaryV1, ProviderItemBuildRecord,
    ProviderItemKind, ProviderItemValidationError, ProviderObservationIssue,
    ProviderObservationIssueReason, ProviderStorageRecordError, SealedProviderFrameReference,
    SourceEventSequence, SyndicPointReadLimit, SyndicRecordError, SyndicValueError,
};

use super::{
    BoundProviderObservation, PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES, ProviderField,
    ProviderObservationBegin, ProviderObservationCursorError, ProviderObservationRoute,
    ProviderObservationValidatorError, cursor::ProviderObservationReplay,
};

pub use stage::stage_provider_observation_frame;

/// Immutable destination identities and published predecessor for one observation frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationFramePreparationPlan {
    item_id: SyndicItemId,
    turn_id: SyndicTurnId,
    source: CasItemSource,
    source_event: SourceEventSequence,
    first_content_id: Option<SyndicContentId>,
    prior: Option<SealedProviderFrameReference>,
}

impl ProviderObservationFramePreparationPlan {
    /// Plans the first frame under a caller-owned fresh content identity.
    #[must_use]
    pub const fn first(
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        source: CasItemSource,
        source_event: SourceEventSequence,
        fresh_content_id: SyndicContentId,
    ) -> Self {
        Self {
            item_id,
            turn_id,
            source,
            source_event,
            first_content_id: Some(fresh_content_id),
            prior: None,
        }
    }

    /// Plans the frame immediately following `prior` in the same provider content stream.
    #[must_use]
    pub const fn subsequent(
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        source: CasItemSource,
        source_event: SourceEventSequence,
        prior: SealedProviderFrameReference,
    ) -> Self {
        Self {
            item_id,
            turn_id,
            source,
            source_event,
            first_content_id: None,
            prior: Some(prior),
        }
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn source(&self) -> &CasItemSource {
        &self.source
    }

    #[must_use]
    pub const fn source_event(&self) -> SourceEventSequence {
        self.source_event
    }

    #[must_use]
    pub const fn prior(&self) -> Option<&SealedProviderFrameReference> {
        self.prior.as_ref()
    }
}

/// Prepared immutable observation authority plus its exact initial provider-item build.
pub struct PreparedProviderObservationFrame {
    replay: ProviderObservationReplay,
    initial_build: ProviderItemBuildRecord,
}

/// Inspected bounded identity and lifecycle facts retaining sole bound replay authority.
pub struct InspectedProviderObservation {
    replay: ProviderObservationReplay,
    item_id: CasItemId,
    item_kind: ProviderItemKind,
    lifecycle: ProviderFrameObservationSummaryV1,
}

impl InspectedProviderObservation {
    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    /// Returns the normalized immutable provider-item kind selected by the observation grammar.
    #[must_use]
    pub const fn item_kind(&self) -> ProviderItemKind {
        self.item_kind
    }

    /// Returns the exact normalized lifecycle summary carried by the observation.
    #[must_use]
    pub const fn lifecycle(&self) -> ProviderFrameObservationSummaryV1 {
        self.lifecycle
    }

    #[must_use]
    pub const fn begin(&self) -> ProviderObservationBegin {
        self.replay.build().begin()
    }

    #[must_use]
    pub const fn route(&self) -> &ProviderObservationRoute {
        self.replay.route()
    }

    #[must_use]
    pub const fn history_support(&self) -> crate::ProviderFrameHistorySupportV1 {
        self.replay.build().history_support()
    }

    /// Consumes inspected authority into a compact candidate durable lifecycle issue.
    ///
    /// The live-source mutation independently proves the supplied reason against its exact
    /// preceding source frontier before it may publish the issue.
    #[must_use]
    pub fn into_issue(self, reason: ProviderObservationIssueReason) -> ProviderObservationIssue {
        let source = CasTurnSource::new(
            self.replay.route().thread_id().clone(),
            self.replay.route().turn_id().clone(),
        );
        ProviderObservationIssue::from_inspected(
            self.replay.build(),
            source,
            self.item_id,
            self.item_kind,
            self.lifecycle,
            reason,
        )
    }

    /// Explicitly abandons this inspected unpublished observation.
    pub fn abandon(self) {}
}

impl PreparedProviderObservationFrame {
    #[must_use]
    pub const fn initial_build(&self) -> &ProviderItemBuildRecord {
        &self.initial_build
    }

    #[must_use]
    pub const fn target(&self) -> &SealedProviderFrameReference {
        self.initial_build.target()
    }

    #[must_use]
    pub const fn route(&self) -> &ProviderObservationRoute {
        self.replay.route()
    }
}

/// Semantic disagreement found while replaying a sealed typed observation.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderObservationFrameSemanticError {
    #[error("the provider observation route disagrees with the destination source route")]
    RouteMismatch,
    #[error("the provider observation omitted selected field {field:?}")]
    MissingField { field: ProviderField },
    #[error("the provider observation selected field {field:?} more than once")]
    DuplicateFieldSelection { field: ProviderField },
    #[error("the provider observation selected field {field:?} with an incompatible value")]
    ValueMismatch { field: ProviderField },
    #[error("the provider observation item identity differs from the destination source identity")]
    ItemIdentityMismatch,
    #[error("the provider observation structured traversal is inconsistent")]
    StructuredTraversalMismatch,
    #[error("the provider observation typed traversal is inconsistent")]
    TraversalMismatch,
    #[error("the provider observation replay disagrees with its sealed build")]
    ReplayMismatch,
}

/// Why one sealed observation could not be prepared as an exact provider frame.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationFramePreparationError {
    #[error(transparent)]
    Cursor(#[from] ProviderObservationCursorError),
    #[error(transparent)]
    ObservationValidation(#[from] ProviderObservationValidatorError),
    #[error(transparent)]
    Semantic(#[from] ProviderObservationFrameSemanticError),
    #[error(transparent)]
    FrameValidation(#[from] ProviderItemValidationError),
    #[error(transparent)]
    Record(#[from] SyndicRecordError),
    #[error(transparent)]
    Value(#[from] SyndicValueError),
    #[error(transparent)]
    Revision(#[from] RevisionError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
    #[error("provider-observation frame preparation frontier overflowed")]
    FrontierOverflow,
    #[error("provider-observation narrative traversal disagreed with frame preparation")]
    NarrativeTraversalMismatch,
    #[error("provider-observation narrative kind emitted a non-narrative span")]
    NarrativeRoleMismatch,
}

/// Exact durable result of one prepared provider-observation frame staging traversal.
#[derive(Debug)]
pub enum ProviderObservationFrameStageOutcome {
    /// The supplied build was already sealed, so no command was issued.
    Unchanged { value: ProviderItemBuildRecord },
    /// The offered batch definitely did not commit.
    NotCommitted { evidence: beryl_home_store::CommandError },
    /// The returned build is the exact durable successor of the last committed batch.
    Committed {
        value: ProviderItemBuildRecord,
        receipt: beryl_home_store::CommitReceipt,
        later_failure: Option<beryl_home_store::CommandError>,
    },
    /// The offered batch may have committed; no local successor is inferred.
    Indeterminate {
        failure: beryl_home_store::CommandError,
        reconciliation: beryl_home_store::ReconciliationDescriptor,
    },
}

/// Why exact replay could not stage the previously prepared observation frame.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationFrameStageError {
    #[error(transparent)]
    Cursor(#[from] ProviderObservationCursorError),
    #[error(transparent)]
    ObservationValidation(#[from] ProviderObservationValidatorError),
    #[error(transparent)]
    Semantic(#[from] ProviderObservationFrameSemanticError),
    #[error(transparent)]
    FrameValidation(#[from] ProviderItemValidationError),
    #[error(transparent)]
    Record(#[from] SyndicRecordError),
    #[error(transparent)]
    Value(#[from] SyndicValueError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
    #[error(transparent)]
    Batch(#[from] crate::ProviderFrameStageBatchError),
    #[error("the current provider build belongs to another prepared observation frame")]
    BuildPlanMismatch,
    #[error("the resumed provider chunk frontier does not match deterministic observation replay")]
    ResumeChunkFrontierMismatch,
    #[error(
        "the resumed provider narrative frontier does not match deterministic observation replay"
    )]
    ResumeNarrativeFrontierMismatch,
    #[error("provider-observation frame staging frontier overflowed")]
    FrontierOverflow,
    #[error("provider-observation narrative kind emitted a non-narrative span")]
    NarrativeRoleMismatch,
    #[error("provider-observation staging traversal did not equal its prepared target")]
    StagingTraversalMismatch,
    #[error("provider-observation staging traversal ended before every target frontier")]
    IncompleteStagingTraversal,
    #[error("provider-observation frame staging reached a committed batch with a later failure")]
    CommittedLaterFailure {
        value: ProviderItemBuildRecord,
        receipt: beryl_home_store::CommitReceipt,
        later_failure: beryl_home_store::CommandError,
    },
    #[error("provider-observation frame staging batch definitely did not commit")]
    NotCommitted {
        evidence: beryl_home_store::CommandError,
    },
    #[error("provider-observation frame staging batch has an indeterminate durable outcome")]
    Indeterminate {
        failure: beryl_home_store::CommandError,
        reconciliation: beryl_home_store::ReconciliationDescriptor,
    },
}

/// Consumes route-bound authority and extracts only its bounded exact item/lifecycle facts.
pub fn inspect_provider_observation(
    storage: &crate::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    bound: BoundProviderObservation,
    limit: SyndicPointReadLimit,
) -> Result<InspectedProviderObservation, ProviderObservationFramePreparationError> {
    let replay = bound.into_replay();
    let reader = replay::ObservationReplayReader::new(storage, store, &replay, limit);
    let selector = replay::TextSelector::Field(replay::FieldSelector::top(ProviderField::ItemId));
    let summary = reader
        .text_summary(selector)
        .map_err(replay::ReplayError::preparation)?;
    let length = usize::try_from(summary.bytes)
        .ok()
        .filter(|length| *length <= PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES)
        .ok_or(ProviderObservationFrameSemanticError::ValueMismatch {
            field: ProviderField::ItemId,
        })?;
    let mut bytes = [0_u8; PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES];
    let mut offset = 0_usize;
    reader
        .write_text(selector, |fragment| {
            let end = offset.checked_add(fragment.len()).ok_or(())?;
            let destination = bytes.get_mut(offset..end).ok_or(())?;
            destination.copy_from_slice(fragment);
            offset = end;
            Ok(())
        })
        .map_err(|error| match error {
            replay::ReplayWriteError::Replay(error) => error.preparation(),
            replay::ReplayWriteError::Output(()) => {
                ProviderObservationFrameSemanticError::ValueMismatch {
                    field: ProviderField::ItemId,
                }
                .into()
            }
        })?;
    if offset != length {
        return Err(ProviderObservationFrameSemanticError::ValueMismatch {
            field: ProviderField::ItemId,
        }
        .into());
    }
    let text = std::str::from_utf8(&bytes[..length]).map_err(|_| {
        ProviderObservationFrameSemanticError::ValueMismatch {
            field: ProviderField::ItemId,
        }
    })?;
    let item_id =
        CasItemId::new(text).map_err(|_| ProviderObservationFrameSemanticError::ValueMismatch {
            field: ProviderField::ItemId,
        })?;
    let item_kind = stage::normalized_item_kind(reader.begin());
    let lifecycle = stage::observation_summary(&reader)?;
    Ok(InspectedProviderObservation {
        replay,
        item_id,
        item_kind,
        lifecycle,
    })
}

/// Consumes one route-bound sealed observation and prepares its exact immutable frame target.
pub fn prepare_provider_observation_frame(
    storage: &crate::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    inspected: InspectedProviderObservation,
    plan: ProviderObservationFramePreparationPlan,
    limit: SyndicPointReadLimit,
) -> Result<PreparedProviderObservationFrame, ProviderObservationFramePreparationError> {
    let InspectedProviderObservation {
        replay,
        item_id,
        item_kind: _,
        lifecycle: _,
    } = inspected;
    if replay.route().thread_id() != plan.source.turn().thread_id()
        || replay.route().turn_id() != plan.source.turn().turn_id()
    {
        return Err(ProviderObservationFrameSemanticError::RouteMismatch.into());
    }
    if &item_id != plan.source.item_id() {
        return Err(ProviderObservationFrameSemanticError::ItemIdentityMismatch.into());
    }
    let initial_build = stage::prepare(storage, store, &replay, plan, limit)?;
    Ok(PreparedProviderObservationFrame {
        replay,
        initial_build,
    })
}
