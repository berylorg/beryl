use super::*;

pub(super) fn staging_custody(
    begin: DraftMutationBeginV1,
    begin_digest: DraftPieceDigestV1,
    receipt: DraftMutationStagingProgressReceiptReferenceV1,
) -> DraftEditorActiveOperationV1 {
    DraftEditorActiveOperationV1::staging(
        begin.identity().operation_id().as_piece_operation(),
        begin_digest,
        begin.predecessor_candidate_generation(),
        begin.predecessor_root(),
        begin.predecessor_history(),
        receipt,
    )
}

pub(super) fn staging_session_matches_head(
    session: &DraftEditorCandidateSessionV1,
    head: &DraftMutationStagingHeadV1,
) -> bool {
    let begin = head.begin();
    begin
        .session_generation()
        .checked_add(head.receipt().transition_ordinal())
        == Some(session.session_generation())
        && session.draft_id() == begin.identity().draft_id()
        && session.session_id() == begin.identity().session_id()
        && session.newest_candidate_generation() == begin.predecessor_candidate_generation()
        && session.newest_root() == begin.predecessor_root()
        && session.newest_history() == begin.predecessor_history()
        && session.logical_extent() == begin.predecessor_extent()
        && session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
}

pub(super) fn terminal_session_has_same_operation_custody(
    session: &DraftEditorCandidateSessionV1,
    identity: DraftMutationStagingIdentityV1,
) -> bool {
    session.draft_id() == identity.draft_id()
        && session.session_id() == identity.session_id()
        && session.active_operation().is_some_and(|operation| {
            operation.operation_id() == identity.operation_id().as_piece_operation()
        })
}

pub(super) fn terminal_lifecycle(
    evidence: DraftMutationStagingTerminalEvidenceV1,
) -> DraftMutationStagingLifecycleV1 {
    match evidence {
        DraftMutationStagingTerminalEvidenceV1::Rejected { .. } => {
            DraftMutationStagingLifecycleV1::Rejected
        }
        DraftMutationStagingTerminalEvidenceV1::Conflict { .. } => {
            DraftMutationStagingLifecycleV1::Conflict
        }
        DraftMutationStagingTerminalEvidenceV1::Cancelled { .. } => {
            DraftMutationStagingLifecycleV1::Cancelled
        }
        DraftMutationStagingTerminalEvidenceV1::Error { .. } => {
            DraftMutationStagingLifecycleV1::Error
        }
    }
}

fn anchor_matches(
    begin: DraftMutationBeginV1,
    head: Option<&DraftMutationStagingHeadV1>,
    anchor: DraftMutationStagingTerminalAnchorV1,
) -> bool {
    match (head, anchor) {
        (None, DraftMutationStagingTerminalAnchorV1::Begin(identity)) => {
            identity == begin.identity()
        }
        (Some(head), DraftMutationStagingTerminalAnchorV1::Finish(identity)) => {
            identity == head.identity()
        }
        (Some(head), DraftMutationStagingTerminalAnchorV1::Page(key)) => {
            key.identity() == head.identity()
                && match key.lane() {
                    DraftMutationStagingLaneV1::Source => {
                        key.ordinal() == head.source().next_ordinal()
                    }
                    DraftMutationStagingLaneV1::Proposal => {
                        key.ordinal() == head.proposal().next_ordinal()
                    }
                }
        }
        _ => false,
    }
}

fn occupied_key_matches(
    identity: DraftMutationStagingIdentityV1,
    key: DraftMutationStagingOccupiedKeyV1,
) -> bool {
    match key {
        DraftMutationStagingOccupiedKeyV1::Head(value) => value == identity,
        DraftMutationStagingOccupiedKeyV1::Page(value) => value.identity() == identity,
        DraftMutationStagingOccupiedKeyV1::Progress(value) => value.identity() == identity,
        DraftMutationStagingOccupiedKeyV1::Build(value)
        | DraftMutationStagingOccupiedKeyV1::Settlement(value) => {
            value.draft_id() == identity.draft_id()
                && value.session_id() == identity.session_id()
                && value.operation_id() == identity.operation_id().as_piece_operation()
        }
        DraftMutationStagingOccupiedKeyV1::CandidateRoot(value) => {
            value
                == DraftPieceRootKeyV1::editor_candidate(
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                )
        }
    }
}

fn error_matches(
    begin: DraftMutationBeginV1,
    head: Option<&DraftMutationStagingHeadV1>,
    error: DraftMutationStagingErrorEvidenceV1,
) -> bool {
    match error {
        DraftMutationStagingErrorEvidenceV1::Operational { anchor, .. } => {
            anchor_matches(begin, head, anchor)
        }
        DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
            key,
            stored_digest,
            requested_digest,
            stored,
            requested,
            ..
        } => {
            head.is_some()
                && occupied_key_matches(begin.identity(), key)
                && stored_digest != requested_digest
                && stored != requested
        }
    }
}

pub(super) fn terminal_evidence_matches(
    begin: DraftMutationBeginV1,
    head: Option<&DraftMutationStagingHeadV1>,
    session: &DraftEditorCandidateSessionV1,
    evidence: DraftMutationStagingTerminalEvidenceV1,
    rejected_request: Option<(DraftMutationStagingTerminalAnchorV1, DraftPieceDigestV1)>,
) -> bool {
    let current = (
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.session_generation(),
    );
    match evidence {
        DraftMutationStagingTerminalEvidenceV1::Rejected {
            anchor,
            digest,
            candidate_generation,
            root,
            history,
            session_revision,
            ..
        } => {
            anchor_matches(begin, head, anchor)
                && rejected_request.map_or_else(
                    || head.is_none() && digest == begin_digest(begin),
                    |expected| expected == (anchor, digest),
                )
                && (candidate_generation, root, history, session_revision) == current
        }
        DraftMutationStagingTerminalEvidenceV1::Conflict {
            expected_generation,
            expected_root,
            expected_history,
            observed_generation,
            observed_root,
            observed_history,
            session_revision,
        } => {
            head.is_none()
                && expected_generation == begin.predecessor_candidate_generation()
                && expected_root == begin.predecessor_root()
                && expected_history == begin.predecessor_history()
                && (
                    observed_generation,
                    observed_root,
                    observed_history,
                    session_revision,
                ) == current
                && (expected_generation, expected_root, expected_history)
                    != (observed_generation, observed_root, observed_history)
        }
        DraftMutationStagingTerminalEvidenceV1::Cancelled {
            request_id,
            source_lifecycle,
            writer_admitted,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            request_id == begin.identity().operation_id()
                && source_lifecycle
                    == head.map_or(DraftMutationStagingLifecycleV1::Receiving, |v| {
                        v.lifecycle()
                    })
                && writer_admitted == head.is_some()
                && (candidate_generation, root, history, session_revision) == current
        }
        DraftMutationStagingTerminalEvidenceV1::Error {
            error,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            error_matches(begin, head, error)
                && (candidate_generation, root, history, session_revision) == current
        }
    }
}

pub(super) fn compared_byte(bytes: &[u8], offset: usize) -> DraftMutationStagingComparedByteV1 {
    bytes.get(offset).copied().map_or(
        DraftMutationStagingComparedByteV1::End,
        DraftMutationStagingComparedByteV1::Byte,
    )
}

fn stored_anchor_matches(
    begin: DraftMutationBeginV1,
    receipt: &DraftMutationStagingProgressReceiptV1,
    anchor: DraftMutationStagingTerminalAnchorV1,
) -> bool {
    if receipt.key().transition_ordinal() == 1 {
        return anchor == DraftMutationStagingTerminalAnchorV1::Begin(begin.identity());
    }
    match anchor {
        DraftMutationStagingTerminalAnchorV1::Finish(identity) => identity == begin.identity(),
        DraftMutationStagingTerminalAnchorV1::Page(key) => {
            key.identity() == begin.identity()
                && match key.lane() {
                    DraftMutationStagingLaneV1::Source => {
                        key.ordinal() == receipt.before_source().next_ordinal()
                    }
                    DraftMutationStagingLaneV1::Proposal => {
                        key.ordinal() == receipt.before_proposal().next_ordinal()
                    }
                }
        }
        _ => false,
    }
}

pub(super) fn stored_terminal_evidence_matches(
    begin: DraftMutationBeginV1,
    receipt: &DraftMutationStagingProgressReceiptV1,
    evidence: DraftMutationStagingTerminalEvidenceV1,
) -> bool {
    let ordinal = receipt.key().transition_ordinal();
    if (ordinal == 1) != receipt.prior().is_none() {
        return false;
    }
    let expected_revision = if ordinal == 1 {
        begin.session_generation()
    } else {
        match begin.session_generation().checked_add(ordinal - 1) {
            Some(v) => v,
            None => return false,
        }
    };
    let exact = |generation, root, history, revision| {
        generation == begin.predecessor_candidate_generation()
            && root == begin.predecessor_root()
            && history == begin.predecessor_history()
            && revision == expected_revision
    };
    match evidence {
        DraftMutationStagingTerminalEvidenceV1::Rejected {
            anchor,
            digest,
            candidate_generation,
            root,
            history,
            session_revision,
            ..
        } => {
            stored_anchor_matches(begin, receipt, anchor)
                && if ordinal == 1 {
                    digest == begin_digest(begin)
                } else {
                    digest != DraftPieceDigestV1::from_bytes([0; 32])
                }
                && exact(candidate_generation, root, history, session_revision)
        }
        DraftMutationStagingTerminalEvidenceV1::Conflict {
            expected_generation,
            expected_root,
            expected_history,
            observed_generation,
            observed_root,
            observed_history,
            session_revision,
        } => {
            ordinal == 1
                && expected_generation == begin.predecessor_candidate_generation()
                && expected_root == begin.predecessor_root()
                && expected_history == begin.predecessor_history()
                && observed_history.candidate_generation() == observed_generation
                && observed_history.root() == observed_root
                && session_revision >= begin.session_generation()
                && (expected_generation, expected_root, expected_history)
                    != (observed_generation, observed_root, observed_history)
        }
        DraftMutationStagingTerminalEvidenceV1::Cancelled {
            request_id,
            source_lifecycle,
            writer_admitted,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            request_id == begin.identity().operation_id()
                && if ordinal == 1 {
                    source_lifecycle == DraftMutationStagingLifecycleV1::Receiving
                        && !writer_admitted
                } else {
                    Some(source_lifecycle) == receipt.before_lifecycle() && writer_admitted
                }
                && exact(candidate_generation, root, history, session_revision)
        }
        DraftMutationStagingTerminalEvidenceV1::Error {
            error,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            (match error {
                DraftMutationStagingErrorEvidenceV1::Operational { anchor, .. } => {
                    stored_anchor_matches(begin, receipt, anchor)
                }
                DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
                    key,
                    stored_digest,
                    requested_digest,
                    stored,
                    requested,
                    ..
                } => {
                    ordinal > 1
                        && occupied_key_matches(begin.identity(), key)
                        && stored_digest != requested_digest
                        && stored != requested
                }
            }) && exact(candidate_generation, root, history, session_revision)
        }
    }
}

pub(super) fn stored_occupied_error_is_exact(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    evidence: DraftMutationStagingTerminalEvidenceV1,
) -> Result<bool, DraftMutationStagingErrorV1> {
    let DraftMutationStagingTerminalEvidenceV1::Error {
        error:
            DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
                key: DraftMutationStagingOccupiedKeyV1::Page(key),
                stored_digest,
                first_difference,
                stored,
                ..
            },
        ..
    } = evidence
    else {
        return Ok(true);
    };
    let Some(page) = storage.draft_mutation_staging_page(store, key)? else {
        return Ok(false);
    };
    let bytes =
        canonical_staging_page_bytes(&page).map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
    let offset =
        usize::try_from(first_difference).map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
    Ok(page.digest() == stored_digest && compared_byte(&bytes, offset) == stored)
}
