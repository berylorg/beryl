#![cfg(feature = "test-faults")]

#[path = "phase7_stable_prefix/cases.rs"]
mod cases;
mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicItemId, SyndicProjectionId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AdvanceItemProjectionBuild, CasTurnSource, ComposerAtom, ComposerPayload, CreateThread,
    DraftPayloadUpdate, DraftPayloadUpdateDecision, IdleSubmission, ItemProjectionBuildPhase,
    ItemProjectionGeneration, ItemProjectionIndexRecord, ItemProjectionSetRecord, PreparedContent,
    ProjectionLifecycle, ProviderAgentMessageV1, ProviderFrameOrdinalV1, ProviderItemDeltaV1,
    ProviderItemFrameV1, ProviderItemObservationV1, ProviderItemV1, ProviderLifecycleTimestampMsV1,
    ProviderMessagePhaseV1, ProviderTextV1, SourceEventPayload, StartItemProjectionBuild,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TRANSCRIPT_PAGE_MAX_BYTES, TurnLifecycle,
};

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, admit_item_frame, establish_turn},
    id, open, stage_prepared_content, timestamp,
};

const USER_ITEM_BYTE: u8 = 4;
const ASSISTANT_ITEM_BYTE: u8 = 5;

struct LiveAssistantFixture {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    cas_item: CasItemId,
    source: CasTurnSource,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn admit(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &LiveAssistantFixture,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    admit_event(
        store,
        storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        payload,
        observed_at,
    );
}

fn agent_value(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    })
}

fn next_frame_ordinal(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
) -> ProviderFrameOrdinalV1 {
    storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap()
        .provider()
        .unwrap()
        .stream_state()
        .next_ordinal()
}

fn seed_live_assistant(
    store: &HomeStore,
    storage: SyndicStorage,
    initial_text: &str,
) -> LiveAssistantFixture {
    let thread = id(1);
    let draft = draft_id(2);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );

    let payload = ComposerPayload::new(vec![ComposerAtom::text("question").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );

    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        thread_record.revision(),
        draft,
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        draft_id(3),
        SyndicItemId::from_bytes([USER_ITEM_BYTE; 16]),
        None,
        timestamp(3),
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );

    let source = establish_turn(store, storage, thread, turn, timestamp(4));
    let fixture = LiveAssistantFixture {
        thread,
        turn,
        item: SyndicItemId::from_bytes([ASSISTANT_ITEM_BYTE; 16]),
        cas_item: CasItemId::new("phase7-stable-prefix-assistant").unwrap(),
        source,
    };
    admit(
        store,
        storage,
        &fixture,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    admit_item_frame(
        store,
        storage,
        fixture.thread,
        fixture.turn,
        fixture.item,
        &fixture.source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(5),
                item: agent_value(""),
            },
        ),
        timestamp(5),
    );
    admit_item_frame(
        store,
        storage,
        fixture.thread,
        fixture.turn,
        fixture.item,
        &fixture.source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(initial_text),
            }),
        ),
        timestamp(6),
    );
    fixture
}

fn start_build(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
        ),
    );
}

fn advance_build_once(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
) {
    let build = storage
        .item_projection_build(store, item, generation, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.advance_item_projection_build(
            storage.revision(store).unwrap(),
            AdvanceItemProjectionBuild::new(item, generation, build.revision()),
        ),
    );
}

fn finish_build(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
) -> ItemProjectionSetRecord {
    for _ in 0..32 {
        if let Some(set) = storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
        {
            return set.clone();
        }
        advance_build_once(store, storage, item, generation);
    }
    panic!("bounded item projection did not finish");
}

fn read_generation(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
) -> Vec<ItemProjectionIndexRecord> {
    let set = storage
        .item_projection_set(store, item, generation, point_limit())
        .unwrap()
        .unwrap();
    let mut records = Vec::new();
    let mut after = None;
    let mut expected_ordinal = 1_u64;
    loop {
        let page = storage
            .item_projections(
                store,
                item,
                generation,
                after,
                CursorReadLimits::new(1, TRANSCRIPT_PAGE_MAX_BYTES).unwrap(),
            )
            .unwrap();
        assert!(page.stored_bytes() <= TRANSCRIPT_PAGE_MAX_BYTES);
        assert_eq!(page.records().len(), 1);
        let record = page.records()[0].clone();
        assert_eq!(record.item_id(), item);
        assert_eq!(record.generation(), generation);
        assert_eq!(record.ordinal().get(), expected_ordinal);
        expected_ordinal += 1;
        after = Some(record.ordinal());
        records.push(record);
        if !page.has_more() {
            break;
        }
    }
    assert_eq!(records.len() as u64, set.projection_count());
    records
}

fn projection_ids(records: &[ItemProjectionIndexRecord]) -> Vec<SyndicProjectionId> {
    records
        .iter()
        .map(ItemProjectionIndexRecord::projection_id)
        .collect()
}

fn append_text(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &LiveAssistantFixture,
    text: &str,
    at: u64,
) {
    admit_item_frame(
        store,
        storage,
        fixture.thread,
        fixture.turn,
        fixture.item,
        &fixture.source,
        ProviderItemFrameV1::new(
            next_frame_ordinal(store, storage, fixture.item),
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(text),
            }),
        ),
        timestamp(at),
    );
}

fn complete_item(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &LiveAssistantFixture,
    text: &str,
    at: u64,
) {
    admit_item_frame(
        store,
        storage,
        fixture.thread,
        fixture.turn,
        fixture.item,
        &fixture.source,
        ProviderItemFrameV1::new(
            next_frame_ordinal(store, storage, fixture.item),
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(at),
                item: agent_value(text),
            },
        ),
        timestamp(at),
    );
}
