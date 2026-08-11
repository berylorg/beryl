use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, BindingState, CasItemSource, CasLineageProof, CasRepresentedPrefixProof,
    CasTurnSource, ComposerAtom, ComposerPayload, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    IdleSubmission, LiveSourceEvent, NativeCasLineage, PreparedContent, ProviderFrameOrdinalV1,
    ProviderFramePreparationPlan, ProviderFrameStageOutcome, ProviderItemBuildLifecycle, ProviderItemFrameV1,
    ProviderItemObservationV1, ProviderItemV1, ProviderLifecycleTimestampMsV1,
    ProviderSubmittedContentV1, ProviderUserMessageV1, PublishActiveCasTurn, PublishValidBinding,
    SealedProviderFrameReference, SourceEventPayload, SourceEventSequence, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, empty_selected_path_digest, prepare_provider_frame,
    stage_provider_frame,
};

use super::stage_prepared_content;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean exact-CAS fixture command, got {outcome:?}"),
    }
}

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-test-exact-cas",
        )
        .unwrap(),
    )
}

pub const fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([0xa5; 32])
}

/// Submits one nonempty current draft and returns the exact admitted turn identity.
pub fn submit_current_draft(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    next_draft: SyndicDraftId,
    submitted_item: SyndicItemId,
    text: &str,
    at: SyndicTimestamp,
) -> SyndicTurnId {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &prepared);
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, at).unwrap()
    else {
        panic!("test draft must become nonempty")
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        next_draft,
        submitted_item,
        None,
        at,
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );
    turn
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

/// Establishes exact CAS authority for the current pending turn without admitting activation.
pub fn establish_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    started_at: SyndicTimestamp,
) -> CasTurnSource {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    assert_eq!(selected.tail(), Some(turn));
    let turn_record = storage.turn(store, turn, point_limit()).unwrap().unwrap();
    let (parent, parent_digest) = match turn_record.parent().turn() {
        Some(parent) => {
            let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, empty_selected_path_digest()),
    };
    let represented =
        CasRepresentedPrefixProof::new(parent, selected.thread_revision(), parent_digest);
    let prior = current
        .binding()
        .revision()
        .get()
        .checked_sub(1)
        .and_then(|revision| BindingRevision::new(revision).ok())
        .and_then(|revision| {
            storage
                .binding(store, thread, revision, point_limit())
                .unwrap()
        });
    let (execution, cas_thread, lineage, native_turn_count, profile) =
        match prior.as_ref().map(|record| record.state()) {
            Some(BindingState::Valid(usable))
                if usable.represented_prefix().tail() == represented.tail()
                    && usable.represented_prefix().digest() == represented.digest() =>
            {
                (
                    usable.execution().clone(),
                    usable.cas_thread_id().clone(),
                    usable.lineage(),
                    usable.native_turn_count(),
                    usable.tool_profile(),
                )
            }
            _ => {
                let mechanism = if represented.tail().is_some() {
                    NativeCasLineage::Fork
                } else {
                    NativeCasLineage::Fresh
                };
                let execution = storage
                    .thread_execution(store, thread, point_limit())
                    .unwrap()
                    .expect("exact CAS fixture thread must retain canonical execution")
                    .execution()
                    .clone();
                (
                    execution,
                    CasThreadId::new(format!("test-thread-{turn}")).unwrap(),
                    CasLineageProof::native(mechanism, represented).unwrap(),
                    CasNativeTurnCount::ZERO,
                    tool_profile(),
                )
            }
        };
    execute(
        store,
        storage.publish_valid_binding(
            storage.revision(store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution,
                cas_thread.clone(),
                represented,
                native_turn_count,
                profile,
                lineage,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes());
    execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                thread,
                binding.binding().revision(),
                gate.revision(),
                selected,
                snapshot,
                turn,
                loaded_generation(),
                started_at,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(format!("test-turn-{turn}")).unwrap();
    execute(
        store,
        storage.publish_active_cas_turn(
            storage.revision(store).unwrap(),
            PublishActiveCasTurn::new(
                thread,
                binding.binding().revision(),
                gate.revision(),
                snapshot,
                cas_thread.clone(),
                cas_turn.clone(),
                started_at,
            ),
        ),
    );
    CasTurnSource::new(cas_thread, cas_turn)
}

/// Admits one event under the exact currently published CAS turn authority.
pub fn admit_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap();
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    );
}

fn provider_content_id(item_id: SyndicItemId) -> SyndicContentId {
    let mut bytes = *item_id.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0xa5;
    }
    SyndicContentId::from_bytes(bytes)
}

/// Stages and admits one exact target-state ProviderItemV1 frame.
#[allow(clippy::too_many_arguments)]
pub fn admit_item_frame(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
    observed_at: SyndicTimestamp,
) -> SealedProviderFrameReference {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let source_event = SourceEventSequence::new(state.source_event_count() + 1).unwrap();
    let prior = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .and_then(|item| item.provider().cloned());
    let item_source = CasItemSource::new(source.clone(), frame.item_id().clone());
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item_id,
            turn,
            item_source,
            source_event,
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item_id,
            turn,
            item_source,
            source_event,
            provider_content_id(item_id),
            frame,
        ),
    };
    let prepared = prepare_provider_frame(plan).unwrap();
    execute(
        store,
        storage.begin_provider_frame_build(storage.revision(store).unwrap(), &prepared),
    );
    let mut build = match stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &syndic_storage::ProviderFrameStageBatch| {
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.stage_provider_frame_batch(
                    storage.revision(store).unwrap(),
                    batch.clone(),
                ))
                .unwrap();
            store.execute(command)
        },
    )
    .unwrap() {
        ProviderFrameStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => value,
        outcome => panic!("expected clean provider-frame staging, got {outcome:?}"),
    };
    for _ in 0..4_096 {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            let sealed = prepared.target().clone();
            assert_eq!(build.target(), &sealed);
            admit_event(
                store,
                storage,
                thread,
                turn,
                source,
                SourceEventPayload::ItemFrame {
                    item_id,
                    frame: Box::new(sealed.clone()),
                },
                observed_at,
            );
            return sealed;
        }
        execute(
            store,
            storage.compare_provider_completion(storage.revision(store).unwrap(), build),
        );
        build = storage
            .provider_item_build(store, item_id, point_limit())
            .unwrap()
            .unwrap()
            .clone();
    }
    panic!("bounded provider completion comparison did not finish");
}

/// Admits one ordinary started/completed provider item in two exact source events.
#[allow(clippy::too_many_arguments)]
pub fn admit_started_then_completed_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    cas_item: CasItemId,
    started: ProviderItemV1,
    completed: ProviderItemV1,
    started_at: SyndicTimestamp,
    completed_at: SyndicTimestamp,
) {
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(started_at.unix_millis()),
                item: started,
            },
        ),
        started_at,
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(completed_at.unix_millis()),
                item: completed,
            },
        ),
        completed_at,
    );
}

/// Correlates and closes the submitted local user item under exact CAS authority.
pub fn correlate_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let content = item
        .presentation_content()
        .expect("submitted user fixture has sealed composer content");
    let cas_item_id = CasItemId::new(format!("test-user-{item_id}")).unwrap();
    let provider_item = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 { content },
    });
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item_id.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
                item: provider_item.clone(),
            },
        ),
        observed_at,
    );
    admit_item_frame(
        store,
        storage,
        thread,
        turn,
        item_id,
        source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item_id,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
                item: provider_item,
            },
        ),
        observed_at,
    );
}
