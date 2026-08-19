#[path = "phase143_syndic_composer_mutations/custody_cases.rs"]
mod custody_cases;
#[cfg(feature = "test-faults")]
#[path = "phase143_syndic_composer_mutations/fault_cases.rs"]
mod fault_cases;
#[path = "phase143_syndic_composer_mutations/marker_cases.rs"]
mod marker_cases;
#[path = "phase141_syndic_composer_host/support.rs"]
mod support;
#[path = "phase143_syndic_composer_mutations/validation_cases.rs"]
mod validation_cases;

use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    ComposerHostMutationRequest, ComposerHostMutationStatus, ComposerHostReadTarget,
    ComposerHostRequestId, ComposerHostRequestKey, ComposerHostRequestKind,
    ComposerHostRequestPurpose, SyndicComposerHost,
};
use beryl_home_store::CommandCancellation;
use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, MutationFragment, MutationFragmentPayload, MutationKey, MutationKind,
    MutationPositions, MutationProposal, ObjectChange, ObjectTarget, OperationId, SourcePosition,
    SourceRange, SourceRevision, SuccessorObject,
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
#[cfg(feature = "test-faults")]
use syndic_storage::{DraftByThreadRecord, SelectedPathProof, ThreadRecord};
use syndic_storage::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceErrorReasonV1, DraftPieceMarkerAtV1, DraftPieceMarkerV1, DraftPieceOperationIdV1,
    DraftPieceReplacementV1, DraftPieceTextDemandV1, DraftPieceV1, PreparedDraftPieceEditV1,
    SyndicStorage, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

use support::{committed, current, execute, fixture, populate};

#[test]
fn typing_newline_paste_delete_and_cut_advance_sequential_candidates_only() {
    let (_home, store, storage, thread) = fixture("phase143-text", 1);
    let durable_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 2, 3);

    let first = commit_text(&mut host, &store, base, 4, 0, 0, &["hello", "\n"], 6);
    assert_eq!(candidate_text(storage, &store, first), b"hello\n");
    assert_ne!(first.root(), base.root());

    let pasted = commit_text(
        &mut host,
        &store,
        first,
        5,
        6,
        6,
        &["\u{4e16}", "\u{754c}!"],
        13,
    );
    assert_eq!(
        candidate_text(storage, &store, pasted),
        "hello\n\u{4e16}\u{754c}!".as_bytes()
    );
    assert_ne!(pasted.root(), first.root());

    let deleted = commit_text(&mut host, &store, pasted, 6, 6, 12, &[""], 7);
    assert_eq!(candidate_text(storage, &store, deleted), b"hello\n!");
    assert_ne!(deleted.root(), pasted.root());

    let cut = commit_text(&mut host, &store, deleted, 7, 0, 5, &[""], 2);
    assert_eq!(candidate_text(storage, &store, cut), b"\n!");
    assert_eq!(host.binding(), Some(cut));
    assert_eq!(current(storage, &store, thread), durable_before);
}

#[test]
fn all_five_terminal_outcomes_preserve_or_advance_the_binding_exactly() {
    let (_home, store, storage, thread) = fixture("phase143-five-way", 21);
    let durable_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 22, 23);

    let committed_binding = commit_text(&mut host, &store, base, 24, 0, 0, &["a"], 1);
    assert_ne!(committed_binding, base);

    let rejected_request = text_request(committed_binding, 25, 1, 1, &["bad"], 102);
    host.begin_mutation(&store, rejected_request).unwrap();
    assert_eq!(
        host.execute_mutation(&store, &CommandCancellation::new())
            .unwrap(),
        ComposerHostMutationOutcome::Rejected
    );
    assert_eq!(host.binding(), Some(committed_binding));

    let cancelled_request = text_request(committed_binding, 26, 1, 1, &["cancel"], 7);
    host.begin_mutation(&store, cancelled_request).unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert_eq!(
        host.execute_mutation(&store, &cancelled).unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    assert_eq!(host.binding(), Some(committed_binding));

    let error_request = text_request(committed_binding, 27, 1, 1, &["error"], 6);
    host.begin_mutation(&store, error_request.clone()).unwrap();
    let (prepared, _) =
        prepare_text_transaction(storage, &store, committed_binding, 27, 1, 1, "error", 6);
    committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            prepared,
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    assert_eq!(
        host.execute_mutation(&store, &CommandCancellation::new())
            .unwrap(),
        ComposerHostMutationOutcome::Error
    );
    assert_eq!(host.binding(), Some(committed_binding));

    let conflict_request = text_request(committed_binding, 28, 1, 1, &["mine"], 5);
    host.begin_mutation(&store, conflict_request).unwrap();
    let drifted = external_commit_text(storage, &store, committed_binding, 29, 1, 1, "other", 6);
    assert_ne!(drifted, committed_binding.candidate());
    assert_eq!(
        host.execute_mutation(&store, &CommandCancellation::new())
            .unwrap(),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(host.binding(), Some(committed_binding));
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Unavailable)
    );
    let retained = host.retained_mutation_intent().unwrap();
    assert_eq!(retained.binding(), committed_binding);
    assert_eq!(retained.operation_id(), operation_id(28));
    assert_eq!(
        retained.proposal().replacement(),
        range(source_position(1), source_position(1))
    );
    assert_eq!(retained.replacements().len(), 1);
    assert!(retained.targets().is_empty());
    assert!(matches!(
        host.release(),
        Err(ComposerHostError::MutationUnavailable)
    ));
    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 30, 31),
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::MutationUnavailable)
    ));
    assert_eq!(current(storage, &store, thread), durable_before);
}

#[test]
fn wrong_home_and_same_operation_collision_never_advance_the_active_binding() {
    let (_home, store, storage, thread) = fixture("phase143-collision", 71);
    let (_other_home, other_store, _other_storage, _other_thread) =
        fixture("phase143-other-home", 72);
    let (mut host, base) = activated(storage, &store, thread, 73, 74);
    assert!(matches!(
        host.begin_mutation(&other_store, text_request(base, 75, 0, 0, &["x"], 1)),
        Err(ComposerHostError::ForeignHome { .. })
    ));

    let first = commit_text(&mut host, &store, base, 75, 0, 0, &["x"], 1);
    assert!(matches!(
        host.begin_mutation(&store, text_request(first, 75, 1, 1, &["x"], 2)),
        Err(ComposerHostError::MutationIdentityCollision)
    ));
    assert_eq!(host.binding(), Some(first));
    assert_eq!(candidate_text(storage, &store, first), b"x");
}

fn activated(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> (SyndicComposerHost, ComposerHostBinding) {
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(
            store,
            activation(thread, session, operation),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("activation did not yield a composer binding");
    };
    (host, binding)
}

fn activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        NonZeroU64::MIN,
        None,
        Box::new([]),
    )
}

fn commit_text(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    seed: u8,
    start: u64,
    end: u64,
    fragments: &[&str],
    caret: u64,
) -> ComposerHostBinding {
    commit_request(
        host,
        store,
        text_request(binding, seed, start, end, fragments, caret),
    )
}

fn commit_request(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    request: ComposerHostMutationRequest,
) -> ComposerHostBinding {
    host.begin_mutation(store, request).unwrap();
    let outcome = host
        .execute_mutation(store, &CommandCancellation::new())
        .unwrap();
    let ComposerHostMutationOutcome::Committed { binding, positions } = outcome else {
        panic!("mutation did not commit: {outcome:?}");
    };
    assert_eq!(positions.caret(), positions.selection_head());
    binding
}

fn text_request(
    binding: ComposerHostBinding,
    seed: u8,
    start: u64,
    end: u64,
    fragments: &[&str],
    caret: u64,
) -> ComposerHostMutationRequest {
    let mut inserted_offset = 0_u64;
    let payloads = fragments
        .iter()
        .map(|text| {
            let payload = MutationFragmentPayload::Utf8 {
                inserted_offset,
                text: (*text).to_owned(),
            };
            inserted_offset += text.len() as u64;
            payload
        })
        .collect();
    mutation_request(
        binding,
        seed,
        MutationKind::Edit,
        range(source_position(start), source_position(end)),
        payloads,
        MutationPositions::collapsed(source_position(caret)),
        Vec::new(),
    )
}

fn mutation_request(
    binding: ComposerHostBinding,
    seed: u8,
    kind: MutationKind,
    replacement: SourceRange,
    payloads: Vec<MutationFragmentPayload>,
    positions: MutationPositions,
    metadata: Vec<ComposerHostImageMarkerMetadata>,
) -> ComposerHostMutationRequest {
    let key = mutation_key(binding, seed);
    let proposal = MutationProposal::new(key, kind, replacement, 0);
    let mut fragments: Vec<_> = payloads
        .into_iter()
        .enumerate()
        .map(|(ordinal, payload)| MutationFragment::new(key, ordinal, payload))
        .collect();
    fragments.push(MutationFragment::new(
        key,
        fragments.len(),
        MutationFragmentPayload::Terminal {
            intended: positions,
        },
    ));
    ComposerHostMutationRequest::new(
        binding,
        proposal,
        operation_id(seed),
        fragments.into_boxed_slice(),
        metadata.into_boxed_slice(),
    )
}

fn keyed_text_request(
    binding: ComposerHostBinding,
    key: MutationKey,
    storage_seed: u8,
) -> ComposerHostMutationRequest {
    let zero = source_position(0);
    ComposerHostMutationRequest::new(
        binding,
        MutationProposal::new(key, MutationKind::Edit, range(zero, zero), 0),
        operation_id(storage_seed),
        vec![
            MutationFragment::new(
                key,
                0,
                MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "x".to_owned(),
                },
            ),
            MutationFragment::new(
                key,
                1,
                MutationFragmentPayload::Terminal {
                    intended: MutationPositions::collapsed(source_position(1)),
                },
            ),
        ]
        .into_boxed_slice(),
        Box::new([]),
    )
}

fn mutation_key(binding: ComposerHostBinding, seed: u8) -> MutationKey {
    MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(u64::from(seed)),
    )
}

fn operation_id(seed: u8) -> DraftPieceOperationIdV1 {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(&u64::from(seed).to_be_bytes());
    DraftPieceOperationIdV1::from_bytes(bytes)
}

fn source_position(offset: u64) -> SourcePosition {
    SourcePosition::new(ByteOffset::new(offset), InlineObjectGap::NoObjects)
}

fn range(start: SourcePosition, end: SourcePosition) -> SourceRange {
    SourceRange::new(start, end).unwrap()
}

fn candidate_text(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
) -> Vec<u8> {
    let result = storage
        .candidate_draft_piece_text_demand(
            store,
            binding.candidate(),
            DraftPieceTextDemandV1::Forward(0),
            65_536,
        )
        .unwrap();
    assert_eq!(result.binding(), binding.candidate());
    result.value().bytes().to_vec()
}

fn prepare_text_transaction(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    seed: u8,
    start: u64,
    end: u64,
    text: &str,
    caret: u64,
) -> (PreparedDraftPieceEditV1, Vec<DraftPieceBuildFragmentV1>) {
    let source = active_session(storage, store, binding.candidate());
    let replacement = DraftPieceReplacementV1::new(
        storage_position(start),
        storage_position(end),
        vec![DraftPieceV1::Text(text.to_owned())],
    );
    let replacements = vec![replacement];
    let header = DraftPieceEditHeaderV1::new(
        source.draft_id(),
        source.session_id(),
        source.newest_candidate_generation(),
        source.newest_root(),
        operation_id(seed),
        storage_position(caret),
        storage_position(caret),
        1,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage.prepare_draft_piece_edit(header, &source).unwrap();
    let fragment = storage
        .prepare_draft_piece_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            replacements.into_iter().next().unwrap(),
        )
        .unwrap();
    (prepared, vec![fragment])
}

fn external_commit_text(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    seed: u8,
    start: u64,
    end: u64,
    text: &str,
    caret: u64,
) -> DraftEditorCandidateActivationBindingV1 {
    let (prepared, fragments) =
        prepare_text_transaction(storage, store, binding, seed, start, end, text, caret);
    committed(execute(
        store,
        storage.begin_draft_piece_edit(storage.revision(store).unwrap(), prepared.clone()),
    ));
    for fragment in fragments {
        committed(execute(
            store,
            storage.stage_draft_piece_fragment(
                storage.revision(store).unwrap(),
                prepared.clone(),
                fragment,
            ),
        ));
    }
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    let session = active_session(storage, store, binding.candidate());
    DraftEditorCandidateActivationBindingV1::from_head(&session)
}

fn active_session(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: DraftEditorCandidateActivationBindingV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, binding.draft_id(), binding.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("candidate session is not active: {other:?}"),
    }
}

fn storage_position(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

fn inline_id(id: SyndicDraftMarkerId) -> InlineObjectId {
    InlineObjectId::new(u128::from_be_bytes(*id.as_bytes()))
}

#[cfg(feature = "test-faults")]
fn arm_mutation_revision_conflict(
    host: &mut SyndicComposerHost,
    thread: beryl_model::SyndicThreadId,
) {
    host.test_arm_mutation_before_execute_fault(move |store, storage| {
        let before = current(storage, store, thread);
        let next_thread_revision = before.thread().revision().checked_next().unwrap();
        let advanced_thread = ThreadRecord::new(
            before.thread().id(),
            SelectedPathProof::new(
                before.thread().committed_tail(),
                next_thread_revision,
                before.thread().selected_path_digest(),
            ),
            before.thread().current_draft_id(),
            before.thread().lineage(),
            before.thread().image_label_frontiers(),
            before.thread().context_owner_id(),
        );
        let advanced_index = DraftByThreadRecord::new(
            before.thread().id(),
            before.draft().id(),
            before.draft().revision(),
            next_thread_revision,
        );
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Thread(advanced_thread)).unwrap();
        batch
            .put(FixtureRecord::DraftByThread(advanced_index))
            .unwrap();
        committed(execute(
            store,
            storage.fixture_contribution(storage.revision(store).unwrap(), batch),
        ));
    });
}
