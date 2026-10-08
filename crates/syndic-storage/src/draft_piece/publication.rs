use std::{error::Error, fmt};

use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeCandidateRecoveryAccess, HomeStore,
    MutationBuilder, MutationContribution, ReconciliationFailure, ReconciliationReservation,
    ReconciliationResolution,
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
use crate::read::access::ReadAccess;
use crate::{
    DraftByThreadRecord, DraftRecord, HistorySummaryRecord, SyndicMutationError, SyndicReadError,
    SyndicTimestamp,
};

use super::*;

mod abandon_fresh;
mod disposal;
mod disposal_access;
mod preparation;
mod recovery;

pub(crate) use disposal::{PreparedCandidateDisposal, prepare_candidate_disposal};

pub use abandon_fresh::PreparedDraftEditorCandidateSessionAbandonFreshV1;
pub(super) use abandon_fresh::fresh_opening_history_is_exact_with_access;
#[cfg(feature = "test-faults")]
pub use abandon_fresh::test_abandon_fresh_reconciliation_resolution;
pub use recovery::{
    DraftEditorCandidatePublicationCorrespondenceV1, DraftEditorCandidateSavedCorrespondenceV1,
};

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
    home_id: beryl_model::BerylHomeId,
    canonical_path: std::path::PathBuf,
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
    ReconciliationCollision,
    UnauthorizedReconciliationSuccessor,
    NotCommitted,
    ActiveOperation,
    Invariant,
}

impl fmt::Display for DraftEditorCandidatePublicationCommandErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => e.fmt(f),
            Self::Reconciliation(e) => e.fmt(f),
            Self::ReconciliationCollision => {
                f.write_str("candidate command reconciliation collided")
            }
            Self::UnauthorizedReconciliationSuccessor => {
                f.write_str("candidate command reconciliation found an unauthorized successor")
            }
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
    prepared: Box<PreparedDraftEditorCandidatePublicationV1>,
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
    checkpoint::candidate_is_exact_in_transaction(reader, captured, frontier)
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

fn captured_adoption_is_exact_with_access(
    storage: &SyndicStorage,
    store: crate::read::access::ReadAccess<'_>,
    captured: &DraftEditorCandidateSessionV1,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    checkpoint::candidate_is_exact_with_access(storage, store, captured, frontier)
}

fn validate_publication_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<(), SyndicMutationError> {
    validate_publication_receipt_history(reader, receipt)?;
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
    if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
        let operation_id = head
            .disposal_operation_id()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let record = required::<DraftEditorCandidateSessionsFamily>(
            reader,
            &DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                head.draft_id(),
                head.session_id(),
                operation_id,
            ),
        )?;
        let DraftEditorCandidateSessionRecordV1::OpenReceipt(record) = record else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        return validate_disposal_receipt(
            reader,
            record
                .disposal()
                .ok_or(SyndicMutationError::IdentityCollision)?,
        );
    }
    if !published_checkpoint_matches_selector(
        receipt.after_head(),
        current_selector(reader, receipt.successor_selector().thread_id())?,
    ) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

fn validate_publication_receipt_history(
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
    if !session_descends_from_publication(&head, receipt.after_head()) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(super) fn candidate_session_publication_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicMutationError> {
    Ok(
        candidate_session_publication_history_is_exact(reader, head)?
            && published_checkpoint_matches_selector(
                head,
                current_selector(reader, head.thread_id())?,
            ),
    )
}

fn published_checkpoint_matches_selector(
    head: &DraftEditorCandidateSessionV1,
    selector: DraftEditorCurrentSelectorV1,
) -> bool {
    selector.thread_id() == head.thread_id()
        && selector.draft_id() == head.draft_id()
        && selector.selector_revision() >= head.published_selector_revision()
        && (selector.selector_revision() != head.published_selector_revision()
            || selector.root() == head.published_root()
                && selector.history() == head.published_history())
}

fn candidate_session_publication_history_is_exact(
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
    validate_publication_receipt_history(reader, receipt)?;
    Ok(true)
}

pub(super) fn candidate_session_publication_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    candidate_session_publication_is_exact_with_access(
        storage,
        crate::read::access::ReadAccess::Ordinary(store),
        head,
    )
}

pub(super) fn candidate_session_publication_is_exact_with_access(
    storage: &SyndicStorage,
    store: crate::read::access::ReadAccess<'_>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    if !candidate_session_publication_history_is_exact_with_access(storage, store, head)? {
        return Ok(false);
    }
    let Some(current) =
        storage.current_draft_with_access(store, head.thread_id(), point_limit())?
    else {
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
    Ok(published_checkpoint_matches_selector(head, selector))
}

fn candidate_session_publication_history_is_exact_with_access(
    storage: &SyndicStorage,
    store: crate::read::access::ReadAccess<'_>,
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
        storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            point_limit(),
        )?
    else {
        return Ok(false);
    };
    let Some(receipt) = record.publication() else {
        return Ok(false);
    };
    if !validate_publication_receipt_history_with_access(storage, store, receipt)? {
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

fn validate_publication_receipt_history_with_access(
    storage: &SyndicStorage,
    store: crate::read::access::ReadAccess<'_>,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<bool, SyndicReadError> {
    let (_, captured, source_frontier) = match publication_receipt_parts(receipt) {
        Some(parts) => parts,
        None => return Ok(false),
    };
    let frontier = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
        store,
        receipt.captured_frontier().reference().key(),
        point_limit(),
    )?;
    let open_receipt = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            receipt.before_head().draft_id(),
            receipt.before_head().session_id(),
            receipt.before_head().open_operation_id(),
        ),
        point_limit(),
    )?;
    let head = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        session_key(
            receipt.after_head().draft_id(),
            receipt.after_head().session_id(),
        ),
        point_limit(),
    )?;
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(open_receipt)) = open_receipt else {
        return Ok(false);
    };
    let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = head else {
        return Ok(false);
    };
    Ok(frontier.as_ref() == Some(receipt.captured_frontier())
        && draft_edit_history_frontier_is_authenticated_with_access(
            storage,
            store,
            receipt.captured_frontier(),
        )?
        && captured_adoption_is_exact_with_access(storage, store, &captured, &source_frontier)?
        && session::receipt_matches_head(&open_receipt, receipt.before_head())
        && session_descends_from_publication(&head, receipt.after_head()))
}

fn validate_publication_receipt_with_access(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    receipt: &DraftEditorCandidatePublicationReceiptV1,
) -> Result<bool, SyndicReadError> {
    if !validate_publication_receipt_history_with_access(storage, store, receipt)? {
        return Ok(false);
    }
    let head = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
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
    match store {
        ReadAccess::Ordinary(store) => {
            session::candidate_session_closure_is_exact_in_store(storage, store, &head)
        }
        ReadAccess::Candidate(_) => {
            session::idle_candidate_closure_is_exact_with_access(storage, store, &head)
        }
    }
}

fn publication_session_with_access(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    draft: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<DraftEditorCandidateSessionReadOutcomeV1, SyndicReadError> {
    if let ReadAccess::Ordinary(store) = store {
        return storage.draft_editor_candidate_session(store, draft, session_id);
    }
    let key = session_key(draft, session_id);
    let first = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        key,
        point_limit(),
    )?;
    let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = &first else {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
    };
    if head.active_operation().is_some() {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::Active(
            head.clone(),
        ));
    }
    let exact = session::idle_candidate_closure_is_exact_with_access(storage, store, head)?;
    let last = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        key,
        point_limit(),
    )?;
    if first != last {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange);
    }
    if !exact || head.draft_id() != draft || head.session_id() != session_id {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
    }
    Ok(DraftEditorCandidateSessionReadOutcomeV1::Active(
        head.clone(),
    ))
}

fn disposal_request_matches_head(
    request: DraftEditorCandidateSessionDisposeRequestV1,
    head: &DraftEditorCandidateSessionV1,
) -> bool {
    head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && head.disposal_operation_id().is_none()
        && request.draft_id() == head.draft_id()
        && request.session_id() == head.session_id()
        && request.expected_session_generation() == head.session_generation()
        && request.expected_pair()
            == DraftRootHistoryPairV1::new(head.published_root(), head.published_history())
        && checkpoint::has_saved_identity(head)
}

fn disposal_request_names_head(
    request: DraftEditorCandidateSessionDisposeRequestV1,
    head: &DraftEditorCandidateSessionV1,
) -> bool {
    head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && head.disposal_operation_id().is_none()
        && request.draft_id() == head.draft_id()
        && request.session_id() == head.session_id()
        && head.session_generation() == request.expected_session_generation()
        && request.expected_pair()
            == DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisposalTransitionKind {
    Ordinary,
    OpeningNormalization,
    FreshAbandonment,
}

fn disposal_receipt_parts(
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Option<(
    DraftEditorCandidateSessionDisposeRequestV1,
    DisposalTransitionKind,
)> {
    let request = decode_candidate_disposal_request_bytes(receipt.request_bytes()).ok()?;
    let before = receipt.before_head();
    let transition = if before.disposed(request.operation_id()).as_ref()
        == Some(receipt.after_head())
    {
        DisposalTransitionKind::Ordinary
    } else if before.disposed_opening(request.operation_id()).as_ref() == Some(receipt.after_head())
        && disposal_request_matches_head(request, before)
    {
        DisposalTransitionKind::OpeningNormalization
    } else if before.abandoned_fresh(request.operation_id()).as_ref() == Some(receipt.after_head())
    {
        DisposalTransitionKind::FreshAbandonment
    } else {
        return None;
    };
    if canonical_candidate_disposal_request_bytes(request) != receipt.request_bytes()
        || request.draft_id() != before.draft_id()
        || request.session_id() != before.session_id()
        || match transition {
            DisposalTransitionKind::Ordinary | DisposalTransitionKind::OpeningNormalization => {
                !disposal_request_matches_head(request, before)
            }
            DisposalTransitionKind::FreshAbandonment => {
                !disposal_request_names_head(request, before)
            }
        }
        || receipt.frontier().reference() != before.newest_history()
        || receipt.after_head().disposal_operation_id() != Some(request.operation_id())
    {
        return None;
    }
    Some((request, transition))
}

fn validate_disposal_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<(), SyndicMutationError> {
    let (request, transition) =
        disposal_receipt_parts(receipt).ok_or(SyndicMutationError::IdentityCollision)?;
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
    let source_is_exact = match transition {
        DisposalTransitionKind::Ordinary => {
            session::receipt_matches_head(&open, receipt.before_head())
                && candidate_session_publication_history_is_exact(reader, receipt.before_head())?
        }
        DisposalTransitionKind::OpeningNormalization => {
            session::receipt_matches_head(&open, receipt.before_head())
                && candidate_session_publication_history_is_exact(reader, receipt.before_head())?
                && checkpoint::opening_is_exact_in_transaction(
                    reader,
                    receipt.before_head(),
                    &stored,
                )?
        }
        DisposalTransitionKind::FreshAbandonment => {
            abandon_fresh::request_matches_head_and_open(request, receipt.before_head(), &open)
                && abandon_fresh::history_is_exact(reader, receipt.before_head(), &stored)?
        }
    };
    if stored != *receipt.frontier() || head != *receipt.after_head() || !source_is_exact {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &stored)
}

fn validate_disposal_receipt_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<bool, SyndicReadError> {
    validate_disposal_receipt_with_access(storage, ReadAccess::Ordinary(store), receipt)
}

pub(super) fn validate_disposal_receipt_with_access(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    receipt: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<bool, SyndicReadError> {
    let (request, transition) = match disposal_receipt_parts(receipt) {
        Some(parts) => parts,
        None => return Ok(false),
    };
    let frontier = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
        store,
        receipt.frontier().reference().key(),
        point_limit(),
    )?;
    let open = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            request.draft_id(),
            request.session_id(),
            receipt.before_head().open_operation_id(),
        ),
        point_limit(),
    )?;
    let head = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
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
    let Some(frontier) = frontier.as_ref() else {
        return Ok(false);
    };
    let source_is_exact = match transition {
        DisposalTransitionKind::Ordinary => {
            session::receipt_matches_head(&open, receipt.before_head())
                && candidate_session_publication_history_is_exact_with_access(
                    storage,
                    store,
                    receipt.before_head(),
                )?
        }
        DisposalTransitionKind::OpeningNormalization => {
            session::receipt_matches_head(&open, receipt.before_head())
                && candidate_session_publication_history_is_exact_with_access(
                    storage,
                    store,
                    receipt.before_head(),
                )?
                && checkpoint::opening_is_exact_with_access(
                    storage,
                    store,
                    receipt.before_head(),
                    frontier,
                )?
        }
        DisposalTransitionKind::FreshAbandonment => {
            let durable = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
                store,
                receipt.before_head().durable_base_history().key(),
                point_limit(),
            )?;
            abandon_fresh::request_matches_head_and_open(request, receipt.before_head(), &open)
                && durable.as_ref().is_some_and(|durable| {
                    durable.reference() == receipt.before_head().durable_base_history()
                        && durable
                            .fork_session(receipt.before_head().session_id())
                            .as_ref()
                            == Some(frontier)
                })
                && draft_edit_history_frontier_is_authenticated_with_access(
                    storage, store, frontier,
                )?
                && match durable.as_ref() {
                    Some(durable) => draft_edit_history_frontier_is_authenticated_with_access(
                        storage, store, durable,
                    )?,
                    None => false,
                }
        }
    };
    Ok(frontier == receipt.frontier() && source_is_exact && head == *receipt.after_head())
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
    type Prepared = Box<PreparedPublicationMutation>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let request = self.prepared.request;
        crate::mutation::discussion_mutation::require_editable(
            reader,
            request.selector().thread_id(),
        )?;
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
            self.prepared.canonical_request,
            request.selector(),
            next_selector,
            head,
            after_head.clone(),
            self.prepared.captured_frontier.clone(),
        );
        let _ = reverse;
        Ok(Box::new(PreparedPublicationMutation {
            request,
            next_draft,
            next_reverse,
            next_summary,
            captured_frontier: self.prepared.captured_frontier,
            after_head,
            receipt,
        }))
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
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftsCodec>(&prepared.next_draft.id(), &prepared.next_draft)?;
        mutations.put::<DraftByThreadCodec>(
            &prepared.next_reverse.thread_id(),
            &prepared.next_reverse,
        )?;
        mutations.put::<HistorySummariesCodec>(
            &prepared.next_summary.thread_id(),
            &prepared.next_summary,
        )?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &prepared.captured_frontier.reference().key(),
            &prepared.captured_frontier,
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(
                prepared.after_head.draft_id(),
                prepared.after_head.session_id(),
            ),
            &DraftEditorCandidateSessionRecordV1::Head(prepared.after_head),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &publication_key(prepared.request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_publication(prepared.receipt),
            ),
        )?;
        Ok(())
    }
}

struct PreparedPublicationMutation {
    request: DraftEditorCandidatePublicationRequestV1,
    next_draft: DraftRecord,
    next_reverse: DraftByThreadRecord,
    next_summary: HistorySummaryRecord,
    captured_frontier: DraftEditHistoryFrontierV1,
    after_head: DraftEditorCandidateSessionV1,
    receipt: DraftEditorCandidatePublicationReceiptV1,
}

impl DomainMutation<SyndicDomain> for DisposalMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedCandidateDisposal>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let request = self.prepared.request;
        if let Some(record) =
            point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(SyndicMutationError::IdentityCollision);
            };
            validate_disposal_receipt(
                reader,
                receipt
                    .disposal()
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            )?;
            return Ok(None);
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
            return Ok(None);
        }
        prepare_candidate_disposal(reader, request, head, self.prepared.frontier).map(Some)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let Some(prepared) = prepared else {
            return Ok(());
        };
        prepared.contribute(mutations)
    }
}

impl SyndicStorage {
    pub fn validate_draft_editor_candidate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        expected: DraftEditorCandidateActivationBindingV1,
    ) -> Result<(), DraftEditorCandidatePublicationCommandErrorV1> {
        let access = ReadAccess::Candidate(store);
        let revision = self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?;
        let head = match publication_session_with_access(
            self,
            access,
            expected.draft_id(),
            expected.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
            DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "candidate editor checkpoint validation",
                }
                .into());
            }
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        if head.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || DraftEditorCandidateActivationBindingV1::from_head(&head) != expected
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if self
            .revision_with_access(access)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "candidate editor checkpoint validation",
            }
            .into());
        }
        Ok(())
    }

    pub fn capture_draft_editor_candidate_publication_source(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
    ) -> Result<
        CapturedDraftEditorCandidatePublicationSourceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.capture_draft_editor_candidate_publication_source_with_access(
            ReadAccess::Ordinary(store),
            request,
        )
    }

    pub fn capture_draft_editor_candidate_publication_source_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
    ) -> Result<
        CapturedDraftEditorCandidatePublicationSourceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.capture_draft_editor_candidate_publication_source_with_access(
            ReadAccess::Candidate(store),
            request,
        )
    }

    fn capture_draft_editor_candidate_publication_source_with_access(
        &self,
        store: ReadAccess<'_>,
        request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
    ) -> Result<
        CapturedDraftEditorCandidatePublicationSourceV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let revision = self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        let current = self
            .current_draft_with_access(store, request.selector().thread_id(), point_limit())?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        let selected = DraftEditorCurrentSelectorV1::new(
            current.thread().id(),
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().piece_root(),
            current.draft().history(),
        );
        if selected != request.selector() {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "candidate publication source selector",
            }
            .into());
        }
        let candidate = request.candidate();
        let pair = DraftRootHistoryPairV1::new(candidate.root(), candidate.history());
        if candidate.draft_id() != request.selector().draft_id()
            || !pair.is_coherent()
            || candidate.candidate_generation() != candidate.history().candidate_generation()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let root = self
            .point_with_access::<DraftPieceRootsFamily>(store, candidate.root().key(), limit)?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if root.reference() != candidate.root()
            || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let head = match publication_session_with_access(
            self,
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
            .point_with_access::<DraftEditHistoryFrontiersFamily>(
                store,
                candidate.history().key(),
                limit,
            )?
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if source_frontier.reference() != candidate.history()
            || !draft_edit_history_frontier_is_authenticated_with_access(
                self,
                store,
                &source_frontier,
            )?
            || !session::idle_candidate_closure_is_exact_with_access(self, store, &head)?
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "candidate publication source capture",
            }
            .into());
        }
        Ok(CapturedDraftEditorCandidatePublicationSourceV1 {
            storage: self.clone(),
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
        let prepared = preparation::prepare(self, ReadAccess::Ordinary(store), &source, evidence);
        prepared.map(|value| *value).map_err(|error| {
            DraftEditorCandidatePublicationSourcePreparationErrorV1 { source, error }
        })
    }

    pub fn prepare_draft_editor_candidate_publication_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        source: CapturedDraftEditorCandidatePublicationSourceV1,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
    ) -> Result<
        PreparedDraftEditorCandidatePublicationV1,
        DraftEditorCandidatePublicationSourcePreparationErrorV1,
    > {
        let prepared = preparation::prepare(self, ReadAccess::Candidate(store), &source, evidence);
        prepared.map(|value| *value).map_err(|error| {
            DraftEditorCandidatePublicationSourcePreparationErrorV1 { source, error }
        })
    }

    pub fn publish_draft_editor_candidate(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidatePublicationV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublicationMutation {
                prepared: Box::new(prepared),
            },
        )
    }

    pub fn publish_draft_editor_candidate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidatePublicationV1,
    ) -> Result<MutationContribution, DraftEditorCandidatePublicationCommandErrorV1> {
        if store
            .domain_revision(&self.handle)
            .map_err(SyndicReadError::Read)?
            != expected_domain_revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "candidate publication contribution",
            }
            .into());
        }
        Ok(self.publish_draft_editor_candidate(expected_domain_revision, prepared))
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
        self.reconcile_draft_editor_candidate_publication_with_access(
            ReadAccess::Ordinary(store),
            prepared,
            outcome,
        )
    }

    pub fn reconcile_draft_editor_candidate_publication_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidatePublicationOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.reconcile_draft_editor_candidate_publication_with_access(
            ReadAccess::Candidate(store),
            prepared,
            outcome,
        )
    }

    fn reconcile_draft_editor_candidate_publication_with_access(
        &self,
        store: ReadAccess<'_>,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidatePublicationOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let committed = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let handle = reconciliation.install_and_handle();
                let resolution = match store {
                    ReadAccess::Ordinary(store) => store.reconcile(&handle),
                    ReadAccess::Candidate(store) => store.reconcile(&handle),
                }
                .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?;
                match resolution {
                    ReconciliationResolution::ExactNew { .. } => true,
                    ReconciliationResolution::ExactOld => false,
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision => {
                        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
                    }
                }
            }
        };
        self.publication_outcome_with_access(store, prepared, committed)
    }

    fn publication_outcome_with_access(
        &self,
        store: ReadAccess<'_>,
        prepared: &PreparedDraftEditorCandidatePublicationV1,
        committed: bool,
    ) -> Result<
        DraftEditorCandidatePublicationOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let request = prepared.request;
        let limit = point_limit();
        if let Some(record) = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
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
            if !validate_publication_receipt_with_access(self, store, &receipt)? {
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
        let head = match publication_session_with_access(
            self,
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
            .current_draft_with_access(store, request.selector().thread_id(), limit)?
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
}
