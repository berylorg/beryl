use std::{error::Error, fmt};

use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationFailure, ReconciliationReservation, ReconciliationResolution,
};
use beryl_model::DomainRevision;

use crate::codec::{DraftByThreadFamily, ThreadsFamily};
use crate::domain::{SyndicDomain, SyndicStorage};
use crate::mutation::{current_draft, point, required};
use crate::{SyndicMutationError, SyndicReadError};

use super::*;

#[derive(Clone)]
pub struct PreparedDraftEditorCandidateSessionOpenV1 {
    request: DraftEditorCandidateSessionOpenRequestV1,
    canonical_request: Vec<u8>,
    initially_absent: bool,
}

impl PreparedDraftEditorCandidateSessionOpenV1 {
    pub const fn request(&self) -> DraftEditorCandidateSessionOpenRequestV1 {
        self.request
    }
    pub fn canonical_request(&self) -> &[u8] {
        &self.canonical_request
    }
}

#[derive(Debug)]
pub enum DraftEditorCandidateSessionCommandErrorV1 {
    Read(SyndicReadError),
    Reconciliation(ReconciliationFailure),
    Invariant,
}

impl fmt::Display for DraftEditorCandidateSessionCommandErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Reconciliation(error) => error.fmt(formatter),
            Self::Invariant => formatter.write_str("invalid editor-candidate session closure"),
        }
    }
}

impl Error for DraftEditorCandidateSessionCommandErrorV1 {}

impl From<SyndicReadError> for DraftEditorCandidateSessionCommandErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::Read(value)
    }
}

#[derive(Clone)]
struct OpenSessionMutation {
    prepared: PreparedDraftEditorCandidateSessionOpenV1,
}

fn head_key(
    request: DraftEditorCandidateSessionOpenRequestV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::head(
        request.selector().draft_id(),
        request.session_id(),
    )
}

fn receipt_key(
    request: DraftEditorCandidateSessionOpenRequestV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::open_receipt(
        request.selector().draft_id(),
        request.session_id(),
        request.operation_id(),
    )
}

fn selector_matches(
    reader: &DomainReader<'_, SyndicDomain>,
    request: DraftEditorCandidateSessionOpenRequestV1,
) -> Result<bool, SyndicMutationError> {
    let expected = request.selector();
    let thread = required::<ThreadsFamily>(reader, &expected.thread_id())?;
    let draft = current_draft(reader, expected.thread_id())?;
    let reverse = required::<DraftByThreadFamily>(reader, &expected.thread_id())?;
    let root = required::<DraftPieceRootsFamily>(reader, &draft.piece_root().key())?;
    let history = required::<DraftEditHistoryFrontiersFamily>(reader, &draft.history().key())?;
    Ok(thread.id() == expected.thread_id()
        && thread.revision() == expected.thread_revision()
        && draft.id() == expected.draft_id()
        && draft.revision() == expected.selector_revision()
        && draft.piece_root() == expected.root()
        && draft.history() == expected.history()
        && reverse.draft_id() == draft.id()
        && reverse.draft_revision() == draft.revision()
        && reverse.thread_revision() == thread.revision()
        && root.reference() == expected.root()
        && history.reference() == expected.history())
}

fn selected_history(
    reader: &DomainReader<'_, SyndicDomain>,
    request: DraftEditorCandidateSessionOpenRequestV1,
) -> Result<DraftEditHistoryFrontierV1, SyndicMutationError> {
    let history =
        required::<DraftEditHistoryFrontiersFamily>(reader, &request.selector().history().key())?;
    if history.reference() != request.selector().history() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &history)?;
    Ok(history)
}

fn occupied_records(
    reader: &DomainReader<'_, SyndicDomain>,
    request: DraftEditorCandidateSessionOpenRequestV1,
) -> Result<
    (
        Option<DraftEditorCandidateSessionRecordV1>,
        Option<DraftEditorCandidateSessionRecordV1>,
    ),
    SyndicMutationError,
> {
    Ok((
        point::<DraftEditorCandidateSessionsFamily>(reader, &head_key(request))?,
        point::<DraftEditorCandidateSessionsFamily>(reader, &receipt_key(request))?,
    ))
}

fn occupied_open_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<DraftEditorCandidateSessionOpenReceiptV1, SyndicMutationError> {
    match required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            head.draft_id(),
            head.session_id(),
            head.open_operation_id(),
        ),
    )? {
        DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)
            if receipt_matches_head(&receipt, head) =>
        {
            Ok(receipt)
        }
        _ => Err(SyndicMutationError::IdentityCollision),
    }
}

pub(super) fn receipt_matches_head(
    receipt: &DraftEditorCandidateSessionOpenReceiptV1,
    head: &DraftEditorCandidateSessionV1,
) -> bool {
    if !receipt.is_open() {
        return false;
    }
    let opened = receipt.head();
    opened.thread_id() == head.thread_id()
        && opened.draft_id() == head.draft_id()
        && opened.session_id() == head.session_id()
        && opened.open_operation_id() == head.open_operation_id()
        && opened.durable_base_selector_revision() == head.durable_base_selector_revision()
        && opened.durable_base_root() == head.durable_base_root()
        && opened.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && opened.session_generation() <= head.session_generation()
}

pub(super) fn adopted_head_matches_current(
    adopted: &DraftEditorCandidateSessionV1,
    current: &DraftEditorCandidateSessionV1,
) -> bool {
    adopted.thread_id() == current.thread_id()
        && adopted.draft_id() == current.draft_id()
        && adopted.session_id() == current.session_id()
        && adopted.open_operation_id() == current.open_operation_id()
        && adopted.durable_base_selector_revision() == current.durable_base_selector_revision()
        && adopted.durable_base_root() == current.durable_base_root()
        && adopted.durable_base_history() == current.durable_base_history()
        && (adopted.published_candidate_generation() == current.published_candidate_generation()
            && adopted.published_selector_revision() == current.published_selector_revision()
            && adopted.published_root() == current.published_root()
            && adopted.published_history() == current.published_history()
            || adopted.published_candidate_generation() < current.published_candidate_generation()
                && matches!(
                    current.published_history().key(),
                    DraftEditHistoryFrontierKeyV1::Publication { session_id, .. }
                        if session_id == current.session_id()
                ))
        && adopted.newest_candidate_generation() == current.newest_candidate_generation()
        && adopted.newest_root() == current.newest_root()
        && adopted.newest_history() == current.newest_history()
        && adopted.logical_extent() == current.logical_extent()
        && adopted.session_generation() <= current.session_generation()
        && adopted.dirty_generation() <= current.dirty_generation()
}

pub(super) fn candidate_session_adoption_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    if !super::publication::candidate_session_publication_is_exact_in_store(storage, store, head)? {
        return Ok(false);
    }
    if !active_operation_custody_is_exact(storage, store, head)? {
        return Ok(false);
    }
    let root = head.newest_root();
    let history_reference = head.newest_history();
    let stored_root = storage.point::<DraftPieceRootsFamily>(store, root.key(), point_limit())?;
    let stored_history = storage.point::<DraftEditHistoryFrontiersFamily>(
        store,
        history_reference.key(),
        point_limit(),
    )?;
    let (Some(stored_root), Some(stored_history)) = (stored_root.as_ref(), stored_history.as_ref())
    else {
        return Ok(false);
    };
    if stored_root.reference() != root
        || stored_history.reference() != history_reference
        || history_reference.root() != root
        || history_reference.candidate_generation() != head.newest_candidate_generation()
    {
        return Ok(false);
    }
    if !draft_edit_history_frontier_is_authenticated_v1(storage, store, stored_history)? {
        return Ok(false);
    }
    if head.newest_candidate_generation() == 0 {
        let durable_history = storage.point::<DraftEditHistoryFrontiersFamily>(
            store,
            head.durable_base_history().key(),
            point_limit(),
        )?;
        return Ok(root == head.durable_base_root()
            && history_reference.key().session_id() == Some(head.session_id())
            && durable_history.as_ref().is_some_and(|frontier| {
                frontier.reference() == head.durable_base_history()
                    && frontier.fork_session(head.session_id()).as_ref() == Some(stored_history)
            }));
    }
    if head.newest_candidate_generation() == head.published_candidate_generation()
        && head.newest_root() == head.published_root()
    {
        let published = storage.point::<DraftEditHistoryFrontiersFamily>(
            store,
            head.published_history().key(),
            point_limit(),
        )?;
        return Ok(published.as_ref().is_some_and(|frontier| {
            frontier.reference() == head.published_history()
                && (frontier == stored_history
                    && matches!(
                        frontier.reference().key(),
                        DraftEditHistoryFrontierKeyV1::Publication { session_id, .. }
                            if session_id == head.session_id()
                    )
                    || frontier.fork_session(head.session_id()).as_ref() == Some(stored_history))
        }));
    }
    let Some(journal_head) = stored_history.journal_head() else {
        return Ok(false);
    };
    let Some(newest_transition) = storage.point::<DraftEditHistoryTransitionsFamily>(
        store,
        journal_head.key(),
        point_limit(),
    )?
    else {
        return Ok(false);
    };
    if newest_transition.reference() != journal_head {
        return Ok(false);
    }
    if newest_transition.kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit {
        return historical_candidate_session_is_exact_in_store(
            storage,
            store,
            head,
            newest_transition.operation_id(),
        );
    }
    let key = DraftPieceSettlementKeyV1::new(
        head.draft_id(),
        head.session_id(),
        newest_transition.operation_id(),
    );
    let settlement = storage.point::<DraftPieceSettlementsFamily>(store, key, point_limit())?;
    let build = storage.point::<DraftPieceBuildsFamily>(store, key, point_limit())?;
    if let Some(build) = build.as_ref() {
        let receipt = storage.point::<DraftPieceBuildProgressFamily>(
            store,
            build.progress_receipt().key(),
            point_limit(),
        )?;
        let Some(receipt) = receipt else {
            return Ok(false);
        };
        if !progress_receipt_matches_build(&receipt, build) {
            return Ok(false);
        }
        if !progress_receipt_closure_is_exact(storage, store, &receipt)? {
            return Ok(false);
        }
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
        {
            return Ok(false);
        }
    }
    let Some(settlement) = settlement else {
        return Ok(false);
    };
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        return Ok(false);
    };
    let stored_transition = storage.point::<DraftEditHistoryTransitionsFamily>(
        store,
        adoption.transition().key(),
        point_limit(),
    )?;
    Ok(settlement_closure_is_exact(&settlement)
        && settlement_terminal_build_is_exact(&settlement, build.as_ref())
        && adopted_head_matches_current(adoption.adopted_session(), head)
        && adoption.adopted_root() == stored_root
        && adoption.adopted_history() == stored_history
        && stored_transition.as_ref() == Some(adoption.transition())
        && matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Committed {
                successor,
                candidate_generation,
                ..
            } if *successor == root
                && *candidate_generation == head.newest_candidate_generation()
        ))
}

pub(super) fn candidate_session_closure_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    let receipt_key = DraftEditorCandidateSessionRecordKeyV1::open_receipt(
        head.draft_id(),
        head.session_id(),
        head.open_operation_id(),
    );
    let receipt =
        storage.point::<DraftEditorCandidateSessionsFamily>(store, receipt_key, point_limit())?;
    let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) = receipt else {
        return Ok(false);
    };
    if !receipt_matches_head(&receipt, head) {
        return Ok(false);
    }
    match head.lifecycle() {
        DraftEditorCandidateSessionLifecycleV1::Active => {
            candidate_session_adoption_is_exact(storage, store, head)
        }
        DraftEditorCandidateSessionLifecycleV1::Disposed => {
            publication::candidate_session_disposal_is_exact_in_store(storage, store, head)
        }
    }
}

fn active_operation_custody_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    let Some(custody) = head.active_operation() else {
        return Ok(true);
    };
    if let Some(staging_receipt) = custody.staging_receipt() {
        let identity = staging_receipt.identity();
        let staging = storage.draft_mutation_staging_status(store, identity);
        return Ok(matches!(
            staging,
            Ok(DraftMutationStagingStatusV1::Receiving { head: selected }
                | DraftMutationStagingStatusV1::Finished { head: selected })
                if selected == staging_receipt
                    && custody.begin_digest().is_some()
                    && custody.predecessor_candidate_generation()
                        == head.newest_candidate_generation()
                    && custody.predecessor_root() == head.newest_root()
                    && custody.predecessor_history() == head.newest_history()
        ));
    }
    let key =
        DraftPieceSettlementKeyV1::new(head.draft_id(), head.session_id(), custody.operation_id());
    let build = storage.point::<DraftPieceBuildsFamily>(store, key, point_limit())?;
    let settlement = storage.point::<DraftPieceSettlementsFamily>(store, key, point_limit())?;
    let Some(build) = build else { return Ok(false) };
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
    let next_receipt = storage.point::<DraftPieceBuildProgressFamily>(
        store,
        DraftPieceBuildProgressReceiptKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            next_ordinal,
        ),
        point_limit(),
    )?;
    let fragment_ahead = if build.staged_fragment_count() < build.fragment_count() {
        storage
            .point::<DraftPieceBuildFragmentsFamily>(
                store,
                DraftPieceBuildFragmentKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    build.staged_fragment_count() + 1,
                ),
                point_limit(),
            )?
            .is_some()
    } else {
        false
    };
    Ok(settlement.is_none()
        && next_receipt.is_none()
        && !fragment_ahead
        && build_record_is_exact(&build)
        && progress_receipt_matches_build(&receipt, &build)
        && progress_receipt_closure_is_exact(storage, store, &receipt)?
        && matches!(
            build.lifecycle(),
            DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
        )
        && custody.operation_id() == build.operation_id()
        && custody.proposal_digest() == Some(build.proposal_digest())
        && custody.predecessor_candidate_generation() == build.predecessor_candidate_generation()
        && custody.predecessor_root() == build.predecessor_root()
        && custody.build_receipt() == Some(build.progress_receipt()))
}

pub(super) fn progress_receipt_closure_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<bool, SyndicReadError> {
    if !progress_receipt_is_exact(receipt) {
        return Ok(false);
    }
    if let Some(previous) = receipt.previous() {
        let stored =
            storage.point::<DraftPieceBuildProgressFamily>(store, previous.key(), point_limit())?;
        let Some(stored) = stored else {
            return Ok(false);
        };
        if stored.reference() != previous
            || !progress_receipt_is_exact(&stored)
            || !super::read::progress_receipt_effects_are_exact(
                storage,
                store,
                &stored,
                point_limit(),
            )?
        {
            return Ok(false);
        }
        if !super::read::progress_receipt_transition_is_exact(
            storage,
            store,
            &stored,
            receipt,
            point_limit(),
        )? {
            return Ok(false);
        }
    }
    super::read::progress_receipt_effects_are_exact(storage, store, receipt, point_limit())
}

impl DomainMutation<SyndicDomain> for OpenSessionMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let (head, receipt) = occupied_records(reader, self.prepared.request)?;
        if let Some(DraftEditorCandidateSessionRecordV1::Head(head)) = head {
            let _ = occupied_open_receipt(reader, &head)?;
            let frontier =
                required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
            if frontier.reference() != head.newest_history() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            authenticate_draft_edit_history_frontier_v1(reader, &frontier)?;
            return if receipt.is_none()
                || matches!(
                    receipt,
                    Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(_))
                ) {
                Ok(())
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        } else if head.is_some() || receipt.is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let _ = selector_matches(reader, self.prepared.request)?;
        let source = selected_history(reader, self.prepared.request)?;
        let target_key = DraftEditHistoryFrontierKeyV1::session(
            self.prepared.request.selector().draft_id(),
            self.prepared.request.session_id(),
        );
        if source
            .fork_session(self.prepared.request.session_id())
            .is_none()
            || point::<DraftEditHistoryFrontiersFamily>(reader, &target_key)?.is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let (occupied_head, occupied_receipt) = occupied_records(reader, self.prepared.request)?;
        if occupied_head.is_some() || occupied_receipt.is_some() {
            return Ok(());
        }
        if !selector_matches(reader, self.prepared.request)? {
            return Ok(());
        }
        let source_history = selected_history(reader, self.prepared.request)?;
        let forked_history = source_history
            .fork_session(self.prepared.request.session_id())
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let head = DraftEditorCandidateSessionV1::opened(
            self.prepared.request,
            forked_history.reference(),
        );
        let receipt = DraftEditorCandidateSessionOpenReceiptV1::new(
            self.prepared.canonical_request.clone(),
            head.clone(),
        );
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &head_key(self.prepared.request),
            &DraftEditorCandidateSessionRecordV1::Head(head),
        )?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &forked_history.reference().key(),
            &forked_history,
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &receipt_key(self.prepared.request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt),
        )?;
        Ok(())
    }
}

impl SyndicStorage {
    pub fn prepare_open_draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidateSessionOpenRequestV1,
    ) -> Result<PreparedDraftEditorCandidateSessionOpenV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        if request.selector().draft_id() != request.selector().root().key().draft_id()
            || request.selector().draft_id() != request.selector().history().key().draft_id()
            || request.selector().root() != request.selector().history().root()
            || !request
                .selector()
                .root()
                .summary()
                .text_summary()
                .is_canonical()
        {
            return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let head =
            self.point::<DraftEditorCandidateSessionsFamily>(store, head_key(request), limit)?;
        let receipt =
            self.point::<DraftEditorCandidateSessionsFamily>(store, receipt_key(request), limit)?;
        if head.is_none() && receipt.is_some() {
            return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
        }
        Ok(PreparedDraftEditorCandidateSessionOpenV1 {
            request,
            canonical_request: canonical_session_open_request_bytes(request),
            initially_absent: head.is_none(),
        })
    }

    pub fn open_draft_editor_candidate_session(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidateSessionOpenV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, OpenSessionMutation { prepared })
    }

    pub fn reconcile_draft_editor_candidate_session_open(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidateSessionOpenV1,
        outcome: CommandOutcome,
    ) -> Result<DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        let created_by_command = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let handle = reconciliation.install_and_handle();
                match store
                    .reconcile(&handle)
                    .map_err(DraftEditorCandidateSessionCommandErrorV1::Reconciliation)?
                {
                    ReconciliationResolution::ExactNew { .. } => true,
                    ReconciliationResolution::ExactOld => false,
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision => {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    }
                }
            }
        };
        self.draft_editor_candidate_session_open_outcome(store, prepared, created_by_command)
    }

    fn draft_editor_candidate_session_open_outcome(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidateSessionOpenV1,
        created_by_command: bool,
    ) -> Result<DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionCommandErrorV1>
    {
        let request = prepared.request;
        let limit = point_limit();
        let head =
            self.point::<DraftEditorCandidateSessionsFamily>(store, head_key(request), limit)?;
        match head {
            Some(DraftEditorCandidateSessionRecordV1::Head(head)) => {
                for (root_reference, history_reference) in [
                    (head.durable_base_root(), head.durable_base_history()),
                    (head.published_root(), head.published_history()),
                    (head.newest_root(), head.newest_history()),
                ] {
                    let Some(root) =
                        self.point::<DraftPieceRootsFamily>(store, root_reference.key(), limit)?
                    else {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    };
                    let Some(history) = self.point::<DraftEditHistoryFrontiersFamily>(
                        store,
                        history_reference.key(),
                        limit,
                    )?
                    else {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    };
                    if root.reference() != root_reference
                        || history.reference() != history_reference
                        || !history.is_locally_valid()
                    {
                        return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                    }
                }
                let occupied_key = DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                    head.draft_id(),
                    head.session_id(),
                    head.open_operation_id(),
                );
                let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) =
                    self.point::<DraftEditorCandidateSessionsFamily>(store, occupied_key, limit)?
                else {
                    return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                };
                if !receipt_matches_head(&receipt, &head) {
                    return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                }
                if receipt.request_bytes() != prepared.canonical_request {
                    return Ok(
                        DraftEditorCandidateSessionOpenOutcomeV1::OccupiedIdentityCollision(
                            DraftEditorCandidateSessionCollisionProofV1::new(request, receipt),
                        ),
                    );
                }
                match head.lifecycle() {
                    DraftEditorCandidateSessionLifecycleV1::Disposed => Ok(
                        DraftEditorCandidateSessionOpenOutcomeV1::StaleDisposed(head),
                    ),
                    DraftEditorCandidateSessionLifecycleV1::Active
                        if prepared.initially_absent && created_by_command =>
                    {
                        Ok(DraftEditorCandidateSessionOpenOutcomeV1::Opened(head))
                    }
                    DraftEditorCandidateSessionLifecycleV1::Active => {
                        Ok(DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head))
                    }
                }
            }
            None => {
                let Some(current) =
                    self.current_draft(store, request.selector().thread_id(), limit)?
                else {
                    return Err(DraftEditorCandidateSessionCommandErrorV1::Invariant);
                };
                let current_selector = DraftEditorCurrentSelectorV1::new(
                    current.thread().id(),
                    current.thread().revision(),
                    current.draft().id(),
                    current.draft().revision(),
                    current.draft().piece_root(),
                    current.draft().history(),
                );
                if current_selector == request.selector() {
                    Err(DraftEditorCandidateSessionCommandErrorV1::Invariant)
                } else {
                    Ok(DraftEditorCandidateSessionOpenOutcomeV1::SelectorConflict(
                        current_selector,
                    ))
                }
            }
            Some(_) => Err(DraftEditorCandidateSessionCommandErrorV1::Invariant),
        }
    }

    pub fn draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        draft_id: beryl_model::SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> Result<DraftEditorCandidateSessionReadOutcomeV1, SyndicReadError> {
        let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
        let limit = point_limit();
        let first = self.point::<DraftEditorCandidateSessionsFamily>(store, key, limit)?;
        let second = self.point::<DraftEditorCandidateSessionsFamily>(store, key, limit)?;
        if first != second {
            return Ok(DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange);
        }
        match first {
            None => Ok(DraftEditorCandidateSessionReadOutcomeV1::Absent),
            Some(DraftEditorCandidateSessionRecordV1::Head(head))
                if head.draft_id() == draft_id && head.session_id() == session_id =>
            {
                if !candidate_session_closure_is_exact_in_store(self, store, &head)? {
                    return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
                }
                Ok(match head.lifecycle() {
                    DraftEditorCandidateSessionLifecycleV1::Active => {
                        DraftEditorCandidateSessionReadOutcomeV1::Active(head)
                    }
                    DraftEditorCandidateSessionLifecycleV1::Disposed => {
                        DraftEditorCandidateSessionReadOutcomeV1::Disposed(head)
                    }
                })
            }
            Some(_) => Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure),
        }
    }
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
struct DisposeSessionFixtureMutation {
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
}

#[cfg(feature = "test-faults")]
fn fixture_disposal_operation_id(head: &DraftEditorCandidateSessionV1) -> DraftPieceOperationIdV1 {
    let mut bytes = *head.open_operation_id().as_bytes();
    bytes[0] ^= 0xff;
    DraftPieceOperationIdV1::from_bytes(bytes)
}

#[cfg(feature = "test-faults")]
impl DomainMutation<SyndicDomain> for DisposeSessionFixtureMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let value = required::<DraftEditorCandidateSessionsFamily>(
            reader,
            &DraftEditorCandidateSessionRecordKeyV1::head(self.draft_id, self.session_id),
        )?;
        if !matches!(value, DraftEditorCandidateSessionRecordV1::Head(_)) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if matches!(
            value,
            DraftEditorCandidateSessionRecordV1::Head(ref head)
                if head.active_operation().is_some()
                    || head.published_candidate_generation()
                        != head.newest_candidate_generation()
                    || head.published_root() != head.newest_root()
        ) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
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
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &DraftEditorCandidateSessionRecordKeyV1::head(self.draft_id, self.session_id),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
            return Ok(());
        }
        let clean = if head.published_history() == head.newest_history() {
            head
        } else {
            DraftEditorCandidateSessionV1::from_parts(
                head.thread_id(),
                head.draft_id(),
                head.session_id(),
                head.open_operation_id(),
                head.session_generation(),
                head.durable_base_selector_revision(),
                head.durable_base_root(),
                head.durable_base_history(),
                head.published_candidate_generation(),
                head.published_selector_revision(),
                head.published_root(),
                head.published_history(),
                head.newest_candidate_generation(),
                head.newest_root(),
                head.published_history(),
                head.dirty_generation(),
                head.logical_extent(),
                DraftEditorCandidateSessionLifecycleV1::Active,
                None,
            )
        };
        let operation_id = fixture_disposal_operation_id(&clean);
        let request = DraftEditorCandidateSessionDisposeRequestV1::new(
            clean.draft_id(),
            clean.session_id(),
            operation_id,
            clean.session_generation(),
            DraftRootHistoryPairV1::new(clean.newest_root(), clean.newest_history()),
        );
        let frontier =
            required::<DraftEditHistoryFrontiersFamily>(reader, &clean.newest_history().key())?;
        let disposed = clean
            .disposed(operation_id)
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let receipt = DraftEditorCandidateSessionDisposeReceiptV1::new(
            canonical_candidate_disposal_request_bytes(request),
            clean,
            disposed.clone(),
            frontier,
        );
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &DraftEditorCandidateSessionRecordKeyV1::head(self.draft_id, self.session_id),
            &DraftEditorCandidateSessionRecordV1::Head(disposed),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                self.draft_id,
                self.session_id,
                operation_id,
            ),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_disposal(receipt),
            ),
        )?;
        Ok(())
    }
}

#[cfg(feature = "test-faults")]
impl SyndicStorage {
    pub fn test_dispose_draft_editor_candidate_session(
        &self,
        expected_domain_revision: DomainRevision,
        draft_id: beryl_model::SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            DisposeSessionFixtureMutation {
                draft_id,
                session_id,
            },
        )
    }
}
