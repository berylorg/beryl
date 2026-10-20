#[path = "../syndic_composer_host/support.rs"]
mod base;
#[path = "../syndic_composer_history/support.rs"]
mod composer;
#[path = "../syndic_composer_publication/support.rs"]
mod publication;

use crate::composer_host::*;
use beryl_home_store::{CommandCancellation, CommandOutcome, HomeStore};
use beryl_state::{AssetState, BerylState};
use syndic_storage::{
    DiscussionParentEligibility, DiscussionParentRequest, GeneratedDiscussionInput, SyndicStorage,
    SyndicTimestamp,
};

fn fixture() -> (
    base::TestHome,
    HomeStore,
    BerylState,
    SyndicStorage,
    DiscussionParentRequest,
) {
    let (directory, store, state, storage, _) = base::fixture_with_state("composer-selector", 170);
    let parent = crate::support::generated_input::seed_pending(&store, &storage);
    (directory, store, state, storage, parent)
}

fn admit(
    store: &HomeStore,
    storage: &SyndicStorage,
    parent: DiscussionParentRequest,
    admitted_at: u64,
) {
    let DiscussionParentEligibility::Proven(proven) =
        storage.prepare_discussion_parent(store, parent).unwrap()
    else {
        panic!("exact parent is not idle");
    };
    let prepared = storage
        .prepare_generated_discussion_input(
            store,
            proven,
            GeneratedDiscussionInput {
                parent_turn_id: beryl_model::SyndicTurnId::from_bytes([234; 16]),
                canonical_item_id: beryl_model::SyndicItemId::from_bytes([232; 16]),
                resolution: "original generated resolution".into(),
                admitted_at: SyndicTimestamp::from_unix_millis(admitted_at),
            },
        )
        .unwrap();
    crate::support::discussion_input::committed(store, prepared.into_contribution());
}

fn begin(host: &mut SyndicComposerHost) -> ComposerHostFlushTicket {
    let ComposerHostFlushAdmission::Started { ticket, .. } = host
        .begin_flush(ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("original close barrier was not admitted");
    };
    ticket
}

fn capture(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: AssetState,
    seals: &crate::composer_marker_seal::DraftMarkerSealService,
    flush: ComposerHostFlushTicket,
    operation: u64,
) -> ComposerHostFlushCapture {
    host.capture_flush_publication(
        store,
        flush,
        assets,
        seals,
        composer::operation_id(operation),
        None,
        SyndicTimestamp::from_unix_millis(operation),
        &CommandCancellation::new(),
    )
    .unwrap()
}

fn ready(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: AssetState,
    seals: &crate::composer_marker_seal::DraftMarkerSealService,
    flush: ComposerHostFlushTicket,
) {
    for step in 0..32 {
        match host.flush_state(flush).unwrap() {
            ComposerHostFlushState::CloseReady => return,
            ComposerHostFlushState::CaptureRequired => {
                assert!(matches!(
                    capture(host, store, assets.clone(), seals, flush, 1_000 + step),
                    ComposerHostFlushCapture::Captured(_) | ComposerHostFlushCapture::State(_)
                ));
            }
            ComposerHostFlushState::PublicationPending => assert!(matches!(
                host.advance_flush(store, flush).unwrap(),
                ComposerHostFlushAdvance::Progress(_)
                    | ComposerHostFlushAdvance::ReconciliationPending
            )),
            other => panic!("ordinary close changed its publication obligation: {other:?}"),
        }
    }
    panic!("original close publication did not settle");
}

#[test]
fn generated_parent_admission_preserves_clean_and_dirty_editor_close_publication() {
    for dirty in [false, true] {
        let (_directory, store, state, storage, parent) = fixture();
        let assets = state.assets();
        let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
        let (mut host, opening) =
            composer::activated(storage.clone(), &store, parent.parent_thread_id, 180, 181);
        let binding = if dirty {
            composer::commit_text(
                &mut host,
                &store,
                opening,
                1,
                0,
                0,
                "preserved café 🌸 draft",
                26,
                1,
            )
        } else {
            opening
        };
        let text = composer::candidate_text(storage.clone(), &store, binding);
        let durable = base::current(storage.clone(), &store, parent.parent_thread_id);
        admit(&store, &storage, parent, 100);
        let before_capture = store.home_revision().unwrap();
        assert_eq!(host.binding(), Some(binding));
        assert_eq!(host.is_dirty(), dirty);
        let flush = begin(&mut host);
        ready(&mut host, &store, assets, &seals, flush);
        let after = host.binding().unwrap();
        assert_eq!(
            after.candidate().session_id(),
            binding.candidate().session_id()
        );
        assert_eq!(after.root(), binding.root());
        assert_eq!(
            composer::candidate_text(storage.clone(), &store, after),
            text
        );
        assert!(!host.is_dirty());
        assert_eq!(host.publication_custody_count(), 0);
        let saved = base::current(storage.clone(), &store, parent.parent_thread_id);
        assert_eq!(saved.draft().id(), durable.draft().id());
        assert_eq!(saved.draft().piece_root(), after.root());
        if !dirty {
            assert_eq!(store.home_revision().unwrap(), before_capture);
            assert_eq!(saved.draft(), durable.draft());
            assert_eq!(after, opening);
        }
    }
}

#[test]
fn original_committed_publication_converges_after_generated_parent_thread_advance() {
    let (_directory, store, state, storage, parent) = fixture();
    let assets = state.assets();
    let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, opening) =
        composer::activated(storage.clone(), &store, parent.parent_thread_id, 180, 181);
    let binding = composer::commit_text(
        &mut host,
        &store,
        opening,
        1,
        0,
        0,
        "original parent draft",
        21,
        1,
    );
    let observation = ComposerHostPublicationExecutionObservation::new();
    host.test_observe_publication_execution(observation.clone())
        .unwrap();
    let original_storage = storage.clone();
    host.test_after_publication_execute(Box::new(move |store, outcome| {
        assert!(matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        admit(store, &original_storage, parent, 2_000);
    }))
    .unwrap();
    let flush = begin(&mut host);
    ready(&mut host, &store, assets, &seals, flush);
    assert_eq!(observation.snapshot().original_attempts, 1);
    assert_eq!(observation.snapshot().different_identity_attempts, 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(host.binding().unwrap().root(), binding.root());
    assert_eq!(
        composer::candidate_text(storage, &store, host.binding().unwrap()),
        b"original parent draft"
    );
}

#[test]
fn changed_successor_checkpoint_retains_the_original_committed_publication_custody() {
    let (_directory, store, state, storage, parent) = fixture();
    let assets = state.assets();
    let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, opening) =
        composer::activated(storage.clone(), &store, parent.parent_thread_id, 180, 181);
    let _ = composer::commit_text(&mut host, &store, opening, 1, 0, 0, "original", 8, 1);
    let observation = ComposerHostPublicationExecutionObservation::new();
    host.test_observe_publication_execution(observation.clone())
        .unwrap();
    let alternate_storage = storage.clone();
    let alternate_assets = assets.clone();
    let alternate_seals = seals.clone();
    host.test_after_publication_execute(Box::new(move |store, outcome| {
        assert!(matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let (mut alternate, binding) = composer::activated(
            alternate_storage.clone(),
            store,
            parent.parent_thread_id,
            190,
            191,
        );
        let _ = composer::commit_text(&mut alternate, store, binding, 2, 0, 0, "other ", 6, 1);
        let flush = begin(&mut alternate);
        ready(
            &mut alternate,
            store,
            alternate_assets,
            &alternate_seals,
            flush,
        );
    }))
    .unwrap();
    let flush = begin(&mut host);
    assert!(matches!(
        capture(&mut host, &store, assets, &seals, flush, 1_000),
        ComposerHostFlushCapture::Captured(_)
    ));
    let result = host.advance_flush(&store, flush).unwrap();
    assert!(matches!(
        result,
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::DurableBaseConflict)
    ));
    assert_eq!(host.publication_custody_count(), 1);
    assert!(host.publication.retained.is_some());
    assert_eq!(observation.snapshot().original_attempts, 1);
    assert_eq!(observation.snapshot().different_identity_attempts, 0);
}

#[test]
fn uncertain_original_publication_retains_its_exact_outcome_until_reconciliation() {
    let (_directory, store, state, storage, thread, faults) =
        base::fault_fixture_with_state("selector-unknown", 170);
    let assets = state.assets();
    let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, opening) = composer::activated(storage, &store, thread, 180, 181);
    let binding = composer::commit_text(&mut host, &store, opening, 1, 0, 0, "original", 8, 1);
    let observation = ComposerHostPublicationExecutionObservation::new();
    host.test_observe_publication_execution(observation.clone())
        .unwrap();
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist)
    });
    let flush = begin(&mut host);
    assert!(matches!(
        capture(&mut host, &store, assets.clone(), &seals, flush, 1_000),
        ComposerHostFlushCapture::Captured(_)
    ));
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::ReconciliationPending
    );
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(store.pending_reconciliations().len(), 1);
    ready(&mut host, &store, assets, &seals, flush);
    assert_eq!(host.publication_custody_count(), 0);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(observation.snapshot().original_attempts, 1);
    assert_eq!(observation.snapshot().different_identity_attempts, 0);
}
