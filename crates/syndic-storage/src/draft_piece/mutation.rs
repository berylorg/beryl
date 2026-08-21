use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{DomainRevision, SyndicDraftId};

use crate::domain::SyndicDomain;
use crate::mutation::{point, required};
use crate::{SyndicMutationError, SyndicStorage};

use super::*;

#[derive(Clone)]
pub struct PreparedDraftPieceEditV1 {
    header: DraftPieceEditHeaderV1,
    source_session: DraftEditorCandidateSessionV1,
    canonical_header: Vec<u8>,
    proposal_digest: DraftPieceDigestV1,
    prebuild_rejection: Option<DraftPieceRejectedReasonV1>,
    predecessor_positions_authenticated: bool,
}

impl PreparedDraftPieceEditV1 {
    pub const fn header(&self) -> DraftPieceEditHeaderV1 {
        self.header
    }

    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }

    fn source_session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.source_session
    }

    pub const fn proposal_digest(&self) -> DraftPieceDigestV1 {
        self.proposal_digest
    }

    pub const fn prebuild_rejection(&self) -> Option<DraftPieceRejectedReasonV1> {
        self.prebuild_rejection
    }
}

#[derive(Clone)]
pub struct PreparedDraftPieceAdvanceV1 {
    expected: DraftPieceBuildRecordV1,
    expected_session: DraftEditorCandidateSessionV1,
    next: DraftPieceBuildRecordV1,
    next_receipt: DraftPieceBuildProgressReceiptV1,
    next_session: DraftEditorCandidateSessionV1,
    leaves: Vec<DraftPieceLeafRecordV1>,
    nodes: Vec<DraftPieceNodeRecordV1>,
    index_records: Vec<DraftMarkerIdentityRecordV1>,
    records_read: u64,
}

impl PreparedDraftPieceAdvanceV1 {
    pub const fn records_read(&self) -> u64 {
        self.records_read
    }

    pub fn staged_record_count(&self) -> usize {
        self.leaves.len() + self.nodes.len() + self.index_records.len() + 1
    }

    pub const fn frontier(&self) -> DraftPieceBuildFrontierV1 {
        self.next.frontier()
    }
}

#[derive(Clone)]
struct BeginMutation {
    prepared: PreparedDraftPieceEditV1,
}

#[derive(Clone)]
struct StageFragmentMutation {
    prepared: PreparedDraftPieceEditV1,
    fragment: DraftPieceBuildFragmentV1,
}

#[derive(Clone)]
struct AdvanceMutation {
    prepared: PreparedDraftPieceAdvanceV1,
}

#[derive(Clone)]
struct SettleMutation {
    prepared: PreparedDraftPieceEditV1,
}

#[derive(Clone, Copy)]
enum TerminalKind {
    Cancelled,
    Rejected(DraftPieceRejectedReasonV1),
    Error(DraftPieceErrorReasonV1),
}

#[derive(Clone)]
struct TerminalMutation {
    prepared: PreparedDraftPieceEditV1,
    kind: TerminalKind,
}

impl SyndicStorage {
    pub fn prepare_draft_piece_edit(
        &self,
        store: &HomeStore,
        header: DraftPieceEditHeaderV1,
        source_session: &DraftEditorCandidateSessionV1,
    ) -> Result<PreparedDraftPieceEditV1, DraftPiecePrepareErrorV1> {
        if header.predecessor_root().key().draft_id() != header.draft_id()
            || source_session.draft_id() != header.draft_id()
            || source_session.session_id() != header.session_id()
            || source_session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || source_session.active_operation().is_some()
            || source_session.newest_candidate_generation()
                != header.predecessor_candidate_generation()
            || source_session.newest_root() != header.predecessor_root()
            || source_session.newest_history() != header.predecessor_history()
            || header.predecessor_history().root() != header.predecessor_root()
            || header.predecessor_history().candidate_generation()
                != header.predecessor_candidate_generation()
            || !source_session.is_coherent()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        validate_position(
            self,
            store,
            header.predecessor_root(),
            header.predecessor_caret(),
        )?;
        validate_position(
            self,
            store,
            header.predecessor_root(),
            header.predecessor_selection(),
        )?;
        let canonical_header =
            canonical_edit_command_bytes(header, source_session.session_generation());
        let proposal_digest = canonical_proposal_digest(&canonical_header);
        let prebuild_rejection =
            (header.fragment_count() == 0).then_some(DraftPieceRejectedReasonV1::EmptyTransaction);
        Ok(PreparedDraftPieceEditV1 {
            header,
            source_session: source_session.clone(),
            canonical_header,
            proposal_digest,
            prebuild_rejection,
            predecessor_positions_authenticated: true,
        })
    }

    pub fn prepare_draft_piece_fragment(
        &self,
        prepared: &PreparedDraftPieceEditV1,
        ordinal: u64,
        preceding_chain: DraftPieceDigestV1,
        replacement: DraftPieceReplacementV1,
    ) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
        if ordinal == 0 || ordinal > prepared.header.fragment_count() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        validate_fragment(&replacement).map_err(DraftPiecePrepareErrorV1::Rejected)?;
        if ordinal == 1 && replacement.is_continuation() {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::OutOfOrder,
            ));
        }
        let chain_digest =
            draft_piece_fragment_chain_link_v1(preceding_chain, ordinal, &replacement);
        Ok(DraftPieceBuildFragmentV1::new(
            DraftPieceBuildFragmentKeyV1::new(
                prepared.header.draft_id(),
                prepared.header.session_id(),
                prepared.header.operation_id(),
                ordinal,
            ),
            replacement,
            preceding_chain,
            chain_digest,
        ))
    }

    pub fn begin_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, BeginMutation { prepared })
    }

    pub fn stage_draft_piece_fragment(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
        fragment: DraftPieceBuildFragmentV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StageFragmentMutation { prepared, fragment },
        )
    }

    pub fn prepare_draft_piece_build_advance(
        &self,
        store: &HomeStore,
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Result<Option<PreparedDraftPieceAdvanceV1>, DraftPiecePrepareErrorV1> {
        let key = DraftPieceSettlementKeyV1::new(draft_id, session_id, operation_id);
        let (build, expected_session) = authenticated_build_from_store(self, store, key)?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if build.lifecycle() != DraftPieceBuildLifecycleV1::Open {
            return Ok(None);
        }
        let fragment = match build.frontier() {
            DraftPieceBuildFrontierV1::ReconcilingMoves {
                fragment_ordinal, ..
            }
            | DraftPieceBuildFrontierV1::Planning { fragment_ordinal }
            | DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal, ..
            }
            | DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal, ..
            }
            | DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal, ..
            } => Some(load_authenticated_build_fragment(
                self,
                store,
                &build,
                fragment_ordinal,
            )?),
            DraftPieceBuildFrontierV1::Receiving { .. } | DraftPieceBuildFrontierV1::Complete => {
                return Ok(None);
            }
            _ => None,
        };
        let quantum = advance_persistent_tree_build(self, store, &build, fragment.as_ref())?;
        let fragment_endpoint = if build.staged_fragment_count() == 0 {
            None
        } else {
            Some(canonical_fragment_endpoint(
                &load_authenticated_build_fragment(
                    self,
                    store,
                    &build,
                    build.staged_fragment_count(),
                )?,
            ))
        };
        let staged_record_count =
            quantum.leaves.len() + quantum.nodes.len() + quantum.index_records.len();
        if staged_record_count > DRAFT_PIECE_STAGE_MAX_RECORDS {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
        let (next, next_receipt) = next_build_record(
            &build,
            quantum.roots,
            quantum.base_frontier,
            quantum.successor_frontier,
            quantum.next_record_ordinal,
            quantum.frontier,
            quantum.successor.map(|root| root.reference()),
            quantum.build_digest,
            if quantum.frontier == DraftPieceBuildFrontierV1::Complete {
                DraftPieceBuildLifecycleV1::Complete
            } else {
                DraftPieceBuildLifecycleV1::Open
            },
            fragment_endpoint,
        )
        .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        let next_session = expected_session
            .advance_active_operation(&custody_for(&build), custody_for(&next))
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        Ok(Some(PreparedDraftPieceAdvanceV1 {
            expected: build,
            expected_session,
            next,
            next_receipt,
            next_session,
            leaves: quantum.leaves,
            nodes: quantum.nodes,
            index_records: quantum.index_records,
            records_read: quantum.records_read,
        }))
    }

    pub fn advance_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceAdvanceV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AdvanceMutation { prepared })
    }

    pub fn settle_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, SettleMutation { prepared })
    }

    pub fn cancel_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Cancelled,
            },
        )
    }

    pub fn reject_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
        reason: DraftPieceRejectedReasonV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Rejected(reason),
            },
        )
    }

    pub fn error_draft_piece_edit(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
        reason: DraftPieceErrorReasonV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Error(reason),
            },
        )
    }
}

fn authenticated_build_from_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftPieceSettlementKeyV1,
) -> Result<
    Option<(DraftPieceBuildRecordV1, DraftEditorCandidateSessionV1)>,
    DraftPiecePrepareErrorV1,
> {
    let build = storage.point::<DraftPieceBuildsFamily>(store, key, point_limit())?;
    let Some(build) = build else { return Ok(None) };
    let receipt = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            build.progress_receipt().key(),
            point_limit(),
        )?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if !progress_receipt_matches_build(&receipt, &build) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    authenticate_progress_receipt_from_store(storage, store, &receipt)?;
    let session =
        storage.draft_editor_candidate_session(store, build.draft_id(), build.session_id())?;
    let expected_custody = custody_for(&build);
    let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = session else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if head.active_operation() != Some(&expected_custody)
        || !active_session_generation_matches_build(&head, &build)
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(Some((build, head)))
}

pub(super) fn authenticated_staging_build_from_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftPieceSettlementKeyV1,
) -> Result<
    Option<(DraftPieceBuildRecordV1, DraftEditorCandidateSessionV1)>,
    DraftPiecePrepareErrorV1,
> {
    authenticated_build_from_store(storage, store, key)
}

fn authenticate_progress_receipt_from_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    if !progress_receipt_is_exact(receipt) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if let Some(previous) = receipt.previous() {
        let stored = storage
            .point::<DraftPieceBuildProgressFamily>(store, previous.key(), point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if stored.reference() != previous || !progress_receipt_is_exact(&stored) {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        authenticate_progress_receipt_effects_from_store(storage, store, &stored)?;
    }
    authenticate_progress_receipt_effects_from_store(storage, store, receipt)
}

fn authenticate_progress_receipt_effects_from_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    if let Some(endpoint) = receipt.fragment_endpoint() {
        let fragment = storage
            .point::<DraftPieceBuildFragmentsFamily>(store, endpoint.key(), point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if canonical_fragment_endpoint(&fragment) != endpoint {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    let roots = receipt.working_roots();
    if let Some(id) = roots.sequence_root() {
        let node = storage
            .point::<DraftPieceNodesFamily>(
                store,
                DraftPieceRecordKeyV1::new(receipt.key().draft_id(), id),
                point_limit(),
            )?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        validate_sequence_root_node(node, roots.sequence_summary())?;
    } else if roots.sequence_summary().piece_count() != 0 {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if let Some(id) = roots.marker_index_root() {
        let record = storage
            .point::<DraftMarkerIdentityIndexFamily>(
                store,
                DraftMarkerIdentityRecordKeyV1::new(
                    receipt.key().draft_id(),
                    DraftMarkerIdentityRecordKindV1::Internal,
                    id,
                ),
                point_limit(),
            )?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        validate_index_root_record(record, roots.marker_index_summary())?;
    } else if roots.marker_index_summary().record_count() != 0 {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

fn build_record(
    prepared: &PreparedDraftPieceEditV1,
) -> Result<
    (
        DraftPieceBuildRecordV1,
        DraftPieceBuildProgressReceiptV1,
        DraftEditorCandidateSessionV1,
    ),
    SyndicMutationError,
> {
    let header = prepared.header;
    let origin = DraftPieceBuildBoundaryV1::new(0, 0);
    let (build, receipt) = authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            header.draft_id(),
            header.session_id(),
            header.predecessor_candidate_generation(),
            header.predecessor_root(),
            header.predecessor_history(),
            header.operation_id(),
            header.predecessor_caret(),
            header.predecessor_selection(),
            header.caret(),
            header.selection(),
            header.fragment_count(),
            header.fragment_chain(),
            prepared.canonical_header.clone(),
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
            prepared.proposal_digest,
            DraftPieceBuildRootsV1::from_root(header.predecessor_root()),
            origin,
            origin,
            1,
            DraftPieceBuildFrontierV1::Receiving {
                next_ordinal: 1,
                chain: canonical_empty_draft_piece_fragment_chain_v1(),
            },
            DraftPieceDigestV1::from_bytes([0; 32]),
            DraftPieceBuildProgressReceiptReferenceV1::new(
                DraftPieceBuildProgressReceiptKeyV1::new(
                    header.draft_id(),
                    header.session_id(),
                    header.operation_id(),
                    1,
                ),
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            None,
            None,
            DraftPieceBuildLifecycleV1::Open,
        ),
        None,
        None,
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)?;
    let session = expected_active_session(prepared, &build)?;
    Ok((build, receipt, session))
}

pub(super) fn initial_build_for_staging(
    header: DraftPieceEditHeaderV1,
    source_session: &DraftEditorCandidateSessionV1,
    expected_staging: DraftEditorActiveOperationV1,
) -> Result<
    (
        PreparedDraftPieceEditV1,
        DraftPieceBuildRecordV1,
        DraftPieceBuildProgressReceiptV1,
        DraftEditorCandidateSessionV1,
    ),
    SyndicMutationError,
> {
    if !expected_staging.is_staging()
        || source_session.active_operation() != Some(&expected_staging)
        || header.draft_id() != source_session.draft_id()
        || header.session_id() != source_session.session_id()
        || header.predecessor_candidate_generation() != source_session.newest_candidate_generation()
        || header.predecessor_root() != source_session.newest_root()
        || header.predecessor_history() != source_session.newest_history()
        || expected_staging.predecessor_history() != header.predecessor_history()
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let canonical_header =
        canonical_edit_command_bytes(header, source_session.session_generation());
    let proposal_digest = canonical_proposal_digest(&canonical_header);
    let prepared = PreparedDraftPieceEditV1 {
        header,
        source_session: source_session.clone(),
        canonical_header,
        proposal_digest,
        prebuild_rejection: None,
        predecessor_positions_authenticated: true,
    };
    let origin = DraftPieceBuildBoundaryV1::new(0, 0);
    let (build, receipt) = authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            header.draft_id(),
            header.session_id(),
            header.predecessor_candidate_generation(),
            header.predecessor_root(),
            header.predecessor_history(),
            header.operation_id(),
            header.predecessor_caret(),
            header.predecessor_selection(),
            header.caret(),
            header.selection(),
            header.fragment_count(),
            header.fragment_chain(),
            prepared.canonical_header.clone(),
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
            proposal_digest,
            DraftPieceBuildRootsV1::from_root(header.predecessor_root()),
            origin,
            origin,
            1,
            DraftPieceBuildFrontierV1::Receiving {
                next_ordinal: 1,
                chain: canonical_empty_draft_piece_fragment_chain_v1(),
            },
            DraftPieceDigestV1::from_bytes([0; 32]),
            DraftPieceBuildProgressReceiptReferenceV1::new(
                DraftPieceBuildProgressReceiptKeyV1::new(
                    header.draft_id(),
                    header.session_id(),
                    header.operation_id(),
                    1,
                ),
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            None,
            None,
            DraftPieceBuildLifecycleV1::Open,
        ),
        None,
        None,
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)?;
    let target_session = source_session
        .advance_active_operation(&expected_staging, custody_for(&build))
        .ok_or(SyndicMutationError::CurrentDraftConflict)?;
    Ok((prepared, build, receipt, target_session))
}

fn terminal_first_build(
    prepared: &PreparedDraftPieceEditV1,
    lifecycle: DraftPieceBuildLifecycleV1,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), SyndicMutationError> {
    let header = prepared.header;
    let origin = DraftPieceBuildBoundaryV1::new(0, 0);
    authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            header.draft_id(),
            header.session_id(),
            header.predecessor_candidate_generation(),
            header.predecessor_root(),
            header.predecessor_history(),
            header.operation_id(),
            header.predecessor_caret(),
            header.predecessor_selection(),
            header.caret(),
            header.selection(),
            header.fragment_count(),
            header.fragment_chain(),
            prepared.canonical_header.clone(),
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
            prepared.proposal_digest,
            DraftPieceBuildRootsV1::from_root(header.predecessor_root()),
            origin,
            origin,
            1,
            DraftPieceBuildFrontierV1::Receiving {
                next_ordinal: 1,
                chain: canonical_empty_draft_piece_fragment_chain_v1(),
            },
            DraftPieceDigestV1::from_bytes([0; 32]),
            DraftPieceBuildProgressReceiptReferenceV1::new(
                DraftPieceBuildProgressReceiptKeyV1::new(
                    header.draft_id(),
                    header.session_id(),
                    header.operation_id(),
                    1,
                ),
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            None,
            None,
            lifecycle,
        ),
        None,
        None,
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)
}

fn build_from_progress_receipt(
    template: &DraftPieceBuildRecordV1,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<DraftPieceBuildRecordV1, SyndicMutationError> {
    let (staged_fragment_count, staged_fragment_chain) = receipt.fragment_endpoint().map_or(
        (0, canonical_empty_draft_piece_fragment_chain_v1()),
        |endpoint| (endpoint.key().ordinal(), endpoint.chain()),
    );
    let build = DraftPieceBuildRecordV1::new(
        template.draft_id(),
        template.session_id(),
        template.predecessor_candidate_generation(),
        template.predecessor_root(),
        template.predecessor_history(),
        template.operation_id(),
        template.predecessor_caret(),
        template.predecessor_selection(),
        template.caret(),
        template.selection(),
        template.fragment_count(),
        template.fragment_chain(),
        template.canonical_header().to_vec(),
        staged_fragment_count,
        staged_fragment_chain,
        template.proposal_digest(),
        receipt.working_roots(),
        receipt.base_frontier(),
        receipt.successor_frontier(),
        receipt.next_record_ordinal(),
        receipt.frontier(),
        receipt.state_digest(),
        receipt.reference(),
        receipt.successor(),
        receipt.build_digest(),
        receipt.lifecycle(),
    );
    if progress_receipt_matches_build(receipt, &build) && build_record_is_exact(&build) {
        Ok(build)
    } else {
        Err(SyndicMutationError::IdentityCollision)
    }
}

fn stage_transition(
    prepared: &PreparedDraftPieceEditV1,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<
    (
        DraftPieceBuildRecordV1,
        DraftPieceBuildProgressReceiptV1,
        DraftEditorCandidateSessionV1,
    ),
    SyndicMutationError,
> {
    let staged = build
        .staged_fragment_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let chain = fragment.chain_digest();
    let frontier = if staged == build.fragment_count() {
        if chain != build.fragment_chain() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal: 1,
            next_move: 0,
        }
    } else {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal: staged
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
            chain,
        }
    };
    let (next, receipt) = authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            build.draft_id(),
            build.session_id(),
            build.predecessor_candidate_generation(),
            build.predecessor_root(),
            build.predecessor_history(),
            build.operation_id(),
            build.predecessor_caret(),
            build.predecessor_selection(),
            build.caret(),
            build.selection(),
            build.fragment_count(),
            build.fragment_chain(),
            build.canonical_header().to_vec(),
            staged,
            chain,
            build.proposal_digest(),
            build.working_roots(),
            build.base_frontier(),
            build.successor_frontier(),
            build.next_record_ordinal(),
            frontier,
            DraftPieceDigestV1::from_bytes([0; 32]),
            build.progress_receipt(),
            None,
            None,
            DraftPieceBuildLifecycleV1::Open,
        ),
        Some(build.progress_receipt()),
        Some(canonical_fragment_endpoint(fragment)),
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)?;
    let session = expected_active_session(prepared, &next)?;
    Ok((next, receipt, session))
}

pub(super) fn staged_page_transition(
    prepared: &PreparedDraftPieceEditV1,
    build: &DraftPieceBuildRecordV1,
    source_session: &DraftEditorCandidateSessionV1,
    replacements: &[DraftPieceReplacementV1],
) -> Result<
    (
        DraftPieceBuildRecordV1,
        DraftPieceBuildProgressReceiptV1,
        DraftEditorCandidateSessionV1,
        Box<[DraftPieceBuildFragmentV1]>,
    ),
    SyndicMutationError,
> {
    if prepared.header().draft_id() != build.draft_id()
        || prepared.header().session_id() != build.session_id()
        || prepared.header().operation_id() != build.operation_id()
        || prepared.header().predecessor_candidate_generation()
            != build.predecessor_candidate_generation()
        || prepared.header().predecessor_root() != build.predecessor_root()
        || prepared.header().fragment_count() != build.fragment_count()
        || prepared.header().fragment_chain() != build.fragment_chain()
        || replacements.len() > DRAFT_PIECE_STAGE_MAX_RECORDS
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let mut ordinal = build
        .staged_fragment_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let mut chain = build.staged_fragment_chain();
    let fragments = replacements
        .iter()
        .map(|replacement| {
            validate_fragment(replacement).map_err(|_| SyndicMutationError::IdentityCollision)?;
            if ordinal == 1 && replacement.is_continuation() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let next_chain = draft_piece_fragment_chain_link_v1(chain, ordinal, replacement);
            let fragment = DraftPieceBuildFragmentV1::new(
                DraftPieceBuildFragmentKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    ordinal,
                ),
                replacement.clone(),
                chain,
                next_chain,
            );
            chain = next_chain;
            ordinal = ordinal
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?;
            Ok(fragment)
        })
        .collect::<Result<Box<[_]>, _>>()?;
    let staged = build
        .staged_fragment_count()
        .checked_add(
            u64::try_from(fragments.len()).map_err(|_| SyndicMutationError::IdentityCollision)?,
        )
        .ok_or(SyndicMutationError::IdentityCollision)?;
    if staged > build.fragment_count() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let frontier = if staged == build.fragment_count() {
        if chain != build.fragment_chain() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal: 1,
            next_move: 0,
        }
    } else {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal: staged
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
            chain,
        }
    };
    let endpoint = fragments.last().map(canonical_fragment_endpoint);
    let (next, receipt) = authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            build.draft_id(),
            build.session_id(),
            build.predecessor_candidate_generation(),
            build.predecessor_root(),
            build.predecessor_history(),
            build.operation_id(),
            build.predecessor_caret(),
            build.predecessor_selection(),
            build.caret(),
            build.selection(),
            build.fragment_count(),
            build.fragment_chain(),
            build.canonical_header().to_vec(),
            staged,
            chain,
            build.proposal_digest(),
            build.working_roots(),
            build.base_frontier(),
            build.successor_frontier(),
            build.next_record_ordinal(),
            frontier,
            DraftPieceDigestV1::from_bytes([0; 32]),
            build.progress_receipt(),
            None,
            None,
            DraftPieceBuildLifecycleV1::Open,
        ),
        Some(build.progress_receipt()),
        endpoint,
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)?;
    let session = source_session
        .advance_active_operation(&custody_for(build), custody_for(&next))
        .ok_or(SyndicMutationError::CurrentDraftConflict)?;
    Ok((next, receipt, session, fragments))
}

pub(super) fn prepared_edit_from_staging_build(
    build: &DraftPieceBuildRecordV1,
    session: &DraftEditorCandidateSessionV1,
) -> Result<PreparedDraftPieceEditV1, SyndicMutationError> {
    if session.active_operation() != Some(&custody_for(build))
        || !active_session_generation_matches_build(session, build)
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let header = DraftPieceEditHeaderV1::new(
        build.draft_id(),
        build.session_id(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.operation_id(),
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        build.fragment_count(),
        build.fragment_chain(),
    );
    if canonical_edit_command_bytes(
        header,
        canonical_edit_command_source_generation(build.canonical_header())
            .ok_or(SyndicMutationError::IdentityCollision)?,
    ) != build.canonical_header()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(PreparedDraftPieceEditV1 {
        header,
        source_session: session.clone(),
        canonical_header: build.canonical_header().to_vec(),
        proposal_digest: build.proposal_digest(),
        prebuild_rejection: None,
        predecessor_positions_authenticated: true,
    })
}

fn next_build_record(
    build: &DraftPieceBuildRecordV1,
    roots: DraftPieceBuildRootsV1,
    base_frontier: DraftPieceBuildBoundaryV1,
    successor_frontier: DraftPieceBuildBoundaryV1,
    next_record_ordinal: u64,
    frontier: DraftPieceBuildFrontierV1,
    successor: Option<DraftPieceRootReferenceV1>,
    build_digest: Option<DraftPieceDigestV1>,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), SyndicMutationError> {
    authenticated_build_transition(
        DraftPieceBuildRecordV1::new(
            build.draft_id(),
            build.session_id(),
            build.predecessor_candidate_generation(),
            build.predecessor_root(),
            build.predecessor_history(),
            build.operation_id(),
            build.predecessor_caret(),
            build.predecessor_selection(),
            build.caret(),
            build.selection(),
            build.fragment_count(),
            build.fragment_chain(),
            build.canonical_header().to_vec(),
            build.staged_fragment_count(),
            build.staged_fragment_chain(),
            build.proposal_digest(),
            roots,
            base_frontier,
            successor_frontier,
            next_record_ordinal,
            frontier,
            DraftPieceDigestV1::from_bytes([0; 32]),
            build.progress_receipt(),
            successor,
            build_digest,
            lifecycle,
        ),
        Some(build.progress_receipt()),
        fragment_endpoint,
    )
    .map_err(|()| SyndicMutationError::IdentityCollision)
}

fn terminal_build(
    build: &DraftPieceBuildRecordV1,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), SyndicMutationError> {
    next_build_record(
        build,
        build.working_roots(),
        build.base_frontier(),
        build.successor_frontier(),
        build.next_record_ordinal(),
        build.frontier(),
        build.successor(),
        build.build_digest(),
        lifecycle,
        fragment_endpoint,
    )
}

fn settlement_key(prepared: &PreparedDraftPieceEditV1) -> DraftPieceSettlementKeyV1 {
    DraftPieceSettlementKeyV1::new(
        prepared.header.draft_id(),
        prepared.header.session_id(),
        prepared.header.operation_id(),
    )
}

fn build_key(build: &DraftPieceBuildRecordV1) -> DraftPieceSettlementKeyV1 {
    DraftPieceSettlementKeyV1::new(build.draft_id(), build.session_id(), build.operation_id())
}

fn progress_key(
    reference: DraftPieceBuildProgressReceiptReferenceV1,
) -> DraftPieceBuildProgressReceiptKeyV1 {
    reference.key()
}

fn authenticate_build(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
) -> Result<(), SyndicMutationError> {
    let receipt =
        required::<DraftPieceBuildProgressFamily>(reader, &progress_key(build.progress_receipt()))?;
    if !progress_receipt_matches_build(&receipt, build) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_progress_receipt(reader, &receipt)?;
    Ok(())
}

fn authenticate_source_transition(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    expected_session: &DraftEditorCandidateSessionV1,
    target_receipt: DraftPieceBuildProgressReceiptKeyV1,
) -> Result<DraftEditorCandidateSessionV1, SyndicMutationError> {
    authenticate_build(reader, build)?;
    let session = session_head(reader, build.draft_id(), build.session_id())?;
    if session != *expected_session
        || session.active_operation() != Some(&custody_for(build))
        || !active_session_generation_matches_build(&session, build)
        || point::<DraftPieceBuildProgressFamily>(reader, &target_receipt)?.is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(session)
}

fn authenticate_target_transition(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    receipt: &DraftPieceBuildProgressReceiptV1,
    expected_target_session: &DraftEditorCandidateSessionV1,
) -> Result<(), SyndicMutationError> {
    if build.progress_receipt() != receipt.reference()
        || required::<DraftPieceBuildProgressFamily>(reader, &receipt.key())? != *receipt
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_build(reader, build)?;
    if expected_target_session.active_operation().is_some()
        && (expected_target_session.active_operation() != Some(&custody_for(build))
            || !active_session_generation_matches_build(expected_target_session, build))
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if session_head(reader, build.draft_id(), build.session_id())? != *expected_target_session {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

fn active_session_generation_matches_build(
    session: &DraftEditorCandidateSessionV1,
    build: &DraftPieceBuildRecordV1,
) -> bool {
    canonical_edit_command_source_generation(build.canonical_header()).and_then(|generation| {
        generation.checked_add(build.progress_receipt().key().transition_ordinal())
    }) == Some(session.session_generation())
}

fn expected_active_session(
    prepared: &PreparedDraftPieceEditV1,
    build: &DraftPieceBuildRecordV1,
) -> Result<DraftEditorCandidateSessionV1, SyndicMutationError> {
    let source = prepared.source_session();
    let transition_ordinal = build.progress_receipt().key().transition_ordinal();
    let target = custody_for(build);
    match source.active_operation() {
        None => source
            .with_active_operation_at_transition(transition_ordinal, target)
            .ok_or(SyndicMutationError::IdentityCollision),
        Some(staging)
            if source.draft_id() == build.draft_id()
                && source.session_id() == build.session_id()
                && staging.same_operation(&target) =>
        {
            source
                .staging_to_building_at_transition(staging, target, transition_ordinal)
                .ok_or(SyndicMutationError::IdentityCollision)
        }
        Some(_) => Err(SyndicMutationError::IdentityCollision),
    }
}

fn authenticate_progress_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), SyndicMutationError> {
    if !progress_receipt_is_exact(receipt) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if let Some(previous) = receipt.previous() {
        let stored = required::<DraftPieceBuildProgressFamily>(reader, &previous.key())?;
        if stored.reference() != previous || !progress_receipt_is_exact(&stored) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_progress_receipt_effects(reader, &stored)?;
    }
    authenticate_progress_receipt_effects(reader, receipt)
}

fn authenticate_progress_receipt_effects(
    reader: &DomainReader<'_, SyndicDomain>,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), SyndicMutationError> {
    if let Some(endpoint) = receipt.fragment_endpoint() {
        let fragment = required::<DraftPieceBuildFragmentsFamily>(reader, &endpoint.key())?;
        if canonical_fragment_endpoint(&fragment) != endpoint {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    let roots = receipt.working_roots();
    if let Some(id) = roots.sequence_root() {
        let node = required::<DraftPieceNodesFamily>(
            reader,
            &DraftPieceRecordKeyV1::new(receipt.key().draft_id(), id),
        )?;
        validate_sequence_root_node(node, roots.sequence_summary())
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
    } else if roots.sequence_summary().piece_count() != 0 {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if let Some(id) = roots.marker_index_root() {
        let record = required::<DraftMarkerIdentityIndexFamily>(
            reader,
            &DraftMarkerIdentityRecordKeyV1::new(
                receipt.key().draft_id(),
                DraftMarkerIdentityRecordKindV1::Internal,
                id,
            ),
        )?;
        validate_index_root_record(record, roots.marker_index_summary())
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
    } else if roots.marker_index_summary().record_count() != 0 {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

fn point_build(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &DraftPieceSettlementKeyV1,
) -> Result<Option<DraftPieceBuildRecordV1>, SyndicMutationError> {
    let build = point::<DraftPieceBuildsFamily>(reader, key)?;
    if let Some(build) = build.as_ref() {
        authenticate_build(reader, build)?;
    }
    Ok(build)
}

fn required_build(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &DraftPieceSettlementKeyV1,
) -> Result<DraftPieceBuildRecordV1, SyndicMutationError> {
    point_build(reader, key)?.ok_or(SyndicMutationError::IdentityCollision)
}

fn put_build_transition(
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    receipt: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), SyndicMutationError> {
    let key = build_key(build);
    mutations.put::<DraftPieceBuildProgressCodec>(&receipt.key(), receipt)?;
    mutations.put::<DraftPieceBuildsCodec>(&key, build)?;
    Ok(())
}

pub(super) fn session_head(
    reader: &DomainReader<'_, SyndicDomain>,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<DraftEditorCandidateSessionV1, SyndicMutationError> {
    let head = match required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id),
    )? {
        DraftEditorCandidateSessionRecordV1::Head(head) => head,
        DraftEditorCandidateSessionRecordV1::OpenReceipt(_) => {
            return Err(SyndicMutationError::IdentityCollision);
        }
    };
    let receipt = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
            draft_id,
            session_id,
            head.open_operation_id(),
        ),
    )?;
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = receipt else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if !super::session::receipt_matches_head(&receipt, &head) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    if let Some(custody) = head.active_operation() {
        if let Some(staging_receipt) = custody.staging_receipt() {
            let identity = staging_receipt.identity();
            let staging_head = required::<DraftMutationStagingHeadsFamily>(reader, &identity)?;
            let receipt = super::staging::authenticate_staging_head_reader(reader, &staging_head)?;
            if staging_head.receipt() != staging_receipt
                || custody.operation_id() != identity.operation_id().as_piece_operation()
                || custody.begin_digest() != Some(staging_head.begin_digest())
                || custody.predecessor_candidate_generation()
                    != staging_head.begin().predecessor_candidate_generation()
                || custody.predecessor_root() != staging_head.begin().predecessor_root()
                || custody.predecessor_history() != staging_head.begin().predecessor_history()
                || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        } else {
            let key = DraftPieceSettlementKeyV1::new(
                head.draft_id(),
                head.session_id(),
                custody.operation_id(),
            );
            let build = required_build(reader, &key)?;
            if Some(build.proposal_digest()) != custody.proposal_digest()
                || build.predecessor_candidate_generation()
                    != custody.predecessor_candidate_generation()
                || build.predecessor_root() != custody.predecessor_root()
                || Some(build.progress_receipt()) != custody.build_receipt()
                || !matches!(
                    build.lifecycle(),
                    DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
                )
                || point::<DraftPieceSettlementsFamily>(reader, &key)?.is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let next_ordinal = build
                .progress_receipt()
                .key()
                .transition_ordinal()
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?;
            if point::<DraftPieceBuildProgressFamily>(
                reader,
                &DraftPieceBuildProgressReceiptKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    next_ordinal,
                ),
            )?
            .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            if build.staged_fragment_count() < build.fragment_count()
                && point::<DraftPieceBuildFragmentsFamily>(
                    reader,
                    &DraftPieceBuildFragmentKeyV1::new(
                        build.draft_id(),
                        build.session_id(),
                        build.operation_id(),
                        build.staged_fragment_count() + 1,
                    ),
                )?
                .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
    }
    if head.newest_candidate_generation() == head.published_candidate_generation()
        && head.newest_candidate_generation() != 0
    {
        let published =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.published_history().key())?;
        let newest =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if published.reference() != head.published_history()
            || newest.reference() != head.newest_history()
            || published.fork_session(head.session_id()).as_ref() != Some(&newest)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    } else if head.newest_candidate_generation() != head.published_candidate_generation() {
        let root = head.newest_root();
        let key = DraftPieceSettlementKeyV1::new(
            head.draft_id(),
            head.session_id(),
            root.key().operation_id(),
        );
        let stored_root = required::<DraftPieceRootsFamily>(reader, &root.key())?;
        let settlement = required::<DraftPieceSettlementsFamily>(reader, &key)?;
        let build = point_build(reader, &key)?;
        let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if stored_root.reference() != root
            || !settlement_closure_is_exact(&settlement)
            || !settlement_terminal_build_is_exact(&settlement, build.as_ref())
            || !super::session::adopted_head_matches_current(adoption.adopted_session(), &head)
            || !matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed {
                    successor,
                    candidate_generation,
                    ..
                } if *successor == root
                    && *candidate_generation == head.newest_candidate_generation()
            )
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    Ok(head)
}

fn custody_for(build: &DraftPieceBuildRecordV1) -> DraftEditorActiveOperationV1 {
    DraftEditorActiveOperationV1::building(
        build.operation_id(),
        build.proposal_digest(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.progress_receipt(),
    )
}

fn next_progress_key(
    build: &DraftPieceBuildRecordV1,
) -> Result<DraftPieceBuildProgressReceiptKeyV1, SyndicMutationError> {
    Ok(DraftPieceBuildProgressReceiptKeyV1::new(
        build.draft_id(),
        build.session_id(),
        build.operation_id(),
        build
            .progress_receipt()
            .key()
            .transition_ordinal()
            .checked_add(1)
            .ok_or(SyndicMutationError::IdentityCollision)?,
    ))
}

fn put_session_head(
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), SyndicMutationError> {
    mutations.put::<DraftEditorCandidateSessionsCodec>(
        &DraftEditorCandidateSessionRecordKeyV1::head(head.draft_id(), head.session_id()),
        &DraftEditorCandidateSessionRecordV1::Head(head.clone()),
    )?;
    Ok(())
}

fn authenticated_history_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    reference: DraftEditHistoryFrontierReferenceV1,
) -> Result<DraftEditHistoryFrontierV1, SyndicMutationError> {
    let frontier = required::<DraftEditHistoryFrontiersFamily>(reader, &reference.key())?;
    if frontier.reference() != reference || !frontier.is_locally_valid() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &frontier)?;
    Ok(frontier)
}

fn build_matches(build: &DraftPieceBuildRecordV1, prepared: &PreparedDraftPieceEditV1) -> bool {
    build.canonical_header() == prepared.canonical_header()
        && build.proposal_digest() == prepared.proposal_digest()
}

fn settlement_matches(
    reader: &DomainReader<'_, SyndicDomain>,
    settlement: &DraftPieceSettlementV1,
    prepared: &PreparedDraftPieceEditV1,
) -> Result<bool, SyndicMutationError> {
    let header = prepared.header;
    let header_matches = prepared.predecessor_positions_authenticated
        && settlement.key() == settlement_key(prepared)
        && settlement.proposal_digest() == prepared.proposal_digest()
        && settlement.predecessor_candidate_generation()
            == header.predecessor_candidate_generation()
        && settlement.predecessor_root() == header.predecessor_root()
        && settlement.predecessor_history() == header.predecessor_history()
        && settlement.fragment_count() == header.fragment_count()
        && settlement.fragment_chain() == header.fragment_chain()
        && settlement.predecessor_caret() == header.predecessor_caret()
        && settlement.predecessor_selection() == header.predecessor_selection()
        && settlement.caret() == header.caret()
        && settlement.selection() == header.selection()
        && settlement.canonical_header() == prepared.canonical_header()
        && settlement_closure_is_exact(settlement);
    if !header_matches {
        return Ok(false);
    }
    let stored = point_build(reader, &settlement_key(prepared))?;
    let current_session = session_head(
        reader,
        prepared.header().draft_id(),
        prepared.header().session_id(),
    )?;
    let target_session = match settlement.closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => adoption.adopted_session(),
        DraftPieceSettlementClosureV1::Noncommit(noncommit) => noncommit.observed_session(),
    };
    let terminal_exact = settlement_terminal_build_is_exact(settlement, stored.as_ref())
        && &current_session == target_session;
    if !terminal_exact {
        return Ok(false);
    }
    let Some(stored) = stored.as_ref() else {
        return Ok(false);
    };
    authenticate_target_transition(
        reader,
        stored,
        &required::<DraftPieceBuildProgressFamily>(reader, &stored.progress_receipt().key())?,
        target_session,
    )?;
    match settlement.closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => {
            authenticate_draft_edit_history_frontier_v1(reader, adoption.adopted_history())?;
            Ok(
                point::<DraftPieceRootsFamily>(reader, &adoption.adopted_root().reference().key())?
                    .as_ref()
                    == Some(adoption.adopted_root())
                    && point::<DraftEditHistoryTransitionsFamily>(
                        reader,
                        &adoption.transition().key(),
                    )?
                    .as_ref()
                        == Some(adoption.transition())
                    && point::<DraftEditHistoryFrontiersFamily>(
                        reader,
                        &adoption.adopted_history().reference().key(),
                    )?
                    .as_ref()
                        == Some(adoption.adopted_history()),
            )
        }
        DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            if noncommit.occupied_identity().is_some() {
                return Ok(false);
            }
            authenticate_draft_edit_history_frontier_v1(reader, noncommit.observed_history())?;
            Ok(point::<DraftEditHistoryFrontiersFamily>(
                reader,
                &noncommit.observed_history().reference().key(),
            )?
            .as_ref()
                == Some(noncommit.observed_history())
                && match noncommit.proposed_successor() {
                    Some(successor) => {
                        point::<DraftPieceRootsFamily>(reader, &successor.key())?.is_none()
                    }
                    None => true,
                })
        }
    }
}

fn settlement_is_settle_target(settlement: &DraftPieceSettlementV1) -> bool {
    matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Committed { .. }
            | DraftPieceSettlementOutcomeV1::Conflict { .. }
    )
}

fn settlement_is_terminal_target(settlement: &DraftPieceSettlementV1, kind: TerminalKind) -> bool {
    match (settlement.outcome(), kind) {
        (DraftPieceSettlementOutcomeV1::Cancelled, TerminalKind::Cancelled) => true,
        (DraftPieceSettlementOutcomeV1::Rejected(actual), TerminalKind::Rejected(expected)) => {
            *actual == expected
        }
        (DraftPieceSettlementOutcomeV1::Error(actual), TerminalKind::Error(expected)) => {
            *actual == expected
        }
        _ => false,
    }
}

impl DomainMutation<SyndicDomain> for BeginMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if !self.prepared.predecessor_positions_authenticated {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if let Some(settlement) =
            point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?
        {
            settlement_matches(reader, &settlement, &self.prepared)?;
            return Err(SyndicMutationError::IdentityCollision);
        }
        if self.prepared.prebuild_rejection().is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let (target_build, target_receipt, target_session) = build_record(&self.prepared)?;
        let session = session_head(
            reader,
            self.prepared.header.draft_id(),
            self.prepared.header.session_id(),
        )?;
        let target_root = DraftPieceRootKeyV1::editor_candidate(
            self.prepared.header.draft_id(),
            self.prepared.header.session_id(),
            self.prepared.header.operation_id(),
        );
        if point::<DraftPieceRootsFamily>(reader, &target_root)?.is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if let Some(build) = point_build(reader, &settlement_key(&self.prepared))? {
            return if build == target_build {
                authenticate_target_transition(reader, &build, &target_receipt, &target_session)
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        }
        if point::<DraftPieceBuildProgressFamily>(reader, &target_receipt.key())?.is_some()
            || session != *self.prepared.source_session()
        {
            return Err(SyndicMutationError::CurrentDraftConflict);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point_build(reader, &settlement_key(&self.prepared))?.is_none() {
            let (build, receipt, claimed) = build_record(&self.prepared)?;
            put_build_transition(mutations, &build, &receipt)?;
            put_session_head(mutations, &claimed)?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for StageFragmentMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let build = required_build(reader, &settlement_key(&self.prepared))?;
        let ordinal = self.fragment.key().ordinal();
        if !build_matches(&build, &self.prepared)
            || build.lifecycle() != DraftPieceBuildLifecycleV1::Open
            || self.fragment.key().draft_id() != build.draft_id()
            || self.fragment.key().session_id() != build.session_id()
            || self.fragment.key().operation_id() != build.operation_id()
            || ordinal > build.staged_fragment_count().saturating_add(1)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if let Some(existing) =
            point::<DraftPieceBuildFragmentsFamily>(reader, &self.fragment.key())?
        {
            if existing != self.fragment || build.staged_fragment_count() != ordinal {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let target_receipt =
                required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
            let previous = target_receipt
                .previous()
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let previous_receipt =
                required::<DraftPieceBuildProgressFamily>(reader, &previous.key())?;
            if previous_receipt.reference() != previous {
                return Err(SyndicMutationError::IdentityCollision);
            }
            authenticate_progress_receipt(reader, &previous_receipt)?;
            let source = build_from_progress_receipt(&build, &previous_receipt)?;
            let DraftPieceBuildFrontierV1::Receiving {
                next_ordinal,
                chain,
            } = source.frontier()
            else {
                return Err(SyndicMutationError::IdentityCollision);
            };
            if source.lifecycle() != DraftPieceBuildLifecycleV1::Open
                || source.staged_fragment_count().checked_add(1) != Some(ordinal)
                || ordinal != next_ordinal
                || self.fragment.preceding_chain() != chain
                || self.fragment.chain_digest()
                    != draft_piece_fragment_chain_link_v1(
                        chain,
                        ordinal,
                        self.fragment.replacement(),
                    )
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let (expected_build, expected_receipt, expected_session) =
                stage_transition(&self.prepared, &source, &self.fragment)?;
            if build != expected_build || target_receipt != expected_receipt {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return authenticate_target_transition(
                reader,
                &build,
                &target_receipt,
                &expected_session,
            );
        }
        let DraftPieceBuildFrontierV1::Receiving {
            next_ordinal,
            chain,
        } = build.frontier()
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if ordinal != next_ordinal
            || self.fragment.preceding_chain() != chain
            || self.fragment.chain_digest()
                != draft_piece_fragment_chain_link_v1(chain, ordinal, self.fragment.replacement())
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let (_, target_receipt, _) = stage_transition(&self.prepared, &build, &self.fragment)?;
        let expected_session = expected_active_session(&self.prepared, &build)?;
        authenticate_source_transition(reader, &build, &expected_session, target_receipt.key())?;
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildFragmentsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftPieceBuildFragmentsFamily>(reader, &self.fragment.key())?.is_some() {
            return Ok(());
        }
        let build = required_build(reader, &settlement_key(&self.prepared))?;
        mutations.put::<DraftPieceBuildFragmentsCodec>(&self.fragment.key(), &self.fragment)?;
        let (next, receipt, advanced) = stage_transition(&self.prepared, &build, &self.fragment)?;
        put_build_transition(mutations, &next, &receipt)?;
        put_session_head(mutations, &advanced)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for AdvanceMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = required_build(reader, &build_key(&self.prepared.expected))?;
        if current == self.prepared.next {
            let receipt = required::<DraftPieceBuildProgressFamily>(
                reader,
                &self.prepared.next_receipt.key(),
            )?;
            if receipt != self.prepared.next_receipt {
                return Err(SyndicMutationError::IdentityCollision);
            }
            authenticate_target_transition(
                reader,
                &current,
                &receipt,
                &self.prepared.next_session,
            )?;
            for leaf in &self.prepared.leaves {
                if point::<DraftPieceLeavesFamily>(reader, &leaf.key())?.as_ref() != Some(leaf) {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            for node in &self.prepared.nodes {
                if point::<DraftPieceNodesFamily>(reader, &node.key())?.as_ref() != Some(node) {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            for record in &self.prepared.index_records {
                if point::<DraftMarkerIdentityIndexFamily>(reader, &record.key())?.as_ref()
                    != Some(record)
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            return Ok(());
        }
        if current != self.prepared.expected {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_source_transition(
            reader,
            &current,
            &self.prepared.expected_session,
            self.prepared.next_receipt.key(),
        )?;
        for leaf in &self.prepared.leaves {
            if point::<DraftPieceLeavesFamily>(reader, &leaf.key())?.is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        for node in &self.prepared.nodes {
            if point::<DraftPieceNodesFamily>(reader, &node.key())?.is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        for record in &self.prepared.index_records {
            if point::<DraftMarkerIdentityIndexFamily>(reader, &record.key())?.is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if !self.prepared.leaves.is_empty() {
            reservation.reserve_records::<DraftPieceLeavesCodec>(self.prepared.leaves.len())?;
        }
        if !self.prepared.nodes.is_empty() {
            reservation.reserve_records::<DraftPieceNodesCodec>(self.prepared.nodes.len())?;
        }
        if !self.prepared.index_records.is_empty() {
            reservation.reserve_records::<DraftMarkerIdentityIndexCodec>(
                self.prepared.index_records.len(),
            )?;
        }
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if required_build(reader, &build_key(&self.prepared.expected))? == self.prepared.next {
            return Ok(());
        }
        for leaf in &self.prepared.leaves {
            if point::<DraftPieceLeavesFamily>(reader, &leaf.key())?.is_none() {
                mutations.put::<DraftPieceLeavesCodec>(&leaf.key(), leaf)?;
            }
        }
        for node in &self.prepared.nodes {
            if point::<DraftPieceNodesFamily>(reader, &node.key())?.is_none() {
                mutations.put::<DraftPieceNodesCodec>(&node.key(), node)?;
            }
        }
        for record in &self.prepared.index_records {
            if point::<DraftMarkerIdentityIndexFamily>(reader, &record.key())?.is_none() {
                mutations.put::<DraftMarkerIdentityIndexCodec>(&record.key(), record)?;
            }
        }
        put_build_transition(mutations, &self.prepared.next, &self.prepared.next_receipt)?;
        put_session_head(mutations, &self.prepared.next_session)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for SettleMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if let Some(settlement) =
            point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?
        {
            return if settlement_is_settle_target(&settlement)
                && settlement_matches(reader, &settlement, &self.prepared)?
            {
                Ok(())
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        }
        let build = required_build(reader, &settlement_key(&self.prepared))?;
        if !build_matches(&build, &self.prepared)
            || build.lifecycle() != DraftPieceBuildLifecycleV1::Complete
            || build.successor().is_none()
            || build.build_digest().is_none()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let expected_session = expected_active_session(&self.prepared, &build)?;
        authenticate_source_transition(
            reader,
            &build,
            &expected_session,
            next_progress_key(&build)?,
        )?;
        let successor = build
            .successor()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        if point::<DraftPieceRootsFamily>(reader, &successor.key())?.is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        reservation.reserve_records::<DraftPieceRootsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?.is_some()
        {
            return Ok(());
        }
        let build = required_build(reader, &settlement_key(&self.prepared))?;
        let current = session_head(reader, build.draft_id(), build.session_id())?;
        if current.active_operation() != Some(&custody_for(&build)) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let source_receipt =
            required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
        let key = settlement_key(&self.prepared);
        let successor = build
            .successor()
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let root = DraftPieceRootRecordV1::new(successor);
        let observed_history = authenticated_history_frontier(reader, current.newest_history())?;
        let (outcome, closure, lifecycle, target_session) = if current.lifecycle()
            == DraftEditorCandidateSessionLifecycleV1::Active
            && current.newest_candidate_generation() == build.predecessor_candidate_generation()
            && current.newest_root() == build.predecessor_root()
            && current.newest_history() == build.predecessor_history()
        {
            match append_ordinary_draft_edit_history_with_retention_v1(
                reader,
                &observed_history,
                current
                    .newest_candidate_generation()
                    .checked_add(1)
                    .ok_or(SyndicMutationError::IdentityCollision)?,
                successor,
                build.predecessor_caret(),
                build.predecessor_selection(),
                build.caret(),
                build.selection(),
                build.operation_id(),
            ) {
                Ok((transition, adopted_history)) => {
                    let next = current
                        .adopted(successor, adopted_history.reference())
                        .ok_or(SyndicMutationError::IdentityCollision)?;
                    mutations.put::<DraftPieceRootsCodec>(&successor.key(), &root)?;
                    mutations
                        .put::<DraftEditHistoryTransitionsCodec>(&transition.key(), &transition)?;
                    mutations.put::<DraftEditHistoryFrontiersCodec>(
                        &adopted_history.reference().key(),
                        &adopted_history,
                    )?;
                    (
                        DraftPieceSettlementOutcomeV1::Committed {
                            candidate_generation: next.newest_candidate_generation(),
                            successor,
                            history: adopted_history.reference(),
                            caret: build.caret(),
                            selection: build.selection(),
                        },
                        Box::new(DraftPieceSettlementClosureV1::Committed(
                            DraftPieceCommittedAdoptionV1::new(
                                current.clone(),
                                next.clone(),
                                root,
                                observed_history,
                                transition,
                                adopted_history,
                            ),
                        )),
                        DraftPieceBuildLifecycleV1::Committed,
                        next,
                    )
                }
                Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable) => {
                    let cleared = current
                        .clear_active_operation(&custody_for(&build))
                        .ok_or(SyndicMutationError::IdentityCollision)?;
                    (
                        DraftPieceSettlementOutcomeV1::Error(
                            DraftPieceErrorReasonV1::HistoryCapacityUnavailable,
                        ),
                        Box::new(DraftPieceSettlementClosureV1::Noncommit(
                            DraftPieceNoncommitClosureV1::new(
                                cleared.clone(),
                                observed_history,
                                build.successor(),
                            ),
                        )),
                        DraftPieceBuildLifecycleV1::Error,
                        cleared,
                    )
                }
                Err(DraftEditHistoryRetentionErrorV1::Invalid) => {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
        } else {
            let cleared = current
                .clear_active_operation(&custody_for(&build))
                .ok_or(SyndicMutationError::IdentityCollision)?;
            (
                DraftPieceSettlementOutcomeV1::Conflict {
                    current_candidate_generation: current.newest_candidate_generation(),
                    current_root: current.newest_root(),
                    current_history: current.newest_history(),
                },
                Box::new(DraftPieceSettlementClosureV1::Noncommit(
                    DraftPieceNoncommitClosureV1::new(
                        cleared.clone(),
                        observed_history,
                        build.successor(),
                    ),
                )),
                DraftPieceBuildLifecycleV1::Conflict,
                cleared,
            )
        };
        let (terminal, receipt) =
            terminal_build(&build, lifecycle, source_receipt.fragment_endpoint())?;
        let settlement = DraftPieceSettlementV1::new_boxed(
            key,
            build.proposal_digest(),
            build.predecessor_candidate_generation(),
            build.predecessor_root(),
            build.predecessor_history(),
            build.fragment_count(),
            build.fragment_chain(),
            build.predecessor_caret(),
            build.predecessor_selection(),
            build.caret(),
            build.selection(),
            build.build_digest(),
            build.canonical_header().to_vec(),
            Some(build.clone()),
            receipt.reference(),
            outcome,
            closure,
        );
        put_session_head(mutations, &target_session)?;
        mutations.put::<DraftPieceSettlementsCodec>(&key, &settlement)?;
        put_build_transition(mutations, &terminal, &receipt)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if let Some(settlement) =
            point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?
        {
            return if settlement_is_terminal_target(&settlement, self.kind)
                && settlement_matches(reader, &settlement, &self.prepared)?
            {
                Ok(())
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        }
        let existing_build = point_build(reader, &settlement_key(&self.prepared))?;
        if existing_build.is_none() {
            if self.prepared.prebuild_rejection().is_some_and(
                |reason| !matches!(self.kind, TerminalKind::Rejected(actual) if actual == reason),
            ) {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let session = session_head(
                reader,
                self.prepared.header.draft_id(),
                self.prepared.header.session_id(),
            )?;
            let target_root = DraftPieceRootKeyV1::editor_candidate(
                self.prepared.header.draft_id(),
                self.prepared.header.session_id(),
                self.prepared.header.operation_id(),
            );
            if session != *self.prepared.source_session()
                || point::<DraftPieceRootsFamily>(reader, &target_root)?.is_some()
                || point::<DraftPieceBuildProgressFamily>(
                    reader,
                    &DraftPieceBuildProgressReceiptKeyV1::new(
                        self.prepared.header.draft_id(),
                        self.prepared.header.session_id(),
                        self.prepared.header.operation_id(),
                        1,
                    ),
                )?
                .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return Ok(());
        }
        if self.prepared.prebuild_rejection().is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let build = existing_build.ok_or(SyndicMutationError::IdentityCollision)?;
        if !build_matches(&build, &self.prepared)
            || !matches!(
                build.lifecycle(),
                DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
            )
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let expected_session = expected_active_session(&self.prepared, &build)?;
        authenticate_source_transition(
            reader,
            &build,
            &expected_session,
            next_progress_key(&build)?,
        )?;
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        reservation.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?.is_some()
        {
            return Ok(());
        }
        if point_build(reader, &settlement_key(&self.prepared))?.is_none() {
            let header = self.prepared.header;
            let (outcome, lifecycle) = match self.kind {
                TerminalKind::Cancelled => (
                    DraftPieceSettlementOutcomeV1::Cancelled,
                    DraftPieceBuildLifecycleV1::Cancelled,
                ),
                TerminalKind::Rejected(reason) => (
                    DraftPieceSettlementOutcomeV1::Rejected(reason),
                    DraftPieceBuildLifecycleV1::Rejected,
                ),
                TerminalKind::Error(reason) => (
                    DraftPieceSettlementOutcomeV1::Error(reason),
                    DraftPieceBuildLifecycleV1::Error,
                ),
            };
            let (build, receipt) = terminal_first_build(&self.prepared, lifecycle)?;
            let claimed = self
                .prepared
                .source_session()
                .with_active_operation(custody_for(&build))
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let cleared = claimed
                .clear_active_operation(&custody_for(&build))
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let settlement = DraftPieceSettlementV1::new_boxed(
                settlement_key(&self.prepared),
                self.prepared.proposal_digest(),
                header.predecessor_candidate_generation(),
                header.predecessor_root(),
                header.predecessor_history(),
                header.fragment_count(),
                header.fragment_chain(),
                header.predecessor_caret(),
                header.predecessor_selection(),
                header.caret(),
                header.selection(),
                None,
                self.prepared.canonical_header().to_vec(),
                None,
                receipt.reference(),
                outcome,
                Box::new(DraftPieceSettlementClosureV1::Noncommit(
                    DraftPieceNoncommitClosureV1::new(
                        cleared.clone(),
                        authenticated_history_frontier(reader, cleared.newest_history())?,
                        None,
                    ),
                )),
            );
            put_build_transition(mutations, &build, &receipt)?;
            put_session_head(mutations, &cleared)?;
            mutations.put::<DraftPieceSettlementsCodec>(&settlement.key(), &settlement)?;
            return Ok(());
        }
        let build = required_build(reader, &settlement_key(&self.prepared))?;
        let current = session_head(reader, build.draft_id(), build.session_id())?;
        if current.active_operation() != Some(&custody_for(&build)) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let source_receipt =
            required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
        if let Some(successor) = build.successor()
            && point::<DraftPieceRootsFamily>(reader, &successor.key())?.is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let (outcome, lifecycle) = match self.kind {
            TerminalKind::Cancelled => (
                DraftPieceSettlementOutcomeV1::Cancelled,
                DraftPieceBuildLifecycleV1::Cancelled,
            ),
            TerminalKind::Rejected(reason) => (
                DraftPieceSettlementOutcomeV1::Rejected(reason),
                DraftPieceBuildLifecycleV1::Rejected,
            ),
            TerminalKind::Error(reason) => (
                DraftPieceSettlementOutcomeV1::Error(reason),
                DraftPieceBuildLifecycleV1::Error,
            ),
        };
        let cleared = current
            .clear_active_operation(&custody_for(&build))
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let (terminal, receipt) =
            terminal_build(&build, lifecycle, source_receipt.fragment_endpoint())?;
        let settlement = DraftPieceSettlementV1::new_boxed(
            settlement_key(&self.prepared),
            build.proposal_digest(),
            build.predecessor_candidate_generation(),
            build.predecessor_root(),
            build.predecessor_history(),
            build.fragment_count(),
            build.fragment_chain(),
            build.predecessor_caret(),
            build.predecessor_selection(),
            build.caret(),
            build.selection(),
            build.build_digest(),
            build.canonical_header().to_vec(),
            Some(build.clone()),
            receipt.reference(),
            outcome,
            Box::new(DraftPieceSettlementClosureV1::Noncommit(
                DraftPieceNoncommitClosureV1::new(
                    cleared.clone(),
                    authenticated_history_frontier(reader, cleared.newest_history())?,
                    build.successor(),
                ),
            )),
        );
        put_session_head(mutations, &cleared)?;
        mutations.put::<DraftPieceSettlementsCodec>(&settlement.key(), &settlement)?;
        put_build_transition(mutations, &terminal, &receipt)?;
        Ok(())
    }
}
