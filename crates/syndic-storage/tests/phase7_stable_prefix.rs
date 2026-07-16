#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicItemId, SyndicProjectionId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AdmissionMarkers, AdvanceItemProjectionBuild, AssistantMessagePhase, CasTurnSource,
    ComposerAtom, ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    IdleSubmission, ItemProjectionBuildPhase, ItemProjectionGeneration, ItemProjectionIndexRecord,
    ItemProjectionSetRecord, PreparedContent, ProjectionLifecycle, ProviderItemDisposition,
    ProviderItemKind, SourceEventPayload, SourceEventText, SourceItemDescriptor,
    StartItemProjectionBuild, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TRANSCRIPT_PAGE_MAX_BYTES, TurnLifecycle,
};

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, establish_turn},
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

impl LiveAssistantFixture {
    fn descriptor(&self) -> SourceItemDescriptor {
        SourceItemDescriptor::new(
            self.item,
            self.cas_item.clone(),
            ProviderItemKind::AgentMessage,
            ProviderItemDisposition::CanonicalText,
        )
        .unwrap()
    }
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
            CreateThread::ordinary(thread, draft, timestamp(1)),
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
        thread_record.record().revision(),
        draft,
        current.draft().revision(),
        current.draft().content(),
        gate.record().revision(),
        draft_id(3),
        SyndicItemId::from_bytes([USER_ITEM_BYTE; 16]),
        AdmissionMarkers::default(),
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
    admit(
        store,
        storage,
        &fixture,
        SourceEventPayload::ItemStarted {
            item: fixture.descriptor(),
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(5),
    );
    admit(
        store,
        storage,
        &fixture,
        SourceEventPayload::ItemDelta {
            item_id: fixture.item,
            cas_item_id: fixture.cas_item.clone(),
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new(initial_text).unwrap(),
        },
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
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
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
            AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
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
            return set.record().clone();
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
    assert_eq!(records.len() as u64, set.record().projection_count());
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
    admit(
        store,
        storage,
        fixture,
        SourceEventPayload::ItemDelta {
            item_id: fixture.item,
            cas_item_id: fixture.cas_item.clone(),
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new(text).unwrap(),
        },
        timestamp(at),
    );
}

fn complete_item(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &LiveAssistantFixture,
    at: u64,
) {
    admit(
        store,
        storage,
        fixture,
        SourceEventPayload::ItemCompleted {
            item: fixture.descriptor(),
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(at),
    );
}

#[test]
fn live_closed_prefix_survives_resume_supersession_and_eof_promotion() {
    let home = TestHome::new("phase7-stable-prefix");
    let mut store = open(home.path());
    let mut storage = SyndicStorage::register(&mut store).unwrap();
    let initial = "stable paragraph\n\nopen suffix";
    let fixture = seed_live_assistant(&store, storage, initial);

    let generation_1 = ItemProjectionGeneration::FIRST;
    start_build(&store, storage, fixture.item, generation_1);
    advance_build_once(&store, storage, fixture.item, generation_1);
    let interrupted = storage
        .item_projection_build(&store, fixture.item, generation_1, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert!(matches!(
        interrupted.phase(),
        ItemProjectionBuildPhase::Parsing(_)
    ));
    assert_eq!(interrupted.projection_count(), 1);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .item_projection_build(&store, fixture.item, generation_1, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        &interrupted
    );

    let set_1 = finish_build(&store, storage, fixture.item, generation_1);
    assert_eq!(set_1.stable_projection_count(), 1);
    assert_eq!(set_1.projection_count(), 2);
    assert!(!set_1.stable_eof_resolved());
    assert_eq!(
        set_1.resume_checkpoint().consumed_source_bytes(),
        initial.len() as u64
    );
    assert_eq!(
        set_1.resume_checkpoint().closed_source_bytes(),
        "stable paragraph\n\n".len() as u64
    );
    let generation_1_records = read_generation(&store, storage, fixture.item, generation_1);

    append_text(&store, storage, &fixture, " continued", 7);
    let stale_head = storage
        .item_projection_head(&store, fixture.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stale_head.record().generation(), generation_1);
    assert_eq!(stale_head.record().lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(
        storage
            .item_projection_set(&store, fixture.item, generation_1, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        &set_1
    );

    let generation_2 = generation_1.checked_next().unwrap();
    start_build(&store, storage, fixture.item, generation_2);
    let resumed = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(resumed.record().projection_count(), 1);
    assert_eq!(
        resumed.record().phase().clone(),
        ItemProjectionBuildPhase::Parsing(set_1.resume_checkpoint().clone())
    );
    advance_build_once(&store, storage, fixture.item, generation_2);
    let resumed_after_append = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert_eq!(resumed_after_append.projection_count(), 1);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .item_projection_build(&store, fixture.item, generation_2, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        &resumed_after_append
    );

    append_text(&store, storage, &fixture, " again", 8);
    let superseded = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert!(matches!(
        superseded.phase(),
        ItemProjectionBuildPhase::Superseded(_)
    ));
    assert_eq!(superseded.projection_count(), 1);

    let generation_3 = generation_2.checked_next().unwrap();
    start_build(&store, storage, fixture.item, generation_3);
    let replacement = storage
        .item_projection_build(&store, fixture.item, generation_3, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.record().projection_count(), 1);
    assert_eq!(
        replacement.record().output_digest(),
        superseded.output_digest()
    );
    let set_3 = finish_build(&store, storage, fixture.item, generation_3);
    assert_eq!(set_3.stable_projection_count(), 1);
    assert_eq!(set_3.projection_count(), 2);
    assert!(!set_3.stable_eof_resolved());
    let generation_3_records = read_generation(&store, storage, fixture.item, generation_3);
    assert_eq!(
        generation_3_records[0].projection_id(),
        generation_1_records[0].projection_id()
    );
    assert_ne!(
        generation_3_records[1].projection_id(),
        generation_1_records[1].projection_id()
    );

    complete_item(&store, storage, &fixture, 9);
    assert_eq!(
        storage
            .turn_state(&store, fixture.turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .lifecycle(),
        TurnLifecycle::Active
    );
    let completed_head = storage
        .item_projection_head(&store, fixture.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(completed_head.record().generation(), generation_3);
    assert_eq!(
        completed_head.record().lifecycle(),
        ProjectionLifecycle::Stale
    );

    let generation_4 = generation_3.checked_next().unwrap();
    start_build(&store, storage, fixture.item, generation_4);
    let set_4 = finish_build(&store, storage, fixture.item, generation_4);
    assert_eq!(set_4.stable_projection_count(), 2);
    assert_eq!(set_4.projection_count(), 2);
    assert!(set_4.stable_eof_resolved());
    assert_eq!(set_4.stable_digest(), set_4.digest());
    assert_eq!(
        set_4.resume_checkpoint().closed_source_bytes(),
        set_4.source_bytes()
    );
    let generation_4_records = read_generation(&store, storage, fixture.item, generation_4);
    assert_eq!(
        projection_ids(&generation_4_records),
        projection_ids(&generation_3_records)
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    for generation in [generation_1, generation_3, generation_4] {
        read_generation(&reopened, reopened_storage, fixture.item, generation);
    }
    let retained_superseded = reopened_storage
        .item_projection_build(&reopened, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(retained_superseded.record(), &superseded);
    reopened.close().unwrap();
}
