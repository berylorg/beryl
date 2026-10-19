use super::super::*;

pub(crate) fn import_settlement_is_exact(s: &DraftPieceSettlementV1) -> bool {
    let Some(source) = s.terminal_source() else {
        return false;
    };
    if !import_build_is_exact(source)
        || !matches!(
            source.lifecycle(),
            DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
        )
        || source.draft_id() != s.key().draft_id()
        || source.session_id() != s.key().session_id()
        || source.operation_id() != s.key().operation_id()
        || source.proposal_digest() != s.proposal_digest()
        || source.canonical_header() != s.canonical_header()
        || source.predecessor_root() != s.predecessor_root()
        || source.predecessor_history() != s.predecessor_history()
        || source.predecessor_candidate_generation() != s.predecessor_candidate_generation()
        || source.fragment_count() != s.fragment_count()
        || source.fragment_chain() != s.fragment_chain()
        || source.predecessor_caret() != s.predecessor_caret()
        || source.predecessor_selection() != s.predecessor_selection()
        || source.caret() != s.caret()
        || source.selection() != s.selection()
        || source.build_digest() != s.build_digest()
        || source
            .progress_receipt()
            .key()
            .transition_ordinal()
            .checked_add(1)
            != Some(s.terminal_receipt().key().transition_ordinal())
        || s.terminal_receipt().key().draft_id() != s.key().draft_id()
        || s.terminal_receipt().key().session_id() != s.key().session_id()
        || s.terminal_receipt().key().operation_id() != s.key().operation_id()
    {
        return false;
    }
    match (s.outcome(), s.closure()) {
        (
            DraftPieceSettlementOutcomeV1::Committed {
                candidate_generation,
                successor,
                history,
                caret,
                selection,
            },
            DraftPieceSettlementClosureV1::SealedImport(c),
        ) => {
            c.is_locally_exact(source)
                && *candidate_generation == c.after_session.newest_candidate_generation()
                && *successor == c.imported_root
                && *history == c.baseline.reference()
                && *caret == s.caret()
                && *selection == s.selection()
        }
        (
            DraftPieceSettlementOutcomeV1::Cancelled
            | DraftPieceSettlementOutcomeV1::Rejected(_)
            | DraftPieceSettlementOutcomeV1::Error(_),
            DraftPieceSettlementClosureV1::Noncommit(c),
        ) => {
            c.observed_session().is_coherent()
                && c.observed_session().active_operation().is_none()
                && c.observed_session().draft_id() == source.draft_id()
                && c.observed_session().session_id() == source.session_id()
                && c.observed_session().newest_root() == source.predecessor_root()
                && c.observed_session().newest_history() == source.predecessor_history()
                && c.observed_session().newest_candidate_generation()
                    == source.predecessor_candidate_generation()
                && import_header_generation(source)
                    .and_then(|g| g.checked_add(s.terminal_receipt().key().transition_ordinal()))
                    == Some(c.observed_session().session_generation())
                && c.observed_history().reference() == source.predecessor_history()
                && c.observed_history().is_locally_valid()
                && c.proposed_successor() == source.successor()
                && c.occupied_identity().is_none()
        }
        _ => false,
    }
}

pub(crate) fn import_build_is_exact(build: &DraftPieceBuildRecordV1) -> bool {
    let DraftPieceBuildFrontierV1::SealedImport(frontier) = build.frontier() else {
        return false;
    };
    let key = build.progress_receipt().key();
    let complete = frontier.phase() == DraftSealedImportPhaseV1::Complete;
    frontier.is_locally_exact(build.lifecycle())
        && import_header_generation(build).is_some()
        && build.proposal_digest() == canonical_proposal_digest(build.canonical_header())
        && build.fragment_count() == 0
        && build.staged_fragment_count() == 0
        && build.fragment_chain() == canonical_empty_draft_piece_fragment_chain_v1()
        && build.staged_fragment_chain() == build.fragment_chain()
        && build.mapping().is_none()
        && build.writer_admission().is_none()
        && build.durable_continuation().is_none()
        && build.marker_effect_continuation()
            == DraftPieceMarkerEffectContinuationV1::canonical_empty()
        && key.draft_id() == build.draft_id()
        && key.session_id() == build.session_id()
        && key.operation_id() == build.operation_id()
        && key.transition_ordinal() != 0
        && build.next_record_ordinal() != 0
        && build.session_id().as_bytes() != &[0; 16]
        && build.operation_id().as_bytes() != &[0; 16]
        && build.predecessor_root().summary().logical_utf8_bytes() == 0
        && build.predecessor_root().summary().marker_count() == 0
        && build.predecessor_root().summary().piece_count() == 0
        && build.predecessor_history().root() == build.predecessor_root()
        && build.predecessor_history().candidate_generation()
            == build.predecessor_candidate_generation()
        && frontier.basis().selector().draft_id() == build.draft_id()
        && frontier.basis().selector().root() == build.predecessor_root()
        && import_roots_are_exact(
            frontier,
            build.working_roots(),
            build.base_frontier(),
            build.successor_frontier(),
        )
        && match (build.successor(), build.build_digest(), complete) {
            (Some(root), Some(digest), true) => {
                root.key()
                    == DraftPieceRootKeyV1::editor_candidate(
                        build.draft_id(),
                        build.session_id(),
                        build.operation_id(),
                    )
                    && build.working_roots() == DraftPieceBuildRootsV1::from_root(root)
                    && digest == draft_piece_build_digest_v1(build.proposal_digest(), root)
            }
            (None, None, false) => true,
            _ => false,
        }
}

pub(crate) fn import_progress_is_exact(receipt: &DraftPieceBuildProgressReceiptV1) -> bool {
    let DraftPieceBuildFrontierV1::SealedImport(frontier) = receipt.frontier() else {
        return false;
    };
    sealed_progress_is_exact(receipt, frontier)
}

pub(crate) fn sealed_progress_is_exact<B: super::model::SealedComposerBasisV1>(
    receipt: &DraftPieceBuildProgressReceiptV1,
    frontier: DraftSealedComposerFrontierV1<B>,
) -> bool {
    let key = receipt.key();
    let complete = frontier.phase() == DraftSealedImportPhaseV1::Complete;
    frontier.is_locally_exact(receipt.lifecycle())
        && key.transition_ordinal() != 0
        && key.session_id().as_bytes() != &[0; 16]
        && key.operation_id().as_bytes() != &[0; 16]
        && match receipt.previous() {
            Some(previous) => {
                previous.key().draft_id() == key.draft_id()
                    && previous.key().session_id() == key.session_id()
                    && previous.key().operation_id() == key.operation_id()
                    && previous.key().transition_ordinal().checked_add(1)
                        == Some(key.transition_ordinal())
            }
            None => {
                key.transition_ordinal() == 1
                    && frontier.decoder == DraftSealedImportDecoderV1::BeforeHeader
                    && receipt.lifecycle() == DraftPieceBuildLifecycleV1::Open
            }
        }
        && receipt.fragment_endpoint().is_none()
        && receipt.durable_continuation().is_none()
        && receipt.mapping().is_none()
        && receipt.writer_admission().is_none()
        && receipt.marker_effect_continuation()
            == DraftPieceMarkerEffectContinuationV1::canonical_empty()
        && receipt.next_record_ordinal() != 0
        && import_roots_are_exact(
            frontier,
            receipt.working_roots(),
            receipt.base_frontier(),
            receipt.successor_frontier(),
        )
        && match (receipt.successor(), receipt.build_digest(), complete) {
            (Some(root), Some(_), true) => {
                root.key()
                    == DraftPieceRootKeyV1::editor_candidate(
                        key.draft_id(),
                        key.session_id(),
                        key.operation_id(),
                    )
                    && receipt.working_roots() == DraftPieceBuildRootsV1::from_root(root)
            }
            (None, None, false) => true,
            _ => false,
        }
}

pub(crate) fn import_transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    let (DraftPieceBuildFrontierV1::SealedImport(a), DraftPieceBuildFrontierV1::SealedImport(b)) =
        (previous.frontier(), current.frontier())
    else {
        return false;
    };
    sealed_transition_is_exact(previous, current, a, b)
}

pub(crate) fn sealed_transition_is_exact<B: super::model::SealedComposerBasisV1 + PartialEq>(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    a: DraftSealedComposerFrontierV1<B>,
    b: DraftSealedComposerFrontierV1<B>,
) -> bool {
    use DraftSealedImportDecoderV1 as Decoder;
    use DraftSealedImportPhaseV1 as Phase;
    if !progress_receipt_is_exact(previous)
        || !progress_receipt_is_exact(current)
        || current.previous() != Some(previous.reference())
        || a.basis != b.basis
        || previous.next_record_ordinal() > current.next_record_ordinal()
        || !matches!(
            previous.lifecycle(),
            DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
        )
    {
        return false;
    }
    if !matches!(
        current.lifecycle(),
        DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
    ) {
        return a == b
            && previous.working_roots() == current.working_roots()
            && previous.next_record_ordinal() == current.next_record_ordinal()
            && previous.successor() == current.successor()
            && previous.build_digest() == current.build_digest();
    }
    let Some(encoded) = b.encoded_cursor.checked_sub(a.encoded_cursor) else {
        return false;
    };
    let Some(logical) = b.logical_bytes.checked_sub(a.logical_bytes) else {
        return false;
    };
    let Some(atoms) = b.atoms.checked_sub(a.atoms) else {
        return false;
    };
    let Some(markers) = b.markers.checked_sub(a.markers) else {
        return false;
    };
    if atoms > 1
        || markers > 1
        || logical > DRAFT_PIECE_TEXT_LEAF_MAX_BYTES as u64
        || b.next_piece < a.next_piece
    {
        return false;
    }
    let same_roots = previous.working_roots() == current.working_roots();
    let reset = a.phase == Phase::Streaming && b.phase == Phase::CrossValidating;
    let folds_valid = if reset {
        a.sequential_digest == a.basis.content().summary().marker_digest()
            && a.maximum_label == a.basis.content().summary().maximum_image_label()
            && a.basis
                .assets()
                .is_none_or(|p| a.ordered_digest == p.ordered_assets().marker_asset_digest())
            && b.sequential_digest == beryl_model::sequential_marker_digest_seed()
            && b.ordered_digest == beryl_model::ordered_marker_asset_digest_seed()
            && b.maximum_label.is_none()
    } else {
        markers == 1
            || b.checked_markers.checked_sub(a.checked_markers) == Some(1)
            || a.sequential_digest == b.sequential_digest
                && a.maximum_label == b.maximum_label
                && a.ordered_digest == b.ordered_digest
    };
    (match (a.phase, b.phase) {
        (Phase::Streaming, Phase::Streaming) => {
            if a.next_leaf_rank != b.next_leaf_rank || a.checked_markers != b.checked_markers {
                return false;
            }
            match (a.decoder, b.decoder) {
                (Decoder::BeforeHeader, Decoder::BetweenAtoms) => {
                    encoded == 9 && logical == 0 && atoms == 0 && markers == 0 && same_roots
                }
                (Decoder::BetweenAtoms, Decoder::TextBody) => {
                    encoded == 9 && logical == 0 && atoms == 0 && markers == 0 && same_roots
                }
                (Decoder::BetweenAtoms, Decoder::BetweenAtoms) => {
                    logical == 0
                        && atoms == 1
                        && ((markers == 0
                            && encoded == 9
                            && same_roots
                            && b.next_piece == a.next_piece)
                            || markers == 1
                                && encoded == 25
                                && b.next_piece == a.next_piece.checked_add(1).unwrap_or(0))
                }
                (Decoder::TextBody, Decoder::TextBody | Decoder::BetweenAtoms) => {
                    encoded == logical
                        && logical > 0
                        && markers == 0
                        && a.text_remaining.checked_sub(logical) == Some(b.text_remaining)
                        && atoms == u64::from(b.decoder == Decoder::BetweenAtoms)
                }
                _ => false,
            }
        }
        (Phase::Streaming, Phase::CrossValidating) => {
            a.decoder == Decoder::BetweenAtoms
                && b.decoder == Decoder::End
                && encoded == 0
                && logical == 0
                && atoms == 0
                && markers == 0
                && same_roots
        }
        (Phase::CrossValidating, Phase::CrossValidating | Phase::Complete) => {
            encoded == 0
                && logical == 0
                && atoms == 0
                && markers == 0
                && same_roots
                && a.next_piece == b.next_piece
                && a.piece_text_offset == b.piece_text_offset
                && (b.next_leaf_rank == a.next_leaf_rank.checked_add(1).unwrap_or(0)
                    && b.checked_markers
                        .checked_sub(a.checked_markers)
                        .is_some_and(|n| n <= 1)
                    || b.phase == Phase::Complete
                        && b.next_leaf_rank == a.next_leaf_rank
                        && b.checked_markers == a.checked_markers)
        }
        _ => false,
    }) && folds_valid
}

pub(crate) fn import_roots_are_exact<B: super::model::SealedComposerBasisV1>(
    frontier: DraftSealedComposerFrontierV1<B>,
    roots: DraftPieceBuildRootsV1,
    base: DraftPieceBuildBoundaryV1,
    successor: DraftPieceBuildBoundaryV1,
) -> bool {
    let sequence = roots.sequence_summary();
    draft_piece_build_roots_are_locally_exact_v1(roots)
        && base == DraftPieceBuildBoundaryV1::new(0, 0)
        && successor == DraftPieceBuildBoundaryV1::new(sequence.piece_count(), 0)
        && sequence.logical_utf8_bytes() == frontier.logical_bytes
        && sequence.marker_count() == frontier.markers
        && roots.marker_index_summary().record_count() == frontier.markers
        && roots.marker_commitment().marker_count() == frontier.markers
        && (frontier.phase == DraftSealedImportPhaseV1::CrossValidating
            || roots.marker_commitment().maximum_image_label() == frontier.maximum_label)
        && frontier.next_leaf_rank <= sequence.piece_count()
        && (frontier.phase != DraftSealedImportPhaseV1::Complete
            || frontier.next_leaf_rank == sequence.piece_count())
        && (frontier.decoder != DraftSealedImportDecoderV1::BeforeHeader
            || sequence.piece_count() == 0)
}
