#![cfg(feature = "test-faults")]

mod support;

#[path = "phase9_binding_invariants/native_turn_count.rs"]
mod native_turn_count;
#[path = "phase9_binding_invariants/recovered_handoff.rs"]
mod recovered_handoff;
#[path = "phase9_binding_invariants/recovered_lineage.rs"]
mod recovered_lineage;
#[path = "phase9_binding_invariants/reopen_binding_records.rs"]
mod reopen_binding_records;
#[path = "phase9_binding_invariants/reopen_correlations.rs"]
mod reopen_correlations;
#[path = "phase9_binding_invariants/retirement.rs"]
mod retirement;
#[path = "phase9_binding_invariants/route_allocator.rs"]
mod route_allocator;
#[path = "phase9_binding_invariants/selected_prefix.rs"]
mod selected_prefix;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, CasTurnId, ExecutionBinding, InputGateRevision, PathFlavor,
    ProjectionRevision, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::*;

use support::semantic::exercise_case;
use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    match execute_outcome(store, contribution) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed binding-invariant command, got {outcome:?}"),
    }
}

fn execute_outcome(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn typed_error(outcome: &CommandOutcome) -> &SyndicMutationError {
    let CommandOutcome::NotCommitted { evidence } = outcome else {
        panic!("expected not-committed Syndic mutation rejection, got {outcome:?}");
    };
    let beryl_home_store::CommandError::ContributorValidation { source, .. } = evidence else {
        panic!("expected Syndic mutation rejection, got {evidence}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([90; 16]),
        RootId::from_bytes([91; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase9-binding-invariants",
        )
        .unwrap(),
    )
}

fn create_thread(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
) {
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
}

fn current_binding_revision(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> BindingRevision {
    storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .head()
        .revision()
}

fn current_gate_revision(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> InputGateRevision {
    storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .revision()
}

fn loaded_generation(process: u64, thread: u64) -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(process).unwrap(),
        CasLoadedThreadGeneration::new(thread).unwrap(),
    )
}

fn same_home_path_records(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    tail: SyndicTurnId,
    digest: beryl_model::SyndicPathDigest,
    history_complete: bool,
    last_activity_at: SyndicTimestamp,
) -> Vec<FixtureRecord> {
    let thread_revision = ThreadRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(Some(tail), thread_revision, digest);
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        draft,
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        ThreadImageLabelFrontiers::empty(),
        None,
    );
    let execution = ThreadExecutionRecord::new(
        thread,
        storage
            .thread_execution(store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
    );
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let history = HistorySummaryRecord::new(
        thread,
        projection_revision,
        thread_revision,
        Some(tail),
        digest,
        history_complete,
        last_activity_at,
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);
    vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            Some(tail),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]
}

fn fault_pending_path(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_byte: u8,
    non_root: bool,
) -> (
    SyndicThreadId,
    Option<SyndicTurnId>,
    SyndicTurnId,
    SelectedPathProof,
) {
    let thread = id(thread_byte);
    let draft = draft_id(thread_byte.wrapping_add(10));
    create_thread(store, storage, thread, draft);
    let root = SyndicTurnId::from_bytes([thread_byte.wrapping_add(1); 16]);
    let root_digest = root_turn_chain_digest(root);
    let (tail, digest, depth, parent) = if non_root {
        let child = SyndicTurnId::from_bytes([thread_byte.wrapping_add(2); 16]);
        (
            child,
            child_turn_chain_digest(child, root, root_digest),
            TurnDepth::new(2).unwrap(),
            ConversationParent::Turn(root),
        )
    } else {
        (
            root,
            root_digest,
            TurnDepth::FIRST,
            ConversationParent::Root,
        )
    };
    let selected = SelectedPathProof::new(Some(tail), ThreadRevision::new(1).unwrap(), digest);
    let mut records = same_home_path_records(
        store,
        storage,
        thread,
        draft,
        tail,
        digest,
        false,
        timestamp(4),
    );
    if non_root {
        records.extend([
            FixtureRecord::Turn(TurnRecord::new(
                root,
                thread,
                TurnKind::OrdinaryUser,
                ConversationParent::Root,
                None,
                TurnDepth::FIRST,
                root_digest,
                timestamp(2),
            )),
            FixtureRecord::TurnState(fixture_turn_state(
                root,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                0,
                timestamp(3),
            )),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    root,
                    SourceEventSequence::FIRST,
                    None,
                    SourceEventPayload::TurnEnded(
                        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                    ),
                )
                .unwrap(),
            ),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(root, tail, depth, digest)),
        ]);
    }
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            tail,
            thread,
            TurnKind::OrdinaryUser,
            parent,
            non_root.then_some(root),
            depth,
            digest,
            timestamp(4),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            tail,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            timestamp(4),
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(1).unwrap(),
                InputGateState::PendingTurn(tail),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
    ]);
    let transcript_path = if non_root {
        vec![
            (
                root,
                root_digest,
                TurnLifecycle::Interrupted,
                1,
                timestamp(3),
            ),
            (tail, digest, TurnLifecycle::Pending, 0, timestamp(4)),
        ]
    } else {
        vec![(tail, digest, TurnLifecycle::Pending, 0, timestamp(4))]
    };
    records.extend(item_free_transcript_build_records(
        thread,
        ThreadRevision::new(1).unwrap(),
        &transcript_path,
    ));
    commit(store, storage, batch(records));
    (thread, non_root.then_some(root), tail, selected)
}

#[allow(clippy::too_many_arguments)]
fn valid_request_with_count(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
    represented: CasRepresentedPrefixProof,
    native_turn_count: CasNativeTurnCount,
    lineage: CasLineageProof,
) -> PublishValidBinding {
    let execution = storage
        .thread_execution(store, thread, point_limit())
        .unwrap()
        .expect("binding fixture thread must retain canonical execution")
        .execution()
        .clone();
    PublishValidBinding::new(
        thread,
        current_binding_revision(store, storage, thread),
        selected,
        execution,
        cas_thread,
        represented,
        native_turn_count,
        test_tool_profile(),
        lineage,
    )
}

fn valid_request(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
) -> PublishValidBinding {
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    valid_request_with_count(
        store,
        storage,
        thread,
        selected,
        cas_thread,
        represented,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    )
}

fn publish_valid(store: &HomeStore, storage: SyndicStorage, request: PublishValidBinding) {
    execute(
        store,
        storage.publish_valid_binding(storage.revision(store).unwrap(), request),
    );
}

#[test]
fn immutable_binding_history_and_current_head_survive_reopen() {
    let home = TestHome::new("phase9-immutable-binding-history");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let thread = id(30);
    let expected: Vec<_> = (1..=4)
        .map(|revision| {
            storage
                .binding(
                    &store,
                    thread,
                    BindingRevision::new(revision).unwrap(),
                    point_limit(),
                )
                .unwrap()
                .unwrap()
        })
        .collect();
    let head = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.binding(), expected.last().unwrap());
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    for (index, expected) in expected.iter().enumerate() {
        assert_eq!(
            storage
                .binding(
                    &reopened,
                    thread,
                    BindingRevision::new(index as u64 + 1).unwrap(),
                    point_limit(),
                )
                .unwrap()
                .as_ref(),
            Some(expected)
        );
    }
    assert_eq!(
        storage
            .current_binding(&reopened, thread, point_limit())
            .unwrap()
            .unwrap()
            .binding(),
        expected.last().unwrap()
    );
    reopened.close().unwrap();
}

#[test]
fn cas_thread_reservation_survives_stale_unbound_history_and_reopen() {
    let home = TestHome::new("phase9-permanent-cas-thread-reservation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let owner = id(70);
    let contender = id(80);
    create_thread(&store, storage, owner, draft_id(71));
    create_thread(&store, storage, contender, draft_id(81));
    let owner_selected = storage
        .current_binding(&store, owner, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .selected_path();
    let contender_selected = storage
        .current_binding(&store, contender, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .selected_path();
    let cas_thread = CasThreadId::new("permanently-reserved-cas").unwrap();
    let stale = StaleCasBinding::new(
        execution_binding(),
        cas_thread.clone(),
        None,
        None,
        None,
        None,
        None,
        "abandoned canonical projection",
        timestamp(2),
    )
    .unwrap();
    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                owner,
                current_binding_revision(&store, storage, owner),
                owner_selected,
                stale,
            ),
        ),
    );

    let contender_request = || {
        valid_request(
            &store,
            storage,
            contender,
            contender_selected,
            cas_thread.clone(),
        )
    };
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), contender_request()),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::CasThreadOwnershipConflict
    ));
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                owner,
                current_binding_revision(&store, storage, owner),
                owner_selected,
                "projection remains retired",
            )
            .unwrap(),
        ),
    );
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), contender_request()),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::CasThreadOwnershipConflict
    ));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let request = valid_request(
        &reopened,
        storage,
        contender,
        contender_selected,
        cas_thread,
    );
    let outcome = execute_outcome(
        &reopened,
        storage.publish_valid_binding(storage.revision(&reopened).unwrap(), request),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::CasThreadOwnershipConflict
    ));
    reopened.close().unwrap();
}
