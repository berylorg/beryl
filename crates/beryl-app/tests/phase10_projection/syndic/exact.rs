#![allow(dead_code)]

use std::convert::Infallible;

use beryl_app::conversation_tools::ConversationToolRegistry;
use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicContentId, SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, BindingState, CasItemSource, CasLineageProof, CasRepresentedPrefixProof,
    CasTurnSource, LiveSourceEvent, NativeCasLineage, ProviderFrameOrdinalV1,
    ProviderFramePreparationPlan, ProviderItemBuildLifecycle, ProviderItemFrameV1,
    ProviderItemObservationV1, ProviderItemV1, ProviderLifecycleTimestampMsV1,
    ProviderSubmittedContentV1, ProviderUserMessageV1, PublishActiveCasTurn, PublishValidBinding,
    SealedProviderFrameReference, SourceEventPayload, SourceEventSequence, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, empty_selected_path_digest, prepare_provider_frame,
    stage_provider_frame,
};

use crate::EXECUTION_ROOT;

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, EXECUTION_ROOT)
            .unwrap(),
    )
}

pub fn wsl_execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::wsl("Ubuntu-24.04").unwrap(),
            PathFlavor::Posix,
            "/work/beryl",
        )
        .unwrap(),
    )
}

pub fn tool_profile() -> CasConversationToolProfile {
    ConversationToolRegistry::canonical().profile()
}

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
    let exact_source = [
        Some(current.binding().state()),
        prior.as_ref().map(|record| record.state()),
    ]
    .into_iter()
    .flatten()
    .find_map(|state| match state {
        BindingState::Valid(usable)
            if usable.represented_prefix().tail() == represented.tail()
                && usable.represented_prefix().digest() == represented.digest() =>
        {
            Some((
                usable.cas_thread_id().clone(),
                usable.lineage(),
                usable.native_turn_count(),
            ))
        }
        _ => None,
    });
    let (cas_thread, lineage, native_turn_count) = exact_source.unwrap_or_else(|| {
        (
            CasThreadId::new(format!("phase10-source-{thread}")).unwrap(),
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            CasNativeTurnCount::ZERO,
        )
    });
    let active_loaded_generation = lineage
        .recovered_injection_generation()
        .unwrap_or_else(loaded_generation);
    let current_already_valid = matches!(
        current.binding().state(),
        BindingState::Valid(usable)
            if usable.cas_thread_id() == &cas_thread
                && usable.represented_prefix() == represented
    );
    if !current_already_valid {
        execute(
            store,
            storage.publish_valid_binding(
                storage.revision(store).unwrap(),
                PublishValidBinding::new(
                    thread,
                    current.binding().revision(),
                    selected,
                    execution_binding(),
                    cas_thread.clone(),
                    represented,
                    native_turn_count,
                    tool_profile(),
                    lineage,
                ),
            ),
        );
    }
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
                active_loaded_generation,
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
    let cas_turn = CasTurnId::new(format!("phase10-turn-{turn}")).unwrap();
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
    let mut build = stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &syndic_storage::ProviderFrameStageBatch| {
            execute(
                store,
                storage.stage_provider_frame_batch(storage.revision(store).unwrap(), batch.clone()),
            );
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
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
            .unwrap();
    }
    panic!("bounded provider completion comparison did not finish");
}

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
    let cas_item_id = CasItemId::new(format!("phase10-user-{item_id}")).unwrap();
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

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

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
        CommandOutcome::NotCommitted { evidence } => {
            panic!("exact Syndic contribution unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("exact Syndic contribution committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("exact Syndic contribution indeterminate: {outcome:?}")
        }
    }
}
