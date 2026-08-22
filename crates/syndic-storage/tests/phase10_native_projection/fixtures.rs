use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId,
    ExecutionBinding, InputGateRevision, PathFlavor, ProjectionRevision, RootId, RuntimeId,
    RuntimeMode, RuntimeNativePath, SyndicAcceptedInputId, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::*;

use crate::support;

#[derive(Clone)]
pub(super) struct PendingFixture {
    pub(super) thread: SyndicThreadId,
    pub(super) pending: SyndicTurnId,
    pub(super) selected: SelectedPathProof,
    pub(super) binding_revision: BindingRevision,
    pub(super) execution: ExecutionBinding,
    pub(super) tool_profile: CasConversationToolProfile,
}

pub(super) struct AdvancedSourceFixture {
    pub(super) turn: SyndicTurnId,
    pub(super) selected: SelectedPathProof,
    pub(super) binding_revision: BindingRevision,
    pub(super) native_turn_count: CasNativeTurnCount,
}

pub(super) struct AcceptedAdmissionFixture {
    pub(super) selected: SelectedPathProof,
    pub(super) input: SyndicAcceptedInputId,
    pub(super) gate_revision: InputGateRevision,
}

pub(super) fn alternate_execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([210; 16]),
        RootId::from_bytes([211; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\phase10-alternate-root",
        )
        .unwrap(),
    )
}

pub(super) fn seed_child_at_tail(
    store: &HomeStore,
    storage: SyndicStorage,
    source_thread: SyndicThreadId,
    child: SyndicThreadId,
    draft: SyndicDraftId,
) {
    let tail = storage
        .thread_tail(store, source_thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let request = CreateThread::from_tail(
        child,
        draft,
        tail.last_activity_at(),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(store).unwrap(), request))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub(super) fn finish_current_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) {
    let thread_record = storage
        .thread(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.generation();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, crate::point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
    panic!("bounded divergent-source transcript build did not finish")
}

pub(super) fn seed_root_pending(
    store: &HomeStore,
    storage: SyndicStorage,
    byte: u8,
    valid_binding: bool,
) -> PendingFixture {
    let thread = SyndicThreadId::from_bytes([byte; 16]);
    let draft = SyndicDraftId::from_bytes([byte.checked_add(1).unwrap(); 16]);
    let pending = SyndicTurnId::from_bytes([byte.checked_add(2).unwrap(); 16]);
    support::seed_canonical_empty_thread(store, storage, thread, draft);

    let selected = SelectedPathProof::new(
        Some(pending),
        ThreadRevision::new(2).unwrap(),
        root_turn_chain_digest(pending),
    );
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
    let execution = support::exact_cas::execution_binding();
    let tool_profile = support::exact_cas::tool_profile();
    let binding_revision = BindingRevision::new(2).unwrap();
    let cas_thread = CasThreadId::new(format!("phase10-root-{byte}")).unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let state = if valid_binding {
        BindingState::valid(UsableCasBinding::new(
            execution.clone(),
            cas_thread.clone(),
            represented,
            CasNativeTurnCount::ZERO,
            tool_profile,
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
        ))
    } else {
        BindingState::unbound("pending fixture has no CAS projection").unwrap()
    };
    let history = HistorySummaryRecord::new(
        thread,
        ProjectionRevision::new(2).unwrap(),
        selected.thread_revision(),
        selected.tail(),
        selected.digest(),
        false,
        support::timestamp(2),
    );
    let execution_record = ThreadExecutionRecord::new(thread, execution.clone());
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let catalog = ThreadCatalogSummaryRecord::initial(
        &thread_record,
        &execution_record,
        &attributes,
        &history,
    );
    let mut batch = FixtureBatch::new();
    for record in [
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            beryl_model::DraftRevision::new(1).unwrap(),
            selected.thread_revision(),
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(2).unwrap(),
                InputGateState::PendingTurn(pending),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            pending,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            selected.digest(),
            support::timestamp(2),
        )),
        FixtureRecord::TurnState(support::fixture_turn_state(
            pending,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            support::timestamp(2),
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            ProjectionRevision::new(2).unwrap(),
            0,
            selected.tail(),
            selected.digest(),
            ProjectionLifecycle::Stale,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            state,
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            if valid_binding {
                BindingLifecycle::Valid
            } else {
                BindingLifecycle::Unbound
            },
            selected.digest(),
        )),
    ] {
        batch.put(record).unwrap();
    }
    if valid_binding {
        batch
            .put(FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
                cas_thread.clone(),
                thread,
                binding_revision,
                binding_revision,
            )))
            .unwrap();
        batch
            .put(FixtureRecord::CasThreadBinding(
                CasThreadBindingIndexRecord::new(cas_thread, thread, binding_revision),
            ))
            .unwrap();
    }
    batch
        .delete(FixtureDelete::TranscriptBuild {
            thread,
            generation: TranscriptGeneration::FIRST,
        })
        .unwrap();
    support::commit(store, storage, batch);
    PendingFixture {
        thread,
        pending,
        selected,
        binding_revision,
        execution,
        tool_profile,
    }
}

pub(super) fn append_pending(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    pending: SyndicTurnId,
    parent: SyndicTurnId,
) -> PendingFixture {
    let current_thread = storage
        .thread(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_draft = storage
        .current_draft(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_gate = storage
        .input_gate(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_binding = storage
        .current_binding(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_transcript = storage
        .transcript_view_head(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_history = storage
        .history_summary(store, thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let parent_record = storage
        .turn(store, parent, crate::point_limit())
        .unwrap()
        .unwrap();
    let source_binding = storage
        .current_binding(store, support::id(30), crate::point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(source_usable) = source_binding.binding().state() else {
        panic!("canonical source binding must remain valid")
    };
    let selected = SelectedPathProof::new(
        Some(pending),
        current_thread.revision().checked_next().unwrap(),
        child_turn_chain_digest(pending, parent, parent_record.chain_digest()),
    );
    let binding_revision = current_binding.binding().revision().checked_next().unwrap();
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        current_draft.draft().id(),
        current_thread.lineage(),
        current_thread.image_label_frontiers(),
        current_thread.context_owner_id(),
    );
    let mut batch = FixtureBatch::new();
    for record in [
        FixtureRecord::Thread(thread_record),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current_draft.draft().id(),
            current_draft.draft().revision(),
            selected.thread_revision(),
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                current_gate.revision().checked_next().unwrap(),
                InputGateState::PendingTurn(pending),
                current_gate.accepted_high_water(),
                current_gate.route_generation_high_water(),
                current_gate.selected_route(),
                current_gate.live_steering_count(),
                current_gate.live_next_turn_count(),
                current_gate.live_logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            pending,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(parent),
            Some(parent),
            parent_record.depth().checked_next().unwrap(),
            selected.digest(),
            support::timestamp(20),
        )),
        FixtureRecord::TurnState(support::fixture_turn_state(
            pending,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            support::timestamp(20),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            parent,
            pending,
            parent_record.depth().checked_next().unwrap(),
            selected.digest(),
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            current_transcript.generation().checked_next().unwrap(),
            current_transcript.revision().checked_next().unwrap(),
            0,
            selected.tail(),
            selected.digest(),
            ProjectionLifecycle::Stale,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            current_history.revision().checked_next().unwrap(),
            selected.thread_revision(),
            selected.tail(),
            selected.digest(),
            false,
            support::timestamp(20),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("pending fixture awaits native projection").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            selected.digest(),
        )),
    ] {
        batch.put(record).unwrap();
    }
    support::commit(store, storage, batch);
    PendingFixture {
        thread,
        pending,
        selected,
        binding_revision,
        execution: source_usable.execution().clone(),
        tool_profile: source_usable.tool_profile(),
    }
}

pub(super) fn seed_accepted_input_admission_descendant(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &PendingFixture,
) -> AcceptedAdmissionFixture {
    let current_thread = storage
        .thread(store, fixture.thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_draft = storage
        .current_draft(store, fixture.thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_history = storage
        .history_summary(store, fixture.thread, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_gate = storage
        .input_gate(store, fixture.thread, crate::point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        current_gate.state(),
        InputGateState::PendingTurn(turn) if *turn == fixture.pending
    ));
    let source_draft = current_draft.draft();
    let admitted_content = support::empty_composer_content();
    let replacement_draft = SyndicDraftId::from_bytes([171; 16]);
    let detached_thread = SyndicThreadId::from_bytes([172; 16]);
    let replacement_root_history = support::seed_detached_canonical_draft_backing(
        store,
        storage,
        detached_thread,
        replacement_draft,
    );
    let input = source_draft.id().accepted_input_id();
    let ordinal = AcceptedInputOrdinal::new(current_gate.accepted_high_water() + 1).unwrap();
    let generation = current_gate
        .route_generation_high_water()
        .map_or(AcceptedRouteGeneration::FIRST, |current| {
            current.checked_next().unwrap()
        });
    let route_revision = AcceptedRouteRevision::FIRST;
    let gate_revision = current_gate.revision().checked_next().unwrap();
    let selected = SelectedPathProof::new(
        fixture.selected.tail(),
        fixture.selected.thread_revision().checked_next().unwrap(),
        fixture.selected.digest(),
    );
    let thread = ThreadRecord::new(
        fixture.thread,
        selected,
        replacement_draft,
        current_thread.lineage(),
        current_thread.image_label_frontiers(),
        current_thread.context_owner_id(),
    );
    let history = HistorySummaryRecord::new(
        fixture.thread,
        current_history.revision().checked_next().unwrap(),
        selected.thread_revision(),
        selected.tail(),
        selected.digest(),
        current_history.complete(),
        support::timestamp(22),
    );
    let mut batch = FixtureBatch::new();
    for record in [
        FixtureRecord::Thread(thread),
        FixtureRecord::Draft(DraftRecord::new(
            replacement_draft,
            fixture.thread,
            beryl_model::DraftRevision::new(1).unwrap(),
            DraftSubmissionIntent::Ordinary,
            replacement_root_history,
            support::timestamp(22),
            support::timestamp(22),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            fixture.thread,
            replacement_draft,
            beryl_model::DraftRevision::new(1).unwrap(),
            selected.thread_revision(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input,
                fixture.thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    fixture.selected.thread_revision(),
                    source_draft.id(),
                    source_draft.revision(),
                    current_gate.revision(),
                    replacement_draft,
                )
                .unwrap(),
                generation,
                admitted_content,
                None,
                support::timestamp(22),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            fixture.thread,
            ordinal,
            input,
            generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                fixture.thread,
                generation,
                route_revision,
                AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                Some(ordinal),
                Some(ordinal),
                1,
                0,
                0,
                1,
                0,
                admitted_content.summary().logical_utf8_bytes(),
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input,
            fixture.thread,
            generation,
            ordinal,
            AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            fixture.thread,
            generation,
            route_revision,
            ordinal,
            ordinal,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                fixture.thread,
                gate_revision,
                current_gate.state().clone(),
                ordinal.get(),
                Some(generation),
                current_gate.selected_route(),
                current_gate.live_steering_count(),
                current_gate.live_next_turn_count() + 1,
                current_gate.live_logical_utf8_bytes()
                    + admitted_content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
    ] {
        batch.put(record).unwrap();
    }
    batch
        .delete(FixtureDelete::Draft(source_draft.id()))
        .unwrap();
    support::commit(store, storage, batch);
    AcceptedAdmissionFixture {
        selected,
        input,
        gate_revision,
    }
}

pub(super) fn advance_source_to_divergent_prefix(
    store: &HomeStore,
    storage: SyndicStorage,
) -> AdvancedSourceFixture {
    let thread_id = support::id(30);
    let parent_id = support::populated::source_turn();
    let current_thread = storage
        .thread(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_draft = storage
        .current_draft(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_history = storage
        .history_summary(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_transcript = storage
        .transcript_view_head(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let execution = storage
        .thread_execution(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let attributes = storage
        .thread_attributes(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_binding = storage
        .current_binding(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let current_gate = storage
        .input_gate(store, thread_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(current_usable) = current_binding.binding().state() else {
        panic!("canonical source binding must remain valid")
    };
    let parent = storage
        .turn(store, parent_id, crate::point_limit())
        .unwrap()
        .unwrap();
    let owner = storage
        .cas_thread_owner(
            store,
            current_usable.cas_thread_id().clone(),
            crate::point_limit(),
        )
        .unwrap()
        .unwrap();
    let turn = SyndicTurnId::from_bytes([112; 16]);
    let selected = SelectedPathProof::new(
        Some(turn),
        current_thread.revision().checked_next().unwrap(),
        child_turn_chain_digest(turn, parent_id, parent.chain_digest()),
    );
    let unbound_binding_revision = current_binding.binding().revision().checked_next().unwrap();
    let resumed_binding_revision = unbound_binding_revision.checked_next().unwrap();
    let active_binding_revision = resumed_binding_revision.checked_next().unwrap();
    let binding_revision = active_binding_revision.checked_next().unwrap();
    let represented =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let activation_represented = CasRepresentedPrefixProof::new(
        current_usable.represented_prefix().tail(),
        selected.thread_revision(),
        current_usable.represented_prefix().digest(),
    );
    let activation_usable = UsableCasBinding::new(
        current_usable.execution().clone(),
        current_usable.cas_thread_id().clone(),
        activation_represented,
        current_usable.native_turn_count(),
        current_usable.tool_profile(),
        current_usable.lineage(),
    );
    let native_turn_count = current_usable.native_turn_count().checked_next().unwrap();
    let cas_turn = beryl_model::CasTurnId::new("source-history-turn-advanced").unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes([113; 16]);
    let loaded_generation = CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(2).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    );
    let active = ActiveCasBinding::new(
        activation_usable.clone(),
        snapshot,
        turn,
        current_gate.revision(),
        support::timestamp(21),
    );
    let thread = ThreadRecord::new(
        thread_id,
        selected,
        current_draft.draft().id(),
        current_thread.lineage(),
        current_thread.image_label_frontiers(),
        current_thread.context_owner_id(),
    );
    let history = HistorySummaryRecord::new(
        thread_id,
        current_history.revision().checked_next().unwrap(),
        selected.thread_revision(),
        selected.tail(),
        selected.digest(),
        false,
        support::timestamp(21),
    );
    let catalog = ThreadCatalogSummaryRecord::initial(&thread, &execution, &attributes, &history);
    support::commit(
        store,
        storage,
        support::batch([
            FixtureRecord::Thread(thread),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                thread_id,
                current_draft.draft().id(),
                current_draft.draft().revision(),
                selected.thread_revision(),
            )),
            FixtureRecord::Turn(TurnRecord::new(
                turn,
                thread_id,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(parent_id),
                Some(parent_id),
                parent.depth().checked_next().unwrap(),
                selected.digest(),
                support::timestamp(21),
            )),
            FixtureRecord::TurnState(support::fixture_turn_state(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Complete,
                1,
                0,
                support::timestamp(21),
            )),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                parent_id,
                turn,
                parent.depth().checked_next().unwrap(),
                selected.digest(),
            )),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    turn,
                    SourceEventSequence::FIRST,
                    Some(CasTurnSource::new(
                        current_usable.cas_thread_id().clone(),
                        cas_turn.clone(),
                    )),
                    SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
                )
                .unwrap(),
            ),
            FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                thread_id,
                current_transcript.generation().checked_next().unwrap(),
                current_transcript.revision().checked_next().unwrap(),
                0,
                selected.tail(),
                selected.digest(),
                ProjectionLifecycle::Stale,
            )),
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    thread_id,
                    current_gate.revision().checked_next().unwrap(),
                    InputGateState::FinalizingHistory(turn),
                    current_gate.accepted_high_water(),
                    current_gate.route_generation_high_water(),
                    current_gate.selected_route(),
                    current_gate.live_steering_count(),
                    current_gate.live_next_turn_count(),
                    current_gate.live_logical_utf8_bytes(),
                )
                .unwrap(),
            ),
            FixtureRecord::HistorySummary(history),
            FixtureRecord::ThreadCatalogSummary(catalog),
            FixtureRecord::Binding(BindingRecord::new(
                thread_id,
                unbound_binding_revision,
                selected,
                BindingState::unbound("advanced source awaits its exact native resume").unwrap(),
            )),
            FixtureRecord::Binding(BindingRecord::new(
                thread_id,
                resumed_binding_revision,
                selected,
                BindingState::valid(activation_usable.clone()),
            )),
            FixtureRecord::Binding(BindingRecord::new(
                thread_id,
                active_binding_revision,
                selected,
                BindingState::active(active),
            )),
            FixtureRecord::Binding(BindingRecord::new(
                thread_id,
                binding_revision,
                selected,
                BindingState::valid(UsableCasBinding::new(
                    current_usable.execution().clone(),
                    current_usable.cas_thread_id().clone(),
                    represented,
                    native_turn_count,
                    current_usable.tool_profile(),
                    current_usable.lineage(),
                )),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread_id,
                binding_revision,
                BindingLifecycle::Valid,
                selected.digest(),
            )),
            FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
                current_usable.cas_thread_id().clone(),
                thread_id,
                owner.first_binding_revision(),
                binding_revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                current_usable.cas_thread_id().clone(),
                thread_id,
                resumed_binding_revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                current_usable.cas_thread_id().clone(),
                thread_id,
                active_binding_revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                current_usable.cas_thread_id().clone(),
                thread_id,
                binding_revision,
            )),
            FixtureRecord::ExecutionSnapshot(ExecutionSnapshotRecord::new(
                snapshot,
                thread_id,
                active_binding_revision,
                current_gate.revision(),
                turn,
                current_usable.cas_thread_id().clone(),
                selected,
                activation_usable.represented_prefix(),
                activation_usable.native_turn_count(),
                activation_usable.tool_profile(),
                activation_usable.lineage(),
                activation_usable.execution().clone(),
                loaded_generation,
                support::timestamp(21),
            )),
            FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
                snapshot,
                thread_id,
                turn,
                active_binding_revision,
                current_usable.cas_thread_id().clone(),
                cas_turn.clone(),
                support::timestamp(21),
            )),
            FixtureRecord::CasTurn(CasTurnIndexRecord::new(
                current_usable.cas_thread_id().clone(),
                cas_turn,
                thread_id,
                turn,
                active_binding_revision,
                snapshot,
                native_turn_count,
            )),
        ]),
    );
    AdvancedSourceFixture {
        turn,
        selected,
        binding_revision,
        native_turn_count,
    }
}
