#![cfg(feature = "test-faults")]

#[path = "syndic_composer_history/support.rs"]
mod history_support;
#[path = "syndic_composer_host/support.rs"]
mod host_support;
#[path = "main_window_composer_slot/support.rs"]
mod slot_support;
#[path = "historical_replacement_host/support.rs"]
mod support;

use beryl_app::composer_host::*;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandCancellation, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use beryl_state::BerylState;
use support::*;
use syndic_storage::*;

fn fixture() -> (
    tempfile::TempDir,
    HomeStore,
    SyndicStorage,
    BerylState,
    FaultController,
    ComposerHostHistoricalTarget,
) {
    let directory = tempfile::Builder::new()
        .prefix("historical-retirement-")
        .tempdir()
        .unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([201; 16]);
    let execution = ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\historical-retirement",
        )
        .unwrap(),
    );
    host_support::committed(host_support::execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([202; 16]),
                execution,
                timestamp(),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let target = seed_source(
        &store,
        &storage,
        thread,
        &ComposerPayload::new(vec![
            ComposerAtom::text("historical recovery input").unwrap(),
        ])
        .unwrap(),
    );
    (directory, store, storage, state, faults, target)
}

fn fail(store: &HomeStore, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
}

#[test]
fn unexecuted_begin_retires_without_creating_an_import_or_writing_recovery() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let prior = host.binding().unwrap();
    host.begin_historical_replacement(&store, prior, target, operation(211))
        .unwrap();
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    assert_eq!(retired.historical_prior_binding(), Some(prior));
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(
        retired.historical_restoration().unwrap().candidate(),
        prior.candidate()
    );
    assert_eq!(
        retired
            .historical_restoration()
            .unwrap()
            .imported_position(),
        None
    );
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    assert_eq!(fresh.binding().unwrap().candidate(), prior.candidate());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn admitted_import_is_terminated_on_fresh_candidate_without_selecting_its_root() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let prior = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, prior, target, operation(211))
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
    assert_eq!(
        host.historical_replacement_stage(),
        Some(ComposerHostHistoricalStage::Reconciling)
    );
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let before = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    let after = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_ne!(before, after);
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.candidate().root(), prior.root());
    assert_eq!(restored.intent(), DraftSubmissionIntent::Ordinary);
    assert_eq!(restored.imported_position(), None);
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        after
    );
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    assert_eq!(fresh.binding().unwrap().root(), prior.root());
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn committed_unacknowledged_import_retains_prior_widget_binding_and_saved_successor() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let prior = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, prior, target, operation(211))
        .unwrap();
    let mut completion = None;
    for _ in 0..1_000 {
        match host
            .advance_historical_replacement(
                &store,
                ticket,
                &state.assets(),
                timestamp(),
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostHistoricalAdvance::Progress => assert_eq!(host.binding(), Some(prior)),
            ComposerHostHistoricalAdvance::Ready(value) => {
                completion = Some(value);
                break;
            }
            other => panic!("unexpected historical start outcome: {other:?}"),
        }
    }
    let completion = completion.expect("historical start did not complete within fixture bound");
    assert!(host.historical_recovery_accepts_binding(prior));
    assert!(host.historical_recovery_accepts_binding(completion.binding()));
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.prior_binding(), prior);
    assert_eq!(restored.candidate(), completion.binding().candidate());
    assert_eq!(restored.intent(), completion.intent());
    assert!(restored.imported_position().is_some());
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    assert_eq!(fresh.binding().unwrap().root(), completion.binding().root());
    assert_eq!(fresh.submission_intent(), Some(completion.intent()));
    assert!(!fresh.is_dirty());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn committed_metadata_cancellation_retains_text_and_history_without_changed_source_seed() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let ticket = host
        .begin_historical_replacement(&store, host.binding().unwrap(), target, operation(211))
        .unwrap();
    let imported = finish(&mut host, &store, &state.assets(), ticket);
    host.acknowledge_historical_replacement(ticket, imported.binding())
        .unwrap();
    let cancellation = host
        .begin_historical_replacement_cancellation(
            &store,
            imported.binding(),
            operation(212),
            timestamp(),
        )
        .unwrap();
    let cancelled = finish(&mut host, &store, &state.assets(), cancellation);
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.prior_binding(), imported.binding());
    assert_eq!(restored.candidate(), cancelled.binding().candidate());
    assert_eq!(restored.candidate().root(), imported.binding().root());
    assert_eq!(restored.intent(), DraftSubmissionIntent::Ordinary);
    assert_eq!(restored.imported_position(), None);
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    assert_eq!(
        fresh.submission_intent(),
        Some(DraftSubmissionIntent::Ordinary)
    );
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn unresolved_original_start_is_classified_before_candidate_saved_reads() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let prior = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, prior, target, operation(211))
        .unwrap();
    for _ in 0..1_000 {
        if host.historical_replacement_stage() == Some(ComposerHostHistoricalStage::Starting) {
            break;
        }
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
    }
    assert_eq!(
        host.historical_replacement_stage(),
        Some(ComposerHostHistoricalStage::Starting)
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let classification_faults = faults.clone();
    host.test_arm_historical_after_execute_fault(move |_, outcome| {
        assert!(matches!(
            outcome,
            beryl_home_store::CommandOutcome::Indeterminate { .. }
        ));
        classification_faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    });
    assert!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .is_err()
    );
    assert_eq!(host.binding(), Some(prior));
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.prior_binding(), prior);
    assert_ne!(restored.candidate().root(), prior.root());
    assert!(restored.imported_position().is_some());
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    assert_eq!(fresh.binding().unwrap().candidate(), restored.candidate());
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn exact_old_candidate_cleanup_is_reprepared_only_on_explicit_retry() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let prior = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, prior, target, operation(211))
        .unwrap();
    host.advance_historical_replacement(
        &store,
        ticket,
        &state.assets(),
        timestamp(),
        &CommandCancellation::new(),
    )
    .unwrap();
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(retired.qualify_saved(&mut candidate, &storage).is_err());
    assert_eq!(retired.historical_prior_binding(), Some(prior));
    assert!(retired.historical_restoration().is_none());
    drop(storage);
    let mut candidate = candidate.abort().recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let classification = retired.qualify_saved(&mut candidate, &storage).unwrap_err();
    assert!(classification.contains("cleanup did not commit"));
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(
        retired.historical_restoration().unwrap().candidate().root(),
        prior.root()
    );
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn cancellation_recovery_preserves_dirty_newest_and_published_split() {
    let (_directory, store, storage, state, faults, target) = fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let ticket = host
        .begin_historical_replacement(&store, host.binding().unwrap(), target, operation(211))
        .unwrap();
    let imported = finish(&mut host, &store, &state.assets(), ticket);
    host.acknowledge_historical_replacement(ticket, imported.binding())
        .unwrap();
    let dirty = history_support::commit_text(
        &mut host,
        &store,
        imported.binding(),
        812,
        0,
        0,
        "changed ",
        imported.binding().logical_extent().logical_utf8_bytes() + 8,
        1,
    );
    assert!(host.is_dirty());
    let cancellation = host
        .begin_historical_replacement_cancellation(&store, dirty, operation(213), timestamp())
        .unwrap();
    let cancelled = finish(&mut host, &store, &state.assets(), cancellation);
    assert!(host.is_dirty());
    fail(&store, &faults);
    let mut retired = Box::new(host).retire_failed_resident(&store).ok().unwrap();
    drop((storage, state));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(!retired.qualify_saved(&mut candidate, &storage).unwrap());
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.candidate(), cancelled.binding().candidate());
    assert_eq!(restored.candidate().root(), dirty.root());
    assert_eq!(restored.candidate().history(), dirty.history());
    assert_eq!(restored.selector().root(), imported.binding().root());
    assert_eq!(restored.selector().history(), imported.binding().history());
    assert_eq!(restored.imported_position(), None);
    assert_eq!(restored.intent(), DraftSubmissionIntent::Ordinary);
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn failed_slot_carries_exact_historical_prior_through_same_claim_reconstruction() {
    use beryl_app::main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowConversationComposerService,
    };
    let fixture = slot_support::Fixture::new("historical-slot-retirement", 151);
    let target = seed_source(
        &fixture.store,
        &fixture.storage,
        fixture.selected_thread,
        &ComposerPayload::new(vec![
            ComposerAtom::text("retained slot historical input").unwrap(),
        ])
        .unwrap(),
    );
    let mut host = fixture.activated_host(fixture.selected_thread, 221, 222, 1);
    let prior = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&fixture.store, prior, target, operation(211))
        .unwrap();
    let completion = finish(&mut host, &fixture.store, &fixture.assets(), ticket);
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        fixture.claims().0,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let identity = slot.selected_identity().unwrap();
    let service = std::sync::Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    fail(&fixture.store, &fixture.faults);
    let mut retired = service.retire_failed_resident().ok().unwrap();
    assert_eq!(retired.selection(), identity);
    assert_eq!(retired.historical_prior_binding(), Some(prior));
    let (_directory, store, old_storage) = fixture.into_store();
    drop(old_storage);
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retired
            .qualify_saved(&mut candidate, &storage, &state, None)
            .unwrap()
    );
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.prior_binding(), prior);
    assert_eq!(restored.candidate(), completion.binding().candidate());
    assert!(restored.imported_position().is_some());
    let (slot, window, retired) = retired
        .reconstruct(&mut candidate, storage, &state, None)
        .ok()
        .unwrap();
    assert_eq!(window.window_id(), identity.window_id());
    assert_eq!(
        slot.selected_identity().unwrap().binding().candidate(),
        completion.binding().candidate()
    );
    drop((slot, retired, state));
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn dirty_empty_preflush_preserves_exact_initiating_publication_binding() {
    let fixture = slot_support::Fixture::new("historical-preflush-prior", 161);
    let target = seed_source(
        &fixture.store,
        &fixture.storage,
        fixture.selected_thread,
        &ComposerPayload::new(vec![ComposerAtom::text("historical input").unwrap()]).unwrap(),
    );
    let mut host = fixture.activated_host(fixture.selected_thread, 221, 222, 1);
    let initial = host.binding().unwrap();
    let typed =
        history_support::commit_text(&mut host, &fixture.store, initial, 901, 0, 0, "x", 1, 1);
    let prior = history_support::commit_text(&mut host, &fixture.store, typed, 902, 0, 1, "", 0, 1);
    assert!(host.is_dirty());
    let ComposerHostFlushAdmission::Started {
        ticket: flush,
        mut state,
    } = host.begin_historical_replacement_flush(prior).unwrap()
    else {
        panic!("new historical flush expected")
    };
    let seals = fixture.marker_seals();
    let assets = fixture.assets();
    let mut satisfied = false;
    for _ in 0..100 {
        if state == ComposerHostFlushState::CaptureRequired {
            match host
                .capture_flush_publication(
                    &fixture.store,
                    flush,
                    assets.clone(),
                    &seals,
                    operation(214),
                    None,
                    timestamp(),
                    &CommandCancellation::new(),
                )
                .unwrap()
            {
                ComposerHostFlushCapture::Captured(_) => {
                    state = ComposerHostFlushState::PublicationPending
                }
                ComposerHostFlushCapture::State(next) => state = next,
                ComposerHostFlushCapture::Satisfied(_) => {
                    satisfied = true;
                    break;
                }
                other => panic!("unexpected historical flush capture: {other:?}"),
            }
        }
        match host.advance_flush(&fixture.store, flush).unwrap() {
            ComposerHostFlushAdvance::Progress(next) => state = next,
            ComposerHostFlushAdvance::Satisfied(_) => {
                satisfied = true;
                break;
            }
            other => panic!("unexpected historical flush advance: {other:?}"),
        }
    }
    assert!(satisfied);
    let saved = host.binding().unwrap();
    assert_ne!(saved, prior);
    assert_eq!(host.historical_recovery_prior_binding(), Some(prior));
    assert!(host.historical_recovery_accepts_binding(prior));
    let ticket = host
        .begin_historical_replacement(&fixture.store, saved, target, operation(215))
        .unwrap();
    let completion = finish(&mut host, &fixture.store, &assets, ticket);
    assert_eq!(host.historical_recovery_prior_binding(), Some(prior));
    assert!(host.historical_recovery_accepts_binding(saved));
    fail(&fixture.store, &fixture.faults);
    let mut retired = Box::new(host)
        .retire_failed_resident(&fixture.store)
        .ok()
        .unwrap();
    assert_eq!(retired.historical_prior_binding(), Some(prior));
    drop((seals, assets));
    let (_directory, store, old_storage) = fixture.into_store();
    drop(old_storage);
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(retired.qualify_saved(&mut candidate, &storage).unwrap());
    let restored = retired.historical_restoration().unwrap();
    assert_eq!(restored.prior_binding(), prior);
    assert_eq!(restored.candidate(), completion.binding().candidate());
    let fresh = retired.reconstruct(&mut candidate, storage).unwrap();
    drop(fresh);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn all_resident_original_scopes_settle_before_any_import_cleanup_reads() {
    let fixture = slot_support::Fixture::new("historical-multiple-residents", 171);
    let target_a = seed_source(
        &fixture.store,
        &fixture.storage,
        fixture.selected_thread,
        &ComposerPayload::new(vec![ComposerAtom::text("first historical input").unwrap()]).unwrap(),
    );
    let target_b = seed_source_with_identities(
        &fixture.store,
        &fixture.storage,
        fixture.target_thread,
        &ComposerPayload::new(vec![ComposerAtom::text("second historical input").unwrap()])
            .unwrap(),
        None,
        (
            beryl_model::SyndicTurnId::from_bytes([231; 16]),
            beryl_model::SyndicItemId::from_bytes([232; 16]),
        ),
    );
    let mut first = fixture.activated_host(fixture.selected_thread, 211, 212, 1);
    let mut second = fixture.activated_host(fixture.target_thread, 213, 214, 1);
    let first_prior = first.binding().unwrap();
    let second_prior = second.binding().unwrap();
    let first_ticket = first
        .begin_historical_replacement(&fixture.store, first_prior, target_a, operation(215))
        .unwrap();
    first
        .advance_historical_replacement(
            &fixture.store,
            first_ticket,
            &fixture.assets(),
            timestamp(),
            &CommandCancellation::new(),
        )
        .unwrap();
    let second_ticket = second
        .begin_historical_replacement(&fixture.store, second_prior, target_b, operation(216))
        .unwrap();
    for _ in 0..1_000 {
        if second.historical_replacement_stage() == Some(ComposerHostHistoricalStage::Starting) {
            break;
        }
        assert_eq!(
            second
                .advance_historical_replacement(
                    &fixture.store,
                    second_ticket,
                    &fixture.assets(),
                    timestamp(),
                    &CommandCancellation::new()
                )
                .unwrap(),
            ComposerHostHistoricalAdvance::Progress
        );
    }
    assert_eq!(
        second.historical_replacement_stage(),
        Some(ComposerHostHistoricalStage::Starting)
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let faults = fixture.faults.clone();
    second.test_arm_historical_after_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::BeforeReconciliationSnapshot)
    });
    assert!(
        second
            .advance_historical_replacement(
                &fixture.store,
                second_ticket,
                &fixture.assets(),
                timestamp(),
                &CommandCancellation::new()
            )
            .is_err()
    );
    fail(&fixture.store, &fixture.faults);
    let mut first = Box::new(first)
        .retire_failed_resident(&fixture.store)
        .ok()
        .unwrap();
    let mut second = Box::new(second)
        .retire_failed_resident(&fixture.store)
        .ok()
        .unwrap();
    let (_directory, store, old_storage) = fixture.into_store();
    drop(old_storage);
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let revision_before_observation = {
        let access = candidate.recovery_access().unwrap();
        assert_eq!(access.pending_reconciliations().len(), 1);
        access.home_revision().unwrap()
    };
    assert!(first.historical_restoration().is_none());
    first
        .observe_historical_recovery(&mut candidate, &storage)
        .unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .pending_reconciliations()
            .len(),
        1
    );
    second
        .observe_historical_recovery(&mut candidate, &storage)
        .unwrap();
    {
        let access = candidate.recovery_access().unwrap();
        assert!(access.pending_reconciliations().is_empty());
        assert_eq!(access.home_revision().unwrap(), revision_before_observation);
    }
    assert!(first.historical_restoration().is_none());
    assert!(second.historical_restoration().is_none());
    assert!(first.qualify_saved(&mut candidate, &storage).unwrap());
    assert!(second.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(
        first.historical_restoration().unwrap().candidate().root(),
        first_prior.root()
    );
    assert_eq!(
        first.historical_restoration().unwrap().imported_position(),
        None
    );
    assert_ne!(
        second.historical_restoration().unwrap().candidate().root(),
        second_prior.root()
    );
    assert!(
        second
            .historical_restoration()
            .unwrap()
            .imported_position()
            .is_some()
    );
    let first = first.reconstruct(&mut candidate, storage.clone()).unwrap();
    let second = second.reconstruct(&mut candidate, storage).unwrap();
    drop((first, second));
    candidate.publish().unwrap().close().unwrap();
}
