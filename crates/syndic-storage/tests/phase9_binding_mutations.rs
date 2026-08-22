#![cfg(feature = "test-faults")]

mod support;

#[path = "phase9_binding_mutations/lifecycle.rs"]
mod lifecycle;

use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, ExecutionBinding, InputGateRevision, PathFlavor,
    ProjectionRevision, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicAcceptedInputId,
    SyndicDraftId, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute_outcome(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    match execute_outcome(store, contribution) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed binding mutation, got {outcome:?}"),
    }
}

fn typed_error(outcome: &CommandOutcome) -> &SyndicMutationError {
    let CommandOutcome::NotCommitted { evidence } = outcome else {
        panic!("expected rejected binding mutation, got {outcome:?}");
    };
    let beryl_home_store::CommandError::ContributorValidation { source, .. } = evidence else {
        panic!("expected contributor validation rejection, got {evidence}");
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
            "C:\\phase9-binding-mutations",
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

fn same_home_pending_path(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_byte: u8,
) -> (
    SyndicThreadId,
    SyndicDraftId,
    SyndicTurnId,
    SelectedPathProof,
) {
    let thread = id(thread_byte);
    let draft = draft_id(thread_byte.wrapping_add(10));
    create_thread(store, storage, thread, draft);
    let turn = SyndicTurnId::from_bytes([thread_byte.wrapping_add(1); 16]);
    let digest = root_turn_chain_digest(turn);
    let thread_revision = ThreadRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
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
        Some(turn),
        digest,
        false,
        timestamp(4),
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);
    let activity_source = ActivityQuerySource::new(thread, turn);
    let mut records = vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::ActivityQueryHead(
            ActivityQueryHeadRecord::new(
                thread,
                ActivityWorkPeriod::FIRST,
                Some(activity_source),
                true,
                0,
                ActivityQueryRevision::FIRST,
                1,
                0,
                0,
                0,
                0,
                None,
                ProjectionLifecycle::Current,
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
            thread,
            ActivityWorkPeriod::FIRST,
            activity_source,
            None,
            0,
            true,
            None,
        )),
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
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(4),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
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
                InputGateState::PendingTurn(turn),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
    ];
    records.extend(item_free_transcript_build_records(
        thread,
        thread_revision,
        &[(turn, digest, TurnLifecycle::Pending, 0, timestamp(4))],
    ));
    commit(store, storage, batch(records));
    (thread, draft, turn, selected)
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
    PublishValidBinding::new(
        thread,
        current_binding_revision(store, storage, thread),
        selected,
        storage
            .thread_execution(store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
        cas_thread,
        represented,
        CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    )
}

fn seed_queued_input(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    source_draft: SyndicDraftId,
) -> SyndicAcceptedInputId {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let history = storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let next_thread_revision = thread_record.revision().checked_next().unwrap();
    let next_gate_revision = gate.revision().checked_next().unwrap();
    let generation = AcceptedRouteGeneration::FIRST;
    let ordinal = AcceptedInputOrdinal::FIRST;
    let input = source_draft.accepted_input_id();
    let (content, content_records) = composer_content_records(&ComposerPayload::default());
    let mut records = content_records;
    records.extend([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            SelectedPathProof::new(
                thread_record.committed_tail(),
                next_thread_revision,
                thread_record.selected_path_digest(),
            ),
            current.draft().id(),
            thread_record.lineage(),
            thread_record.image_label_frontiers(),
            thread_record.context_owner_id(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current.draft().id(),
            current.draft().revision(),
            next_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            history.revision().checked_next().unwrap(),
            next_thread_revision,
            history.committed_tail(),
            history.selected_path_digest(),
            history.complete(),
            history.last_activity_at(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input,
                thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    thread_record.revision(),
                    source_draft,
                    beryl_model::DraftRevision::new(1).unwrap(),
                    gate.revision(),
                    current.draft().id(),
                )
                .unwrap(),
                generation,
                content,
                None,
                timestamp(1),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread, ordinal, input, generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
                AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                Some(ordinal),
                Some(ordinal),
                1,
                0,
                0,
                1,
                0,
                content.summary().logical_utf8_bytes(),
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input,
            thread,
            generation,
            ordinal,
            beryl_model::AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            generation,
            AcceptedRouteRevision::FIRST,
            ordinal,
            ordinal,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                next_gate_revision,
                gate.state().clone(),
                1,
                Some(generation),
                None,
                0,
                1,
                content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
    ]);
    commit(store, storage, batch(records));
    input
}

fn seed_active_queued_input(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    source_draft: SyndicDraftId,
) -> SyndicAcceptedInputId {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let history = storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected_route = gate.selected_route().unwrap();
    let route =
        test_faults::accepted_route_generation(store, storage, thread, selected_route.generation())
            .unwrap();
    let next_thread_revision = thread_record.revision().checked_next().unwrap();
    let next_gate_revision = gate.revision().checked_next().unwrap();
    let next_route_revision = route.revision().checked_next().unwrap();
    let ordinal = AcceptedInputOrdinal::FIRST;
    let input = source_draft.accepted_input_id();
    let (content, content_records) = composer_content_records(&ComposerPayload::default());
    let mut records = content_records;
    records.extend([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            SelectedPathProof::new(
                thread_record.committed_tail(),
                next_thread_revision,
                thread_record.selected_path_digest(),
            ),
            current.draft().id(),
            thread_record.lineage(),
            thread_record.image_label_frontiers(),
            thread_record.context_owner_id(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current.draft().id(),
            current.draft().revision(),
            next_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            history.revision().checked_next().unwrap(),
            next_thread_revision,
            history.committed_tail(),
            history.selected_path_digest(),
            history.complete(),
            history.last_activity_at(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input,
                thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    thread_record.revision(),
                    source_draft,
                    beryl_model::DraftRevision::new(1).unwrap(),
                    gate.revision(),
                    current.draft().id(),
                )
                .unwrap(),
                selected_route.generation(),
                content,
                None,
                timestamp(1),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            ordinal,
            input,
            selected_route.generation(),
        )),
        FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
            thread,
            AcceptedRouteHeadProof::new(selected_route.generation(), next_route_revision),
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                selected_route.generation(),
                next_route_revision,
                route.target().clone(),
                Some(ordinal),
                Some(ordinal),
                1,
                1,
                0,
                0,
                0,
                content.summary().logical_utf8_bytes(),
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedReadySource(AcceptedReadySourceRecord::new(
            thread,
            next_gate_revision,
            selected_route.generation(),
            next_route_revision,
            ordinal,
            ordinal,
        )),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input,
            thread,
            selected_route.generation(),
            ordinal,
            beryl_model::AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                next_gate_revision,
                gate.state().clone(),
                1,
                gate.route_generation_high_water(),
                Some(AcceptedRouteHeadProof::new(
                    selected_route.generation(),
                    next_route_revision,
                )),
                1,
                0,
                content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
    ]);
    commit(store, storage, batch(records));
    input
}

fn seed_child_pending_after_terminal(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &ActiveFixture,
    child: SyndicTurnId,
) -> SelectedPathProof {
    let thread_record = storage
        .thread(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let current = storage
        .current_draft(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let history = storage
        .history_summary(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let parent = storage
        .turn(store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let parent_state = storage
        .turn_state(store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let binding = storage
        .current_binding(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let binding_revision = binding.binding().revision().checked_next().unwrap();
    let thread_revision = thread_record.revision().checked_next().unwrap();
    let digest = child_turn_chain_digest(child, fixture.turn, parent.chain_digest());
    let selected = SelectedPathProof::new(Some(child), thread_revision, digest);
    let history_record = HistorySummaryRecord::new(
        fixture.thread,
        history.revision().checked_next().unwrap(),
        thread_revision,
        Some(child),
        digest,
        false,
        timestamp(8),
    );
    let replacement_thread = ThreadRecord::new(
        fixture.thread,
        selected,
        current.draft().id(),
        thread_record.lineage(),
        thread_record.image_label_frontiers(),
        thread_record.context_owner_id(),
    );
    let execution = storage
        .thread_execution(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let attributes = ThreadAttributesRecord::ordinary(fixture.thread);
    let mut records = vec![
        FixtureRecord::Thread(replacement_thread.clone()),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            fixture.thread,
            current.draft().id(),
            current.draft().revision(),
            thread_revision,
        )),
        FixtureRecord::HistorySummary(history_record.clone()),
        FixtureRecord::ThreadCatalogSummary(ThreadCatalogSummaryRecord::initial(
            &replacement_thread,
            &execution,
            &attributes,
            &history_record,
        )),
        FixtureRecord::Turn(TurnRecord::new(
            child,
            fixture.thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(fixture.turn),
            Some(fixture.turn),
            TurnDepth::new(2).unwrap(),
            digest,
            timestamp(8),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            child,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            timestamp(8),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            fixture.turn,
            child,
            TurnDepth::new(2).unwrap(),
            digest,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                fixture.thread,
                gate.revision().checked_next().unwrap(),
                InputGateState::PendingTurn(child),
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                None,
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::Binding(BindingRecord::new(
            fixture.thread,
            binding_revision,
            selected,
            BindingState::unbound("pending post-terminal continuation").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            fixture.thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            fixture.thread,
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            0,
            Some(child),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            fixture.thread,
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(1).unwrap(),
            thread_revision,
            Some(child),
            digest,
            2,
            0,
            syndic_storage::test_faults::fixture_transcript_digest_seed(),
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            fixture.thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            fixture.turn,
            parent.chain_digest(),
            parent_state.revision(),
            parent_state.lifecycle(),
            parent_state.source_event_count(),
            0,
            0,
            timestamp(7),
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            fixture.thread,
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            child,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            0,
            timestamp(8),
        )),
    ];
    records.shrink_to_fit();
    commit(store, storage, batch(records));
    selected
}

struct ActiveFixture {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
    cas_turn: beryl_model::CasTurnId,
    snapshot: SyndicExecutionSnapshotId,
    valid: PublishValidBinding,
    activation: ActivateBinding,
}

fn activate_pending(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_byte: u8,
    publish_cas_turn: bool,
) -> ActiveFixture {
    let (thread, _, turn, selected) = same_home_pending_path(store, storage, thread_byte);
    let cas_thread = CasThreadId::new(format!("phase9-lifecycle-cas-{thread_byte}")).unwrap();
    let valid = valid_request(store, storage, thread, selected, cas_thread.clone());
    execute(
        store,
        storage.publish_valid_binding(storage.revision(store).unwrap(), valid.clone()),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes([thread_byte.wrapping_add(2); 16]);
    let activation = ActivateBinding::new(
        thread,
        current_binding_revision(store, storage, thread),
        current_gate_revision(store, storage, thread),
        selected,
        snapshot,
        turn,
        loaded_generation(31, u64::from(thread_byte) + 1),
        timestamp(5),
    );
    execute(
        store,
        storage.activate_binding(storage.revision(store).unwrap(), activation.clone()),
    );
    let cas_turn =
        beryl_model::CasTurnId::new(format!("phase9-lifecycle-turn-{thread_byte}")).unwrap();
    if publish_cas_turn {
        execute(
            store,
            storage.publish_active_cas_turn(
                storage.revision(store).unwrap(),
                PublishActiveCasTurn::new(
                    thread,
                    current_binding_revision(store, storage, thread),
                    current_gate_revision(store, storage, thread),
                    snapshot,
                    cas_thread.clone(),
                    cas_turn.clone(),
                    timestamp(6),
                ),
            ),
        );
    }
    ActiveFixture {
        thread,
        turn,
        selected,
        cas_thread,
        cas_turn,
        snapshot,
        valid,
        activation,
    }
}

fn abandonment(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    wrong_native_count: bool,
) -> AbandonActiveBinding {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = current.binding().state() else {
        panic!("canonical populated binding must be active");
    };
    let snapshot = storage
        .execution_snapshot(store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let route = gate.selected_route().unwrap();
    let route_record =
        test_faults::accepted_route_generation(store, storage, thread, route.generation()).unwrap();
    let target = match route_record.target() {
        AcceptedRouteTarget::AwaitingSteering(target) => {
            AcceptedRouteLostTarget::AwaitingSteering(target.clone())
        }
        AcceptedRouteTarget::Steering(target) => AcceptedRouteLostTarget::Steering(target.clone()),
        AcceptedRouteTarget::AwaitingTerminal(target) => {
            AcceptedRouteLostTarget::AwaitingTerminal(target.clone())
        }
        other => panic!("canonical populated active route has unexpected target {other:?}"),
    };
    let native_count = if wrong_native_count {
        active.usable().native_turn_count().checked_next().unwrap()
    } else {
        active.usable().native_turn_count()
    };
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(native_count),
        Some(snapshot.loaded_generation()),
        "canonical active projection lost",
        timestamp(9),
    )
    .unwrap();
    AbandonActiveBinding::new(
        thread,
        current.binding().revision(),
        route.generation(),
        target,
        current.binding().selected_path(),
        stale,
    )
}

#[test]
fn invalid_abandonment_preserves_the_exact_active_binding_and_route() {
    let home = TestHome::new("phase9-invalid-abandonment-preserves-state");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, storage, 40, false);
    let thread = fixture.thread;
    let before_revision = storage.revision(&store).unwrap();
    let before_binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();

    let outcome = execute_outcome(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            abandonment(&store, storage, thread, true),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn exact_active_abandonment_is_reconcilable_and_survives_reopen() {
    let home = TestHome::new("phase9-exact-active-abandonment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, storage, 50, false);
    let thread = fixture.thread;
    let request = abandonment(&store, storage, thread, false);
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request.clone()),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(current.binding().state(), BindingState::Stale(_)));
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 0);
    let route = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(&store, thread, route.generation(), route.revision(), None)
        .unwrap();
    assert!(page.records().is_empty());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert!(matches!(
        storage
            .current_binding(&reopened, thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(_)
    ));
    reopened.close().unwrap();
}

#[test]
fn retired_projection_rejects_late_activation_and_source_less_complete() {
    let home = TestHome::new("phase9-retired-projection-rejects-late-events");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, storage, 55, true);
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            LiveSourceEvent::new(
                fixture.thread,
                fixture.turn,
                state.revision(),
                gate.revision(),
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                SourceEventPayload::TurnActivated,
                timestamp(7),
            )
            .unwrap(),
        ),
    );
    let request = abandonment(&store, storage, fixture.thread, false);
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request.clone()),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let before_revision = storage.revision(&store).unwrap();
    let before_binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        before_binding.binding().state(),
        BindingState::Stale(_)
    ));
    assert_eq!(
        storage
            .cas_thread_owner(&store, fixture.cas_thread.clone(), point_limit())
            .unwrap()
            .unwrap()
            .retired_binding_revision(),
        Some(before_binding.binding().revision())
    );
    let before_gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let before_state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();

    let late_activation = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        before_state.revision(),
        before_gate.revision(),
        SourceEventSequence::new(before_state.source_event_count() + 1).unwrap(),
        Some(CasTurnSource::new(
            fixture.cas_thread.clone(),
            fixture.cas_turn.clone(),
        )),
        SourceEventPayload::TurnActivated,
        timestamp(10),
    )
    .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), late_activation),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::SourceIdentityConflict
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    assert_eq!(
        storage
            .turn_state(&store, fixture.turn, point_limit())
            .unwrap()
            .unwrap(),
        before_state
    );

    let source_less_complete = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        before_state.revision(),
        before_gate.revision(),
        SourceEventSequence::new(before_state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(10),
    )
    .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), source_less_complete),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::SourceIdentityConflict
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    assert_eq!(
        storage
            .turn_state(&store, fixture.turn, point_limit())
            .unwrap()
            .unwrap(),
        before_state
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .current_binding(&reopened, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&reopened, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    assert_eq!(
        storage
            .turn_state(&reopened, fixture.turn, point_limit())
            .unwrap()
            .unwrap(),
        before_state
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn queued_input_survives_active_abandonment_retry_and_activation() {
    let home = TestHome::new("phase9-queued-active-abandonment-retry");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, storage, 60, true);
    let accepted = seed_active_queued_input(&store, storage, fixture.thread, draft_id(73));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let request = abandonment(&store, storage, fixture.thread, false);
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request.clone()),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    let owner = storage
        .cas_thread_owner(&store, fixture.cas_thread.clone(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        owner.retired_binding_revision(),
        Some(binding.binding().revision())
    );
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(fixture.turn));
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 1);
    let lost_route = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(
            &store,
            fixture.thread,
            lost_route.generation(),
            lost_route.revision(),
            None,
        )
        .unwrap();
    let input = page
        .records()
        .iter()
        .find(|row| row.input().id() == accepted)
        .unwrap();
    assert_eq!(
        input.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(input.leaf().lifecycle(), AcceptedInputLifecycle::Admitted);

    let retry = valid_request(
        &store,
        storage,
        fixture.thread,
        fixture.selected,
        CasThreadId::new("phase9-abandonment-retry-cas").unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), retry),
    );
    let retry_snapshot = SyndicExecutionSnapshotId::from_bytes([74; 16]);
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                fixture.thread,
                current_binding_revision(&store, storage, fixture.thread),
                current_gate_revision(&store, storage, fixture.thread),
                fixture.selected,
                retry_snapshot,
                fixture.turn,
                loaded_generation(62, 63),
                timestamp(9),
            ),
        ),
    );
    assert_eq!(
        storage
            .turn_state(&store, fixture.turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Pending
    );
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(gate.state(), InputGateState::AwaitingSteering(_)));
    assert_eq!(gate.live_next_turn_count(), 1);
    assert_ne!(
        gate.selected_route().unwrap().generation(),
        lost_route.generation()
    );
    let retained = storage
        .accepted_input(&store, accepted, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(retained.route_generation(), lost_route.generation());
    assert_eq!(retained.admission().source_draft_id(), draft_id(73));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_idle_gate_leaving_abandoned_turn_blocking() {
    let home = TestHome::new("phase9-abandoned-idle-gate-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, storage, 150, true);
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            LiveSourceEvent::new(
                fixture.thread,
                fixture.turn,
                state.revision(),
                gate.revision(),
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                SourceEventPayload::TurnActivated,
                timestamp(7),
            )
            .unwrap(),
        ),
    );
    let request = abandonment(&store, storage, fixture.thread, false);
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request),
    );
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            LiveSourceEvent::new(
                fixture.thread,
                fixture.turn,
                state.revision(),
                gate.revision(),
                SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::UnknownTerminal, None).unwrap(),
                ),
                timestamp(8),
            )
            .unwrap(),
        ),
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                fixture.thread,
                binding.binding().revision(),
                fixture.selected,
                "abandoned projection has no usable lineage",
            )
            .unwrap(),
        ),
    );
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(
            InputGateRecord::new(
                gate.thread_id(),
                gate.revision(),
                InputGateState::Idle,
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                gate.selected_route(),
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap(),
        )]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("idle input gate leaves committed turn blocking"),
        "unexpected abandoned gate scrub error: {error}"
    );
    reopened.close().unwrap();
}
