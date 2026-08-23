#![cfg(feature = "test-faults")]

mod support;

use std::sync::Mutex;

use beryl_model::{SyndicTurnId, ThreadRevision};
use syndic_storage::test_faults::{
    FixtureRecord, reset_validation_page_metrics, validation_page_metrics,
};
use syndic_storage::{
    ConversationParent, SourceEventPayload, SourceEventRecord, SourceEventSequence, SyndicStorage,
    TurnChildIndexRecord, TurnDepth, TurnEndStatus, TurnKind, TurnLifecycle, TurnRecord,
    TurnStateRevision, TurnTerminalOutcome, child_turn_chain_digest, root_turn_chain_digest,
};

use support::*;

static VALIDATION_METRICS_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn empty_and_populated_domains_reopen_authoritatively() {
    let _metrics_guard = VALIDATION_METRICS_LOCK.lock().unwrap();
    let home = TestHome::new("reopen");
    let mut store = open(home.path());
    let _storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    seed_canonical_empty_thread(&reopened, storage, id(1), draft_id(2));
    commit(
        &reopened,
        storage,
        batch(empty_thread_records(id(1), draft_id(2))),
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let expected_revision = storage.revision(&reopened).unwrap();
    reopened.close().unwrap();

    let mut final_open = open(home.path());
    let storage = SyndicStorage::register(&mut final_open).unwrap();
    assert_eq!(storage.revision(&final_open).unwrap(), expected_revision);
    final_open
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    final_open.close().unwrap();
}

#[test]
fn large_shared_and_unreachable_history_validates_with_bounded_pages() {
    let _metrics_guard = VALIDATION_METRICS_LOCK.lock().unwrap();
    let home = TestHome::new("large-history");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);

    let thread_a = id(10);
    let thread_b = id(11);
    seed_canonical_empty_thread(&store, storage, thread_a, draft_id(20));
    seed_canonical_empty_thread(&store, storage, thread_b, draft_id(21));
    let mut history_delta = Vec::new();
    let mut parent = None;
    let mut parent_digest = None;
    let mut root = None;
    let mut ancestry = Vec::new();
    let mut transcript_path = Vec::new();
    for number in 1..=320_u16 {
        let bytes = number.to_be_bytes();
        let mut raw = [0_u8; 16];
        raw[0] = bytes[0];
        raw[1] = bytes[1];
        raw[15] = 0x44;
        let turn_id = SyndicTurnId::from_bytes(raw);
        let digest = match (parent, parent_digest) {
            (Some(parent), Some(parent_digest)) => {
                child_turn_chain_digest(turn_id, parent, parent_digest)
            }
            _ => {
                root = Some(turn_id);
                root_turn_chain_digest(turn_id)
            }
        };
        let depth = TurnDepth::new(u64::from(number)).unwrap();
        let ancestor_skip = if depth == TurnDepth::FIRST {
            None
        } else {
            let skip_depth = (depth.get() & (depth.get() - 1)).max(1);
            Some(ancestry[usize::try_from(skip_depth - 1).unwrap()])
        };
        history_delta.push(FixtureRecord::Turn(TurnRecord::new(
            turn_id,
            thread_a,
            TurnKind::OrdinaryUser,
            ConversationParent::from_turn(parent),
            ancestor_skip,
            depth,
            digest,
            timestamp(u64::from(number)),
        )));
        history_delta.push(FixtureRecord::TurnState(fixture_turn_state(
            turn_id,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            timestamp(u64::from(number)),
        )));
        history_delta.push(FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn_id,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ));
        if let Some(parent) = parent {
            history_delta.push(FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                parent, turn_id, depth, digest,
            )));
        }
        ancestry.push(turn_id);
        transcript_path.push((
            turn_id,
            digest,
            TurnLifecycle::Interrupted,
            1,
            timestamp(u64::from(number)),
        ));
        parent = Some(turn_id);
        parent_digest = Some(digest);
        if number % 32 == 0 {
            commit(&store, storage, batch(std::mem::take(&mut history_delta)));
        }
    }
    if !history_delta.is_empty() {
        commit(&store, storage, batch(history_delta));
    }
    let tail = parent.unwrap();
    let digest = parent_digest.unwrap();

    let unreachable = SyndicTurnId::from_bytes([0xEE; 16]);
    let root = root.unwrap();
    let root_record_digest = root_turn_chain_digest(root);
    let unreachable_digest = child_turn_chain_digest(unreachable, root, root_record_digest);
    let unreachable_delta = vec![
        FixtureRecord::Turn(TurnRecord::new(
            unreachable,
            thread_a,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(root),
            Some(root),
            TurnDepth::new(2).unwrap(),
            unreachable_digest,
            timestamp(999),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            unreachable,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            0,
            0,
            timestamp(999),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            root,
            unreachable,
            TurnDepth::new(2).unwrap(),
            unreachable_digest,
        )),
    ];
    commit(&store, storage, batch(unreachable_delta));

    for (thread, draft) in [(thread_a, draft_id(20)), (thread_b, draft_id(21))] {
        let mut thread_delta =
            thread_records_with_activity(thread, draft, Some(tail), digest, timestamp(320));
        thread_delta.extend(item_free_transcript_build_records(
            thread,
            ThreadRevision::new(1).unwrap(),
            &transcript_path,
        ));
        commit(&store, storage, batch(thread_delta));
    }
    reset_validation_page_metrics();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_bounded_turn_pages();
    store.close().unwrap();

    let mut reopened = open(home.path());
    reset_validation_page_metrics();
    SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(validation_page_metrics().page_count(), 0);
    reopened.close().unwrap();

    let mut scrubbed = open(home.path());
    SyndicStorage::register(&mut scrubbed).unwrap();
    scrubbed
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_bounded_turn_pages();
    scrubbed.close().unwrap();
}

fn assert_bounded_turn_pages() {
    let metrics = validation_page_metrics();
    assert!(metrics.page_count() > metrics.turn_page_count());
    assert!(metrics.turn_page_count() > 1);
    assert_eq!(metrics.item_limit(), 64);
    assert!(metrics.max_page_items() <= metrics.item_limit());
    assert!(metrics.max_page_stored_bytes() <= metrics.byte_limit());
}
