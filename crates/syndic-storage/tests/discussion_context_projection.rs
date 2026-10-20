#![cfg(feature = "test-faults")]

#[path = "discussion_context_projection/classification.rs"]
mod classification;
#[path = "discussion_context_projection/encoding.rs"]
mod encoding;
#[path = "discussion_context_projection/execution.rs"]
mod execution;
#[path = "discussion_context_projection/lineage.rs"]
mod lineage;
#[path = "native_projection/fixtures.rs"]
mod native_fixtures;
#[path = "discussion_context_projection/reopen.rs"]
mod reopen;
mod support;

use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    SyndicItemId,
};
use beryl_stream::PagePool;
use std::num::NonZeroUsize;
use support::{TestHome, draft_id, id, open, seed_populated, timestamp};
use syndic_storage::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn seeded(name: &str) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let root = beryl_model::SyndicTurnId::from_bytes([29; 16]);
    let root_item = SyndicItemId::from_bytes([209; 16]);
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("source question").unwrap()]).unwrap();
    let (content, mut records) = support::composer_content_records(&payload);
    let revision = beryl_model::ProjectionRevision::new(1).unwrap();
    records.extend([
        test_faults::FixtureRecord::TurnState(support::fixture_turn_state(
            root,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            1,
            timestamp(2),
        )),
        test_faults::FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            root_item,
            root,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        test_faults::FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            root,
            TurnItemOrdinal::FIRST,
            root_item,
            revision,
        )),
    ]);
    support::commit(&store, storage.clone(), support::batch(records));
    support::exact_cas::project_item_if_needed(&store, storage.clone(), root_item);
    support::discussion_creation::create_child(&store, &storage, id(30), 210, 211);
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(210),
        draft_id(212),
        SyndicItemId::from_bytes([213; 16]),
        "discuss this exactly",
        timestamp(10),
    );
    (home, store, storage)
}

fn request(store: &HomeStore, storage: &SyndicStorage) -> NativeProjectionRequest {
    request_for(store, storage, id(210))
}

fn request_for(
    store: &HomeStore,
    storage: &SyndicStorage,
    target: beryl_model::SyndicThreadId,
) -> NativeProjectionRequest {
    let thread = storage
        .thread(store, target, point_limit())
        .unwrap()
        .unwrap();
    let execution = storage
        .thread_execution(store, target, point_limit())
        .unwrap()
        .unwrap();
    NativeProjectionRequest::new(
        target,
        thread.selected_path(),
        execution.execution().clone(),
        support::test_tool_profile(),
    )
}

fn native_seed(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> (
    NativeProjectionBasis,
    RecoveryProjection,
    beryl_model::CasNativeTurnCount,
) {
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = storage
        .prepare_native_projection(store, &request(store, storage), point_limit())
        .unwrap()
    else {
        panic!("new discussion requires a native fork");
    };
    assert!(basis.context().unwrap().needs_seed());
    let seed = storage
        .prepare_native_context_seed(store, basis, Some(1_000_000))
        .unwrap();
    (basis, seed, native_turn_count)
}

fn replay(
    store: &HomeStore,
    storage: &SyndicStorage,
    proof: RecoveryProjection,
    page_bytes: usize,
) -> Vec<(RecoveryItemSequenceRole, String)> {
    let pool = PagePool::new(
        NonZeroUsize::new(page_bytes).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    let mut cursor = storage.open_recovery_cursor(store, proof).unwrap();
    let mut lease = pool.try_lease().unwrap();
    let mut items: Vec<(RecoveryItemSequenceRole, String)> = Vec::new();
    loop {
        let Some(page) = storage
            .read_recovery_cursor_page(store, &mut cursor, lease, page_bytes)
            .unwrap()
        else {
            break;
        };
        let ordinal = page.sequence_ordinal() as usize;
        if items.len() < ordinal {
            items.push((page.role(), String::new()));
        }
        assert_eq!(items[ordinal - 1].0, page.role());
        assert_eq!(items[ordinal - 1].1.len() as u64, page.item_offset());
        assert!(page.stored_bytes() >= page.text().len());
        items[ordinal - 1].1.push_str(page.text());
        lease = page.into_page_lease();
    }
    assert_eq!(items.len(), proof.item_count().get() as usize);
    items
}

fn generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(20).unwrap(),
        CasLoadedThreadGeneration::new(21).unwrap(),
    )
}

#[test]
fn native_seed_is_exact_assistant_frame_and_requires_the_opaque_publication_witness() {
    let (_home, store, storage) = seeded("context-native-seed");
    let (basis, seed, native_count) = native_seed(&store, &storage);
    let items = replay(&store, &storage, seed, 17);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, RecoveryItemSequenceRole::AssistantOutputText);
    assert_eq!(items[0].1.len(), 470);
    assert!(
        items[0]
            .1
            .starts_with("[Beryl discussion context v1]\nsource_thread=")
    );
    assert!(
        items[0]
            .1
            .ends_with("\n\nassistant\n[/Beryl discussion context v1]")
    );
    assert_eq!(seed.context().unwrap().summary().count(), 1);
    assert_eq!(seed.context().unwrap().summary().utf8_bytes(), 470);
    let lineage = seed
        .completed_lineage(timestamp(100), generation())
        .unwrap();
    let execution = request(&store, &storage).execution().clone();
    let target = CasThreadId::new("discussion-seeded-target").unwrap();
    let invented = PublishValidBinding::new(
        id(210),
        basis.expected_binding_revision(),
        basis.selected_path(),
        execution.clone(),
        target.clone(),
        basis.represented_prefix(),
        native_count,
        basis.tool_profile(),
        lineage,
    );
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(invented)),
        CommandOutcome::NotCommitted { .. }
    ));
    let publish = PublishValidBinding::from_native(basis, execution, target, native_count, lineage);
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(publish)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("published usable context binding");
    };
    storage
        .validate_context_binding(&store, id(210), usable)
        .unwrap();
    assert_eq!(
        usable.lineage().recovered_completed_at(),
        Some(timestamp(100))
    );
    store.close().unwrap();
}

#[test]
fn complete_replay_places_context_after_source_and_excludes_current_user_input() {
    let (_home, store, storage) = seeded("context-complete-replay");
    let target = request(&store, &storage);
    let RecoveryAssembly::Ready(proof) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(210),
                target.selected_path(),
                Some(1_000_000),
            )
            .with_tool_profile(target.tool_profile()),
        )
        .unwrap()
    else {
        panic!("nonempty discussion prefix");
    };
    assert_eq!(proof.version(), RecoveryProjectionVersion::ContextAwareV2);
    let items = replay(&store, &storage, proof, 19);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].1, "source question");
    assert_eq!(
        items[1],
        (
            RecoveryItemSequenceRole::AssistantOutputText,
            "assistant".to_owned()
        )
    );
    assert!(
        items[2]
            .1
            .ends_with("\n\nassistant\n[/Beryl discussion context v1]")
    );
    assert!(
        !items
            .iter()
            .any(|(_, text)| text.contains("discuss this exactly"))
    );
    let CasLineageProof::ContextAware(lineage) = proof
        .completed_lineage(timestamp(100), generation())
        .unwrap()
    else {
        panic!("context-aware replay proof");
    };
    assert_eq!(lineage.summary().count(), 1);
    store.close().unwrap();
}

#[test]
fn seed_budget_cancellation_and_source_revision_drift_refuse_without_changing_input() {
    let (_home, store, storage) = seeded("context-source-fences");
    let (basis, seed, _) = native_seed(&store, &storage);
    let selected = request(&store, &storage).selected_path();
    assert!(matches!(
        storage.prepare_native_context_seed(&store, basis, Some(939)),
        Err(RecoveryProjectionError::BudgetOverflow { .. })
    ));
    assert!(
        storage
            .prepare_native_context_seed(&store, basis, Some(940))
            .is_ok()
    );
    assert!(matches!(
        storage.prepare_native_context_seed_cancellable(&store, basis, Some(1_000_000), &|| true),
        Err(RecoveryProjectionError::Cancelled)
    ));
    assert!(matches!(
        storage.prepare_native_projection_cancellable(
            &store,
            &request(&store, &storage),
            point_limit(),
            &|| true
        ),
        Err(NativeProjectionError::Context(
            RecoveryProjectionError::Cancelled
        ))
    ));
    let mut batch = syndic_storage::test_faults::FixtureBatch::new();
    let attributes = storage
        .thread_attributes(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    batch
        .put(syndic_storage::test_faults::FixtureRecord::ThreadAttributes(attributes))
        .unwrap();
    support::commit(&store, storage.clone(), batch);
    assert!(matches!(
        storage.open_recovery_cursor(&store, seed),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
    assert_eq!(request(&store, &storage).selected_path(), selected);
    store.close().unwrap();
}
