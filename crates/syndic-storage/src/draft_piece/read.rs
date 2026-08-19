use std::{error::Error, fmt};

use beryl_home_store::{CommandOutcome, HomeStore, ReadError, ReconciliationFailure};
use beryl_model::SyndicThreadId;

use crate::codec::{DraftByThreadFamily, DraftsFamily, ThreadsFamily};
use crate::{SyndicReadError, SyndicStorage};

use super::*;

#[derive(Debug)]
pub enum DraftPieceCommandReconciliationErrorV1 {
    Read(SyndicReadError),
    Reconciliation(ReconciliationFailure),
    InvalidFragmentPage,
}

impl fmt::Display for DraftPieceCommandReconciliationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Reconciliation(error) => error.fmt(formatter),
            Self::InvalidFragmentPage => formatter.write_str("invalid draft-piece fragment page"),
        }
    }
}

impl Error for DraftPieceCommandReconciliationErrorV1 {}

impl From<SyndicReadError> for DraftPieceCommandReconciliationErrorV1 {
    fn from(error: SyndicReadError) -> Self {
        Self::Read(error)
    }
}

fn range_error(error: DraftPiecePrepareErrorV1) -> DraftPieceRangeSourceErrorV1 {
    match error {
        DraftPiecePrepareErrorV1::Read(SyndicReadError::Read(
            error @ (ReadError::HealthGate(_)
            | ReadError::GenerationPoisoned
            | ReadError::Storage { .. }),
        )) => DraftPieceRangeSourceErrorV1::Operational(SyndicReadError::Read(error)),
        DraftPiecePrepareErrorV1::Read(SyndicReadError::ConcurrentChange { .. }) => {
            DraftPieceRangeSourceErrorV1::ConcurrentChange
        }
        DraftPiecePrepareErrorV1::Read(_) => DraftPieceRangeSourceErrorV1::Invariant,
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::InvalidUtf8Boundary) => {
            DraftPieceRangeSourceErrorV1::Malformed(DraftPieceMalformedRangeRequestV1::Utf8Boundary)
        }
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::InvalidGapWitness) => {
            DraftPieceRangeSourceErrorV1::Malformed(DraftPieceMalformedRangeRequestV1::Cursor)
        }
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::TreeLimit) => {
            DraftPieceRangeSourceErrorV1::Limit
        }
        DraftPiecePrepareErrorV1::Rejected(_) | DraftPiecePrepareErrorV1::InvalidRoot => {
            DraftPieceRangeSourceErrorV1::Invariant
        }
        DraftPiecePrepareErrorV1::Absent => DraftPieceRangeSourceErrorV1::Absent,
        DraftPiecePrepareErrorV1::ConcurrentChange => {
            DraftPieceRangeSourceErrorV1::ConcurrentChange
        }
    }
}

impl SyndicStorage {
    fn current_range_selector(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<Option<DraftEditorCurrentSelectorV1>, DraftPieceRangeSourceErrorV1> {
        let limit = point_limit();
        let Some(first) = self.point::<DraftByThreadFamily>(store, thread_id, limit)? else {
            return match self.point::<DraftByThreadFamily>(store, thread_id, limit)? {
                None => Ok(None),
                Some(_) => Err(DraftPieceRangeSourceErrorV1::ConcurrentChange),
            };
        };
        let Some(thread) = self.point::<ThreadsFamily>(store, thread_id, limit)? else {
            return Err(DraftPieceRangeSourceErrorV1::Invariant);
        };
        let Some(draft) = self.point::<DraftsFamily>(store, first.draft_id(), limit)? else {
            return Err(DraftPieceRangeSourceErrorV1::Invariant);
        };
        let Some(second) = self.point::<DraftByThreadFamily>(store, thread_id, limit)? else {
            return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
        };
        if second != first {
            return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
        }
        if thread.current_draft_id() != draft.id()
            || draft.thread_id() != thread.id()
            || first.thread_id() != thread.id()
            || first.draft_id() != draft.id()
            || first.draft_revision() != draft.revision()
            || first.thread_revision() != thread.revision()
        {
            return Err(DraftPieceRangeSourceErrorV1::Invariant);
        }
        Ok(Some(DraftEditorCurrentSelectorV1::new(
            thread.id(),
            thread.revision(),
            draft.id(),
            draft.revision(),
            draft.piece_root(),
        )))
    }

    fn stabilized_current_range<T>(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        read: impl FnOnce(DraftPieceRootReferenceV1) -> Result<T, DraftPieceRangeSourceErrorV1>,
    ) -> Result<Option<DraftPieceCurrentRangeResultV1<T>>, DraftPieceRangeSourceErrorV1> {
        let Some(selector) = self.current_range_selector(store, thread_id)? else {
            return Ok(None);
        };
        let value = read(selector.root())?;
        #[cfg(feature = "test-faults")]
        crate::test_faults::run_draft_piece_current_read_fault(store, *self);
        if self.current_range_selector(store, thread_id)? != Some(selector) {
            return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
        }
        Ok(Some(DraftPieceCurrentRangeResultV1::new(selector, value)))
    }

    fn stabilized_candidate_range<T>(
        &self,
        store: &HomeStore,
        expected: DraftEditorCandidateActivationBindingV1,
        read: impl FnOnce(DraftPieceRootReferenceV1) -> Result<T, DraftPieceRangeSourceErrorV1>,
    ) -> Result<DraftPieceCandidateRangeResultV1<T>, DraftPieceRangeSourceErrorV1> {
        let head = match self.draft_editor_candidate_session(
            store,
            expected.draft_id(),
            expected.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => {
                return Err(DraftPieceRangeSourceErrorV1::Disposed(head));
            }
            DraftEditorCandidateSessionReadOutcomeV1::Absent => {
                return Err(DraftPieceRangeSourceErrorV1::Absent);
            }
            DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
                return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
            }
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
                return Err(DraftPieceRangeSourceErrorV1::Invariant);
            }
        };
        if head.session_generation() != expected.session_generation() {
            return Err(DraftPieceRangeSourceErrorV1::StaleSession);
        }
        if head.newest_candidate_generation() != expected.candidate_generation()
            || head.newest_root() != expected.root()
            || head.logical_extent() != expected.logical_extent()
        {
            return Err(DraftPieceRangeSourceErrorV1::StaleCandidate);
        }
        let value = read(expected.root())?;
        #[cfg(feature = "test-faults")]
        crate::test_faults::run_draft_piece_candidate_read_fault(store, *self);
        match self.draft_editor_candidate_session(
            store,
            expected.draft_id(),
            expected.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(after) if after == head => {}
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(after) => {
                return Err(DraftPieceRangeSourceErrorV1::Disposed(after));
            }
            DraftEditorCandidateSessionReadOutcomeV1::Absent => {
                return Err(DraftPieceRangeSourceErrorV1::Absent);
            }
            DraftEditorCandidateSessionReadOutcomeV1::Active(_)
            | DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
                return Err(DraftPieceRangeSourceErrorV1::ConcurrentChange);
            }
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
                return Err(DraftPieceRangeSourceErrorV1::Invariant);
            }
        }
        Ok(DraftPieceCandidateRangeResultV1::new(expected, value))
    }

    pub fn draft_piece_text_demand(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<DraftPieceTextDemandResultV1, DraftPieceRangeSourceErrorV1> {
        if !(4..=DRAFT_PIECE_PAGE_MAX_BYTES).contains(&max_bytes) {
            return Err(DraftPieceRangeSourceErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Limit,
            ));
        }
        read_text_demand(self, store, root, demand, max_bytes).map_err(range_error)
    }

    pub fn current_draft_piece_text_demand(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<
        Option<DraftPieceCurrentRangeResultV1<DraftPieceTextDemandResultV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_current_range(store, thread_id, |root| {
            self.draft_piece_text_demand(store, root, demand, max_bytes)
        })
    }

    pub fn candidate_draft_piece_text_demand(
        &self,
        store: &HomeStore,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceTextDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_candidate_range(store, binding, |root| {
            self.draft_piece_text_demand(store, root, demand, max_bytes)
        })
    }

    pub fn draft_piece_marker_demand(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<DraftPieceMarkerDemandResultV1, DraftPieceRangeSourceErrorV1> {
        if !(1..=DRAFT_PIECE_PAGE_MAX_RECORDS).contains(&demand.object_ceiling())
            || !(1..=DRAFT_PIECE_PAGE_MAX_BYTES).contains(&demand.retained_byte_ceiling())
        {
            return Err(DraftPieceRangeSourceErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Limit,
            ));
        }
        read_marker_demand(self, store, root, &demand).map_err(range_error)
    }

    pub fn current_draft_piece_marker_demand(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<
        Option<DraftPieceCurrentRangeResultV1<DraftPieceMarkerDemandResultV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_current_range(store, thread_id, |root| {
            self.draft_piece_marker_demand(store, root, demand)
        })
    }

    pub fn candidate_draft_piece_marker_demand(
        &self,
        store: &HomeStore,
        binding: DraftEditorCandidateActivationBindingV1,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<DraftPieceMarkerDemandResultV1>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_candidate_range(store, binding, |root| {
            self.draft_piece_marker_demand(store, root, demand)
        })
    }

    pub fn draft_piece_marker_edge_proof(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<Option<DraftPieceMarkerEdgeProofV1>, DraftPieceRangeSourceErrorV1> {
        if !(1..=DRAFT_PIECE_PAGE_MAX_BYTES).contains(&retained_byte_ceiling) {
            return Err(DraftPieceRangeSourceErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Limit,
            ));
        }
        let required = match request {
            DraftPieceMarkerEdgeProofRequestV1::Absence { .. } => 9,
            DraftPieceMarkerEdgeProofRequestV1::First { .. }
            | DraftPieceMarkerEdgeProofRequestV1::Last { .. } => 41,
            DraftPieceMarkerEdgeProofRequestV1::Adjacent { .. } => 81,
        };
        if retained_byte_ceiling < required {
            return Err(DraftPieceRangeSourceErrorV1::Limit);
        }
        prove_marker_edge(self, store, root, request).map_err(range_error)
    }

    pub fn current_draft_piece_marker_edge_proof(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<
        Option<DraftPieceCurrentRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_current_range(store, thread_id, |root| {
            self.draft_piece_marker_edge_proof(store, root, request, retained_byte_ceiling)
        })
    }

    pub fn candidate_draft_piece_marker_edge_proof(
        &self,
        store: &HomeStore,
        binding: DraftEditorCandidateActivationBindingV1,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    ) -> Result<
        DraftPieceCandidateRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>,
        DraftPieceRangeSourceErrorV1,
    > {
        self.stabilized_candidate_range(store, binding, |root| {
            self.draft_piece_marker_edge_proof(store, root, request, retained_byte_ceiling)
        })
    }
    pub fn reconcile_draft_piece_command_outcome(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftPieceEditV1,
        outcome: CommandOutcome,
        mut fragment_page: impl FnMut(u64) -> Vec<DraftPieceBuildFragmentV1>,
    ) -> Result<DraftPieceReconciledCommandV1, DraftPieceCommandReconciliationErrorV1> {
        if let CommandOutcome::Indeterminate { reconciliation, .. } = outcome {
            let handle = reconciliation.install_and_handle();
            store
                .reconcile(&handle)
                .map_err(DraftPieceCommandReconciliationErrorV1::Reconciliation)?;
        }
        let mut ordinal = 1;
        loop {
            let fragments = fragment_page(ordinal);
            if fragments.len() > DRAFT_PIECE_PAGE_MAX_RECORDS {
                return Err(DraftPieceCommandReconciliationErrorV1::InvalidFragmentPage);
            }
            match self.draft_piece_operation_status_page(store, prepared, ordinal, &fragments)? {
                DraftPieceOperationVerificationV1::More { next_ordinal } => {
                    if next_ordinal <= ordinal {
                        return Err(DraftPieceCommandReconciliationErrorV1::InvalidFragmentPage);
                    }
                    ordinal = next_ordinal;
                }
                DraftPieceOperationVerificationV1::Status(status) => {
                    return Ok(reconciled_status(status));
                }
            }
        }
    }

    fn stabilized_current_draft_piece<T>(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        read: impl FnOnce(DraftPieceRootReferenceV1) -> Result<T, DraftPiecePrepareErrorV1>,
    ) -> Result<Option<T>, DraftPiecePrepareErrorV1> {
        let limit = point_limit();
        let Some(before) = self.current_draft(store, thread_id, limit)? else {
            return Ok(None);
        };
        let value = read(before.draft().piece_root())?;
        #[cfg(feature = "test-faults")]
        crate::test_faults::run_draft_piece_current_read_fault(store, *self);
        let after = self.current_draft(store, thread_id, limit)?;
        if after.as_ref() != Some(&before) {
            return Err(DraftPiecePrepareErrorV1::ConcurrentChange);
        }
        Ok(Some(value))
    }

    pub fn validate_current_draft_piece_position(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        position: DraftCompositePositionV1,
    ) -> Result<Option<()>, DraftPiecePrepareErrorV1> {
        self.stabilized_current_draft_piece(store, thread_id, |root| {
            validate_position(self, store, root, position)
        })
    }

    pub fn validate_current_draft_piece_restoration(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        scroll: DraftCompositePositionV1,
        undo_frontier: Option<[u8; 32]>,
    ) -> Result<Option<DraftPieceRestorationV1>, DraftPiecePrepareErrorV1> {
        self.stabilized_current_draft_piece(store, thread_id, |root| {
            self.validate_draft_piece_restoration(
                store,
                DraftPieceRestorationV1::new(root, caret, selection, scroll, undo_frontier),
            )
        })
    }

    pub fn draft_piece_root(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
    ) -> Result<Option<DraftPieceRootRecordV1>, SyndicReadError> {
        let value = self.point::<DraftPieceRootsFamily>(
            store,
            root.key(),
            crate::SyndicPointReadLimit::new(65_536).expect("draft-piece point limit is nonzero"),
        )?;
        match value {
            Some(value) if value.reference() == root => Ok(Some(value)),
            Some(_) => Err(SyndicReadError::Invariant(
                "draft piece root reference mismatch",
            )),
            None => Ok(None),
        }
    }

    pub fn validate_draft_piece_restoration(
        &self,
        store: &HomeStore,
        restoration: DraftPieceRestorationV1,
    ) -> Result<DraftPieceRestorationV1, DraftPiecePrepareErrorV1> {
        validate_position(self, store, restoration.root(), restoration.caret())?;
        validate_position(self, store, restoration.root(), restoration.selection())?;
        validate_position(self, store, restoration.root(), restoration.scroll())?;
        Ok(restoration)
    }

    pub fn draft_marker_identity(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
        marker_id: beryl_model::SyndicDraftMarkerId,
    ) -> Result<Option<DraftMarkerIdentityOccurrenceV1>, DraftPiecePrepareErrorV1> {
        marker_identity_lookup(self, store, root, marker_id)
    }

    pub fn validate_draft_marker_location(
        &self,
        store: &HomeStore,
        root: DraftPieceRootReferenceV1,
        witness: DraftPieceMarkerAtV1,
    ) -> Result<bool, DraftPiecePrepareErrorV1> {
        validate_marker_location(self, store, root, witness)
    }

    pub fn draft_piece_operation_status_page(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftPieceEditV1,
        start_ordinal: u64,
        fragments: &[DraftPieceBuildFragmentV1],
    ) -> Result<DraftPieceOperationVerificationV1, SyndicReadError> {
        if fragments.len() > DRAFT_PIECE_PAGE_MAX_RECORDS
            || start_ordinal == 0
            || start_ordinal > prepared.header().fragment_count().saturating_add(1)
        {
            return Err(SyndicReadError::Invariant(
                "invalid draft-piece verification page",
            ));
        }
        let limit = point_limit();
        let key = DraftPieceSettlementKeyV1::new(
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        );
        let settlement = self.point::<DraftPieceSettlementsFamily>(store, key, limit)?;
        let build = self.point::<DraftPieceBuildsFamily>(store, key, limit)?;
        if let Some(build) = build.as_ref() {
            let receipt = self
                .point::<DraftPieceBuildProgressFamily>(
                    store,
                    build.progress_receipt().key(),
                    limit,
                )?
                .ok_or(SyndicReadError::Invariant(
                    "draft-piece progress receipt missing",
                ))?;
            if !progress_receipt_matches_build(&receipt, build) {
                return Err(SyndicReadError::Invariant(
                    "draft-piece progress receipt disagrees with build",
                ));
            }
            if let Some(previous) = receipt.previous() {
                let stored = self
                    .point::<DraftPieceBuildProgressFamily>(store, previous.key(), limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "draft-piece predecessor progress receipt missing",
                    ))?;
                if stored.reference() != previous || !progress_receipt_is_exact(&stored) {
                    return Err(SyndicReadError::Invariant(
                        "draft-piece predecessor progress receipt disagrees",
                    ));
                }
                if !progress_receipt_effects_are_exact(self, store, &stored, limit)? {
                    return Err(SyndicReadError::Invariant(
                        "draft-piece predecessor progress effects disagree",
                    ));
                }
            }
            if !progress_receipt_effects_are_exact(self, store, &receipt, limit)? {
                return Err(SyndicReadError::Invariant(
                    "draft-piece progress effects disagree",
                ));
            }
            let next_ordinal = build
                .progress_receipt()
                .key()
                .transition_ordinal()
                .checked_add(1)
                .ok_or(SyndicReadError::Invariant(
                    "draft-piece progress ordinal exhausted",
                ))?;
            if self
                .point::<DraftPieceBuildProgressFamily>(
                    store,
                    DraftPieceBuildProgressReceiptKeyV1::new(
                        build.draft_id(),
                        build.session_id(),
                        build.operation_id(),
                        next_ordinal,
                    ),
                    limit,
                )?
                .is_some()
            {
                return Err(SyndicReadError::Invariant(
                    "draft-piece progress receipt is ahead of its build head",
                ));
            }
            if settlement.is_none() && build.staged_fragment_count() < build.fragment_count() {
                let next_fragment = DraftPieceBuildFragmentKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    build.staged_fragment_count() + 1,
                );
                if self
                    .point::<DraftPieceBuildFragmentsFamily>(store, next_fragment, limit)?
                    .is_some()
                {
                    return Err(SyndicReadError::Invariant(
                        "draft-piece fragment is ahead of durable progress",
                    ));
                }
            }
            if settlement.is_none()
                && let Some(successor) = build.successor()
                && self
                    .point::<DraftPieceRootsFamily>(store, successor.key(), limit)?
                    .is_some()
            {
                return Err(SyndicReadError::Invariant(
                    "draft-piece candidate root is ahead of durable settlement",
                ));
            }
        }
        let occupied_digest = settlement
            .as_ref()
            .map(DraftPieceSettlementV1::proposal_digest)
            .or_else(|| build.as_ref().map(DraftPieceBuildRecordV1::proposal_digest));
        let Some(occupied_digest) = occupied_digest else {
            return Ok(DraftPieceOperationVerificationV1::Status(
                DraftPieceOperationStatusV1::Absent,
            ));
        };
        let occupied_header = settlement
            .as_ref()
            .map(DraftPieceSettlementV1::canonical_header)
            .or_else(|| {
                build
                    .as_ref()
                    .map(DraftPieceBuildRecordV1::canonical_header)
            })
            .ok_or(SyndicReadError::Invariant(
                "draft-piece operation authority disappeared",
            ))?;
        if occupied_header != prepared.canonical_header() {
            let offset = prepared
                .canonical_header()
                .iter()
                .zip(occupied_header)
                .position(|(requested, occupied)| requested != occupied)
                .unwrap_or_else(|| prepared.canonical_header().len().min(occupied_header.len()));
            let proof = OccupiedIdentityNoncommitProofV1::new(
                prepared.proposal_digest(),
                occupied_digest,
                key,
                OccupiedIdentityDifferenceV1::Header {
                    offset: offset as u64,
                    requested: prepared.canonical_header().get(offset).copied(),
                    occupied: occupied_header.get(offset).copied(),
                },
            );
            return Ok(DraftPieceOperationVerificationV1::Status(
                DraftPieceOperationStatusV1::Collision(proof),
            ));
        }
        let authority_fragment_count = settlement
            .as_ref()
            .map(|settlement| {
                settlement
                    .terminal_source()
                    .map_or(0, DraftPieceBuildRecordV1::staged_fragment_count)
            })
            .or_else(|| {
                build
                    .as_ref()
                    .map(DraftPieceBuildRecordV1::staged_fragment_count)
            })
            .ok_or(SyndicReadError::Invariant(
                "draft-piece fragment authority disappeared",
            ))?;
        if start_ordinal > authority_fragment_count.saturating_add(1) {
            return Err(SyndicReadError::Invariant(
                "draft-piece verification starts beyond durable fragments",
            ));
        }
        let retained = authority_fragment_count + 1 - start_ordinal;
        if retained != 0 && fragments.is_empty() {
            return Err(SyndicReadError::Invariant(
                "invalid draft-piece verification page length",
            ));
        }
        for (page_index, requested) in fragments
            .iter()
            .take(usize::try_from(retained).unwrap_or(usize::MAX))
            .enumerate()
        {
            let ordinal = start_ordinal + page_index as u64;
            let expected_key = DraftPieceBuildFragmentKeyV1::new(
                prepared.header().draft_id(),
                prepared.header().session_id(),
                prepared.header().operation_id(),
                ordinal,
            );
            let authority_build = build.as_ref().ok_or(SyndicReadError::Invariant(
                "draft-piece fragment build authority disappeared",
            ))?;
            let occupied = Some(
                load_authenticated_build_fragment(self, store, authority_build, ordinal).map_err(
                    |_| SyndicReadError::Invariant("draft-piece fragment authentication failed"),
                )?,
            );
            if requested.key() != expected_key || occupied.as_ref() != Some(requested) {
                let proof = OccupiedIdentityNoncommitProofV1::new(
                    prepared.proposal_digest(),
                    occupied_digest,
                    key,
                    OccupiedIdentityDifferenceV1::Fragment {
                        key: expected_key,
                        requested: Some(requested.clone()),
                        occupied,
                    },
                );
                return Ok(DraftPieceOperationVerificationV1::Status(
                    DraftPieceOperationStatusV1::Collision(proof),
                ));
            }
        }
        let verified = retained.min(fragments.len() as u64);
        let next_ordinal = start_ordinal + verified;
        if next_ordinal <= authority_fragment_count {
            if fragments.is_empty() {
                return Err(SyndicReadError::Invariant(
                    "empty draft-piece verification page made no progress",
                ));
            }
            return Ok(DraftPieceOperationVerificationV1::More { next_ordinal });
        }
        if settlement.is_some() && next_ordinal != authority_fragment_count.saturating_add(1) {
            return Err(SyndicReadError::Invariant(
                "draft-piece verification page exceeded fragment count",
            ));
        }
        let status = if let Some(settlement) = settlement {
            if settlement.proposal_digest() != prepared.proposal_digest()
                || settlement.predecessor_candidate_generation()
                    != prepared.header().predecessor_candidate_generation()
                || settlement.predecessor_root() != prepared.header().predecessor_root()
                || settlement.fragment_count() != prepared.header().fragment_count()
                || settlement.fragment_chain() != prepared.header().fragment_chain()
                || settlement.caret() != prepared.header().caret()
                || settlement.selection() != prepared.header().selection()
                || settlement.canonical_header() != prepared.canonical_header()
                || !settlement_closure_is_exact(&settlement)
                || !settlement_terminal_build_is_exact(&settlement, build.as_ref())
            {
                return Err(SyndicReadError::Invariant(
                    "settlement disagrees with its exact build header",
                ));
            }
            let target_session = match settlement.closure() {
                DraftPieceSettlementClosureV1::Committed(adoption) => adoption.adopted_session(),
                DraftPieceSettlementClosureV1::Noncommit(noncommit) => noncommit.observed_session(),
            };
            if !operation_terminal_session_is_authenticated(self, store, target_session)? {
                return Err(SyndicReadError::Invariant(
                    "draft-piece terminal session effect disagrees",
                ));
            }
            match settlement.closure() {
                DraftPieceSettlementClosureV1::Committed(adoption) => {
                    let published = self.point::<DraftPieceRootsFamily>(
                        store,
                        adoption.adopted_root().reference().key(),
                        limit,
                    )?;
                    if published.as_ref() != Some(adoption.adopted_root()) {
                        return Err(SyndicReadError::Invariant(
                            "draft-piece committed publication closure is missing",
                        ));
                    }
                }
                DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
                    if let Some(proof) = noncommit.occupied_identity() {
                        let OccupiedIdentityDifferenceV1::Root { key, occupied, .. } =
                            proof.difference()
                        else {
                            return Err(SyndicReadError::Invariant(
                                "draft-piece settlement has a non-root occupied proof",
                            ));
                        };
                        if self
                            .point::<DraftPieceRootsFamily>(store, *key, limit)?
                            .as_ref()
                            != Some(occupied)
                        {
                            return Err(SyndicReadError::Invariant(
                                "draft-piece occupied root proof is missing",
                            ));
                        }
                    } else if let Some(successor) = noncommit.proposed_successor()
                        && self
                            .point::<DraftPieceRootsFamily>(store, successor.key(), limit)?
                            .is_some()
                    {
                        return Err(SyndicReadError::Invariant(
                            "draft-piece noncommit closure has a published successor",
                        ));
                    }
                }
            }
            DraftPieceOperationStatusV1::Settled(settlement)
        } else {
            let build = build.ok_or(SyndicReadError::Invariant(
                "draft-piece build authority disappeared",
            ))?;
            if build.proposal_digest() != prepared.proposal_digest()
                || (build.lifecycle() == DraftPieceBuildLifecycleV1::Complete
                    && build.staged_fragment_count() != prepared.header().fragment_count())
            {
                return Err(SyndicReadError::Invariant(
                    "draft-piece build disagrees with its exact header",
                ));
            }
            let head = operation_session_head(self, store, &build, limit)?;
            let expected_custody = DraftEditorActiveOperationV1::new(
                build.operation_id(),
                build.proposal_digest(),
                build.predecessor_candidate_generation(),
                build.predecessor_root(),
                build.progress_receipt(),
            );
            if head.active_operation() != Some(&expected_custody) {
                return Err(SyndicReadError::Invariant(
                    "draft-piece active-operation custody disagrees",
                ));
            }
            match build.lifecycle() {
                DraftPieceBuildLifecycleV1::Open => DraftPieceOperationStatusV1::Open(build),
                DraftPieceBuildLifecycleV1::Complete => {
                    DraftPieceOperationStatusV1::Complete(build)
                }
                _ => {
                    return Err(SyndicReadError::Invariant(
                        "terminal draft-piece build is missing its settlement",
                    ));
                }
            }
        };
        Ok(DraftPieceOperationVerificationV1::Status(status))
    }
}

fn operation_session_head(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    limit: crate::SyndicPointReadLimit,
) -> Result<DraftEditorCandidateSessionV1, SyndicReadError> {
    let record = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id()),
            limit,
        )?
        .ok_or(SyndicReadError::Invariant(
            "draft-piece candidate-session head missing",
        ))?;
    let DraftEditorCandidateSessionRecordV1::Head(head) = record else {
        return Err(SyndicReadError::Invariant(
            "draft-piece candidate-session head has wrong record kind",
        ));
    };
    Ok(head)
}

fn operation_terminal_session_is_authenticated(
    storage: &SyndicStorage,
    store: &HomeStore,
    expected: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    if expected.active_operation().is_some() {
        return Ok(false);
    }
    let outcome = storage.draft_editor_candidate_session(
        store,
        expected.draft_id(),
        expected.session_id(),
    )?;
    let current = match outcome {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head)
        | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
        DraftEditorCandidateSessionReadOutcomeV1::Absent
        | DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange
        | DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => return Ok(false),
    };
    Ok(current.thread_id() == expected.thread_id()
        && current.durable_base_selector_revision() == expected.durable_base_selector_revision()
        && current.durable_base_root() == expected.durable_base_root()
        && current.session_generation() >= expected.session_generation()
        && current.newest_candidate_generation() >= expected.newest_candidate_generation()
        && current.published_candidate_generation() >= expected.published_candidate_generation()
        && current.dirty_generation() >= expected.dirty_generation())
}

fn progress_receipt_effects_are_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftPieceBuildProgressReceiptV1,
    limit: crate::SyndicPointReadLimit,
) -> Result<bool, SyndicReadError> {
    if let Some(endpoint) = receipt.fragment_endpoint() {
        let fragment =
            storage.point::<DraftPieceBuildFragmentsFamily>(store, endpoint.key(), limit)?;
        if fragment
            .as_ref()
            .is_none_or(|fragment| canonical_fragment_endpoint(fragment) != endpoint)
        {
            return Ok(false);
        }
    }
    let roots = receipt.working_roots();
    if let Some(id) = roots.sequence_root() {
        let node = storage.point::<DraftPieceNodesFamily>(
            store,
            DraftPieceRecordKeyV1::new(receipt.key().draft_id(), id),
            limit,
        )?;
        if node
            .is_none_or(|node| validate_sequence_root_node(node, roots.sequence_summary()).is_err())
        {
            return Ok(false);
        }
    } else if roots.sequence_summary().piece_count() != 0 {
        return Ok(false);
    }
    if let Some(id) = roots.marker_index_root() {
        let record = storage.point::<DraftMarkerIdentityIndexFamily>(
            store,
            DraftMarkerIdentityRecordKeyV1::new(
                receipt.key().draft_id(),
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            ),
            limit,
        )?;
        if record.is_none_or(|record| {
            validate_index_root_record(record, roots.marker_index_summary()).is_err()
        }) {
            return Ok(false);
        }
    } else if roots.marker_index_summary().record_count() != 0 {
        return Ok(false);
    }
    Ok(true)
}

fn reconciled_status(status: DraftPieceOperationStatusV1) -> DraftPieceReconciledCommandV1 {
    let proof = match status {
        DraftPieceOperationStatusV1::Settled(settlement) => {
            DraftPieceSettlementProofV1::Settlement(settlement)
        }
        DraftPieceOperationStatusV1::Collision(proof) => {
            return DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Error(
                DraftPieceSettlementProofV1::OccupiedIdentityNoncommit(proof),
            ));
        }
        pending => return DraftPieceReconciledCommandV1::Pending(pending),
    };
    let DraftPieceSettlementProofV1::Settlement(settlement) = &proof else {
        unreachable!()
    };
    let outcome = match settlement.outcome() {
        DraftPieceSettlementOutcomeV1::Committed { .. } => {
            DraftPieceTransactionOutcomeV1::Committed(proof)
        }
        DraftPieceSettlementOutcomeV1::Rejected(_) => {
            DraftPieceTransactionOutcomeV1::Rejected(proof)
        }
        DraftPieceSettlementOutcomeV1::Conflict { .. } => {
            DraftPieceTransactionOutcomeV1::Conflict(proof)
        }
        DraftPieceSettlementOutcomeV1::Cancelled => {
            DraftPieceTransactionOutcomeV1::Cancelled(proof)
        }
        DraftPieceSettlementOutcomeV1::Error(_) => DraftPieceTransactionOutcomeV1::Error(proof),
    };
    DraftPieceReconciledCommandV1::Terminal(outcome)
}
