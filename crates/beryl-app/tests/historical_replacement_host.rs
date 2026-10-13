#![cfg(feature = "test-faults")]

#[path = "historical_replacement_host/faults.rs"]
mod faults;
#[path = "syndic_composer_host/support.rs"]
mod host_support;
#[path = "historical_replacement_host/images.rs"]
mod images;
#[path = "historical_replacement_host/support.rs"]
mod support;

use beryl_app::composer_host::*;
use beryl_home_store::CommandCancellation;
use support::*;
use syndic_storage::*;

#[test]
fn imported_checkpoint_uses_same_session_and_waits_for_exact_publication_acknowledgement() {
    let (_home, store, state, storage, target) = fixture("replacement-host-import");
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let original = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, original, target, operation(211))
        .unwrap();
    assert!(host.historical_replacement_pending());
    let completion = finish(&mut host, &store, &state.assets(), ticket);
    let imported = completion.binding();
    assert_eq!(
        imported.candidate().session_id(),
        original.candidate().session_id()
    );
    assert_eq!(
        imported.candidate().draft_id(),
        original.candidate().draft_id()
    );
    assert_eq!(imported.host_generation(), original.host_generation());
    assert_ne!(imported.root(), original.root());
    assert!(!host.is_dirty());
    assert!(completion.requires_rebind());
    assert!(!imported.range_history_frontier().undo_available);
    assert!(!imported.range_history_frontier().redo_available);
    assert!(matches!(
        host.submission_intent(),
        Some(DraftSubmissionIntent::Replacement(_))
    ));
    assert!(
        host.begin_historical_replacement_cancellation(
            &store,
            imported,
            operation(212),
            timestamp()
        )
        .is_err()
    );
    assert!(
        host.acknowledge_historical_replacement(ticket, original)
            .is_err()
    );
    assert!(host.historical_replacement_pending());
    host.acknowledge_historical_replacement(ticket, imported)
        .unwrap();
    assert!(!host.historical_replacement_pending());
    let current = host_support::current(storage, &store, target.thread_id);
    assert_eq!(current.draft().piece_root(), imported.root());
    assert_eq!(current.thread().committed_tail(), Some(target.turn_id));
}

#[test]
fn cancellation_preserves_range_history_and_restoration_uses_durable_intent() {
    let (_home, store, state, storage, target) = fixture("replacement-host-cancel");
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let ticket = host
        .begin_historical_replacement(&store, host.binding().unwrap(), target, operation(211))
        .unwrap();
    let completion = finish(&mut host, &store, &state.assets(), ticket);
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    let restored = activate_with_ids(&store, storage.clone(), target.thread_id, 221, 222);
    assert_eq!(restored.submission_intent(), host.submission_intent());
    let before = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement_cancellation(&store, before, operation(213), timestamp())
        .unwrap();
    let completion = finish(&mut host, &store, &state.assets(), ticket);
    let after = completion.binding();
    assert!(!completion.requires_rebind());
    assert_eq!(before.range_binding(), after.range_binding());
    assert_eq!(
        before.range_history_frontier(),
        after.range_history_frontier()
    );
    assert_eq!(before.root(), after.root());
    assert_eq!(before.history(), after.history());
    assert_eq!(before.logical_extent(), after.logical_extent());
    assert!(after.candidate().session_generation() > before.candidate().session_generation());
    assert!(!host.is_dirty());
    host.acknowledge_historical_replacement(ticket, after)
        .unwrap();
    assert_eq!(
        host.submission_intent(),
        Some(DraftSubmissionIntent::Ordinary)
    );
    assert_eq!(
        host_support::current(storage, &store, target.thread_id)
            .draft()
            .submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
}

#[test]
fn rejected_exact_target_does_not_take_custody_or_change_the_empty_composer() {
    let (_home, store, _state, storage, target) = fixture("replacement-host-stale");
    let mut host = activate(&store, storage, target.thread_id);
    let original = host.binding().unwrap();
    let stale = ComposerHostHistoricalTarget {
        item_id: beryl_model::SyndicItemId::from_bytes([99; 16]),
        ..target
    };
    assert!(
        host.begin_historical_replacement(&store, original, stale, operation(211))
            .is_err()
    );
    assert_eq!(host.binding(), Some(original));
    assert!(!host.historical_replacement_pending());
    assert_eq!(
        host.submission_intent(),
        Some(DraftSubmissionIntent::Ordinary)
    );
}

#[test]
fn cancellation_after_begin_terminates_the_original_import_without_selecting_content() {
    let (_home, store, state, storage, target) = fixture("replacement-host-terminate");
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let original = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, original, target, operation(211))
        .unwrap();
    assert_eq!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostHistoricalAdvance::Progress
    );
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let completion = loop {
        match host
            .advance_historical_replacement(
                &store,
                ticket,
                &state.assets(),
                timestamp(),
                &cancellation,
            )
            .unwrap()
        {
            ComposerHostHistoricalAdvance::Progress => {}
            ComposerHostHistoricalAdvance::Ready(completion) => break completion,
            other => panic!("admitted begin must terminate its import: {other:?}"),
        }
    };
    assert_eq!(
        completion.binding().range_binding(),
        original.range_binding()
    );
    assert_eq!(completion.intent(), DraftSubmissionIntent::Ordinary);
    assert!(!completion.requires_rebind());
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    let retry = host
        .begin_historical_replacement(&store, host.binding().unwrap(), target, operation(214))
        .unwrap();
    let completion = finish(&mut host, &store, &state.assets(), retry);
    assert!(completion.requires_rebind());
}

#[test]
fn service_disposal_drains_admitted_import_without_its_completion_observer() {
    let (_home, store, state, storage, target) = fixture("replacement-host-disposal");
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let original = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, original, target, operation(211))
        .unwrap();
    host.advance_historical_replacement(
        &store,
        ticket,
        &state.assets(),
        timestamp(),
        &CommandCancellation::new(),
    )
    .unwrap();
    for _ in 0..100 {
        if host.dispose_composer_service(&store).unwrap()
            == ComposerHostServiceDisposalCompletion::Disposed
        {
            assert!(!host.historical_replacement_pending());
            assert!(host.binding().is_none());
            let current = host_support::current(storage, &store, target.thread_id);
            assert_eq!(current.draft().piece_root(), original.root());
            assert_eq!(
                current.draft().submission_intent(),
                DraftSubmissionIntent::Ordinary
            );
            return;
        }
    }
    panic!("original import disposal failed to converge")
}
