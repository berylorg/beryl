use super::pending_for_handoff;
use crate::support;
use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use std::cell::Cell;
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, RecoveredPendingSource, SettleLifecycleCompaction,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, TurnKind,
};

#[allow(dead_code)]
mod compaction_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../syndic-storage/tests/compaction_storage/support.rs"
    ));
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn source(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> RecoveredPendingSource {
    *storage
        .recovered_pending_page(
            store,
            storage.revision(store).unwrap(),
            None,
            CursorReadLimits::new(16, syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES)
                .unwrap(),
            limit(),
        )
        .unwrap()
        .records()
        .iter()
        .find(|source| source.thread_id() == thread)
        .unwrap()
}

fn unrelated_write(store: &HomeStore, storage: &SyndicStorage, seed: u8) {
    support::discussion_input::committed(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([seed; 16]),
                SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                support::exact_cas::execution_binding(),
                support::timestamp(1000),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
}

fn bypasses_discussion_read(
    store: &HomeStore,
    storage: &SyndicStorage,
    original: RecoveredPendingSource,
) {
    let pending = original.pending_dispatch_evidence();
    unrelated_write(store, storage, 243);
    assert_ne!(storage.revision(store).unwrap(), original.source_revision());
    let called = Cell::new(false);
    let reservation = pending_for_handoff(Some(original), || {
        called.set(true);
        syndic_storage::test_faults::pending_dispatch_evidence_with_confirmation_hook(
            storage,
            store,
            pending.thread_id(),
            limit(),
            || unrelated_write(store, storage, 245),
        )
    })
    .unwrap();
    assert_eq!(reservation, None);
    assert!(!called.get());
    let current = storage
        .pending_dispatch_evidence(store, pending.thread_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.turn_id(), pending.turn_id());
    assert_eq!(current.turn_kind(), pending.turn_kind());
    assert_eq!(current.gate_revision(), pending.gate_revision());
    assert_eq!(current.state_revision(), pending.state_revision());
}

#[test]
fn authenticated_ordinary_pending_preparation_ignores_unrelated_discussion_read_contention() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = support::open(directory.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = support::id(201);
    support::seed_canonical_empty_thread(&store, storage.clone(), thread, support::draft_id(202));
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        thread,
        support::draft_id(203),
        beryl_model::SyndicItemId::from_bytes([204; 16]),
        "ordinary pending preparation",
        support::timestamp(2),
    );
    let original = source(&store, &storage, thread);
    assert_eq!(
        original.pending_dispatch_evidence().turn_kind(),
        TurnKind::OrdinaryUser
    );
    bypasses_discussion_read(&store, &storage, original);
    store.close().unwrap();
}

#[test]
fn authenticated_lifecycle_pending_preparation_ignores_unrelated_discussion_read_contention() {
    let fixture =
        compaction_support::CompactionFixture::new("preparation-lifecycle-provenance", 30);
    let id = fixture.admit(40, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    fixture.publish_request(id, syndic_storage::CompactionRequestDisposition::Accepted);
    let content = fixture.prepare_lifecycle_content();
    let operation = fixture.operation(id);
    compaction_support::execute(
        &fixture.store,
        fixture.storage.settle_lifecycle_compaction(
            fixture.storage.revision(&fixture.store).unwrap(),
            SettleLifecycleCompaction::new(&operation, content, compaction_support::timestamp(40)),
        ),
    );
    let original = source(&fixture.store, &fixture.storage, fixture.thread);
    assert_eq!(
        original.pending_dispatch_evidence().turn_kind(),
        TurnKind::BerylLifecycleContinuation
    );
    bypasses_discussion_read(&fixture.store, &fixture.storage, original);
}

#[test]
fn generated_and_unknown_preparation_keep_fresh_pending_evidence_and_preserve_source_on_drift() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = support::open(directory.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let (input, receipt, child_gate) =
        support::generated_input::seed(&store, storage.clone(), "original generated input");
    let original = source(&store, &storage, input.thread_id());
    assert_eq!(
        original.pending_dispatch_evidence().turn_kind(),
        TurnKind::BerylDiscussionHandoff
    );
    let before = storage
        .pending_dispatch_evidence(&store, input.thread_id(), limit())
        .unwrap()
        .unwrap();
    for (index, provenance) in [Some(original), None].into_iter().enumerate() {
        let called = Cell::new(false);
        let result = pending_for_handoff(provenance, || {
            called.set(true);
            syndic_storage::test_faults::pending_dispatch_evidence_with_confirmation_hook(
                &storage,
                &store,
                input.thread_id(),
                limit(),
                || unrelated_write(&store, &storage, 243 + index as u8 * 2),
            )
        });
        assert!(called.get());
        assert!(matches!(
            result,
            Err(SyndicReadError::ConcurrentChange { .. })
        ));
        let current = pending_for_handoff(provenance, || {
            storage.pending_dispatch_evidence(&store, input.thread_id(), limit())
        })
        .unwrap()
        .unwrap();
        assert_eq!(current.turn_id(), receipt.parent_turn_id);
        assert_eq!(current.item_id(), receipt.canonical_item_id);
        assert_eq!(current.turn_kind(), before.turn_kind());
        assert_eq!(current.gate_revision(), before.gate_revision());
        assert_eq!(current.state_revision(), before.state_revision());
        assert_eq!(
            storage
                .accepted_input(&store, input.id(), limit())
                .unwrap()
                .unwrap(),
            input
        );
        assert_eq!(
            storage
                .discussion_handoff_gate(&store, child_gate.thread_id(), limit())
                .unwrap()
                .unwrap(),
            child_gate
        );
    }
    store.close().unwrap();
}

#[test]
fn original_ordinary_preparation_cannot_reinterpret_a_successor_generated_pending_turn() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = support::open(directory.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let (request, original) =
        support::generated_input::seed_pending_with_original_parent(&store, &storage);
    let syndic_storage::DiscussionParentEligibility::Proven(parent) =
        storage.prepare_discussion_parent(&store, request).unwrap()
    else {
        panic!("the original discussion parent must be ready");
    };
    let prepared = storage
        .prepare_generated_discussion_input(
            &store,
            parent,
            syndic_storage::GeneratedDiscussionInput {
                parent_turn_id: beryl_model::SyndicTurnId::from_bytes([234; 16]),
                canonical_item_id: beryl_model::SyndicItemId::from_bytes([232; 16]),
                resolution: "exact successor result".to_owned(),
                admitted_at: support::timestamp(100),
            },
        )
        .unwrap();
    support::discussion_input::committed(&store, prepared.into_contribution());
    let current = source(&store, &storage, original.thread_id()).pending_dispatch_evidence();
    assert_eq!(
        original.pending_dispatch_evidence().turn_kind(),
        TurnKind::OrdinaryUser
    );
    assert_eq!(current.turn_kind(), TurnKind::BerylDiscussionHandoff);
    assert_ne!(current.turn_id(), original.turn_id());
    assert_eq!(
        pending_for_handoff(Some(original), || panic!(
            "original ordinary provenance does not claim the new generated source"
        ))
        .unwrap(),
        None
    );
    assert!(matches!(
        storage.prepare_native_projection(
            &store,
            &syndic_storage::NativeProjectionRequest::new(
                original.thread_id(),
                original.pending_dispatch_evidence().selected_path(),
                support::exact_cas::execution_binding(),
                support::test_tool_profile()
            ),
            limit()
        ),
        Err(syndic_storage::NativeProjectionError::StaleSelectedPath)
    ));
    assert_eq!(
        storage
            .pending_dispatch_evidence(&store, current.thread_id(), limit())
            .unwrap(),
        Some(current)
    );
    store.close().unwrap();
}
