#![cfg(feature = "test-faults")]

mod support;

#[path = "phase53_accepted_delivery/fixtures.rs"]
mod accepted_fixtures;
#[path = "phase58_accepted_next_pages/support.rs"]
mod accepted_next_support;

use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::{InputGateRevision, SyndicThreadId, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use accepted_next_support::{GenerationSpec, next_turn_records, set_gate_state};
use support::{TestHome, batch, commit, id, open, seed_detached_canonical_draft_backing};

const FIXTURE_DELTA_RECORDS_PER_COMMAND: usize = 96;

fn seeded(
    name: &str,
    mut records: Vec<FixtureRecord>,
) -> (TestHome, beryl_home_store::HomeStore, SyndicStorage) {
    let current_drafts = records
        .iter()
        .filter_map(|record| match record {
            FixtureRecord::Thread(thread) => Some(thread.current_draft_id()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    for current_draft in current_drafts {
        let root_history = seed_detached_canonical_draft_backing(
            &store,
            storage,
            SyndicThreadId::from_bytes([0xf3; 16]),
            current_draft,
        );
        let latest_admission = records
            .iter()
            .filter_map(|record| match record {
                FixtureRecord::AcceptedInput(input) => Some(input.admitted_at()),
                _ => None,
            })
            .max();
        for record in &mut records {
            let FixtureRecord::Draft(draft) = record else {
                continue;
            };
            if draft.id() != current_draft {
                continue;
            }
            let created_at = latest_admission.unwrap_or(draft.created_at());
            *draft = DraftRecord::new(
                draft.id(),
                draft.thread_id(),
                draft.revision(),
                draft.submission_intent(),
                root_history.clone(),
                created_at,
                created_at,
            );
        }
    }
    for delta in records.chunks(FIXTURE_DELTA_RECORDS_PER_COMMAND) {
        commit(&store, storage, batch(delta.iter().cloned()));
    }
    (home, store, storage)
}

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap()
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn sources(store: &beryl_home_store::HomeStore, storage: SyndicStorage) -> Vec<AcceptedNextSource> {
    let revision = storage.revision(store).unwrap();
    storage
        .accepted_next_source_page(store, revision, None, limits(256))
        .unwrap()
        .records()
        .iter()
        .cloned()
        .collect()
}

fn ordered_thread(value: u64) -> SyndicThreadId {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(&value.to_be_bytes());
    SyndicThreadId::from_bytes(bytes)
}

#[test]
fn global_source_pages_are_ordered_clamped_and_revision_fenced() {
    let records = (1..=300).flat_map(|value| {
        next_turn_records(
            value,
            ordered_thread(value),
            &[GenerationSpec::new(0, 1, NextTurnReason::PendingTurn)],
        )
    });
    let (_home, store, storage) = seeded("phase58-next-global-bounds", records.collect::<Vec<_>>());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let revision = storage.revision(&store).unwrap();
    let oversized = CursorReadLimits::new(usize::MAX, usize::MAX).unwrap();
    let first = storage
        .accepted_next_source_page(&store, revision, None, oversized)
        .unwrap();
    assert_eq!(first.records().len(), ACCEPTED_NEXT_PAGE_MAX_RECORDS);
    assert!(first.stored_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
    assert!(first.decoded_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
    assert!(first.records().windows(2).all(|pair| {
        (pair[0].thread_id(), pair[0].generation()) < (pair[1].thread_id(), pair[1].generation())
    }));
    assert!(
        first
            .records()
            .iter()
            .all(|source| source.source_revision() == revision)
    );
    let cursor = first
        .next_cursor()
        .expect("bounded page has a continuation");
    let second = storage
        .accepted_next_source_page(&store, revision, Some(cursor), oversized)
        .unwrap();
    assert_eq!(second.records().len(), 44);
    assert!(second.next_cursor().is_none());

    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedNextSource(
            AcceptedNextSourceRecord::new(
                ordered_thread(400),
                AcceptedRouteGeneration::FIRST,
                AcceptedRouteRevision::FIRST,
                AcceptedInputOrdinal::FIRST,
                AcceptedInputOrdinal::FIRST,
            ),
        )]),
    );
    assert!(matches!(
        storage.accepted_next_source_page(&store, revision, Some(cursor), oversized),
        Err(SyndicReadError::StaleAcceptedNextSourceScan)
    ));
    assert!(matches!(
        storage.accepted_next_source_page(
            &store,
            storage.revision(&store).unwrap(),
            Some(cursor),
            oversized,
        ),
        Err(SyndicReadError::InvalidAcceptedNextSourceCursor)
    ));
}

#[test]
fn candidate_pages_advance_terminal_history_and_stop_at_the_first_candidate() {
    let thread = id(80);
    let records = next_turn_records(
        80,
        thread,
        &[GenerationSpec::new(300, 2, NextTurnReason::PendingTurn)],
    );
    let (_home, store, storage) = seeded("phase58-next-terminal-heavy", records);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let source = sources(&store, storage)[0];
    let first = storage
        .accepted_next_candidate_page(&store, source, None, limits(256))
        .unwrap();
    assert!(first.candidate().is_none());
    assert!(first.stored_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
    assert!(first.decoded_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
    let cursor = first
        .next_cursor()
        .expect("terminal prefix must advance to later accepted order");

    let second = storage
        .accepted_next_candidate_page(&store, source, Some(cursor), limits(256))
        .unwrap();
    let candidate = second.candidate().expect("first effective candidate");
    assert_eq!(candidate.thread_id(), thread);
    assert_eq!(candidate.ordinal().get(), 301);
    assert_eq!(candidate.next_turn_reason(), NextTurnReason::PendingTurn);
    assert_eq!(candidate.source_revision(), source.source_revision());
    assert_eq!(
        candidate.leaf_revision(),
        beryl_model::AcceptedInputRevision::new(1).unwrap()
    );
    assert!(second.next_cursor().is_none());
    assert!(second.stored_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
    assert!(second.decoded_bytes() <= ACCEPTED_NEXT_PAGE_MAX_BYTES);
}

#[test]
fn source_order_is_exact_across_generations_and_later_same_thread_source_is_rejected() {
    let thread = id(83);
    let records = next_turn_records(
        83,
        thread,
        &[
            GenerationSpec::new(0, 1, NextTurnReason::PendingTurn),
            GenerationSpec::new(0, 1, NextTurnReason::Compaction),
        ],
    );
    let (_home, store, storage) = seeded("phase58-next-generation-order", records);
    let sources = sources(&store, storage);
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].thread_id(), thread);
    assert_eq!(sources[0].generation(), AcceptedRouteGeneration::FIRST);
    assert_eq!(
        sources[1].generation(),
        AcceptedRouteGeneration::new(2).unwrap()
    );

    let first = storage
        .accepted_next_candidate_page(&store, sources[0], None, limits(256))
        .unwrap();
    assert_eq!(first.candidate().unwrap().ordinal().get(), 1);
    assert!(first.next_cursor().is_none());
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, sources[1], None, limits(256)),
        Err(SyndicReadError::InvalidAcceptedNextCandidateSource)
    ));
}

#[test]
fn candidate_basis_accepts_an_unselected_current_head_from_another_generation() {
    let thread = id(85);
    let second = AcceptedRouteGeneration::new(2).unwrap();
    let mut records = next_turn_records(
        85,
        thread,
        &[
            GenerationSpec::new(0, 1, NextTurnReason::PendingTurn),
            GenerationSpec::new(0, 1, NextTurnReason::PendingTurn),
        ],
    );
    records.push(FixtureRecord::AcceptedRouteGenerationHead(
        AcceptedRouteGenerationHeadRecord::new(
            thread,
            AcceptedRouteHeadProof::new(second, AcceptedRouteRevision::FIRST),
        ),
    ));
    let (_home, store, storage) = seeded("phase58-next-unselected-head", records);
    let source = sources(&store, storage)[0];
    assert_eq!(source.generation(), AcceptedRouteGeneration::FIRST);
    assert!(
        storage
            .accepted_next_candidate_page(&store, source, None, limits(256))
            .unwrap()
            .candidate()
            .is_some()
    );
}

#[test]
fn candidate_cursor_from_another_source_is_rejected() {
    let first_thread = id(84);
    let second_thread = id(85);
    let mut records = next_turn_records(
        84,
        first_thread,
        &[GenerationSpec::new(1, 1, NextTurnReason::PendingTurn)],
    );
    records.extend(next_turn_records(
        85,
        second_thread,
        &[GenerationSpec::new(1, 1, NextTurnReason::Stop)],
    ));
    let (_home, store, storage) = seeded("phase58-next-wrong-source-cursor", records);
    let sources = sources(&store, storage);
    assert_eq!(sources.len(), 2);
    let first = storage
        .accepted_next_candidate_page(&store, sources[0], None, limits(1))
        .unwrap();
    let cursor = first.next_cursor().expect("one candidate remains");
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, sources[1], Some(cursor), limits(1)),
        Err(SyndicReadError::InvalidAcceptedNextCandidateCursor)
    ));
}

#[test]
fn projection_lost_routed_input_stays_ineligible_while_gate_is_not_idle() {
    let thread = id(40);
    let home = TestHome::new("phase58-next-projection-lost");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    accepted_fixtures::seed_mixed_abandonment(&store, storage);
    let request = accepted_fixtures::abandonment_request(&store, storage);
    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let source = sources(&store, storage)[0];
    let generation = syndic_storage::test_faults::accepted_route_generation(
        &store,
        storage,
        thread,
        AcceptedRouteGeneration::FIRST,
    )
    .unwrap();
    assert!(matches!(
        generation.target(),
        AcceptedRouteTarget::ProjectionLost(_)
    ));
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.selected_route(),
        Some(AcceptedRouteHeadProof::new(
            AcceptedRouteGeneration::FIRST,
            generation.revision(),
        ))
    );
    assert_eq!(
        syndic_storage::test_faults::accepted_route_generation_head(&store, storage, thread)
            .unwrap()
            .unwrap()
            .proof(),
        gate.selected_route().unwrap()
    );
    assert_ne!(gate.state(), &InputGateState::Idle);
    let page = storage
        .accepted_next_candidate_page(&store, source, None, limits(256))
        .unwrap();
    assert!(page.candidate().is_none());
}

#[test]
fn non_idle_gate_returns_a_completed_empty_candidate_page() {
    let thread = id(87);
    let mut records = next_turn_records(
        87,
        thread,
        &[GenerationSpec::new(0, 1, NextTurnReason::PendingTurn)],
    );
    set_gate_state(
        &mut records,
        thread,
        InputGateState::PendingTurn(SyndicTurnId::from_bytes([87; 16])),
    );
    let (_home, store, storage) = seeded("phase58-next-non-idle", records);
    let source = sources(&store, storage)[0];
    let page = storage
        .accepted_next_candidate_page(&store, source, None, limits(256))
        .unwrap();
    assert!(page.candidate().is_none());
    assert!(page.next_cursor().is_none());
    assert_eq!(page.stored_bytes(), 0);
    assert_eq!(page.decoded_bytes(), 0);
}

#[test]
fn gate_drift_stales_source_and_invalidates_its_cursor_at_the_new_revision() {
    let thread = id(88);
    let records = next_turn_records(
        88,
        thread,
        &[GenerationSpec::new(1, 1, NextTurnReason::PendingTurn)],
    );
    let (_home, store, storage) = seeded("phase58-next-gate-drift", records);
    let old_source = sources(&store, storage)[0];
    let first = storage
        .accepted_next_candidate_page(&store, old_source, None, limits(1))
        .unwrap();
    let old_cursor = first.next_cursor().expect("candidate remains");
    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(4).unwrap(),
                InputGateState::Idle,
                2,
                Some(AcceptedRouteGeneration::FIRST),
                None,
                0,
                1,
                0,
            )
            .unwrap(),
        )]),
    );
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, old_source, Some(old_cursor), limits(1)),
        Err(SyndicReadError::StaleAcceptedNextCandidateSource)
    ));
    let new_source = sources(&store, storage)[0];
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, new_source, Some(old_cursor), limits(1)),
        Err(SyndicReadError::InvalidAcceptedNextCandidateCursor)
    ));
}

#[test]
fn fresh_pages_reject_source_generation_and_gate_counter_incoherence() {
    let thread = id(89);
    let records = next_turn_records(
        89,
        thread,
        &[GenerationSpec::new(0, 1, NextTurnReason::PendingTurn)],
    );
    let (_home, store, storage) = seeded("phase58-next-authority-drift", records);
    let old_source = sources(&store, storage)[0];
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    thread,
                    AcceptedRouteGeneration::FIRST,
                    AcceptedRouteRevision::new(2).unwrap(),
                    AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                    Some(AcceptedInputOrdinal::FIRST),
                    Some(AcceptedInputOrdinal::FIRST),
                    1,
                    0,
                    0,
                    1,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
                thread,
                AcceptedRouteGeneration::FIRST,
                AcceptedRouteRevision::new(2).unwrap(),
                AcceptedInputOrdinal::FIRST,
                AcceptedInputOrdinal::FIRST,
            )),
        ]),
    );
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, old_source, None, limits(256)),
        Err(SyndicReadError::StaleAcceptedNextCandidateSource)
    ));
    let revised_source = sources(&store, storage)[0];
    assert!(
        storage
            .accepted_next_candidate_page(&store, revised_source, None, limits(256))
            .unwrap()
            .candidate()
            .is_some()
    );

    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(3).unwrap(),
                InputGateState::Idle,
                1,
                Some(AcceptedRouteGeneration::FIRST),
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        )]),
    );
    let incoherent_source = sources(&store, storage)[0];
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, incoherent_source, None, limits(256)),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn candidate_scan_rejects_order_and_leaf_identity_drift() {
    let thread = id(90);
    let mut records = next_turn_records(
        90,
        thread,
        &[GenerationSpec::new(0, 1, NextTurnReason::PendingTurn)],
    );
    for record in &mut records {
        if let FixtureRecord::AcceptedRouteLeaf(leaf) = record
            && leaf.thread_id() == thread
        {
            *leaf = AcceptedRouteLeafRecord::new(
                leaf.input_id(),
                leaf.thread_id(),
                leaf.generation(),
                AcceptedInputOrdinal::new(2).unwrap(),
                leaf.revision(),
                leaf.state().clone(),
                leaf.lifecycle(),
            );
            break;
        }
    }
    let (_home, store, storage) = seeded("phase58-next-leaf-identity-drift", records);
    let source = sources(&store, storage)[0];
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, source, None, limits(256)),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn idle_scan_rejects_a_source_claiming_work_without_an_effective_leaf() {
    let thread = id(91);
    let mut records = next_turn_records(
        91,
        thread,
        &[GenerationSpec::new(0, 2, NextTurnReason::PendingTurn)],
    );
    accepted_next_support::corrupt_effective_leaves_as_terminal(
        &mut records,
        thread,
        AcceptedRouteGeneration::FIRST,
    );
    let (_home, store, storage) = seeded("phase58-next-contradictory-source", records);
    let source = sources(&store, storage)[0];

    assert!(matches!(
        storage.accepted_next_candidate_page(&store, source, None, limits(256)),
        Err(SyndicReadError::Invariant(
            "accepted-next source claims work without an effective candidate"
        ))
    ));
}
