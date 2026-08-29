#![cfg(feature = "test-faults")]

#[path = "support/mod.rs"]
mod support;

use std::num::NonZeroUsize;

use beryl_model::{
    BindingRevision, InputGateRevision, ProjectionRevision, RecoveryItemSequenceRole,
    SyndicDraftId, SyndicItemId, SyndicPathDigest, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use beryl_stream::PagePool;
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::{
    TestHome, batch, commit, composer_content_records, draft_id, fixture_turn_state, id, open,
    seed_canonical_empty_thread, timestamp,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn same_home_path_records(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    tail: SyndicTurnId,
    digest: SyndicPathDigest,
    history_complete: bool,
    last_activity_at: SyndicTimestamp,
) -> Vec<FixtureRecord> {
    let thread_revision = ThreadRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(Some(tail), thread_revision, digest);
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
        Some(tail),
        digest,
        history_complete,
        last_activity_at,
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);
    vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            Some(tail),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
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
    ]
}

fn replay(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    projection: RecoveryProjection,
) -> Vec<(RecoveryItemSequenceRole, String)> {
    let mut cursor = storage.open_recovery_cursor(store, projection).unwrap();
    let pool = PagePool::new(
        NonZeroUsize::new(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    let mut lease = pool.try_lease().unwrap();
    let mut items: Vec<(RecoveryItemSequenceRole, String)> = Vec::new();
    loop {
        let Some(page) = storage
            .read_recovery_cursor_page(
                store,
                &mut cursor,
                lease,
                RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
            )
            .unwrap()
        else {
            break;
        };
        if page.item_offset() == 0 {
            items.push((page.role(), String::new()));
        }
        items.last_mut().unwrap().1.push_str(page.text());
        lease = page.into_page_lease();
    }
    items
}

#[test]
fn empty_current_path_is_native_fresh_without_model_metadata_or_state_change() {
    let home = TestHome::new("phase9-recovery-current-empty");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(41);
    seed_canonical_empty_thread(&store, storage, thread, draft_id(42));
    let thread_record = storage
        .thread(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = SelectedPathProof::new(
        thread_record.committed_tail(),
        thread_record.revision(),
        thread_record.selected_path_digest(),
    );
    let before_revision = storage.revision(&store).unwrap();
    let before_draft = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let request = RecoveryProjectionRequest::for_current_selected_path(thread, selected, None);
    assert_eq!(
        request.scope(),
        RecoveryProjectionScope::CurrentSelectedPath
    );

    let RecoveryAssembly::NativeEmptyPrefix {
        thread_id,
        selected_path,
        source_revision,
    } = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("canonical empty path must select native Fresh");
    };
    assert_eq!(thread_id, thread);
    assert_eq!(selected_path, selected);
    assert_eq!(selected_path.tail(), None);
    assert_eq!(source_revision, before_revision);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_draft
    );
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
}

#[test]
fn current_preflight_equals_the_later_pending_parent_projection_without_state_change() {
    let home = TestHome::new("phase9-recovery-current-pending-equivalence");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(210);
    let draft = draft_id(211);
    let completed = SyndicTurnId::from_bytes([212; 16]);
    let item = SyndicItemId::from_bytes([213; 16]);
    let completed_digest = root_turn_chain_digest(completed);
    seed_canonical_empty_thread(&store, storage, thread, draft);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("retained user").unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let revision = ProjectionRevision::new(1).unwrap();
    let mut records = same_home_path_records(
        &store,
        storage,
        thread,
        draft,
        completed,
        completed_digest,
        true,
        timestamp(3),
    );
    records.extend(content_records);
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            completed,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            completed_digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            completed,
            TurnStateRevision::FIRST,
            TurnLifecycle::Complete,
            0,
            1,
            timestamp(3),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            completed,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            completed,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
    ]);
    commit(&store, storage, batch(records));
    let current_selected = SelectedPathProof::new(
        Some(completed),
        ThreadRevision::new(1).unwrap(),
        completed_digest,
    );
    let before_revision = storage.revision(&store).unwrap();
    let before_draft = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let RecoveryAssembly::Ready(preflight) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(
                thread,
                current_selected,
                Some(100_000),
            ),
        )
        .unwrap()
    else {
        panic!("completed current path must be ready")
    };
    let preflight_items = replay(&store, storage, preflight);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_draft
    );
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

    let pending = SyndicTurnId::from_bytes([214; 16]);
    let pending_digest = child_turn_chain_digest(pending, completed, completed_digest);
    let mut pending_records = same_home_path_records(
        &store,
        storage,
        thread,
        draft,
        pending,
        pending_digest,
        false,
        timestamp(4),
    );
    pending_records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            pending,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(completed),
            Some(completed),
            TurnDepth::new(2).unwrap(),
            pending_digest,
            timestamp(4),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            pending,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            timestamp(4),
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            completed,
            pending,
            TurnDepth::new(2).unwrap(),
            pending_digest,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(1).unwrap(),
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
    ]);
    commit(&store, storage, batch(pending_records));
    let pending_selected = SelectedPathProof::new(
        Some(pending),
        ThreadRevision::new(1).unwrap(),
        pending_digest,
    );
    let RecoveryAssembly::Ready(restarted) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                thread,
                pending_selected,
                Some(100_000),
            ),
        )
        .unwrap()
    else {
        panic!("pending-parent path must be ready")
    };
    assert_eq!(
        preflight_items,
        vec![(
            RecoveryItemSequenceRole::UserInputText,
            "retained user".to_owned(),
        )]
    );
    assert_eq!(preflight_items, replay(&store, storage, restarted));
    assert_eq!(preflight.version(), restarted.version());
    assert_eq!(preflight.item_count(), restarted.item_count());
    assert_eq!(preflight.utf8_bytes(), restarted.utf8_bytes());
    assert_eq!(preflight.sequence_digest(), restarted.sequence_digest());
    assert_eq!(
        preflight.represented_prefix(),
        restarted.represented_prefix()
    );
    assert_ne!(preflight.selected_path(), restarted.selected_path());
    assert_ne!(preflight.source_revision(), restarted.source_revision());
    store.close().unwrap();
}

#[test]
fn cursor_and_ready_proof_reject_source_revision_drift_before_emitting_text() {
    let home = TestHome::new("phase9-recovery-cursor-revision");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(200);
    let turn = SyndicTurnId::from_bytes([201; 16]);
    let item = SyndicItemId::from_bytes([202; 16]);
    let digest = root_turn_chain_digest(turn);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("retained user").unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let revision = ProjectionRevision::new(1).unwrap();
    let draft = draft_id(201);
    seed_canonical_empty_thread(&store, storage, thread, draft);
    let mut records = same_home_path_records(
        &store,
        storage,
        thread,
        draft,
        turn,
        digest,
        true,
        timestamp(3),
    );
    records.extend(content_records);
    records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Complete,
            0,
            1,
            timestamp(3),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
    ]);
    commit(&store, storage, batch(records));
    let selected = SelectedPathProof::new(Some(turn), ThreadRevision::new(1).unwrap(), digest);
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(100_000)),
        )
        .unwrap()
    else {
        panic!("canonical completed history must be ready")
    };
    let mut cursor = storage.open_recovery_cursor(&store, projection).unwrap();

    seed_canonical_empty_thread(&store, storage, id(203), draft_id(204));
    let pool = PagePool::new(
        NonZeroUsize::new(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        storage.read_recovery_cursor_page(
            &store,
            &mut cursor,
            pool.try_lease().unwrap(),
            RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
        ),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
    assert!(matches!(
        storage.open_recovery_cursor(&store, projection),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
    assert_eq!(pool.diagnostics().available, 1);
    store.close().unwrap();
}
