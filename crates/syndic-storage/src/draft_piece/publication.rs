use std::{error::Error, fmt};

use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationFailure, ReconciliationReservation, ReconciliationResolution,
};
use beryl_model::DomainRevision;
use beryl_model::{
    OrderedMarkerAssetSummaryV1, SequentialMarkerSummaryV1, ordered_marker_asset_digest_seed,
    sequential_marker_digest_seed,
};

use crate::codec::{
    DraftByThreadCodec, DraftByThreadFamily, DraftsCodec, HistorySummariesCodec,
    HistorySummariesFamily, ThreadsFamily,
};
use crate::domain::{SyndicDomain, SyndicStorage};
use crate::mutation::{current_draft, point, required};
use crate::{
    DraftByThreadRecord, DraftRecord, HistorySummaryRecord, SyndicMutationError, SyndicReadError,
    SyndicTimestamp,
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidatePublicationSourceCaptureRequestV1 {
    selector: DraftEditorCurrentSelectorV1,
    candidate: DraftEditorCandidateActivationBindingV1,
    operation_id: DraftPieceOperationIdV1,
    published_at: SyndicTimestamp,
}

impl DraftEditorCandidatePublicationSourceCaptureRequestV1 {
    pub const fn new(
        selector: DraftEditorCurrentSelectorV1,
        candidate: DraftEditorCandidateActivationBindingV1,
        operation_id: DraftPieceOperationIdV1,
        published_at: SyndicTimestamp,
    ) -> Self {
        Self {
            selector,
            candidate,
            operation_id,
            published_at,
        }
    }

    pub const fn selector(self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }

    pub const fn candidate(self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }

    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub const fn published_at(self) -> SyndicTimestamp {
        self.published_at
    }
}

pub struct CapturedDraftEditorCandidatePublicationSourceV1 {
    storage: SyndicStorage,
    request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
    source_frontier: DraftEditHistoryFrontierV1,
    captured_head: DraftEditorCandidateSessionV1,
}

pub struct DraftEditorCandidatePublicationSourcePreparationErrorV1 {
    source: CapturedDraftEditorCandidatePublicationSourceV1,
    error: DraftEditorCandidatePublicationCommandErrorV1,
}

impl DraftEditorCandidatePublicationSourcePreparationErrorV1 {
    pub fn into_parts(
        self,
    ) -> (
        CapturedDraftEditorCandidatePublicationSourceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    ) {
        (self.source, self.error)
    }
}

impl fmt::Debug for DraftEditorCandidatePublicationSourcePreparationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DraftEditorCandidatePublicationSourcePreparationErrorV1")
            .field("source", &"[opaque]")
            .field("error", &self.error)
            .finish()
    }
}

impl fmt::Display for DraftEditorCandidatePublicationSourcePreparationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl Error for DraftEditorCandidatePublicationSourcePreparationErrorV1 {}

#[derive(Clone)]
pub struct PreparedDraftEditorCandidatePublicationV1 {
    request: DraftEditorCandidatePublicationRequestV1,
    canonical_request: Vec<u8>,
    source_frontier: DraftEditHistoryFrontierV1,
    captured_frontier: DraftEditHistoryFrontierV1,
    captured_head: DraftEditorCandidateSessionV1,
    initially_absent: bool,
}

impl PreparedDraftEditorCandidatePublicationV1 {
    pub const fn request(&self) -> DraftEditorCandidatePublicationRequestV1 {
        self.request
    }
    pub fn canonical_request(&self) -> &[u8] {
        &self.canonical_request
    }
    pub const fn captured_frontier(&self) -> &DraftEditHistoryFrontierV1 {
        &self.captured_frontier
    }
    pub const fn marker_commitment(&self) -> DraftMarkerCommitmentV1 {
        self.request.candidate().root().marker_commitment()
    }
}

#[derive(Clone)]
pub struct PreparedDraftEditorCandidateSessionDisposeV1 {
    request: DraftEditorCandidateSessionDisposeRequestV1,
    canonical_request: Vec<u8>,
    frontier: DraftEditHistoryFrontierV1,
    initially_absent: bool,
}

impl PreparedDraftEditorCandidateSessionDisposeV1 {
    pub const fn request(&self) -> DraftEditorCandidateSessionDisposeRequestV1 {
        self.request
    }
    pub fn canonical_request(&self) -> &[u8] {
        &self.canonical_request
    }
}

#[derive(Debug)]
pub enum DraftEditorCandidatePublicationCommandErrorV1 {
    Read(SyndicReadError),
    Reconciliation(ReconciliationFailure),
    NotCommitted,
    ActiveOperation,
    Invariant,
}

impl fmt::Display for DraftEditorCandidatePublicationCommandErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => e.fmt(f),
            Self::Reconciliation(e) => e.fmt(f),
            Self::NotCommitted => f.write_str("candidate command was not committed"),
            Self::ActiveOperation => f.write_str("candidate session has active operation custody"),
            Self::Invariant => f.write_str("invalid candidate publication or disposal closure"),
        }
    }
}

impl Error for DraftEditorCandidatePublicationCommandErrorV1 {}
impl From<SyndicReadError> for DraftEditorCandidatePublicationCommandErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::Read(value)
    }
}

fn publication_evidence_is_exact(request: DraftEditorCandidatePublicationRequestV1) -> bool {
    let prior = request.selector().root().marker_commitment();
    let captured = request.candidate().root().marker_commitment();
    let changed = prior != captured;
    let marker_count = captured.marker_count();
    let maximum = captured.maximum_image_label();
    let seal_is_exact = |proof: DraftMarkerSealProofV1| {
        proof.source() == request.candidate().root()
            && proof.commitment() == captured
            && proof.sequential().marker_count() == marker_count
            && proof.sequential().maximum_image_label() == maximum
            && proof.ordered_assets().marker_count() == marker_count
    };
    match request.evidence() {
        DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
            seal_proof,
            asset_proof,
        } => {
            changed
                && marker_count != 0
                && seal_is_exact(seal_proof)
                && asset_proof.sequential() == seal_proof.sequential()
                && asset_proof.ordered_assets() == seal_proof.ordered_assets()
        }
        DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty { seal_proof } => {
            let empty = SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None)
                .expect("canonical empty sequential marker summary is valid");
            let empty_assets =
                OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest_seed(), 0);
            changed
                && marker_count == 0
                && seal_is_exact(seal_proof)
                && seal_proof.sequential() == empty
                && seal_proof.ordered_assets() == empty_assets
        }
        DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty { asset_proof } => {
            !changed
                && marker_count != 0
                && asset_proof.sequential().marker_count() == marker_count
                && asset_proof.sequential().maximum_image_label() == maximum
                && asset_proof.ordered_assets().marker_count() == marker_count
        }
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty => !changed && marker_count == 0,
    }
}

#[derive(Clone)]
struct PublicationMutation {
    prepared: PreparedDraftEditorCandidatePublicationV1,
}

#[derive(Clone)]
struct DisposalMutation {
    prepared: PreparedDraftEditorCandidateSessionDisposeV1,
}

fn publication_key(
    request: DraftEditorCandidatePublicationRequestV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::publication_receipt(
        request.selector().draft_id(),
        request.session_id(),
        request.operation_id(),
    )
}

fn disposal_key(
    request: DraftEditorCandidateSessionDisposeRequestV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
        request.draft_id(),
        request.session_id(),
        request.operation_id(),
    )
}

fn session_key(
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id)
}

fn captured_head(
    head: &DraftEditorCandidateSessionV1,
    request: DraftEditorCandidatePublicationRequestV1,
) -> Option<DraftEditorCandidateSessionV1> {
    let pair = request.candidate();
    let session_generation = head
        .session_generation()
        .max(request.candidate_generation().checked_add(1)?);
    let value = DraftEditorCandidateSessionV1::from_parts(
        head.thread_id(),
        head.draft_id(),
        head.session_id(),
        head.open_operation_id(),
        session_generation,
        head.durable_base_selector_revision(),
        head.durable_base_root(),
        head.durable_base_history(),
        head.published_candidate_generation(),
        head.published_selector_revision(),
        head.published_root(),
        head.published_history(),
        request.candidate_generation(),
        pair.root(),
        pair.history(),
        head.dirty_generation(),
        pair.root().summary().logical_extent(),
        DraftEditorCandidateSessionLifecycleV1::Active,
        None,
    );
    value.is_coherent().then_some(value)
}

fn publication_source_matches(
    current: &DraftEditorCandidateSessionV1,
    captured: &DraftEditorCandidateSessionV1,
) -> bool {
    current.thread_id() == captured.thread_id()
        && current.draft_id() == captured.draft_id()
        && current.session_id() == captured.session_id()
        && current.open_operation_id() == captured.open_operation_id()
        && current.durable_base_selector_revision() == captured.durable_base_selector_revision()
        && current.durable_base_root() == captured.durable_base_root()
        && current.durable_base_history() == captured.durable_base_history()
        && current.published_candidate_generation() == captured.published_candidate_generation()
        && current.published_selector_revision() == captured.published_selector_revision()
        && current.published_root() == captured.published_root()
        && current.published_history() == captured.published_history()
        && current.newest_candidate_generation() >= captured.newest_candidate_generation()
}

fn captured_publication_source_matches(
    current: &DraftEditorCandidateSessionV1,
    captured: &DraftEditorCandidateSessionV1,
) -> bool {
    current.thread_id() == captured.thread_id()
        && current.draft_id() == captured.draft_id()
        && current.session_id() == captured.session_id()
        && current.open_operation_id() == captured.open_operation_id()
        && current.durable_base_selector_revision() == captured.durable_base_selector_revision()
        && current.durable_base_root() == captured.durable_base_root()
        && current.durable_base_history() == captured.durable_base_history()
        && current.published_candidate_generation() >= captured.published_candidate_generation()
        && current.newest_candidate_generation() >= captured.newest_candidate_generation()
}

fn current_selector(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: beryl_model::SyndicThreadId,
) -> Result<DraftEditorCurrentSelectorV1, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    let draft = current_draft(reader, thread_id)?;
    Ok(DraftEditorCurrentSelectorV1::new(
        thread.id(),
        thread.revision(),
        draft.id(),
        draft.revision(),
        draft.piece_root(),
        draft.history(),
    ))
}

fn captured_adoption_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    captured: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicMutationError> {
    let root = required::<DraftPieceRootsFamily>(reader, &captured.newest_root().key())?;
    if root.reference() != captured.newest_root()
        || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
    {
        return Ok(false);
    }
    authenticate_draft_edit_history_frontier_v1(reader, frontier)?;
    let Some(journal_head) = frontier.journal_head() else {
        return Ok(false);
    };
    let transition = required::<DraftEditHistoryTransitionsFamily>(reader, &journal_head.key())?;
    if transition.reference() != journal_head {
        return Ok(false);
    }
    if transition.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit {
        return captured_historical_adoption_is_exact(reader, captured, frontier, &transition);
    }
    let key = DraftPieceSettlementKeyV1::new(
        captured.draft_id(),
        captured.session_id(),
        transition.operation_id(),
    );
    let settlement = required::<DraftPieceSettlementsFamily>(reader, &key)?;
    let build = required::<DraftPieceBuildsFamily>(reader, &key)?;
    let receipt =
        required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
    mutation::authenticate_progress_receipt(reader, &receipt)?;
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        return Ok(false);
    };
    Ok(settlement_closure_is_exact(&settlement)
        && settlement_terminal_build_is_exact(&settlement, Some(&build))
        && receipt.reference() == build.progress_receipt()
        && session::adopted_head_matches_current(adoption.adopted_session(), captured)
        && adoption.adopted_root() == &root
        && adoption.adopted_history() == frontier
        && adoption.transition() == &transition)
}

fn captured_historical_adoption_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    captured: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
) -> Result<bool, SyndicMutationError> {
    let key = DraftHistoricalRootAdoptionKeyV1::new(
        captured.draft_id(),
        captured.session_id(),
        transition.operation_id(),
    );
    let Some(settlement) = point::<DraftHistoricalRootAdoptionsFamily>(reader, &key)? else {
        return Ok(false);
    };
    if !settlement.is_locally_valid()
        || authenticate_draft_edit_history_frontier_v1(reader, settlement.source_history()).is_err()
        || match settlement.request().direction() {
            DraftHistoricalRootDirectionV1::Undo => settlement.source_history().undo_head(),
            DraftHistoricalRootDirectionV1::Redo => settlement.source_history().redo_head(),
        } != Some(settlement.selected_transition().reference())
        || point::<DraftEditHistoryTransitionsFamily>(
            reader,
            &settlement.selected_transition().key(),
        )?
        .as_ref()
            != Some(settlement.selected_transition())
        || point::<DraftPieceRootsFamily>(reader, &settlement.target_root().reference().key())?
            .as_ref()
            != Some(settlement.target_root())
    {
        return Ok(false);
    }
    Ok(
        settlement.outcome() == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed
            && settlement.successor_transition() == Some(transition)
            && settlement.successor_history() == Some(frontier)
            && settlement.successor_candidate().is_some_and(|candidate| {
                session::adopted_head_matches_current(candidate, captured)
            })
            && authenticate_draft_edit_history_frontier_v1(reader, frontier).is_ok(),
    )
}

fn publication_receipt_parts(
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Option<(
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidateSessionV1,
    DraftEditHistoryFrontierV1,
)> {
    let request = decode_candidate_publication_request_bytes(receipt.request_bytes()).ok()?;
    let published_pair = receipt.published_pair();
    let source_frontier = receipt
        .captured_frontier()
        .fork_session(request.session_id())?;
    let captured = captured_head(receipt.before_head(), request)?;
    if receipt.prior_selector() != request.selector()
        || request.selector().draft_id() != receipt.before_head().draft_id()
        || request.session_id() != receipt.before_head().session_id()
        || receipt.before_head().lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
        || receipt.before_head().active_operation().is_some()
        || request.candidate().history() != source_frontier.reference()
        || receipt.captured_frontier().reference().key()
            != DraftEditHistoryFrontierKeyV1::publication(
                request.selector().draft_id(),
                request.session_id(),
                request.operation_id(),
            )
        || receipt
            .captured_frontier()
            .reference()
            .candidate_generation()
            != request.candidate_generation()
        || receipt.captured_frontier().reference().root() != request.candidate().root()
        || receipt.successor_selector().thread_id() != request.selector().thread_id()
        || receipt.successor_selector().thread_revision() != request.selector().thread_revision()
        || receipt.successor_selector().draft_id() != request.selector().draft_id()
        || request.selector().selector_revision().checked_next().ok()
            != Some(receipt.successor_selector().selector_revision())
        || published_pair
            != DraftRootHistoryPairV1::new(
                request.candidate().root(),
                receipt.captured_frontier().reference(),
            )
        || receipt
            .before_head()
            .published(
                request.candidate_generation(),
                published_pair,
                receipt.successor_selector().selector_revision(),
            )
            .as_ref()
            != Some(receipt.after_head())
        || captured.newest_history() != source_frontier.reference()
    {
        return None;
    }
    Some((request, captured, source_frontier))
}

fn session_descends_from_publication(
    current: &DraftEditorCandidateSessionV1,
    after: &DraftEditorCandidateSessionV1,
) -> bool {
    current.thread_id() == after.thread_id()
        && current.draft_id() == after.draft_id()
        && current.session_id() == after.session_id()
        && current.open_operation_id() == after.open_operation_id()
        && current.durable_base_selector_revision() == after.durable_base_selector_revision()
        && current.durable_base_root() == after.durable_base_root()
        && current.durable_base_history() == after.durable_base_history()
        && current.session_generation() >= after.session_generation()
        && current.published_candidate_generation() >= after.published_candidate_generation()
        && current.newest_candidate_generation() >= after.newest_candidate_generation()
        && (current.published_candidate_generation() != after.published_candidate_generation()
            || (current.published_selector_revision() == after.published_selector_revision()
                && current.published_root() == after.published_root()
                && current.published_history() == after.published_history()))
        && (current.newest_candidate_generation() != after.newest_candidate_generation()
            || current.newest_root() == after.newest_root()
                && (current.newest_history() == after.newest_history()
                    || current.published_candidate_generation()
                        == current.newest_candidate_generation()
                        && current.published_candidate_generation()
                            > after.published_candidate_generation()
                        && current.newest_history() == current.published_history()))
}

fn captured_adoption_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    captured: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    let root = storage.point::<DraftPieceRootsFamily>(
        store,
        captured.newest_root().key(),
        point_limit(),
    )?;
    let Some(root) = root else { return Ok(false) };
    if root.reference() != captured.newest_root()
        || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
        || !draft_edit_history_frontier_is_authenticated_v1(storage, store, frontier)?
    {
        return Ok(false);
    }
    let Some(journal_head) = frontier.journal_head() else {
        return Ok(false);
    };
    let transition = storage.point::<DraftEditHistoryTransitionsFamily>(
        store,
        journal_head.key(),
        point_limit(),
    )?;
    let Some(transition) = transition else {
        return Ok(false);
    };
    if transition.reference() != journal_head {
        return Ok(false);
    }
    if transition.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit {
        let key = DraftHistoricalRootAdoptionKeyV1::new(
            captured.draft_id(),
            captured.session_id(),
            transition.operation_id(),
        );
        let settlement =
            storage.point::<DraftHistoricalRootAdoptionsFamily>(store, key, point_limit())?;
        let Some(settlement) = settlement else {
            return Ok(false);
        };
        if !settlement.is_locally_valid()
            || !draft_edit_history_frontier_is_authenticated_v1(
                storage,
                store,
                settlement.source_history(),
            )?
            || match settlement.request().direction() {
                DraftHistoricalRootDirectionV1::Undo => settlement.source_history().undo_head(),
                DraftHistoricalRootDirectionV1::Redo => settlement.source_history().redo_head(),
            } != Some(settlement.selected_transition().reference())
            || storage
                .point::<DraftEditHistoryTransitionsFamily>(
                    store,
                    settlement.selected_transition().key(),
                    point_limit(),
                )?
                .as_ref()
                != Some(settlement.selected_transition())
            || storage
                .point::<DraftPieceRootsFamily>(
                    store,
                    settlement.target_root().reference().key(),
                    point_limit(),
                )?
                .as_ref()
                != Some(settlement.target_root())
        {
            return Ok(false);
        }
        return Ok(settlement.outcome()
            == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed
            && settlement.successor_transition() == Some(&transition)
            && settlement.successor_history() == Some(frontier)
            && settlement.successor_candidate().is_some_and(|candidate| {
                session::adopted_head_matches_current(candidate, captured)
            }));
    }
    let key = DraftPieceSettlementKeyV1::new(
        captured.draft_id(),
        captured.session_id(),
        transition.operation_id(),
    );
    let settlement = storage.point::<DraftPieceSettlementsFamily>(store, key, point_limit())?;
    let build = storage.point::<DraftPieceBuildsFamily>(store, key, point_limit())?;
    let (Some(settlement), Some(build)) = (settlement, build) else {
        return Ok(false);
    };
    let receipt = storage.point::<DraftPieceBuildProgressFamily>(
        store,
        build.progress_receipt().key(),
        point_limit(),
    )?;
    let Some(receipt) = receipt else {
        return Ok(false);
    };
    let Some(next_ordinal) = build
        .progress_receipt()
        .key()
        .transition_ordinal()
        .checked_add(1)
    else {
        return Ok(false);
    };
    if storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            DraftPieceBuildProgressReceiptKeyV1::new(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
                next_ordinal,
            ),
            point_limit(),
        )?
        .is_some()
        || !progress_receipt_matches_build(&receipt, &build)
        || !session::progress_receipt_closure_is_exact(storage, store, &receipt)?
    {
        return Ok(false);
    }
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        return Ok(false);
    };
    Ok(settlement_closure_is_exact(&settlement)
        && settlement_terminal_build_is_exact(&settlement, Some(&build))
        && session::adopted_head_matches_current(adoption.adopted_session(), captured)
        && adoption.adopted_root() == &root
        && adoption.adopted_history() == frontier
        && adoption.transition() == &transition)
}

fn validate_publication_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<(), SyndicMutationError> {
    let (_, captured, source_frontier) =
        publication_receipt_parts(receipt).ok_or(SyndicMutationError::IdentityCollision)?;
    let stored_frontier = required::<DraftEditHistoryFrontiersFamily>(
        reader,
        &receipt.captured_frontier().reference().key(),
    )?;
    if &stored_frontier != receipt.captured_frontier() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, receipt.captured_frontier())?;
    if !captured_adoption_is_exact(reader, &captured, &source_frontier)? {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let open_receipt = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            receipt.before_head().draft_id(),
            receipt.before_head().session_id(),
            receipt.before_head().open_operation_id(),
        ),
    )?;
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(open_receipt) = open_receipt else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if !session::receipt_matches_head(&open_receipt, receipt.before_head()) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let selector = current_selector(reader, receipt.successor_selector().thread_id())?;
    let DraftEditorCandidateSessionRecordV1::Head(head) =
        required::<DraftEditorCandidateSessionsFamily>(
            reader,
            &session_key(
                receipt.after_head().draft_id(),
                receipt.after_head().session_id(),
            ),
        )?
    else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if selector.selector_revision() < receipt.successor_selector().selector_revision()
        || (selector.selector_revision() == receipt.successor_selector().selector_revision()
            && selector != receipt.successor_selector())
        || !session_descends_from_publication(&head, receipt.after_head())
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(super) fn candidate_session_publication_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicMutationError> {
    let published_key = head.published_history().key();
    let Some(operation_id) = published_key.publication_operation_id() else {
        return Ok(
            head.published_selector_revision() == head.durable_base_selector_revision()
                && head.published_root() == head.durable_base_root()
                && head.published_history() == head.durable_base_history(),
        );
    };
    let Some(publication_session_id) = published_key.session_id() else {
        return Ok(false);
    };
    let key = DraftEditorCandidateSessionRecordKeyV1::publication_receipt(
        head.draft_id(),
        publication_session_id,
        operation_id,
    );
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(record)) =
        point::<DraftEditorCandidateSessionsFamily>(reader, &key)?
    else {
        return Ok(false);
    };
    let Some(receipt) = record.publication() else {
        return Ok(false);
    };
    let same_session = publication_session_id == head.session_id();
    if receipt.successor_selector().selector_revision() != head.published_selector_revision()
        || receipt.published_pair()
            != DraftRootHistoryPairV1::new(head.published_root(), head.published_history())
        || same_session
            && (receipt.after_head().published_candidate_generation()
                != head.published_candidate_generation()
                || receipt.after_head().published_selector_revision()
                    != head.published_selector_revision())
        || !same_session
            && (head.durable_base_selector_revision() != head.published_selector_revision()
                || head.durable_base_root() != head.published_root()
                || head.durable_base_history() != head.published_history()
                || head.published_candidate_generation()
                    != head.published_history().candidate_generation())
    {
        return Ok(false);
    }
    validate_publication_receipt(reader, receipt)?;
    Ok(true)
}

pub(super) fn candidate_session_publication_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    let published_key = head.published_history().key();
    let Some(operation_id) = published_key.publication_operation_id() else {
        return Ok(
            head.published_selector_revision() == head.durable_base_selector_revision()
                && head.published_root() == head.durable_base_root()
                && head.published_history() == head.durable_base_history(),
        );
    };
    let Some(publication_session_id) = published_key.session_id() else {
        return Ok(false);
    };
    let key = DraftEditorCandidateSessionRecordKeyV1::publication_receipt(
        head.draft_id(),
        publication_session_id,
        operation_id,
    );
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(record)) =
        storage.point::<DraftEditorCandidateSessionsFamily>(store, key, point_limit())?
    else {
        return Ok(false);
    };
    let Some(receipt) = record.publication() else {
        return Ok(false);
    };
    if !validate_publication_receipt_history_in_store(storage, store, receipt)? {
        return Ok(false);
    }
    let same_session = publication_session_id == head.session_id();
    Ok(
        receipt.successor_selector().selector_revision() == head.published_selector_revision()
            && receipt.published_pair()
                == DraftRootHistoryPairV1::new(head.published_root(), head.published_history())
            && (same_session
                && receipt.after_head().published_candidate_generation()
                    == head.published_candidate_generation()
                && receipt.after_head().published_selector_revision()
                    == head.published_selector_revision()
                || !same_session
                    && head.durable_base_selector_revision() == head.published_selector_revision()
                    && head.durable_base_root() == head.published_root()
                    && head.durable_base_history() == head.published_history()
                    && head.published_candidate_generation()
                        == head.published_history().candidate_generation()),
    )
}

fn validate_publication_receipt_history_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<bool, SyndicReadError> {
    let (_, captured, source_frontier) = match publication_receipt_parts(receipt) {
        Some(parts) => parts,
        None => return Ok(false),
    };
    let frontier = storage.point::<DraftEditHistoryFrontiersFamily>(
        store,
        receipt.captured_frontier().reference().key(),
        point_limit(),
    )?;
    let open_receipt = storage.point::<DraftEditorCandidateSessionsFamily>(
        store,
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            receipt.before_head().draft_id(),
            receipt.before_head().session_id(),
            receipt.before_head().open_operation_id(),
        ),
        point_limit(),
    )?;
    let head = storage.point::<DraftEditorCandidateSessionsFamily>(
        store,
        session_key(
            receipt.after_head().draft_id(),
            receipt.after_head().session_id(),
        ),
        point_limit(),
    )?;
    let current = storage.current_draft(
        store,
        receipt.successor_selector().thread_id(),
        point_limit(),
    )?;
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(open_receipt)) = open_receipt else {
        return Ok(false);
    };
    let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = head else {
        return Ok(false);
    };
    let Some(current) = current else {
        return Ok(false);
    };
    let selector = DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    );
    Ok(frontier.as_ref() == Some(receipt.captured_frontier())
        && draft_edit_history_frontier_is_authenticated_v1(
            storage,
            store,
            receipt.captured_frontier(),
        )?
        && captured_adoption_is_exact_in_store(storage, store, &captured, &source_frontier)?
        && session::receipt_matches_head(&open_receipt, receipt.before_head())
        && selector.selector_revision() >= receipt.successor_selector().selector_revision()
        && (selector.selector_revision() != receipt.successor_selector().selector_revision()
            || selector == receipt.successor_selector())
        && session_descends_from_publication(&head, receipt.after_head()))
}

fn validate_publication_receipt_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<bool, SyndicReadError> {
    if !validate_publication_receipt_history_in_store(storage, store, receipt)? {
        return Ok(false);
    }
    let head = storage.point::<DraftEditorCandidateSessionsFamily>(
        store,
        session_key(
            receipt.after_head().draft_id(),
            receipt.after_head().session_id(),
        ),
        point_limit(),
    )?;
    let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = head else {
        return Ok(false);
    };
    session::candidate_session_closure_is_exact_in_store(storage, store, &head)
}

fn disposal_request_matches_head(
    request: DraftEditorCandidateSessionDisposeRequestV1,
    head: &DraftEditorCandidateSessionV1,
) -> bool {
    head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && head.published_candidate_generation() == head.newest_candidate_generation()
        && head.published_root() == head.newest_root()
        && head.published_history() == head.newest_history()
        && head.session_generation() == request.expected_session_generation()
        && request.expected_pair()
            == DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history())
}

fn disposal_receipt_parts(
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Option<DraftEditorCandidateSessionDisposeRequestV1> {
    let request = decode_candidate_disposal_request_bytes(receipt.request_bytes()).ok()?;
    let before = receipt.before_head();
    if canonical_candidate_disposal_request_bytes(request) != receipt.request_bytes()
        || request.draft_id() != before.draft_id()
        || request.session_id() != before.session_id()
        || !disposal_request_matches_head(request, before)
        || before.active_operation().is_some()
        || receipt.frontier().reference() != before.newest_history()
        || before.disposed(request.operation_id()).as_ref() != Some(receipt.after_head())
        || receipt.after_head().disposal_operation_id() != Some(request.operation_id())
    {
        return None;
    }
    Some(request)
}

fn validate_disposal_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<(), SyndicMutationError> {
    let request = disposal_receipt_parts(receipt).ok_or(SyndicMutationError::IdentityCollision)?;
    let stored =
        required::<DraftEditHistoryFrontiersFamily>(reader, &receipt.frontier().reference().key())?;
    let open = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            request.draft_id(),
            request.session_id(),
            receipt.before_head().open_operation_id(),
        ),
    )?;
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(open) = open else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    let DraftEditorCandidateSessionRecordV1::Head(head) =
        required::<DraftEditorCandidateSessionsFamily>(
            reader,
            &session_key(
                receipt.after_head().draft_id(),
                receipt.after_head().session_id(),
            ),
        )?
    else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if stored != *receipt.frontier()
        || head != *receipt.after_head()
        || !session::receipt_matches_head(&open, receipt.before_head())
        || !candidate_session_publication_is_exact(reader, receipt.before_head())?
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &stored)
}

fn validate_disposal_receipt_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<bool, SyndicReadError> {
    let request = match disposal_receipt_parts(receipt) {
        Some(request) => request,
        None => return Ok(false),
    };
    let frontier = storage.point::<DraftEditHistoryFrontiersFamily>(
        store,
        receipt.frontier().reference().key(),
        point_limit(),
    )?;
    let open = storage.point::<DraftEditorCandidateSessionsFamily>(
        store,
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            request.draft_id(),
            request.session_id(),
            receipt.before_head().open_operation_id(),
        ),
        point_limit(),
    )?;
    let head = storage.point::<DraftEditorCandidateSessionsFamily>(
        store,
        session_key(request.draft_id(), request.session_id()),
        point_limit(),
    )?;
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(open)) = open else {
        return Ok(false);
    };
    let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = head else {
        return Ok(false);
    };
    Ok(frontier.as_ref() == Some(receipt.frontier())
        && draft_edit_history_frontier_is_authenticated_v1(storage, store, receipt.frontier())?
        && session::receipt_matches_head(&open, receipt.before_head())
        && candidate_session_publication_is_exact_in_store(storage, store, receipt.before_head())?
        && head == *receipt.after_head())
}

pub(super) fn candidate_session_disposal_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    let Some(operation_id) = head.disposal_operation_id() else {
        return Ok(false);
    };
    let key = DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
        head.draft_id(),
        head.session_id(),
        operation_id,
    );
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(record)) =
        storage.point::<DraftEditorCandidateSessionsFamily>(store, key, point_limit())?
    else {
        return Ok(false);
    };
    let Some(receipt) = record.disposal() else {
        return Ok(false);
    };
    Ok(
        receipt.after_head() == head
            && validate_disposal_receipt_in_store(storage, store, receipt)?,
    )
}

impl DomainMutation<SyndicDomain> for PublicationMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if let Some(record) =
            point::<DraftEditorCandidateSessionsFamily>(reader, &publication_key(request))?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(SyndicMutationError::IdentityCollision);
            };
            let receipt = receipt
                .publication()
                .ok_or(SyndicMutationError::IdentityCollision)?;
            validate_publication_receipt(reader, receipt)?;
            return Err(SyndicMutationError::IdentityCollision);
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.selector().draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed
            || request.candidate_generation() <= head.published_candidate_generation()
            || current_selector(reader, request.selector().thread_id())? != request.selector()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if head.active_operation().is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if !candidate_session_publication_is_exact(reader, &head)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let captured = &self.prepared.captured_head;
        if !publication_source_matches(&head, captured) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if point::<DraftEditHistoryFrontiersFamily>(
            reader,
            &self.prepared.captured_frontier.reference().key(),
        )?
        .is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if !captured_adoption_is_exact(reader, captured, &self.prepared.source_frontier)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let draft = current_draft(reader, request.selector().thread_id())?;
        if request.published_at() < draft.updated_at() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        reservation.reserve_records::<DraftsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if point::<DraftEditorCandidateSessionsFamily>(reader, &publication_key(request))?.is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.selector().draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed
            || request.candidate_generation() <= head.published_candidate_generation()
            || current_selector(reader, request.selector().thread_id())? != request.selector()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let draft = current_draft(reader, request.selector().thread_id())?;
        let thread = required::<ThreadsFamily>(reader, &request.selector().thread_id())?;
        let reverse = required::<DraftByThreadFamily>(reader, &thread.id())?;
        let summary = required::<HistorySummariesFamily>(reader, &thread.id())?;
        let next_revision = draft.revision().checked_next()?;
        let published_pair = DraftRootHistoryPairV1::new(
            request.candidate().root(),
            self.prepared.captured_frontier.reference(),
        );
        let next_selector = DraftEditorCurrentSelectorV1::new(
            thread.id(),
            thread.revision(),
            draft.id(),
            next_revision,
            published_pair.root(),
            published_pair.history(),
        );
        let after_head = head
            .published(
                request.candidate_generation(),
                published_pair,
                next_revision,
            )
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let next_draft = DraftRecord::new(
            draft.id(),
            draft.thread_id(),
            next_revision,
            draft.submission_intent(),
            published_pair,
            draft.created_at(),
            request.published_at(),
        );
        let next_reverse =
            DraftByThreadRecord::new(thread.id(), draft.id(), next_revision, thread.revision());
        let next_summary = HistorySummaryRecord::new(
            summary.thread_id(),
            summary.revision().checked_next()?,
            summary.thread_revision(),
            summary.committed_tail(),
            summary.selected_path_digest(),
            summary.complete(),
            summary.last_activity_at().max(request.published_at()),
        );
        let receipt = DraftEditorCandidatePublicationReceiptV1::new(
            self.prepared.canonical_request.clone(),
            request.selector(),
            next_selector,
            head,
            after_head.clone(),
            self.prepared.captured_frontier.clone(),
        );
        mutations.put::<DraftsCodec>(&next_draft.id(), &next_draft)?;
        mutations.put::<DraftByThreadCodec>(&thread.id(), &next_reverse)?;
        mutations.put::<HistorySummariesCodec>(&thread.id(), &next_summary)?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &self.prepared.captured_frontier.reference().key(),
            &self.prepared.captured_frontier,
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(after_head.draft_id(), after_head.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(after_head),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &publication_key(request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_publication(receipt),
            ),
        )?;
        let _ = reverse;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DisposalMutation {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if let Some(record) =
            point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(SyndicMutationError::IdentityCollision);
            };
            return validate_disposal_receipt(
                reader,
                receipt
                    .disposal()
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            );
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed
            || !disposal_request_matches_head(request, &head)
        {
            return Ok(());
        }
        if head.active_operation().is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if !candidate_session_publication_is_exact(reader, &head)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let stored =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if stored != self.prepared.frontier {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_draft_edit_history_frontier_v1(reader, &stored)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        Ok(())
    }
    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?.is_some() {
            return Ok(());
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed
            || !disposal_request_matches_head(request, &head)
        {
            return Ok(());
        }
        if head.active_operation().is_some()
            || !candidate_session_publication_is_exact(reader, &head)?
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let stored =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if stored != self.prepared.frontier {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_draft_edit_history_frontier_v1(reader, &stored)?;
        let after = head
            .disposed(request.operation_id())
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let receipt = DraftEditorCandidateSessionDisposeReceiptV1::new(
            self.prepared.canonical_request.clone(),
            head,
            after.clone(),
            self.prepared.frontier.clone(),
        );
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(after.draft_id(), after.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(after),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &disposal_key(request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_disposal(receipt),
            ),
        )?;
        Ok(())
    }
}

impl SyndicStorage {
    pub fn capture_draft_editor_candidate_publication_source(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
    ) -> Result<
        CapturedDraftEditorCandidatePublicationSourceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let candidate = request.candidate();
        let pair = DraftRootHistoryPairV1::new(candidate.root(), candidate.history());
        if candidate.draft_id() != request.selector().draft_id()
            || !pair.is_coherent()
            || candidate.candidate_generation() != candidate.history().candidate_generation()
            || (candidate.candidate_generation() != 0
                && candidate.root().key().session_id() != Some(candidate.session_id()))
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let root = self
            .point::<DraftPieceRootsFamily>(store, candidate.root().key(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if root.reference() != candidate.root()
            || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.selector().draft_id(),
            candidate.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        if DraftEditorCandidateActivationBindingV1::from_head(&head) != candidate
            || head.thread_id() != request.selector().thread_id()
            || head.published_selector_revision() != request.selector().selector_revision()
            || head.published_root() != request.selector().root()
            || head.published_history() != request.selector().history()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let source_frontier = self
            .point::<DraftEditHistoryFrontiersFamily>(store, candidate.history().key(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if source_frontier.reference() != candidate.history()
            || !draft_edit_history_frontier_is_authenticated_v1(self, store, &source_frontier)?
            || !session::candidate_session_adoption_is_exact(self, store, &head)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        Ok(CapturedDraftEditorCandidatePublicationSourceV1 {
            storage: *self,
            request,
            source_frontier,
            captured_head: head,
        })
    }

    pub fn prepare_draft_editor_candidate_publication(
        &self,
        store: &HomeStore,
        source: CapturedDraftEditorCandidatePublicationSourceV1,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
    ) -> Result<
        PreparedDraftEditorCandidatePublicationV1,
        DraftEditorCandidatePublicationSourcePreparationErrorV1,
    > {
        let prepared =
            self.prepare_draft_editor_candidate_publication_inner(store, &source, evidence);
        prepared.map_err(
            |error| DraftEditorCandidatePublicationSourcePreparationErrorV1 { source, error },
        )
    }

    fn prepare_draft_editor_candidate_publication_inner(
        &self,
        store: &HomeStore,
        source: &CapturedDraftEditorCandidatePublicationSourceV1,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
    ) -> Result<
        PreparedDraftEditorCandidatePublicationV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        source
            .storage
            .revision(store)
            .map_err(SyndicReadError::Read)?;
        let capture = source.request;
        let candidate = capture.candidate();
        let request = DraftEditorCandidatePublicationRequestV1::new(
            capture.selector(),
            candidate.session_id(),
            capture.operation_id(),
            candidate.candidate_generation(),
            DraftRootHistoryPairV1::new(candidate.root(), candidate.history()),
            evidence,
            capture.published_at(),
        );
        if !request.candidate().is_coherent()
            || request.candidate_generation()
                != request.candidate().history().candidate_generation()
            || request.selector().draft_id() != request.candidate().root().key().draft_id()
            || !publication_evidence_is_exact(request)
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let root = self
            .point::<DraftPieceRootsFamily>(store, request.candidate().root().key(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if root.reference() != request.candidate().root()
            || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if let Some(record) = self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            publication_key(request),
            limit,
        )? {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(occupied) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let occupied = occupied
                .publication()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_publication_receipt_in_store(self, store, occupied)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            return Ok(PreparedDraftEditorCandidatePublicationV1 {
                request,
                canonical_request: canonical_candidate_publication_request_bytes(request),
                source_frontier: occupied.captured_frontier().clone(),
                captured_frontier: occupied.captured_frontier().clone(),
                captured_head: occupied.before_head().clone(),
                initially_absent: false,
            });
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.selector().draft_id(),
            request.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head)
            | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        let captured = &source.captured_head;
        let source_frontier = &source.source_frontier;
        if captured.draft_id() != request.selector().draft_id()
            || captured.session_id() != request.session_id()
            || captured.newest_candidate_generation() != request.candidate_generation()
            || captured.newest_root() != request.candidate().root()
            || captured.newest_history() != request.candidate().history()
            || source_frontier.reference() != request.candidate().history()
            || !captured_publication_source_matches(&head, captured)
            || !candidate_session_publication_is_exact_in_store(self, store, &head)?
            || !candidate_session_publication_is_exact_in_store(self, store, captured)?
            || !captured_adoption_is_exact_in_store(self, store, captured, source_frontier)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let captured_frontier = source_frontier
            .publication_snapshot(request.session_id(), request.operation_id())
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        Ok(PreparedDraftEditorCandidatePublicationV1 {
            request,
            canonical_request: canonical_candidate_publication_request_bytes(request),
            source_frontier: source_frontier.clone(),
            captured_frontier,
            captured_head: captured.clone(),
            initially_absent: true,
        })
    }

    pub fn publish_draft_editor_candidate(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidatePublicationV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, PublicationMutation { prepared })
    }

    pub fn reconcile_draft_editor_candidate_publication(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidatePublicationOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let committed = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => match store
                .reconcile(&reconciliation.install_and_handle())
                .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?
            {
                ReconciliationResolution::ExactNew { .. } => true,
                ReconciliationResolution::ExactOld => false,
                ReconciliationResolution::Collision => {
                    return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
                }
            },
        };
        self.publication_outcome(store, prepared, committed)
    }

    fn publication_outcome(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
        committed: bool,
    ) -> Result<
        DraftEditorCandidatePublicationOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let request = prepared.request;
        let limit = point_limit();
        if let Some(record) = self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            publication_key(request),
            limit,
        )? {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let receipt = receipt
                .publication()
                .cloned()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_publication_receipt_in_store(self, store, &receipt)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            if receipt.request_bytes() != prepared.canonical_request {
                return Ok(
                    DraftEditorCandidatePublicationOutcomeV1::OccupiedIdentityCollision(
                        DraftEditorCandidatePublicationCollisionProofV1::new(request, receipt),
                    ),
                );
            }
            if receipt.captured_frontier() != &prepared.captured_frontier {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            return if committed && prepared.initially_absent {
                Ok(DraftEditorCandidatePublicationOutcomeV1::Published(
                    receipt.successor_selector(),
                    receipt.published_pair(),
                ))
            } else {
                Ok(DraftEditorCandidatePublicationOutcomeV1::ExactReplay(
                    receipt,
                ))
            };
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.selector().draft_id(),
            request.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(h)
            | DraftEditorCandidateSessionReadOutcomeV1::Disposed(h) => h,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
            return Ok(DraftEditorCandidatePublicationOutcomeV1::SessionDisposed);
        }
        if head.published_candidate_generation() >= request.candidate_generation() {
            return Ok(DraftEditorCandidatePublicationOutcomeV1::Superseded(
                head.published_candidate_generation(),
                DraftRootHistoryPairV1::new(head.published_root(), head.published_history()),
            ));
        }
        let current = self
            .current_draft(store, request.selector().thread_id(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        let selector = DraftEditorCurrentSelectorV1::new(
            current.thread().id(),
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().piece_root(),
            current.draft().history(),
        );
        if selector != request.selector() {
            return Ok(DraftEditorCandidatePublicationOutcomeV1::DurableBaseConflict(selector));
        }
        Err(if committed {
            DraftEditorCandidatePublicationCommandErrorV1::Invariant
        } else {
            DraftEditorCandidatePublicationCommandErrorV1::NotCommitted
        })
    }

    pub fn prepare_dispose_draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionDisposeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if !request.expected_pair().is_coherent() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        if let Some(record) =
            self.point::<DraftEditorCandidateSessionsFamily>(store, disposal_key(request), limit)?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(occupied) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let occupied = occupied
                .disposal()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_disposal_receipt_in_store(self, store, occupied)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            return Ok(PreparedDraftEditorCandidateSessionDisposeV1 {
                request,
                canonical_request: canonical_candidate_disposal_request_bytes(request),
                frontier: occupied.frontier().clone(),
                initially_absent: false,
            });
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.draft_id(),
            request.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head)
            | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if disposal_request_matches_head(request, &head) && head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        let frontier = self
            .point::<DraftEditHistoryFrontiersFamily>(store, head.newest_history().key(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if frontier.reference() != head.newest_history()
            || !draft_edit_history_frontier_is_authenticated_v1(self, store, &frontier)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        Ok(PreparedDraftEditorCandidateSessionDisposeV1 {
            request,
            canonical_request: canonical_candidate_disposal_request_bytes(request),
            frontier,
            initially_absent: true,
        })
    }

    pub fn dispose_draft_editor_candidate_session(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidateSessionDisposeV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, DisposalMutation { prepared })
    }

    pub fn reconcile_draft_editor_candidate_session_disposal(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidateSessionDisposeV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionDisposeOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let committed = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => match store
                .reconcile(&reconciliation.install_and_handle())
                .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?
            {
                ReconciliationResolution::ExactNew { .. } => true,
                ReconciliationResolution::ExactOld => false,
                ReconciliationResolution::Collision => {
                    return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
                }
            },
        };
        let request = prepared.request;
        let limit = point_limit();
        if let Some(record) =
            self.point::<DraftEditorCandidateSessionsFamily>(store, disposal_key(request), limit)?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let receipt = receipt
                .disposal()
                .cloned()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_disposal_receipt_in_store(self, store, &receipt)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            if receipt.request_bytes() != prepared.canonical_request {
                return Ok(
                    DraftEditorCandidateSessionDisposeOutcomeV1::OccupiedIdentityCollision(
                        DraftEditorCandidateSessionDisposeCollisionProofV1::new(request, receipt),
                    ),
                );
            }
            return if committed && prepared.initially_absent {
                Ok(DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(
                    receipt.after_head().clone(),
                ))
            } else {
                Ok(DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(
                    receipt,
                ))
            };
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.draft_id(),
            request.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(h)
            | DraftEditorCandidateSessionReadOutcomeV1::Disposed(h) => h,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
            return Ok(DraftEditorCandidateSessionDisposeOutcomeV1::AlreadyDisposed(head));
        }
        if head.published_root() != head.newest_root()
            || head.published_history() != head.newest_history()
            || head.session_generation() != request.expected_session_generation()
            || request.expected_pair()
                != DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history())
        {
            return Ok(DraftEditorCandidateSessionDisposeOutcomeV1::DirtyConflict(
                head,
            ));
        }
        Err(if committed {
            DraftEditorCandidatePublicationCommandErrorV1::Invariant
        } else {
            DraftEditorCandidatePublicationCommandErrorV1::NotCommitted
        })
    }
}
