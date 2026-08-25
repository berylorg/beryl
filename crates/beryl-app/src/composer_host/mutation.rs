use std::cmp::Ordering;

use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{AssetId, ImageLabelOrdinal};
use gpui_text_input::{
    BindingId, MutationBeginRequest, MutationCommitRequest, MutationCursor, MutationFinishInput,
    MutationIdentity, MutationKind, MutationLane, MutationPage, MutationPageAcceptance,
    MutationPageKey, MutationPageRequest, MutationPositions, MutationTotals, SourceRange,
    SourceRevision,
};
use syndic_storage::{
    DraftCompositePositionV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidateSessionV1, DraftMutationBeginV1, DraftMutationFinishInputV1,
    DraftMutationOperationIdV1, DraftMutationStagingHeadV1, DraftMutationStagingIdentityV1,
    DraftMutationStagingLaneV1, DraftPieceBuildProgressReceiptReferenceV1, DraftPieceDigestV1,
    DraftPieceTransactionOutcomeV1, PreparedDraftMutationStagingBatchV1, PreparedDraftPieceEditV1,
};

use super::request::validate_store;
use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};

pub(super) const COMPOSER_HOST_MAX_MUTATION_TRANSITIONS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostImageMarkerMetadata {
    object_id: gpui_text_input::InlineObjectId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl ComposerHostImageMarkerMetadata {
    pub const fn new(
        object_id: gpui_text_input::InlineObjectId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
    ) -> Self {
        Self {
            object_id,
            label,
            asset_id,
        }
    }

    pub const fn object_id(self) -> gpui_text_input::InlineObjectId {
        self.object_id
    }

    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }

    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ComposerHostMutationIdentity {
    begin: MutationBeginRequest,
}

impl ComposerHostMutationIdentity {
    fn new(begin: MutationBeginRequest) -> Self {
        Self { begin }
    }

    fn operation(&self) -> u64 {
        self.begin.proposal().key().operation().get()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostMutationOutcome {
    Committed {
        binding: ComposerHostBinding,
        positions: MutationPositions,
    },
    Rejected,
    Conflict,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostMutationStatus {
    Admitted,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct ComposerHostRetainedMutationIntent {
    binding: ComposerHostBinding,
    begin: MutationBeginRequest,
    identity: DraftMutationStagingIdentityV1,
}

impl ComposerHostRetainedMutationIntent {
    pub const fn binding(&self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn begin(&self) -> MutationBeginRequest {
        self.begin
    }

    pub const fn identity(&self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
}

#[derive(Clone, Copy)]
struct WidgetLaneFrontier {
    next_cursor: MutationCursor,
    next_ordinal: u64,
    cumulative_identity: MutationIdentity,
    totals: MutationTotals,
    last_page: Option<WidgetPageReceipt>,
}

#[derive(Clone, Copy)]
struct WidgetPageReceipt {
    key: MutationPageKey,
    page_identity: MutationIdentity,
    cumulative_identity: MutationIdentity,
}

impl WidgetLaneFrontier {
    const fn initial(cursor: MutationCursor) -> Self {
        Self {
            next_cursor: cursor,
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            totals: MutationTotals {
                pages: 0,
                items: 0,
                retained_bytes: 0,
                inserted_bytes: 0,
                inserted_line_breaks: 0,
                objects: 0,
                object_bytes: 0,
                presentation_bytes: 0,
            },
            last_page: None,
        }
    }

    fn prevalidate(self, page: &MutationPage) -> Result<WidgetPageDisposition, ComposerHostError> {
        let key = page.key();
        if key.ordinal() < self.next_ordinal {
            let Some(last) = self.last_page else {
                return Err(ComposerHostError::StaleRequestIdentity);
            };
            if key != last.key {
                return Err(ComposerHostError::StaleRequestIdentity);
            }
            return if page.page_identity() == last.page_identity
                && page.cumulative_identity() == last.cumulative_identity
            {
                Ok(WidgetPageDisposition::Replay)
            } else {
                Err(ComposerHostError::MutationIdentityCollision)
            };
        }
        if key.cursor() != self.next_cursor
            || key.ordinal() != self.next_ordinal
            || key.prior() != self.cumulative_identity
            || page.items().is_empty()
            || page.items().len() > 256
            || page.totals().retained_bytes > 65_536
        {
            return Err(ComposerHostError::MutationMalformed);
        }
        let canonical = MutationPage::new(key, page.next_cursor(), page.items().to_vec())
            .map_err(|_| ComposerHostError::MutationMalformed)?;
        if canonical != *page {
            return Err(ComposerHostError::MutationMalformed);
        }
        let next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(ComposerHostError::MutationMalformed)?;
        let totals = checked_add_totals(self.totals, page.totals())
            .ok_or(ComposerHostError::MutationMalformed)?;
        Ok(WidgetPageDisposition::Accepted {
            frontier: Self {
                next_cursor: page.next_cursor(),
                next_ordinal,
                cumulative_identity: page.cumulative_identity(),
                totals,
                last_page: Some(WidgetPageReceipt {
                    key,
                    page_identity: page.page_identity(),
                    cumulative_identity: page.cumulative_identity(),
                }),
            },
            acceptance: MutationPageAcceptance::Accepted {
                next_cursor: page.next_cursor(),
                next_ordinal,
                cumulative_identity: page.cumulative_identity(),
                totals,
            },
        })
    }

    fn matches_finish(self, finish: gpui_text_input::MutationStreamFinish) -> bool {
        self.next_cursor.get() == finish.next_cursor.get()
            && self.next_ordinal == finish.next_ordinal
            && self.cumulative_identity.words() == finish.cumulative_identity.words()
            && self.totals.pages == finish.totals.pages
            && self.totals.items == finish.totals.items
            && self.totals.retained_bytes == finish.totals.retained_bytes
            && self.totals.inserted_bytes == finish.totals.inserted_bytes
            && self.totals.inserted_line_breaks == finish.totals.inserted_line_breaks
            && self.totals.objects == finish.totals.objects
            && self.totals.object_bytes == finish.totals.object_bytes
            && self.totals.presentation_bytes == finish.totals.presentation_bytes
    }
}

enum WidgetPageDisposition {
    Replay,
    Accepted {
        frontier: WidgetLaneFrontier,
        acceptance: MutationPageAcceptance,
    },
}

enum ComposerHostMutationPhase {
    Receiving,
    Finished,
    Building {
        prepared: PreparedDraftPieceEditV1,
        endpoint: DraftPieceBuildProgressReceiptReferenceV1,
    },
}

struct ComposerHostInFlightPage {
    prepared: PreparedDraftMutationStagingBatchV1,
    kind: ComposerHostInFlightPageKind,
    #[cfg(feature = "test-faults")]
    custody_serial: u64,
    fragment_count: u64,
    fragment_chain: DraftPieceDigestV1,
    proposal_envelope_applied: bool,
    last_proposal_range: Option<(DraftCompositePositionV1, DraftCompositePositionV1)>,
    remaining_proposal_range: SourceRange,
}

enum ComposerHostInFlightPageKind {
    Widget {
        request: MutationPageRequest,
        lane: MutationLane,
        frontier: WidgetLaneFrontier,
        acceptance: MutationPageAcceptance,
    },
    Internal {
        finish: MutationFinishInput,
    },
}

pub(super) struct ComposerHostMutationCoordinator {
    binding: ComposerHostBinding,
    begin: MutationBeginRequest,
    identity: DraftMutationStagingIdentityV1,
    session: DraftEditorCandidateSessionV1,
    head: DraftMutationStagingHeadV1,
    source: WidgetLaneFrontier,
    proposal: WidgetLaneFrontier,
    phase: ComposerHostMutationPhase,
    fragment_count: u64,
    fragment_chain: DraftPieceDigestV1,
    proposal_envelope_applied: bool,
    last_proposal_range: Option<(DraftCompositePositionV1, DraftCompositePositionV1)>,
    remaining_proposal_range: SourceRange,
    in_flight_page: Option<ComposerHostInFlightPage>,
    finish_input: Option<MutationFinishInput>,
    intended: Option<MutationPositions>,
    pub(super) detached: bool,
}

impl ComposerHostMutationCoordinator {
    fn intent(&self) -> ComposerHostRetainedMutationIntent {
        ComposerHostRetainedMutationIntent {
            binding: self.binding,
            begin: self.begin,
            identity: self.identity,
        }
    }

    const fn lane(&self, lane: MutationLane) -> WidgetLaneFrontier {
        match lane {
            MutationLane::Source => self.source,
            MutationLane::Proposal => self.proposal,
        }
    }

    fn lane_mut(&mut self, lane: MutationLane) -> &mut WidgetLaneFrontier {
        match lane {
            MutationLane::Source => &mut self.source,
            MutationLane::Proposal => &mut self.proposal,
        }
    }

    fn retain_finish_input(
        &mut self,
        finish: MutationFinishInput,
    ) -> Result<MutationFinishInput, ComposerHostError> {
        if let Some(retained) = self.finish_input {
            if retained != finish {
                return Err(ComposerHostError::MutationIdentityCollision);
            }
            return Ok(retained);
        }
        self.finish_input = Some(finish);
        Ok(finish)
    }
}

#[cfg(feature = "test-faults")]
impl SyndicComposerHost {
    pub fn test_mutation_in_flight_custody(&self) -> Option<(u64, bool)> {
        let Some(ComposerHostPendingMutation::Active(pending)) = &self.pending_mutation else {
            return None;
        };
        let page = pending.in_flight_page.as_ref()?;
        Some((
            page.custody_serial,
            matches!(&page.kind, ComposerHostInFlightPageKind::Internal { .. }),
        ))
    }

    pub fn test_mutation_in_flight_finish(&self) -> Option<MutationFinishInput> {
        let Some(ComposerHostPendingMutation::Active(pending)) = &self.pending_mutation else {
            return None;
        };
        let page = pending.in_flight_page.as_ref()?;
        match &page.kind {
            ComposerHostInFlightPageKind::Internal { finish } => Some(*finish),
            ComposerHostInFlightPageKind::Widget { .. } => None,
        }
    }
}

pub(super) enum ComposerHostPendingMutation {
    Active(Box<ComposerHostMutationCoordinator>),
    Terminal(Box<ComposerHostTerminalMutation>),
    Unavailable(Box<ComposerHostRetainedMutationIntent>),
}

impl ComposerHostPendingMutation {
    pub(super) fn key(&self) -> gpui_text_input::MutationKey {
        match self {
            Self::Active(pending) => pending.begin.proposal().key(),
            Self::Terminal(terminal) => terminal.key,
            Self::Unavailable(intent) => intent.begin.proposal().key(),
        }
    }
}

pub(super) struct ComposerHostTerminalMutation {
    binding: ComposerHostBinding,
    key: gpui_text_input::MutationKey,
    outcome: ComposerHostMutationOutcome,
    pub(super) detached: bool,
}

enum StagingCommandResult {
    Target,
    Source,
    Terminal,
}

enum BuildCommandResult {
    Pending(DraftPieceBuildProgressReceiptReferenceV1),
    Terminal(DraftPieceTransactionOutcomeV1),
}

mod drive;
mod execution;
mod settlement;
mod translation;

use translation::canonical_position;

fn checked_add_totals(left: MutationTotals, right: MutationTotals) -> Option<MutationTotals> {
    Some(MutationTotals {
        pages: left.pages.checked_add(right.pages)?,
        items: left.items.checked_add(right.items)?,
        retained_bytes: left.retained_bytes.checked_add(right.retained_bytes)?,
        inserted_bytes: left.inserted_bytes.checked_add(right.inserted_bytes)?,
        inserted_line_breaks: left
            .inserted_line_breaks
            .checked_add(right.inserted_line_breaks)?,
        objects: left.objects.checked_add(right.objects)?,
        object_bytes: left.object_bytes.checked_add(right.object_bytes)?,
        presentation_bytes: left
            .presentation_bytes
            .checked_add(right.presentation_bytes)?,
    })
}
