use super::super::import::model::SealedComposerBasisV1;
use super::super::import::validation::{
    import_roots_are_exact, sealed_progress_is_exact, sealed_transition_is_exact,
};
use super::super::*;
use super::*;

pub(crate) fn recall_build_is_exact(build: &DraftPieceBuildRecordV1) -> bool {
    let DraftPieceBuildFrontierV1::OrdinaryRecall(f) = build.frontier() else {
        return false;
    };
    let complete = f.phase == DraftSealedImportPhaseV1::Complete;
    f.is_locally_exact(build.lifecycle())
        && recall_header_generation(build).is_some()
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
        && build.predecessor_root().key().draft_id() == build.draft_id()
        && build.predecessor_history().root() == build.predecessor_root()
        && build.predecessor_history().candidate_generation()
            == build.predecessor_candidate_generation()
        && build.progress_receipt().key().draft_id() == build.draft_id()
        && build.progress_receipt().key().session_id() == build.session_id()
        && build.progress_receipt().key().operation_id() == build.operation_id()
        && build.progress_receipt().key().transition_ordinal() != 0
        && build.next_record_ordinal() != 0
        && import_roots_are_exact(
            f,
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
pub(crate) fn recall_progress_is_exact(receipt: &DraftPieceBuildProgressReceiptV1) -> bool {
    let DraftPieceBuildFrontierV1::OrdinaryRecall(f) = receipt.frontier() else {
        return false;
    };
    sealed_progress_is_exact(receipt, f)
}
pub(crate) fn recall_transition_is_exact(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> bool {
    let (
        DraftPieceBuildFrontierV1::OrdinaryRecall(a),
        DraftPieceBuildFrontierV1::OrdinaryRecall(b),
    ) = (previous.frontier(), current.frontier())
    else {
        return false;
    };
    sealed_transition_is_exact(previous, current, a, b)
}
pub(crate) fn recall_settlement_is_exact(s: &DraftPieceSettlementV1) -> bool {
    let h = DraftPieceEditHeaderV1::new(
        s.key().draft_id(),
        s.key().session_id(),
        s.predecessor_candidate_generation(),
        s.predecessor_root(),
        s.predecessor_history(),
        s.key().operation_id(),
        s.predecessor_caret(),
        s.predecessor_selection(),
        s.caret(),
        s.selection(),
        0,
        canonical_empty_draft_piece_fragment_chain_v1(),
    );
    if s.proposal_digest() != canonical_proposal_digest(s.canonical_header())
        || s.fragment_count() != 0
        || s.fragment_chain() != canonical_empty_draft_piece_fragment_chain_v1()
    {
        return false;
    }
    if let Some(source) = s.terminal_source() {
        let Some(receipt) = s.terminal_progress_receipt() else {
            return false;
        };
        if !recall_build_is_exact(source)
            || !matches!(
                source.lifecycle(),
                DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
            )
            || source.draft_id() != s.key().draft_id()
            || source.session_id() != s.key().session_id()
            || source.operation_id() != s.key().operation_id()
            || source.predecessor_candidate_generation() != s.predecessor_candidate_generation()
            || source.predecessor_root() != s.predecessor_root()
            || source.predecessor_history() != s.predecessor_history()
            || source.predecessor_caret() != s.predecessor_caret()
            || source.predecessor_selection() != s.predecessor_selection()
            || source.caret() != s.caret()
            || source.selection() != s.selection()
            || source.fragment_count() != s.fragment_count()
            || source.fragment_chain() != s.fragment_chain()
            || source.build_digest() != s.build_digest()
            || source.proposal_digest() != s.proposal_digest()
            || source.canonical_header() != s.canonical_header()
            || receipt.key().draft_id() != s.key().draft_id()
            || receipt.key().session_id() != s.key().session_id()
            || receipt.key().operation_id() != s.key().operation_id()
            || source
                .progress_receipt()
                .key()
                .transition_ordinal()
                .checked_add(1)
                != Some(receipt.key().transition_ordinal())
        {
            return false;
        }
        let lifecycle = match s.outcome() {
            DraftPieceSettlementOutcomeV1::Committed { .. } => {
                DraftPieceBuildLifecycleV1::Committed
            }
            DraftPieceSettlementOutcomeV1::Cancelled => DraftPieceBuildLifecycleV1::Cancelled,
            DraftPieceSettlementOutcomeV1::Error(_) => DraftPieceBuildLifecycleV1::Error,
            _ => return false,
        };
        if !terminal_transition(source, lifecycle)
            .is_ok_and(|(_, terminal)| terminal.reference() == receipt)
        {
            return false;
        }
    }
    match s.closure() {
        DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) => {
            let a = &c.adoption;
            let before = a.predecessor_session();
            let after = a.adopted_session();
            let root = a.adopted_root().reference();
            let append = if matches!(
                a.predecessor_history().reference().key(),
                DraftEditHistoryFrontierKeyV1::Session { .. }
            ) {
                Some(a.predecessor_history().clone())
            } else {
                a.predecessor_history().fork_session(before.session_id())
            };
            let Some(append) = append else {
                return false;
            };
            if !history::ordinary_draft_edit_history_adoption_is_locally_exact(
                &append,
                a.transition(),
                a.adopted_history(),
                s.predecessor_caret(),
                s.predecessor_selection(),
                s.caret(),
                s.selection(),
                s.key().operation_id(),
            ) || before.draft_id() != s.key().draft_id()
                || before.session_id() != s.key().session_id()
                || before.newest_root() != s.predecessor_root()
                || before.newest_history() != s.predecessor_history()
                || before.newest_candidate_generation() != s.predecessor_candidate_generation()
            {
                return false;
            }
            let expected = if before.active_operation().is_some() {
                before.adopted(root, a.adopted_history().reference())
            } else {
                before.adopted_without_custody(root, a.adopted_history().reference())
            };
            if expected.as_ref() != Some(after)
                || a.predecessor_history().reference() != before.newest_history()
                || a.transition().predecessor_root() != before.newest_root()
                || a.transition().successor_root() != root
                || a.transition().operation_id() != s.key().operation_id()
                || a.transition().kind() != DraftEditHistoryTransitionKindV1::OrdinaryEdit
                || a.adopted_history().reference() != after.newest_history()
                || a.adopted_history().journal_head() != Some(a.transition().reference())
                || a.adopted_history().redo_head().is_some()
                || a.transition().before_caret() != s.predecessor_caret()
                || a.transition().before_selection() != s.predecessor_selection()
                || a.transition().after_caret() != s.caret()
                || a.transition().after_selection() != s.selection()
            {
                return false;
            }
            if !matches!(s.outcome(), DraftPieceSettlementOutcomeV1::Committed { candidate_generation, successor, history, caret, selection } if *candidate_generation == after.newest_candidate_generation() && *successor == root && *history == after.newest_history() && *caret == s.caret() && *selection == s.selection())
            {
                return false;
            }
            match &c.purpose {
                DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis) => {
                    if s.terminal_progress_receipt().is_none() {
                        return false;
                    }
                    let Some(source) = s.terminal_source() else {
                        return false;
                    };
                    let DraftPieceBuildFrontierV1::OrdinaryRecall(f) = source.frontier() else {
                        return false;
                    };
                    f.basis == *basis
                        && f.basis.is_locally_exact()
                        && f.basis.thread == before.thread_id()
                        && before.active_operation() == Some(&command::custody(source))
                        && recall_build_is_exact(source)
                        && source.lifecycle() == DraftPieceBuildLifecycleV1::Complete
                        && source.successor() == Some(root)
                        && source.canonical_header() == s.canonical_header()
                        && source
                            .progress_receipt()
                            .key()
                            .transition_ordinal()
                            .checked_add(1)
                            == Some(s.terminal_receipt().key().transition_ordinal())
                }
                DraftComposerRootReplacementPurposeV1::CheckpointRestore {
                    activation,
                    caret,
                    selection,
                } => {
                    activation.draft_id() == s.key().draft_id()
                        && activation.session_id() == s.key().session_id()
                        && activation.root() == root
                        && *caret == s.caret()
                        && *selection == s.selection()
                        && s.terminal_source().is_none()
                        && s.build_digest().is_none()
                        && s.terminal_progress_receipt().is_none()
                        && canonical_recall_header(h, before.session_generation(), &c.purpose)
                            == s.canonical_header()
                }
            }
        }
        DraftPieceSettlementClosureV1::Noncommit(c) => {
            if s.terminal_progress_receipt().is_none() {
                return false;
            }
            let Some(source) = s.terminal_source() else {
                return false;
            };
            recall_build_is_exact(source)
                && source.canonical_header() == s.canonical_header()
                && source
                    .progress_receipt()
                    .key()
                    .transition_ordinal()
                    .checked_add(1)
                    == Some(s.terminal_receipt().key().transition_ordinal())
                && c.observed_session().active_operation().is_none()
                && c.observed_session().newest_root() == source.predecessor_root()
                && c.observed_session().draft_id() == source.draft_id()
                && c.observed_session().session_id() == source.session_id()
                && c.observed_session().newest_candidate_generation()
                    == source.predecessor_candidate_generation()
                && c.observed_session().newest_history() == source.predecessor_history()
                && recall_header_generation(source)
                    .and_then(|g| g.checked_add(s.terminal_receipt().key().transition_ordinal()))
                    == Some(c.observed_session().session_generation())
                && c.observed_history().reference() == source.predecessor_history()
                && c.observed_history().is_locally_valid()
                && c.proposed_successor() == source.successor()
                && c.occupied_identity().is_none()
                && matches!(
                    s.outcome(),
                    DraftPieceSettlementOutcomeV1::Cancelled
                        | DraftPieceSettlementOutcomeV1::Error(_)
                )
        }
        _ => false,
    }
}
