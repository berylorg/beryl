use super::*;

enum PublicationSourceState {
    Occupied(Box<DraftEditorCandidateSessionOpenReceiptV1>),
    Fresh,
}

#[inline(never)]
pub(super) fn prepare(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    source: &CapturedDraftEditorCandidatePublicationSourceV1,
    evidence: DraftEditorCandidatePublicationEvidenceV1,
) -> Result<
    Box<PreparedDraftEditorCandidatePublicationV1>,
    DraftEditorCandidatePublicationCommandErrorV1,
> {
    source
        .storage
        .revision_with_access(store)
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
        || request.candidate_generation() != request.candidate().history().candidate_generation()
        || request.selector().draft_id() != request.candidate().root().key().draft_id()
        || !publication_evidence_is_exact(request)
    {
        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
    }
    let state = validate_source(storage, store, source, request)?;
    assemble(source, request, state)
}

#[inline(never)]
fn validate_source(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    source: &CapturedDraftEditorCandidatePublicationSourceV1,
    request: DraftEditorCandidatePublicationRequestV1,
) -> Result<PublicationSourceState, DraftEditorCandidatePublicationCommandErrorV1> {
    let limit = point_limit();
    let root = storage
        .point_with_access::<DraftPieceRootsFamily>(store, request.candidate().root().key(), limit)?
        .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
    if root.reference() != request.candidate().root()
        || !draft_piece_root_reference_is_locally_exact_v1(root.reference())
    {
        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
    }
    if let Some(record) = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        publication_key(request),
        limit,
    )? {
        let DraftEditorCandidateSessionRecordV1::OpenReceipt(occupied) = record else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        };
        let publication = occupied
            .publication()
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        if !validate_publication_receipt_with_access(storage, store, publication)? {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        return Ok(PublicationSourceState::Occupied(Box::new(occupied)));
    }
    let head = match publication_session_with_access(
        storage,
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
        || !candidate_session_publication_is_exact_with_access(storage, store, &head)?
        || !candidate_session_publication_is_exact_with_access(storage, store, captured)?
        || !captured_adoption_is_exact_with_access(storage, store, captured, source_frontier)?
    {
        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
    }
    Ok(PublicationSourceState::Fresh)
}

#[inline(never)]
fn assemble(
    source: &CapturedDraftEditorCandidatePublicationSourceV1,
    request: DraftEditorCandidatePublicationRequestV1,
    state: PublicationSourceState,
) -> Result<
    Box<PreparedDraftEditorCandidatePublicationV1>,
    DraftEditorCandidatePublicationCommandErrorV1,
> {
    if let PublicationSourceState::Occupied(occupied) = state {
        let occupied = occupied
            .publication()
            .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
        return Ok(Box::new(PreparedDraftEditorCandidatePublicationV1 {
            request,
            canonical_request: canonical_candidate_publication_request_bytes(request),
            source_frontier: occupied.captured_frontier().clone(),
            captured_frontier: occupied.captured_frontier().clone(),
            captured_head: occupied.before_head().clone(),
            initially_absent: false,
        }));
    }
    let source_frontier = &source.source_frontier;
    let captured_frontier = source_frontier
        .publication_snapshot(request.session_id(), request.operation_id())
        .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
    Ok(Box::new(PreparedDraftEditorCandidatePublicationV1 {
        request,
        canonical_request: canonical_candidate_publication_request_bytes(request),
        source_frontier: source_frontier.clone(),
        captured_frontier,
        captured_head: source.captured_head.clone(),
        initially_absent: true,
    }))
}
