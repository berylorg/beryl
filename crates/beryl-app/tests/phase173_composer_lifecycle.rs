use beryl_app::{
    composer_host::{
        ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostAutosaveInterval,
        ComposerHostAutosaveSettingsCompletion, ComposerHostFlushAdmission,
        ComposerHostFlushAdvance, ComposerHostFlushCapture, ComposerHostFlushFailure,
        ComposerHostFlushPurpose, ComposerHostFlushState,
    },
    composer_marker_seal::DraftMarkerSealService,
};
use beryl_home_store::CommandCancellation;
use beryl_state::{AssetState, BerylState};
use syndic_storage::SyndicTimestamp;

#[path = "phase141_syndic_composer_host/support.rs"]
mod base;
#[path = "phase166_syndic_composer_history/support.rs"]
mod composer;
#[path = "phase172_syndic_composer_publication/support.rs"]
mod publication;

#[path = "phase173_composer_lifecycle/admission.rs"]
mod lifecycle_admission;
#[path = "phase173_composer_lifecycle/common.rs"]
mod lifecycle_common;
#[path = "phase173_composer_lifecycle/edges.rs"]
mod lifecycle_edges;
#[path = "phase173_composer_lifecycle/failures.rs"]
mod lifecycle_failures;
#[path = "phase173_composer_lifecycle/marker_noncommit.rs"]
mod lifecycle_marker_noncommit;
#[path = "phase173_composer_lifecycle/reconciliation.rs"]
mod lifecycle_reconciliation;
#[path = "phase173_composer_lifecycle/service_disposal.rs"]
mod lifecycle_service_disposal;

use base::fixture;
use composer::{activated, commit_text, operation_id};
use publication::service;

#[test]
fn first_dirty_arms_once_and_committed_settings_replace_the_generation() {
    let (_home, mut store, storage, thread) = fixture("phase173-autosave-generation", 1);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 2, 3);

    assert_eq!(host.autosave_interval().seconds(), 30);
    assert!(host.autosave_timer().is_none());
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let first_timer = host.autosave_timer().unwrap();
    let second = commit_text(&mut host, &store, first, 2, 1, 1, "b", 1, 1);
    assert_eq!(host.autosave_timer(), Some(first_timer));

    let interval = ComposerHostAutosaveInterval::new(5).unwrap();
    let replacement = match host.publish_autosave_interval(second, 1, interval).unwrap() {
        ComposerHostAutosaveSettingsCompletion::Published(Some(timer)) => timer,
        other => panic!("unexpected settings outcome: {other:?}"),
    };
    assert_ne!(
        replacement.timer_generation(),
        first_timer.timer_generation()
    );
    assert_eq!(replacement.settings_generation(), 1);
    assert_eq!(host.autosave_interval(), interval);
    assert_eq!(
        host.publish_autosave_interval(second, 1, ComposerHostAutosaveInterval::new(10).unwrap())
            .unwrap(),
        ComposerHostAutosaveSettingsCompletion::Stale
    );
    assert_eq!(host.autosave_timer(), Some(replacement));
    assert_eq!(
        host.fire_autosave(
            &store,
            first_timer,
            assets,
            &seals,
            operation_id(3),
            None,
            SyndicTimestamp::from_unix_millis(3),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostAutosaveCapture::Stale
    );
}

#[test]
fn autosave_success_and_noncommit_apply_exact_rearm_rules() {
    let (_home, mut store, storage, thread) = fixture("phase173-autosave-outcomes", 11);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 12, 13);
    let dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let timer = host.autosave_timer().unwrap();
    let cancellation = CommandCancellation::new();
    let ticket = captured_autosave(&mut host, &store, assets, &seals, timer, 2, &cancellation);
    let replacement_timer = match host
        .publish_autosave_interval(dirty, 1, ComposerHostAutosaveInterval::new(10).unwrap())
        .unwrap()
    {
        ComposerHostAutosaveSettingsCompletion::Published(Some(timer)) => timer,
        other => panic!("settings did not replace in-flight timer: {other:?}"),
    };
    assert_eq!(
        host.fire_autosave(
            &store,
            replacement_timer,
            assets,
            &seals,
            operation_id(3),
            None,
            SyndicTimestamp::from_unix_millis(3),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostAutosaveCapture::PublicationPending
    );
    cancellation.cancel();
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Unsatisfied(ComposerHostFlushFailure::Cancelled)
    );
    let retry_timer = host.autosave_timer().unwrap();
    assert_eq!(retry_timer, replacement_timer);

    let ticket = captured_autosave(
        &mut host,
        &store,
        assets,
        &seals,
        retry_timer,
        4,
        &CommandCancellation::new(),
    );
    let newest = commit_text(&mut host, &store, dirty, 5, 1, 1, "b", 2, 1);
    let successor_timer = match host
        .publish_autosave_interval(newest, 2, ComposerHostAutosaveInterval::new(11).unwrap())
        .unwrap()
    {
        ComposerHostAutosaveSettingsCompletion::Published(Some(timer)) => timer,
        other => panic!("settings did not anchor successor timer: {other:?}"),
    };
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Saved {
            dirty_successor: true
        }
    );
    assert_eq!(host.autosave_timer(), Some(successor_timer));
    let ticket = captured_autosave(
        &mut host,
        &store,
        assets,
        &seals,
        successor_timer,
        6,
        &CommandCancellation::new(),
    );
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Saved {
            dirty_successor: false
        }
    );
    assert!(!host.is_dirty());
    assert!(host.autosave_timer().is_none());
    assert_eq!(host.binding().unwrap().root(), newest.root());
}

#[test]
fn joined_flush_repeats_to_the_newest_frontier_without_waiter_retention() {
    let (_home, mut store, storage, thread) = fixture("phase173-joined-flush", 21);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 22, 23);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    let publication = captured_flush(&mut host, &store, assets, &seals, flush, 2);
    let second = commit_text(&mut host, &store, first, 2, 1, 1, "b", 1, 1);

    for _ in 0..32 {
        match host
            .begin_flush(ComposerHostFlushPurpose::Submission)
            .unwrap()
        {
            ComposerHostFlushAdmission::Joined { ticket, .. } => assert_eq!(ticket, flush),
            other => panic!("flush did not join: {other:?}"),
        }
    }
    assert!(matches!(
        host.begin_flush(ComposerHostFlushPurpose::ApplicationExit)
            .unwrap(),
        ComposerHostFlushAdmission::Joined { ticket, .. } if ticket == flush
    ));
    let diagnostics = host.lifecycle_diagnostics();
    assert_eq!(diagnostics.timers(), 0);
    assert_eq!(diagnostics.barriers(), 1);
    assert_eq!(diagnostics.joined_publications(), 1);
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_ne!(host.binding().unwrap().root(), first.root());
    assert!(host.is_dirty());
    assert_eq!(
        host.advance_autosave(&store, publication).unwrap(),
        ComposerHostAutosaveAdvance::Stale
    );

    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 3);
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
    );
    assert_eq!(host.binding().unwrap().root(), second.root());
    assert!(!host.is_dirty());
    assert!(matches!(
        host.capture_flush_disposal(&store, flush, operation_id(4), &CommandCancellation::new())
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::ApplicationExit)
    );
    assert!(host.binding().is_none());
}

#[test]
fn cancelled_flush_ends_unsatisfied_and_rearms_without_retaining_the_barrier() {
    let (_home, mut store, storage, thread) = fixture("phase173-cancelled-flush", 25);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 26, 27);
    let _ = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.capture_flush_publication(
            &store,
            flush,
            assets,
            &seals,
            operation_id(2),
            None,
            SyndicTimestamp::from_unix_millis(2),
            &cancellation,
        )
        .unwrap(),
        ComposerHostFlushCapture::Unsatisfied(ComposerHostFlushFailure::Cancelled)
    );
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    let diagnostics = host.lifecycle_diagnostics();
    assert_eq!(diagnostics.barriers(), 0);
    assert_eq!(diagnostics.joined_publications(), 0);
    assert_eq!(diagnostics.timers(), 1);
}

#[test]
fn lifecycle_release_disposes_only_after_clean_flush_and_rejects_stale_work() {
    let (_home, mut store, storage, thread) = fixture("phase173-release", 31);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 32, 33);
    let binding = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let stale_timer = host.autosave_timer().unwrap();
    let (flush, state) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    let _ = captured_flush(&mut host, &store, assets, &seals, flush, 2);
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
    );
    assert!(host.binding().is_some());
    assert!(matches!(
        host.capture_flush_disposal(&store, flush, operation_id(3), &CommandCancellation::new())
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
    assert!(host.binding().is_none());
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(
        host.fire_autosave(
            &store,
            stale_timer,
            assets,
            &seals,
            operation_id(4),
            None,
            SyndicTimestamp::from_unix_millis(4),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostAutosaveCapture::Stale
    );
    assert!(
        host.begin_flush(ComposerHostFlushPurpose::ThreadSwitch)
            .is_err()
    );
    assert!(
        host.publish_autosave_interval(binding, 2, ComposerHostAutosaveInterval::new(10).unwrap())
            .is_err()
    );
    assert_eq!(host.publication_custody_count(), 0);
}

#[test]
fn service_disposal_releases_all_host_lifecycle_and_publication_custody() {
    let (_home, mut store, storage, thread) = fixture("phase173-service-disposal", 41);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 42, 43);
    let _ = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let timer = host.autosave_timer().unwrap();
    let ticket = captured_autosave(
        &mut host,
        &store,
        assets,
        &seals,
        timer,
        2,
        &CommandCancellation::new(),
    );
    assert_eq!(host.publication_custody_count(), 1);

    host.dispose_composer_service(&store).unwrap();

    assert!(host.binding().is_none());
    assert_eq!(host.publication_custody_count(), 0);
    let diagnostics = host.lifecycle_diagnostics();
    assert_eq!(diagnostics.timers(), 0);
    assert_eq!(diagnostics.barriers(), 0);
    assert_eq!(diagnostics.joined_publications(), 0);
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Stale
    );
    assert!(
        host.begin_flush(ComposerHostFlushPurpose::WindowClose)
            .is_err()
    );
}

fn captured_autosave(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    timer: beryl_app::composer_host::ComposerHostAutosaveTimer,
    operation: u64,
    cancellation: &CommandCancellation,
) -> beryl_app::composer_host::ComposerHostPublicationTicket {
    match host
        .fire_autosave(
            store,
            timer,
            assets,
            seals,
            operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(operation),
            cancellation,
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("autosave was not captured: {other:?}"),
    }
}

fn started_flush(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    purpose: ComposerHostFlushPurpose,
) -> (
    beryl_app::composer_host::ComposerHostFlushTicket,
    ComposerHostFlushState,
) {
    match host.begin_flush(purpose).unwrap() {
        ComposerHostFlushAdmission::Started { ticket, state } => (ticket, state),
        other => panic!("flush did not start: {other:?}"),
    }
}

fn captured_flush(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    flush: beryl_app::composer_host::ComposerHostFlushTicket,
    operation: u64,
) -> beryl_app::composer_host::ComposerHostPublicationTicket {
    captured_flush_with_cancellation(
        host,
        store,
        assets,
        seals,
        flush,
        operation,
        &CommandCancellation::new(),
    )
}

fn captured_flush_with_cancellation(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    flush: beryl_app::composer_host::ComposerHostFlushTicket,
    operation: u64,
    cancellation: &CommandCancellation,
) -> beryl_app::composer_host::ComposerHostPublicationTicket {
    match host
        .capture_flush_publication(
            store,
            flush,
            assets,
            seals,
            operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(operation),
            cancellation,
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(ticket) => ticket,
        other => panic!("flush publication was not captured: {other:?}"),
    }
}
