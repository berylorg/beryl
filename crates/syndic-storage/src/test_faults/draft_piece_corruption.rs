use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation, RecordCodec,
};

use crate::{
    SyndicPointReadLimit, SyndicStorage, codec::Family, domain::SyndicDomain, draft_piece::*,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceDescendantTarget {
    Sequence,
    MarkerIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceImmutableDeletion {
    Root,
    RootNode,
    SequenceDescendant,
    Settlement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceBuildCorruption {
    DropDurableContinuation,
    OpenWithCompleteFrontier,
    CompleteWithoutSuccessor,
    TerminalLifecycle,
    ReconcilingMovesNextMove,
    InsertingNextPiece,
    InsertingByteCursor,
    AdjacentPhaseBoundary,
    StagedToCrossValidating,
    InsertingNextPieceInRange,
    InsertingScalarByteSkip,
    PlanningToApplying,
    RemovingToApplying,
    ApplyingToInserting,
    AdjacentFragmentJump,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateOpenReceiptCorruption {
    Malformed,
    Truncated,
    Noncanonical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceFragmentCorruption {
    ReplacementBytes,
    ChainDigest,
    PrecedingDigest,
    OversizedText,
    EmptyText,
    ContinuationMoves,
    DuplicateMoveDeclarations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceProgressReceiptCorruption {
    Delete,
    DeletePrevious,
    StateMismatch,
    PreviousStateMismatch,
    HeadEndpointMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceCandidateRootCollision {
    Exact,
    DifferentCanonicalBytes,
    MarkerCommitmentDigest,
}

pub fn draft_piece_fragment_zero_ordinal_codec_rejections(
    fragment: &DraftPieceBuildFragmentV1,
) -> [bool; 4] {
    let key = fragment.key();
    let zero_key =
        DraftPieceBuildFragmentKeyV1::new(key.draft_id(), key.session_id(), key.operation_id(), 0);
    let encode_key_rejected = DraftPieceBuildFragmentsFamily::encode_key(&zero_key).is_err();

    let mut encoded_key = DraftPieceBuildFragmentsFamily::encode_key(&key)
        .expect("valid fixture fragment key must encode");
    let ordinal_start = encoded_key
        .len()
        .checked_sub(8)
        .expect("fragment key contains its ordinal");
    encoded_key[ordinal_start..].fill(0);
    let decode_key_rejected = DraftPieceBuildFragmentsFamily::decode_key(&encoded_key).is_err();

    let zero_value = DraftPieceBuildFragmentV1::new(
        zero_key,
        fragment.replacement().clone(),
        fragment.preceding_chain(),
        fragment.chain_digest(),
    );
    let encode_value_rejected = DraftPieceBuildFragmentsFamily::encode_value(&zero_value).is_err();

    let mut encoded_value = DraftPieceBuildFragmentsFamily::encode_value(fragment)
        .expect("valid fixture fragment must encode");
    encoded_value[ordinal_start..ordinal_start + 8].fill(0);
    let decode_value_rejected =
        DraftPieceBuildFragmentsFamily::decode_value(&encoded_value).is_err();

    [
        encode_key_rejected,
        decode_key_rejected,
        encode_value_rejected,
        decode_value_rejected,
    ]
}

pub fn draft_piece_fragment_is_stored_exactly(
    store: &HomeStore,
    storage: SyndicStorage,
    fragment: &DraftPieceBuildFragmentV1,
) -> bool {
    matches!(
        storage.point::<DraftPieceBuildFragmentsFamily>(
            store,
            fragment.key(),
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        ),
        Ok(Some(stored)) if stored == *fragment
    )
}

pub fn delete_draft_piece_terminal_build(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DeleteImmutable(DeletedImmutable::Build(key)),
    )
}

pub fn delete_draft_piece_build_progress_receipt(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceBuildProgressReceiptKeyV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DeleteImmutable(DeletedImmutable::Progress(key)),
    )
}

pub fn inject_draft_piece_candidate_root_collision(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    collision: DraftPieceCandidateRootCollision,
) -> MutationContribution {
    let reference = match collision {
        DraftPieceCandidateRootCollision::Exact => root,
        DraftPieceCandidateRootCollision::DifferentCanonicalBytes => {
            DraftPieceRootReferenceV1::new_authenticated(
                root.key(),
                root.root_node(),
                root.summary(),
                root.marker_index_root(),
                root.marker_index_summary(),
                root.marker_order_root(),
                root.marker_order_height(),
                root.marker_commitment(),
                DraftPieceDigestV1::from_bytes([0xA7; 32]),
            )
        }
        DraftPieceCandidateRootCollision::MarkerCommitmentDigest => {
            let commitment = root.marker_commitment();
            let corrupted = DraftMarkerCommitmentV1::new(
                [0xC3; 32],
                commitment.marker_count(),
                commitment.maximum_image_label(),
            )
            .expect("corrupted marker digest retains the summary shape");
            DraftPieceRootReferenceV1::new_authenticated(
                root.key(),
                root.root_node(),
                root.summary(),
                root.marker_index_root(),
                root.marker_index_summary(),
                root.marker_order_root(),
                root.marker_order_height(),
                corrupted,
                root.combined_digest(),
            )
        }
    };
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Root(
            root.key(),
            DraftPieceRootRecordV1::new(reference),
        )),
    )
}

pub fn rekey_draft_piece_root_for_collision(
    root: DraftPieceRootReferenceV1,
    key: DraftPieceRootKeyV1,
) -> DraftPieceRootReferenceV1 {
    DraftPieceRootReferenceV1::new_authenticated(
        key,
        root.root_node(),
        root.summary(),
        root.marker_index_root(),
        root.marker_index_summary(),
        root.marker_order_root(),
        root.marker_order_height(),
        root.marker_commitment(),
        root.combined_digest(),
    )
}

pub fn inject_draft_piece_build_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    corruption: DraftPieceBuildCorruption,
) -> MutationContribution {
    let build = storage
        .point::<DraftPieceBuildsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture build reads")
        .expect("fixture build exists");
    let (frontier, successor, build_digest, lifecycle) = match corruption {
        DraftPieceBuildCorruption::DropDurableContinuation => (
            build.frontier(),
            build.successor(),
            build.build_digest(),
            build.lifecycle(),
        ),
        DraftPieceBuildCorruption::OpenWithCompleteFrontier => (
            DraftPieceBuildFrontierV1::Complete,
            build.successor(),
            build.build_digest(),
            DraftPieceBuildLifecycleV1::Open,
        ),
        DraftPieceBuildCorruption::CompleteWithoutSuccessor => (
            DraftPieceBuildFrontierV1::Complete,
            None,
            None,
            DraftPieceBuildLifecycleV1::Complete,
        ),
        DraftPieceBuildCorruption::TerminalLifecycle => (
            build.frontier(),
            build.successor(),
            build.build_digest(),
            if build.lifecycle() == DraftPieceBuildLifecycleV1::Cancelled {
                DraftPieceBuildLifecycleV1::Error
            } else {
                DraftPieceBuildLifecycleV1::Cancelled
            },
        ),
        DraftPieceBuildCorruption::ReconcilingMovesNextMove => {
            let DraftPieceBuildFrontierV1::ReconcilingMoves {
                fragment_ordinal, ..
            } = build.frontier()
            else {
                panic!("fixture build is not reconciling moves")
            };
            (
                DraftPieceBuildFrontierV1::ReconcilingMoves {
                    fragment_ordinal,
                    next_move: u64::MAX,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::InsertingNextPiece => {
            let DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                base_end,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not inserting")
            };
            (
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece: u64::MAX,
                    next_byte: 0,
                    base_end,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::InsertingByteCursor => {
            let DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                next_piece,
                base_end,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not inserting")
            };
            (
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece,
                    next_byte: u64::MAX,
                    base_end,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::AdjacentPhaseBoundary => {
            let DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal,
                next_rank,
                end_rank,
                removed_markers,
                base_end,
                successor_start,
                successor_end,
            } = build.frontier()
            else {
                panic!("fixture build is not removing")
            };
            (
                DraftPieceBuildFrontierV1::Removing {
                    fragment_ordinal,
                    next_rank,
                    end_rank: end_rank.saturating_add(1),
                    removed_markers,
                    base_end,
                    successor_start,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::StagedToCrossValidating => (
            DraftPieceBuildFrontierV1::CrossValidating,
            build.successor(),
            build.build_digest(),
            build.lifecycle(),
        ),
        DraftPieceBuildCorruption::InsertingNextPieceInRange => {
            let DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                base_end,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not inserting")
            };
            (
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece: 1,
                    next_byte: 0,
                    base_end,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::InsertingScalarByteSkip => {
            let DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                next_piece,
                base_end,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not inserting")
            };
            (
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece,
                    next_byte: 2,
                    base_end,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::PlanningToApplying => {
            let DraftPieceBuildFrontierV1::Planning { fragment_ordinal } = build.frontier() else {
                panic!("fixture build is not planning")
            };
            let origin = DraftPieceBuildBoundaryV1::new(0, 0);
            (
                DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal,
                    base_end: origin,
                    successor_start: origin,
                    successor_end: origin,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::RemovingToApplying => {
            let DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal,
                base_end,
                successor_start,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not removing")
            };
            (
                DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal,
                    base_end,
                    successor_start,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::ApplyingToInserting => {
            let DraftPieceBuildFrontierV1::Applying {
                fragment_ordinal,
                base_end,
                successor_end,
                ..
            } = build.frontier()
            else {
                panic!("fixture build is not applying")
            };
            (
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece: 0,
                    next_byte: 0,
                    base_end,
                    successor_end,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
        DraftPieceBuildCorruption::AdjacentFragmentJump => {
            let DraftPieceBuildFrontierV1::Planning { fragment_ordinal } = build.frontier() else {
                panic!("fixture build is not planning")
            };
            (
                DraftPieceBuildFrontierV1::Planning {
                    fragment_ordinal: fragment_ordinal + 1,
                },
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            )
        }
    };
    let corrupted = DraftPieceBuildRecordV1::new(
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
        build.working_roots(),
        build.base_frontier(),
        build.successor_frontier(),
        build.next_record_ordinal(),
        frontier,
        build.progress_digest(),
        build.progress_receipt(),
        successor,
        build_digest,
        lifecycle,
    )
    .with_durable_continuation(
        (corruption != DraftPieceBuildCorruption::DropDurableContinuation)
            .then(|| build.durable_continuation())
            .flatten(),
    );
    let corrupted = if matches!(
        corruption,
        DraftPieceBuildCorruption::StagedToCrossValidating
            | DraftPieceBuildCorruption::DropDurableContinuation
    ) {
        authenticated_build_record(corrupted)
    } else {
        corrupted
    };
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Build(key, corrupted)),
    )
}

pub fn inject_draft_piece_progress_receipt_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    corruption: DraftPieceProgressReceiptCorruption,
) -> MutationContribution {
    let limit = SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero");
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, limit)
        .expect("fixture build reads")
        .expect("fixture build exists");
    let receipt_key = build.progress_receipt().key();
    match corruption {
        DraftPieceProgressReceiptCorruption::Delete => storage.handle.contribution(
            storage.revision(store).expect("fixture revision reads"),
            DeleteImmutable(DeletedImmutable::Progress(receipt_key)),
        ),
        DraftPieceProgressReceiptCorruption::DeletePrevious => {
            let receipt = storage
                .point::<DraftPieceBuildProgressFamily>(store, receipt_key, limit)
                .expect("fixture receipt reads")
                .expect("fixture receipt exists");
            let previous = receipt
                .previous()
                .expect("fixture receipt has a predecessor");
            storage.handle.contribution(
                storage.revision(store).expect("fixture revision reads"),
                DeleteImmutable(DeletedImmutable::Progress(previous.key())),
            )
        }
        DraftPieceProgressReceiptCorruption::StateMismatch => {
            let receipt = storage
                .point::<DraftPieceBuildProgressFamily>(store, receipt_key, limit)
                .expect("fixture receipt reads")
                .expect("fixture receipt exists");
            let mut corrupted = DraftPieceBuildProgressReceiptV1::new(
                DraftPieceBuildProgressReceiptReferenceV1::new(
                    receipt.key(),
                    DraftPieceDigestV1::from_bytes([0; 32]),
                ),
                receipt.previous(),
                receipt.fragment_endpoint(),
                DraftPieceDigestV1::from_bytes([0xE1; 32]),
                receipt.working_roots(),
                receipt.base_frontier(),
                receipt.successor_frontier(),
                receipt.next_record_ordinal(),
                receipt.frontier(),
                receipt.successor(),
                receipt.build_digest(),
                receipt.lifecycle(),
            );
            corrupted = recompute_progress_receipt_digest(corrupted);
            storage.handle.contribution(
                storage.revision(store).expect("fixture revision reads"),
                DescendantReplacement(Replacement::Progress(receipt_key, corrupted)),
            )
        }
        DraftPieceProgressReceiptCorruption::PreviousStateMismatch => {
            let receipt = storage
                .point::<DraftPieceBuildProgressFamily>(store, receipt_key, limit)
                .expect("fixture receipt reads")
                .expect("fixture receipt exists");
            let previous = receipt
                .previous()
                .expect("fixture receipt has a predecessor");
            let stored = storage
                .point::<DraftPieceBuildProgressFamily>(store, previous.key(), limit)
                .expect("fixture predecessor receipt reads")
                .expect("fixture predecessor receipt exists");
            let corrupted =
                recompute_progress_receipt_digest(DraftPieceBuildProgressReceiptV1::new(
                    DraftPieceBuildProgressReceiptReferenceV1::new(
                        stored.key(),
                        DraftPieceDigestV1::from_bytes([0; 32]),
                    ),
                    stored.previous(),
                    stored.fragment_endpoint(),
                    DraftPieceDigestV1::from_bytes([0xE3; 32]),
                    stored.working_roots(),
                    stored.base_frontier(),
                    stored.successor_frontier(),
                    stored.next_record_ordinal(),
                    stored.frontier(),
                    stored.successor(),
                    stored.build_digest(),
                    stored.lifecycle(),
                ));
            storage.handle.contribution(
                storage.revision(store).expect("fixture revision reads"),
                DescendantReplacement(Replacement::Progress(previous.key(), corrupted)),
            )
        }
        DraftPieceProgressReceiptCorruption::HeadEndpointMismatch => {
            let corrupted = DraftPieceBuildRecordV1::new(
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
                build.working_roots(),
                build.base_frontier(),
                build.successor_frontier(),
                build.next_record_ordinal(),
                build.frontier(),
                build.progress_digest(),
                DraftPieceBuildProgressReceiptReferenceV1::new(
                    DraftPieceBuildProgressReceiptKeyV1::new(
                        build.draft_id(),
                        build.session_id(),
                        build.operation_id(),
                        build.progress_receipt().key().transition_ordinal() + 1,
                    ),
                    DraftPieceDigestV1::from_bytes([0xE2; 32]),
                ),
                build.successor(),
                build.build_digest(),
                build.lifecycle(),
            );
            storage.handle.contribution(
                storage.revision(store).expect("fixture revision reads"),
                DescendantReplacement(Replacement::Build(key, corrupted)),
            )
        }
    }
}

pub fn inject_draft_piece_occupied_stage_target(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    fragment: DraftPieceBuildFragmentV1,
) -> MutationContribution {
    let limit = SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero");
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, limit)
        .expect("fixture build reads")
        .expect("fixture build exists");
    let staged = build
        .staged_fragment_count()
        .checked_add(1)
        .expect("fixture fragment count advances");
    let chain = fragment.chain_digest();
    let frontier = if staged == build.fragment_count() {
        DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal: 1,
            next_move: 0,
        }
    } else {
        DraftPieceBuildFrontierV1::Receiving {
            next_ordinal: staged + 1,
            chain,
        }
    };
    let (_, receipt) = authenticated_build_transition(
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
        Some(canonical_fragment_endpoint(&fragment)),
    )
    .expect("fixture target transition is exact");
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Progress(receipt.key(), receipt)),
    )
}

pub fn inject_draft_piece_fragment_ahead(
    store: &HomeStore,
    storage: SyndicStorage,
    fragment: DraftPieceBuildFragmentV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Fragment(fragment.key(), fragment)),
    )
}

pub fn inject_draft_piece_custody_endpoint_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> MutationContribution {
    let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
    let DraftEditorCandidateSessionRecordV1::Head(head) = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture session reads")
        .expect("fixture session exists")
    else {
        panic!("fixture record is not a session head")
    };
    let custody = head
        .active_operation()
        .copied()
        .expect("fixture session has active-operation custody");
    let corrupted_custody = DraftEditorActiveOperationV1::building(
        custody.operation_id(),
        custody
            .proposal_digest()
            .expect("fixture custody is building"),
        custody.predecessor_candidate_generation(),
        custody.predecessor_root(),
        custody.predecessor_history(),
        DraftPieceBuildProgressReceiptReferenceV1::new(
            custody.build_receipt().unwrap().key(),
            DraftPieceDigestV1::from_bytes([0xE4; 32]),
        ),
    );
    let corrupted = DraftEditorCandidateSessionV1::from_parts(
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
        head.newest_history(),
        head.dirty_generation(),
        head.logical_extent(),
        head.lifecycle(),
        Some(corrupted_custody),
    );
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Session(
            key,
            DraftEditorCandidateSessionRecordV1::Head(corrupted),
        )),
    )
}

pub fn inject_draft_piece_session_generation_inflation(
    store: &HomeStore,
    storage: SyndicStorage,
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> MutationContribution {
    let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
    let DraftEditorCandidateSessionRecordV1::Head(head) = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture session reads")
        .expect("fixture session exists")
    else {
        panic!("fixture record is not a session head")
    };
    let corrupted = DraftEditorCandidateSessionV1::from_parts(
        head.thread_id(),
        head.draft_id(),
        head.session_id(),
        head.open_operation_id(),
        head.session_generation()
            .checked_add(1)
            .expect("fixture session generation advances"),
        head.durable_base_selector_revision(),
        head.durable_base_root(),
        head.durable_base_history(),
        head.published_candidate_generation(),
        head.published_selector_revision(),
        head.published_root(),
        head.published_history(),
        head.newest_candidate_generation(),
        head.newest_root(),
        head.newest_history(),
        head.dirty_generation(),
        head.logical_extent(),
        head.lifecycle(),
        head.active_operation().copied(),
    );
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Session(
            key,
            DraftEditorCandidateSessionRecordV1::Head(corrupted),
        )),
    )
}

pub fn inject_draft_piece_coordinated_stage_target_replacement(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
) -> MutationContribution {
    let limit = SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero");
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, limit)
        .expect("fixture build reads")
        .expect("fixture build exists");
    let receipt = storage
        .point::<DraftPieceBuildProgressFamily>(store, build.progress_receipt().key(), limit)
        .expect("fixture receipt reads")
        .expect("fixture receipt exists");
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id());
    let DraftEditorCandidateSessionRecordV1::Head(session) = storage
        .point::<DraftEditorCandidateSessionsFamily>(store, session_key, limit)
        .expect("fixture session reads")
        .expect("fixture session exists")
    else {
        panic!("fixture record is not a session head")
    };
    let (corrupted_build, corrupted_receipt) = authenticated_build_transition(
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
            build.working_roots(),
            build.base_frontier(),
            build.successor_frontier(),
            build
                .next_record_ordinal()
                .checked_add(1)
                .expect("fixture record ordinal advances"),
            build.frontier(),
            DraftPieceDigestV1::from_bytes([0; 32]),
            build.progress_receipt(),
            build.successor(),
            build.build_digest(),
            build.lifecycle(),
        ),
        receipt.previous(),
        receipt.fragment_endpoint(),
    )
    .expect("fixture replacement transition is exact");
    let custody = session
        .active_operation()
        .copied()
        .expect("fixture session has active-operation custody");
    let corrupted_session = DraftEditorCandidateSessionV1::from_parts(
        session.thread_id(),
        session.draft_id(),
        session.session_id(),
        session.open_operation_id(),
        session.session_generation(),
        session.durable_base_selector_revision(),
        session.durable_base_root(),
        session.durable_base_history(),
        session.published_candidate_generation(),
        session.published_selector_revision(),
        session.published_root(),
        session.published_history(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.dirty_generation(),
        session.logical_extent(),
        session.lifecycle(),
        Some(DraftEditorActiveOperationV1::building(
            custody.operation_id(),
            custody
                .proposal_digest()
                .expect("fixture custody is building"),
            custody.predecessor_candidate_generation(),
            custody.predecessor_root(),
            custody.predecessor_history(),
            corrupted_build.progress_receipt(),
        )),
    );
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        CoordinatedStageReplacement {
            key,
            build: corrupted_build,
            receipt: corrupted_receipt,
            session_key,
            session: DraftEditorCandidateSessionRecordV1::Head(corrupted_session),
        },
    )
}

pub fn inject_draft_editor_candidate_open_receipt_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    corruption: DraftEditorCandidateOpenReceiptCorruption,
) -> MutationContribution {
    let key =
        DraftEditorCandidateSessionRecordKeyV1::open_receipt(draft_id, session_id, operation_id);
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture receipt reads")
        .expect("fixture receipt exists")
    else {
        panic!("fixture record is not an open receipt")
    };
    let mut bytes = receipt.request_bytes().to_vec();
    match corruption {
        DraftEditorCandidateOpenReceiptCorruption::Malformed => bytes[8] ^= 0x7F,
        DraftEditorCandidateOpenReceiptCorruption::Truncated => bytes.truncate(bytes.len() / 2),
        DraftEditorCandidateOpenReceiptCorruption::Noncanonical => bytes.push(0),
    }
    let corrupted = DraftEditorCandidateSessionRecordV1::OpenReceipt(
        DraftEditorCandidateSessionOpenReceiptV1::new(bytes, receipt.head().clone()),
    );
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Session(key, corrupted)),
    )
}

pub fn inject_draft_piece_fragment_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceBuildFragmentKeyV1,
    corruption: DraftPieceFragmentCorruption,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let fragment = storage
        .point::<DraftPieceBuildFragmentsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture fragment reads")
        .expect("fixture fragment exists");
    let mut preceding = fragment.preceding_chain();
    let mut chain = fragment.chain_digest();
    let replacement = match corruption {
        DraftPieceFragmentCorruption::ReplacementBytes => {
            let mut inserted = fragment.replacement().inserted().to_vec();
            inserted.push(DraftPieceV1::Text("changed".to_owned()));
            let replacement = DraftPieceReplacementV1::new(
                fragment.replacement().start(),
                fragment.replacement().end(),
                inserted,
            );
            match fragment.replacement().marker_effect() {
                Some(effect) => replacement.with_marker_effect(effect),
                None => replacement,
            }
        }
        DraftPieceFragmentCorruption::ChainDigest => {
            chain = DraftPieceDigestV1::from_bytes([0xC1; 32]);
            fragment.replacement().clone()
        }
        DraftPieceFragmentCorruption::PrecedingDigest => {
            preceding = DraftPieceDigestV1::from_bytes([0xC2; 32]);
            fragment.replacement().clone()
        }
        DraftPieceFragmentCorruption::OversizedText => DraftPieceReplacementV1::new(
            fragment.replacement().start(),
            fragment.replacement().end(),
            vec![DraftPieceV1::Text(
                "x".repeat(DRAFT_PIECE_PAGE_MAX_BYTES + 1),
            )],
        ),
        DraftPieceFragmentCorruption::EmptyText => DraftPieceReplacementV1::new(
            fragment.replacement().start(),
            fragment.replacement().end(),
            vec![DraftPieceV1::Text(String::new())],
        ),
        DraftPieceFragmentCorruption::ContinuationMoves
        | DraftPieceFragmentCorruption::DuplicateMoveDeclarations => {
            let marker = DraftPieceMarkerV1::new(
                beryl_model::SyndicDraftMarkerId::from_bytes([0xC3; 16]),
                1,
                beryl_model::ImageLabelOrdinal::new(1).expect("fixture label is nonzero"),
            );
            let insertion = DraftPieceMarkerInsertionV1::new(
                0,
                marker,
                DraftPieceMarkerEffectChargesV1::new(0, 1, 1),
            );
            let replacement = if corruption == DraftPieceFragmentCorruption::ContinuationMoves {
                DraftPieceReplacementV1::continuation(
                    fragment.replacement().start(),
                    fragment.replacement().end(),
                    vec![DraftPieceV1::Marker(marker)],
                )
            } else {
                DraftPieceReplacementV1::new(
                    fragment.replacement().start(),
                    fragment.replacement().end(),
                    vec![DraftPieceV1::Marker(marker)],
                )
            };
            if corruption == DraftPieceFragmentCorruption::DuplicateMoveDeclarations {
                let different = DraftPieceMarkerV1::new(
                    beryl_model::SyndicDraftMarkerId::from_bytes([0xC4; 16]),
                    1,
                    marker.label(),
                );
                replacement.with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        0,
                        different,
                        DraftPieceMarkerEffectChargesV1::new(0, 1, 1),
                    ),
                ))
            } else {
                replacement.with_marker_effect(DraftPieceMarkerEffectV1::Insert(insertion))
            }
        }
    };
    let corrupted = DraftPieceBuildFragmentV1::new(key, replacement, preceding, chain);
    let encoded_key =
        <DraftPieceBuildFragmentsCodec as RecordCodec<SyndicDomain>>::encode_key(&key)
            .expect("fixture fragment key must encode");
    let payload = encode_fragment_unchecked_for_test_fault(&corrupted);
    let mut encoded_value = Vec::with_capacity(4 + payload.len());
    encoded_value.extend_from_slice(
        &DraftPieceBuildFragmentsFamily::RECORD_VERSION
            .get()
            .to_be_bytes(),
    );
    encoded_value.extend_from_slice(&payload);
    store.inject_persisted_corrupt_record::<SyndicDomain, DraftPieceBuildFragmentsCodec>(
        storage.handle,
        &encoded_key,
        &encoded_value,
    )
}

pub fn inject_draft_editor_candidate_session_published_beyond_newest(
    store: &HomeStore,
    storage: SyndicStorage,
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> MutationContribution {
    let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
    let DraftEditorCandidateSessionRecordV1::Head(head) = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture session reads")
        .expect("fixture session exists")
    else {
        panic!("fixture session head is not a head")
    };
    let corrupted = DraftEditorCandidateSessionV1::from_parts(
        head.thread_id(),
        head.draft_id(),
        head.session_id(),
        head.open_operation_id(),
        head.session_generation(),
        head.durable_base_selector_revision(),
        head.durable_base_root(),
        head.durable_base_history(),
        head.newest_candidate_generation().saturating_add(1),
        head.published_selector_revision(),
        head.published_root(),
        head.published_history(),
        head.newest_candidate_generation(),
        head.newest_root(),
        head.newest_history(),
        head.dirty_generation(),
        head.logical_extent(),
        head.lifecycle(),
        head.active_operation().copied(),
    );
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DescendantReplacement(Replacement::Session(
            key,
            DraftEditorCandidateSessionRecordV1::Head(corrupted),
        )),
    )
}

pub fn delete_draft_piece_immutable_record(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    deletion: DraftPieceImmutableDeletion,
) -> MutationContribution {
    let key = match deletion {
        DraftPieceImmutableDeletion::Root => DeletedImmutable::Root(root.key()),
        DraftPieceImmutableDeletion::RootNode => {
            DeletedImmutable::Sequence(DraftPieceRecordKeyV1::new(
                root.key().draft_id(),
                root.root_node().expect("fixture sequence root exists"),
            ))
        }
        DraftPieceImmutableDeletion::SequenceDescendant => {
            let root_id = root.root_node().expect("fixture sequence root exists");
            let root_record = storage
                .point::<DraftPieceNodesFamily>(
                    store,
                    DraftPieceRecordKeyV1::new(root.key().draft_id(), root_id),
                    SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
                )
                .expect("fixture sequence root reads")
                .expect("fixture sequence root exists");
            assert!(root_record.height() >= 2);
            DeletedImmutable::Sequence(DraftPieceRecordKeyV1::new(
                root.key().draft_id(),
                root_record.children()[0].id(),
            ))
        }
        DraftPieceImmutableDeletion::Settlement => {
            return storage.handle.contribution(
                storage.revision(store).expect("fixture revision reads"),
                DeleteImmutable(DeletedImmutable::Settlement(
                    DraftPieceSettlementKeyV1::new(
                        root.key().draft_id(),
                        root.key()
                            .session_id()
                            .expect("fixture root is session-qualified"),
                        root.key().operation_id(),
                    ),
                )),
            );
        }
    };
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DeleteImmutable(key),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceDescendantCorruption {
    Digest,
    Aggregate,
    NewlineAggregate,
    LogicalLineAggregate,
    Envelope,
    Height,
    Shape,
    AggregateOverflow,
    EnvelopeGap,
    EnvelopeOverlap,
    EnvelopeOutOfParent,
    DuplicateMarkerOrderSlot,
    TextBearingMarkerOrderSlot,
}

pub fn inject_draft_piece_descendant_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    target: DraftPieceDescendantTarget,
    corruption: DraftPieceDescendantCorruption,
) -> MutationContribution {
    let replacement = match target {
        DraftPieceDescendantTarget::Sequence => inject_sequence(store, storage, root, corruption),
        DraftPieceDescendantTarget::MarkerIndex => inject_index(store, storage, root, corruption),
    };
    storage.handle.contribution(
        storage
            .revision(store)
            .expect("fixture domain revision reads"),
        DescendantReplacement(replacement),
    )
}

pub fn inject_draft_piece_settlement_closure_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftPieceSettlementKeyV1,
) -> MutationContribution {
    let settlement = storage
        .point::<DraftPieceSettlementsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero"),
        )
        .expect("fixture settlement reads")
        .expect("fixture settlement exists");
    let closure = match settlement.closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => {
            DraftPieceSettlementClosureV1::Committed(DraftPieceCommittedAdoptionV1::new(
                adoption.predecessor_session().clone(),
                adoption.adopted_session().clone(),
                DraftPieceRootRecordV1::new(settlement.predecessor_root()),
                adoption.predecessor_history().clone(),
                adoption.transition().clone(),
                adoption.adopted_history().clone(),
            ))
        }
        DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            DraftPieceSettlementClosureV1::Noncommit(DraftPieceNoncommitClosureV1::new(
                noncommit.observed_session().clone(),
                noncommit.observed_history().clone(),
                if noncommit.proposed_successor().is_some() {
                    None
                } else {
                    Some(settlement.predecessor_root())
                },
            ))
        }
    };
    let corrupted = DraftPieceSettlementV1::new_boxed(
        settlement.key(),
        settlement.proposal_digest(),
        settlement.predecessor_candidate_generation(),
        settlement.predecessor_root(),
        settlement.predecessor_history(),
        settlement.fragment_count(),
        settlement.fragment_chain(),
        settlement.predecessor_caret(),
        settlement.predecessor_selection(),
        settlement.caret(),
        settlement.selection(),
        settlement.build_digest(),
        settlement.canonical_header().to_vec(),
        settlement.terminal_source().cloned(),
        settlement.terminal_receipt(),
        settlement.outcome().clone(),
        Box::new(closure),
    );
    storage.handle.contribution(
        storage
            .revision(store)
            .expect("fixture domain revision reads"),
        DescendantReplacement(Replacement::Settlement(key, corrupted)),
    )
}

pub fn draft_piece_position_record_count(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    position: DraftCompositePositionV1,
) -> Result<u64, DraftPiecePrepareErrorV1> {
    validate_position_record_count(&storage, store, root, position)
}

fn inject_sequence(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    corruption: DraftPieceDescendantCorruption,
) -> Replacement {
    let root_id = root.root_node().expect("fixture sequence root exists");
    let root_record = storage
        .point::<DraftPieceNodesFamily>(
            store,
            DraftPieceRecordKeyV1::new(root.key().draft_id(), root_id),
            SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
        )
        .expect("fixture sequence root reads")
        .expect("fixture sequence root record exists");
    assert!(root_record.height() >= 2);
    if corruption == DraftPieceDescendantCorruption::TextBearingMarkerOrderSlot {
        let mut children = root_record.children().to_vec();
        let first = children[0];
        let second = children[1];
        let (
            DraftCompositeSearchKeyV1::Marker {
                anchor, order_key, ..
            },
            DraftCompositeSearchKeyV1::Marker { marker_id, .. },
        ) = (first.last(), second.first())
        else {
            panic!("text-bearing marker-order corruption requires marker child boundary")
        };
        assert!(first.logical_utf8_bytes() != 0);
        children[1] = DraftPieceChildV1::new(
            second.id(),
            second.digest(),
            second.logical_utf8_bytes(),
            second.newline_count(),
            second.logical_line_count(),
            second.piece_count(),
            second.marker_count(),
            second.marker_digest(),
            DraftCompositeSearchKeyV1::Marker {
                anchor,
                order_key,
                marker_id,
            },
            second.last(),
        );
        let key = DraftPieceRecordKeyV1::new(root.key().draft_id(), root_id);
        return Replacement::Sequence(
            key,
            DraftPieceNodeRecordV1::new(
                key,
                root_record.height(),
                children.clone(),
                node_digest(root_record.height(), &children),
            ),
        );
    }
    let descendant = root_record.children()[0];
    let key = DraftPieceRecordKeyV1::new(root.key().draft_id(), descendant.id());
    let record = storage
        .point::<DraftPieceNodesFamily>(
            store,
            key,
            SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
        )
        .expect("fixture descendant reads")
        .expect("fixture descendant exists");
    let mut children = record.children().to_vec();
    let mut height = record.height();
    let digest = match corruption {
        DraftPieceDescendantCorruption::Digest => DraftPieceDigestV1::from_bytes([0xD1; 32]),
        DraftPieceDescendantCorruption::Aggregate => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count(),
                child.logical_line_count(),
                child.piece_count() + 1,
                child.marker_count(),
                child.marker_digest(),
                child.first(),
                child.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::NewlineAggregate => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count() + 1,
                child.logical_line_count(),
                child.piece_count(),
                child.marker_count(),
                child.marker_digest(),
                child.first(),
                child.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::LogicalLineAggregate => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count(),
                child.logical_line_count() + 1,
                child.piece_count(),
                child.marker_count(),
                child.marker_digest(),
                child.first(),
                child.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::Envelope => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count(),
                child.logical_line_count(),
                child.piece_count(),
                child.marker_count(),
                child.marker_digest(),
                DraftCompositeSearchKeyV1::AfterMarkers(child.first().anchor().saturating_add(1)),
                child.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::Height => {
            height += 1;
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::Shape => {
            children.truncate(1);
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::AggregateOverflow => {
            let first = children[0];
            children[0] = DraftPieceChildV1::new(
                first.id(),
                first.digest(),
                u64::MAX,
                0,
                1,
                first.piece_count(),
                first.marker_count(),
                first.marker_digest(),
                first.first(),
                DraftCompositeSearchKeyV1::AfterMarkers(u64::MAX),
            );
            let second = children[1];
            children[1] = DraftPieceChildV1::new(
                second.id(),
                second.digest(),
                1,
                0,
                1,
                second.piece_count(),
                second.marker_count(),
                second.marker_digest(),
                second.first(),
                DraftCompositeSearchKeyV1::AfterMarkers(1),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeGap => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count(),
                child.logical_line_count(),
                child.piece_count(),
                child.marker_count(),
                child.marker_digest(),
                DraftCompositeSearchKeyV1::BeforeMarkers(1),
                child.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeOverlap => {
            let first = children[0];
            let second = children[1];
            children[1] = DraftPieceChildV1::new(
                second.id(),
                second.digest(),
                second.logical_utf8_bytes(),
                second.newline_count(),
                second.logical_line_count(),
                second.piece_count(),
                second.marker_count(),
                second.marker_digest(),
                first.last(),
                second.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeOutOfParent => {
            let child = children[0];
            children[0] = DraftPieceChildV1::new(
                child.id(),
                child.digest(),
                child.logical_utf8_bytes(),
                child.newline_count(),
                child.logical_line_count(),
                child.piece_count(),
                child.marker_count(),
                child.marker_digest(),
                child.first(),
                DraftCompositeSearchKeyV1::AfterMarkers(
                    child.logical_utf8_bytes().saturating_add(1),
                ),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::DuplicateMarkerOrderSlot => {
            let first = children[0];
            let second = children[1];
            let (
                DraftCompositeSearchKeyV1::Marker {
                    anchor, order_key, ..
                },
                DraftCompositeSearchKeyV1::Marker { marker_id, .. },
            ) = (first.last(), second.first())
            else {
                panic!("duplicate marker-order corruption requires adjacent marker children")
            };
            children[1] = DraftPieceChildV1::new(
                second.id(),
                second.digest(),
                second.logical_utf8_bytes(),
                second.newline_count(),
                second.logical_line_count(),
                second.piece_count(),
                second.marker_count(),
                second.marker_digest(),
                DraftCompositeSearchKeyV1::Marker {
                    anchor,
                    order_key,
                    marker_id,
                },
                second.last(),
            );
            node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::TextBearingMarkerOrderSlot => unreachable!(),
    };
    Replacement::Sequence(
        key,
        DraftPieceNodeRecordV1::new(key, height, children, digest),
    )
}

fn inject_index(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
    corruption: DraftPieceDescendantCorruption,
) -> Replacement {
    let root_id = root
        .marker_index_root()
        .expect("fixture marker-index root exists");
    let root_key = DraftMarkerIdentityRecordKeyV1::new(
        root.key().draft_id(),
        DraftMarkerIdentityRecordKindV1::Internal,
        root_id,
    );
    let root_record = storage
        .point::<DraftMarkerIdentityIndexFamily>(
            store,
            root_key,
            SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
        )
        .expect("fixture marker-index root reads")
        .expect("fixture marker-index root exists");
    assert!(root_record.height() >= 2);
    let descendant = root_record.children().expect("fixture root is internal")[0];
    let key = DraftMarkerIdentityRecordKeyV1::new(
        root.key().draft_id(),
        DraftMarkerIdentityRecordKindV1::Internal,
        descendant.id(),
    );
    let record = storage
        .point::<DraftMarkerIdentityIndexFamily>(
            store,
            key,
            SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
        )
        .expect("fixture marker-index descendant reads")
        .expect("fixture marker-index descendant exists");
    let mut children = record
        .children()
        .expect("fixture descendant is internal")
        .to_vec();
    let mut height = record.height();
    let digest = match corruption {
        DraftPieceDescendantCorruption::Digest => DraftPieceDigestV1::from_bytes([0xD2; 32]),
        DraftPieceDescendantCorruption::Aggregate => {
            let child = children[0];
            children[0] = DraftMarkerIdentityChildV1::new(
                child.id(),
                child.digest(),
                child.record_count() + 1,
                child.first(),
                child.last(),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::NewlineAggregate
        | DraftPieceDescendantCorruption::LogicalLineAggregate => {
            panic!("text-summary corruption requires the sequence target")
        }
        DraftPieceDescendantCorruption::Envelope => {
            let child = children[0];
            children[0] = DraftMarkerIdentityChildV1::new(
                child.id(),
                child.digest(),
                child.record_count(),
                beryl_model::SyndicDraftMarkerId::from_bytes([0xFF; 16]),
                child.first(),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::Height => {
            height += 1;
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::Shape => {
            children.truncate(1);
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::AggregateOverflow => {
            let child = children[0];
            children[0] = DraftMarkerIdentityChildV1::new(
                child.id(),
                child.digest(),
                u64::MAX,
                child.first(),
                child.last(),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeGap => {
            let child = children[0];
            children[0] = DraftMarkerIdentityChildV1::new(
                child.id(),
                child.digest(),
                child.record_count(),
                beryl_model::SyndicDraftMarkerId::from_bytes([0xFE; 16]),
                beryl_model::SyndicDraftMarkerId::from_bytes([0x01; 16]),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeOverlap => {
            let first = children[0];
            let second = children[1];
            children[1] = DraftMarkerIdentityChildV1::new(
                second.id(),
                second.digest(),
                second.record_count(),
                first.last(),
                second.last(),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::EnvelopeOutOfParent => {
            let child = children[0];
            children[0] = DraftMarkerIdentityChildV1::new(
                child.id(),
                child.digest(),
                child.record_count(),
                beryl_model::SyndicDraftMarkerId::from_bytes([0xFF; 16]),
                beryl_model::SyndicDraftMarkerId::from_bytes([0xFF; 16]),
            );
            index_node_digest(height, &children)
        }
        DraftPieceDescendantCorruption::DuplicateMarkerOrderSlot
        | DraftPieceDescendantCorruption::TextBearingMarkerOrderSlot => {
            panic!("duplicate marker-order corruption requires the sequence target")
        }
    };
    Replacement::Index(
        key,
        DraftMarkerIdentityRecordV1::Internal {
            key,
            height,
            children,
            digest,
        },
    )
}

#[derive(Clone)]
enum Replacement {
    Sequence(DraftPieceRecordKeyV1, DraftPieceNodeRecordV1),
    Index(DraftMarkerIdentityRecordKeyV1, DraftMarkerIdentityRecordV1),
    Root(DraftPieceRootKeyV1, DraftPieceRootRecordV1),
    Build(DraftPieceSettlementKeyV1, DraftPieceBuildRecordV1),
    Fragment(DraftPieceBuildFragmentKeyV1, DraftPieceBuildFragmentV1),
    Progress(
        DraftPieceBuildProgressReceiptKeyV1,
        DraftPieceBuildProgressReceiptV1,
    ),
    Session(
        DraftEditorCandidateSessionRecordKeyV1,
        DraftEditorCandidateSessionRecordV1,
    ),
    Settlement(DraftPieceSettlementKeyV1, DraftPieceSettlementV1),
}

#[derive(Clone)]
struct CoordinatedStageReplacement {
    key: DraftPieceSettlementKeyV1,
    build: DraftPieceBuildRecordV1,
    receipt: DraftPieceBuildProgressReceiptV1,
    session_key: DraftEditorCandidateSessionRecordKeyV1,
    session: DraftEditorCandidateSessionRecordV1,
}

#[derive(Clone)]
struct DescendantReplacement(Replacement);

#[derive(Clone, Copy)]
enum DeletedImmutable {
    Root(DraftPieceRootKeyV1),
    Sequence(DraftPieceRecordKeyV1),
    Build(DraftPieceSettlementKeyV1),
    Settlement(DraftPieceSettlementKeyV1),
    Progress(DraftPieceBuildProgressReceiptKeyV1),
}

#[derive(Clone)]
struct DeleteImmutable(DeletedImmutable);

impl DomainMutation<SyndicDomain> for DeleteImmutable {
    type Error = super::FixtureMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.0 {
            DeletedImmutable::Root(_) => reservation.reserve_records::<DraftPieceRootsCodec>(1)?,
            DeletedImmutable::Sequence(_) => {
                reservation.reserve_records::<DraftPieceNodesCodec>(1)?
            }
            DeletedImmutable::Build(_) => {
                reservation.reserve_records::<DraftPieceBuildsCodec>(1)?
            }
            DeletedImmutable::Settlement(_) => {
                reservation.reserve_records::<DraftPieceSettlementsCodec>(1)?
            }
            DeletedImmutable::Progress(_) => {
                reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.0 {
            DeletedImmutable::Root(key) => builder.delete::<DraftPieceRootsCodec>(key)?,
            DeletedImmutable::Sequence(key) => builder.delete::<DraftPieceNodesCodec>(key)?,
            DeletedImmutable::Build(key) => builder.delete::<DraftPieceBuildsCodec>(key)?,
            DeletedImmutable::Settlement(key) => {
                builder.delete::<DraftPieceSettlementsCodec>(key)?
            }
            DeletedImmutable::Progress(key) => {
                builder.delete::<DraftPieceBuildProgressCodec>(key)?
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DescendantReplacement {
    type Error = super::FixtureMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.0 {
            Replacement::Sequence(..) => reservation.reserve_records::<DraftPieceNodesCodec>(1)?,
            Replacement::Index(..) => {
                reservation.reserve_records::<DraftMarkerIdentityIndexCodec>(1)?
            }
            Replacement::Root(..) => reservation.reserve_records::<DraftPieceRootsCodec>(1)?,
            Replacement::Build(..) => reservation.reserve_records::<DraftPieceBuildsCodec>(1)?,
            Replacement::Fragment(..) => {
                reservation.reserve_records::<DraftPieceBuildFragmentsCodec>(1)?
            }
            Replacement::Progress(..) => {
                reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?
            }
            Replacement::Session(..) => {
                reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?
            }
            Replacement::Settlement(..) => {
                reservation.reserve_records::<DraftPieceSettlementsCodec>(1)?
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.0 {
            Replacement::Sequence(key, record) => {
                builder.put::<DraftPieceNodesCodec>(key, record)?
            }
            Replacement::Index(key, record) => {
                builder.put::<DraftMarkerIdentityIndexCodec>(key, record)?
            }
            Replacement::Root(key, record) => builder.put::<DraftPieceRootsCodec>(key, record)?,
            Replacement::Build(key, record) => builder.put::<DraftPieceBuildsCodec>(key, record)?,
            Replacement::Fragment(key, record) => {
                builder.put::<DraftPieceBuildFragmentsCodec>(key, record)?
            }
            Replacement::Progress(key, record) => {
                builder.put::<DraftPieceBuildProgressCodec>(key, record)?
            }
            Replacement::Session(key, record) => {
                builder.put::<DraftEditorCandidateSessionsCodec>(key, record)?
            }
            Replacement::Settlement(key, record) => {
                builder.put::<DraftPieceSettlementsCodec>(key, record)?
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for CoordinatedStageReplacement {
    type Error = crate::SyndicMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
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
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<DraftPieceBuildsCodec>(&self.key, &self.build)?;
        builder.put::<DraftPieceBuildProgressCodec>(&self.receipt.key(), &self.receipt)?;
        builder.put::<DraftEditorCandidateSessionsCodec>(&self.session_key, &self.session)?;
        Ok(())
    }
}
