#![cfg(feature = "test-faults")]

use std::convert::Infallible;

mod support;

#[path = "phase6_fault_reconciliation/delta_persistence.rs"]
mod delta_persistence;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasItemId, CasTurnId, SyndicContentId, SyndicItemId};
use syndic_storage::*;

use support::exact_cas::admit_item_frame;
use support::populated::{active_turn, cas_thread, cas_turn, populated_records};
use support::{TestHome, batch, commit, id, open, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn assert_committed(outcome: beryl_home_store::CommandOutcome) {
    match outcome {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("unexpected live-history reconciliation command outcome: {outcome:?}"),
    }
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn exact_source() -> CasTurnSource {
    CasTurnSource::new(cas_thread(), cas_turn())
}

fn source_event(
    store: &HomeStore,
    storage: SyndicStorage,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let turn = active_turn();
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    let gate = storage.input_gate(store, id(40), limit()).unwrap().unwrap();
    LiveSourceEvent::new(
        id(40),
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(exact_source()),
        payload,
        observed_at,
    )
    .unwrap()
}

fn agent_value(text: impl Into<String>) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    })
}

fn agent_start(cas_item: CasItemId, observed_at: SyndicTimestamp) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        cas_item,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
            item: agent_value(""),
        },
    )
}

fn agent_delta(cas_item: CasItemId, text: impl Into<String>) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::new(2).unwrap(),
        cas_item,
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
            delta: ProviderTextV1::inline(text),
        }),
    )
}

fn agent_completion(
    cas_item: CasItemId,
    text: impl Into<String>,
    observed_at: SyndicTimestamp,
) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::new(3).unwrap(),
        cas_item,
        ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(observed_at.unix_millis()),
            item: agent_value(text),
        },
    )
}

fn start_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId, cas_item: &CasItemId) {
    admit_item_frame(
        store,
        storage,
        id(40),
        active_turn(),
        item,
        &exact_source(),
        agent_start(cas_item.clone(), timestamp(9)),
        timestamp(9),
    );
}

fn provider_content_id(item_id: SyndicItemId) -> SyndicContentId {
    let mut bytes = *item_id.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0xa5;
    }
    SyndicContentId::from_bytes(bytes)
}

fn prepare_item_frame(
    store: &HomeStore,
    storage: SyndicStorage,
    item_id: SyndicItemId,
    frame: ProviderItemFrameV1,
) -> PreparedProviderFrame {
    let turn = active_turn();
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    let source_event = SourceEventSequence::new(state.source_event_count() + 1).unwrap();
    let prior = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .and_then(|item| item.provider().cloned());
    let source = CasItemSource::new(exact_source(), frame.item_id().clone());
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item_id,
            turn,
            source,
            source_event,
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item_id,
            turn,
            source,
            source_event,
            provider_content_id(item_id),
            frame,
        ),
    };
    prepare_provider_frame(plan).unwrap()
}

fn prepared_item_target(
    store: &HomeStore,
    storage: SyndicStorage,
    item_id: SyndicItemId,
    frame: ProviderItemFrameV1,
) -> SealedProviderFrameReference {
    prepare_item_frame(store, storage, item_id, frame)
        .target()
        .clone()
}

fn stage_item_frame_for_publication(
    store: &HomeStore,
    storage: SyndicStorage,
    item_id: SyndicItemId,
    frame: ProviderItemFrameV1,
) -> SealedProviderFrameReference {
    let prepared = prepare_item_frame(store, storage, item_id, frame);
    execute(
        store,
        storage.begin_provider_frame_build(storage.revision(store).unwrap(), &prepared),
    )
    .unwrap();
    let mut build = stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            execute(
                store,
                storage.stage_provider_frame_batch(storage.revision(store).unwrap(), batch.clone()),
            )
            .unwrap();
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    for _ in 0..4_096 {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            assert_eq!(build.target(), prepared.target());
            return prepared.target().clone();
        }
        execute(
            store,
            storage.compare_provider_completion(storage.revision(store).unwrap(), build),
        )
        .unwrap();
        build = storage
            .provider_item_build(store, item_id, limit())
            .unwrap()
            .unwrap()
            .clone();
    }
    panic!("bounded provider completion comparison did not finish");
}

fn item_text(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) -> String {
    let item = storage
        .canonical_item(store, item, limit())
        .unwrap()
        .unwrap();
    let provider = item.provider().unwrap();
    let mut after = None;
    let mut bytes = Vec::new();
    loop {
        let page = storage
            .content_chunks(
                store,
                provider.content().id(),
                after,
                CursorReadLimits::new(8, 1_000_000).unwrap(),
            )
            .unwrap();
        for chunk in page.records() {
            bytes.extend_from_slice(chunk.bytes());
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    let start = usize::try_from(provider.frame().encoded_start()).unwrap();
    let end = usize::try_from(provider.frame().encoded_end()).unwrap();
    let frame = decode_bounded_provider_item_frame_v1(
        &bytes[start..end],
        PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
        provider.frame().encoded_start(),
    )
    .unwrap();
    let text = match frame.observation() {
        ProviderItemObservationV1::Started {
            item: ProviderItemV1::AgentMessage(message),
            ..
        }
        | ProviderItemObservationV1::Completed {
            item: ProviderItemV1::AgentMessage(message),
            ..
        } => &message.text,
        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage { delta }) => delta,
        _ => panic!("assistant fixture retained an unexpected provider frame"),
    };
    text.inline_str().unwrap().to_owned()
}

#[test]
fn live_items_require_the_exact_active_cas_turn_and_item_identity() {
    let home = TestHome::new("phase6-external-identity");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let item = SyndicItemId::from_bytes([70; 16]);
    let cas_item = CasItemId::new("phase6-exact-item").unwrap();
    let state = storage
        .turn_state(&store, active_turn(), limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(40), limit())
        .unwrap()
        .unwrap();
    let mismatched_frame = prepared_item_target(
        &store,
        storage,
        item,
        agent_start(cas_item.clone(), timestamp(9)),
    );
    let mismatched = LiveSourceEvent::new(
        id(40),
        active_turn(),
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count().checked_add(1).unwrap()).unwrap(),
        Some(CasTurnSource::new(
            cas_thread(),
            CasTurnId::new("different-turn").unwrap(),
        )),
        SourceEventPayload::ItemFrame {
            item_id: item,
            frame: Box::new(mismatched_frame),
        },
        timestamp(9),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), mismatched),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));

    start_item(&store, storage, item, &cas_item);
    let colliding_item = SyndicItemId::from_bytes([72; 16]);
    let colliding_frame = stage_item_frame_for_publication(
        &store,
        storage,
        colliding_item,
        agent_start(cas_item.clone(), timestamp(10)),
    );
    let wrong_item = source_event(
        &store,
        storage,
        SourceEventPayload::ItemFrame {
            item_id: colliding_item,
            frame: Box::new(colliding_frame),
        },
        timestamp(10),
    );
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), wrong_item),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));

    admit_item_frame(
        &store,
        storage,
        id(40),
        active_turn(),
        item,
        &exact_source(),
        agent_delta(cas_item.clone(), "exact"),
        timestamp(10),
    );
    admit_item_frame(
        &store,
        storage,
        id(40),
        active_turn(),
        item,
        &exact_source(),
        agent_completion(cas_item.clone(), "exact", timestamp(11)),
        timestamp(11),
    );

    let record = storage
        .canonical_item(&store, item, limit())
        .unwrap()
        .unwrap();
    let source = record.cas_source().unwrap();
    assert_eq!(source.turn(), &exact_source());
    assert_eq!(source.item_id(), &cas_item);
    assert_eq!(record.source_event_count(), 3);
    assert_eq!(item_text(&store, storage, item), "exact");
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
