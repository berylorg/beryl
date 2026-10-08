use super::*;
use crate::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushState,
};
use crate::main_window::{
    MainWindowComposerActivationAdvance, MainWindowComposerMarkerMetadataAuthority,
    MainWindowComposerPublishAdvance,
};
use crate::same_window_thread_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};

#[path = "../syndic_composer_history/support.rs"]
mod edit_support;
#[path = "../main_window_composer_slot/support.rs"]
mod support;

#[path = "failed_thread_creation_retirement/support.rs"]
mod retirement_support;
use retirement_support::*;
#[path = "failed_thread_creation_retirement/completed_cleanup.rs"]
mod completed_cleanup;
#[path = "failed_thread_creation_retirement/prepublication_cleanup.rs"]
mod prepublication_cleanup;
#[path = "failed_thread_creation_retirement/reconstruction_cleanup.rs"]
mod reconstruction_cleanup;

#[test]
fn partial_failed_creation_conversion_retains_actual_prior_and_retired_successor_until_drained() {
    use crate::composer_host::{
        ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey,
        ComposerHostRequestKind, ComposerHostRequestPurpose,
    };
    let fixture = support::Fixture::new("retired-partial-creation", 111);
    let service = service(&fixture, true);
    let saved = save(&fixture, &service);
    let prior = saved.selected();
    let committed = committed(&fixture, prior);
    let receipt = activated(&fixture, &service, &committed);
    let saved = saved.adopt(receipt, committed.selection).ok().unwrap();
    let key = ComposerHostRequestKey::new(
        prior.binding(),
        ComposerHostRequestId::new(std::num::NonZeroU64::new(999).unwrap()),
        ComposerHostRequestPurpose::Viewport,
    );
    service
        .slot
        .lock()
        .unwrap()
        .test_selected_host_mut()
        .unwrap()
        .begin_request(
            key,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: syndic_storage::DraftPieceTextDemandV1::Validate(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut source = source(prior);
    source.saved = Some(saved);
    source.receipt = Some(receipt);
    source.committed_target = Some(committed.selection);
    let (service, source, _) = service
        .retire_failed_thread_creation(source, &markers)
        .err()
        .unwrap();
    assert_eq!(service.selected_identity(), Some(prior));
    assert!(service.slot.lock().unwrap().pending_receipt().is_none());
    assert!(
        service
            .slot
            .lock()
            .unwrap()
            .test_selected_host_mut()
            .unwrap()
            .cancel_request(key)
    );
    let mut retirement = service
        .retire_failed_thread_creation(source, &markers)
        .ok()
        .unwrap();
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    retirement
        .accept_committed_claim(&committed, &mut candidate, &state)
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
}

#[test]
fn retained_original_successor_abandonment_settles_before_remaining_candidate_cleanup() {
    for committed_abandonment in [false, true] {
        let fixture = support::Fixture::new(
            "retired-pending-successor",
            if committed_abandonment { 81 } else { 71 },
        );
        let service = service(&fixture, true);
        let saved = save(&fixture, &service);
        let prior = saved.selected();
        let committed = committed(&fixture, prior);
        let receipt = activated(&fixture, &service, &committed);
        let journal = if committed_abandonment {
            fixture
                .faults
                .fail_next(FaultPoint::AfterCommitBeforePersist);
            None
        } else {
            Some(beryl_home_store::test_faults::fail_next_journal_write())
        };
        assert!(matches!(
            service.retire_pending(receipt).unwrap(),
            crate::main_window::MainWindowComposerRetirementAdvance::Pending
        ));
        drop(journal);
        let seals = fixture.marker_seals();
        fail(&fixture);
        let saved = saved.retire_failed_home().ok().unwrap();
        let markers = seals.capture_failed_home(&fixture.store).unwrap();
        let mut source = source(prior);
        source.saved = Some(saved);
        source.receipt = Some(receipt);
        source.committed_target = Some(committed.selection);
        let mut retirement = service
            .retire_failed_thread_creation(source, &markers)
            .ok()
            .unwrap();
        drop(markers);
        drop(seals);
        let (_directory, store, _) = fixture.into_store();
        let mut candidate = store.recover_same_home().unwrap();
        let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
        assert!(
            retirement
                .settle_predecessor_publication(&mut candidate, &storage)
                .unwrap()
        );
        retirement
            .accept_committed_claim(&committed, &mut candidate, &state)
            .unwrap();
        retirement
            .settle_remaining_cleanup(&mut candidate, &storage)
            .unwrap();
        let revision = candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap();
        retirement
            .settle_remaining_cleanup(&mut candidate, &storage)
            .unwrap();
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
    }
}

#[test]
fn failed_creation_rejects_foreign_original_source_without_consuming_its_resident() {
    let fixture = support::Fixture::new("retired-foreign-source", 91);
    let foreign = support::Fixture::new("retired-foreign-source-other", 101);
    let other = service(&foreign, false);
    let service = service(&fixture, false);
    let prior = service.selected_identity().unwrap();
    let mut source = source(prior);
    source.prior = other.selected_identity().unwrap();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let (service, mut source, _) = service
        .retire_failed_thread_creation(source, &markers)
        .err()
        .unwrap();
    assert_eq!(service.selected_identity(), Some(prior));
    source.prior = prior;
    let mut retirement = service
        .retire_failed_thread_creation(source, &markers)
        .ok()
        .unwrap();
    let other_seals = foreign.marker_seals();
    fail(&foreign);
    drop(other);
    drop(other_seals);
    let (_directory, store, _) = foreign.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .is_err()
    );
}

#[test]
fn failed_creation_conversion_refuses_service_alias_and_returns_original_source() {
    let fixture = support::Fixture::new("retired-creation-alias", 21);
    let service = service(&fixture, false);
    let prior = service.selected_identity().unwrap();
    let alias = service.clone();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let (service, retained, _) = service
        .retire_failed_thread_creation(source(prior), &markers)
        .err()
        .unwrap();
    assert_eq!(retained.prior, prior);
    assert_eq!(service.selected_identity(), Some(prior));
    drop(alias);
    let retirement = service
        .retire_failed_thread_creation(retained, &markers)
        .ok()
        .unwrap();
    assert_eq!(retirement.prior_selection(), prior);
    assert!(retirement.into_prior_after_proven_noncommit().is_err());
}

#[test]
fn never_admitted_failed_creation_retains_newer_unsaved_candidate_for_prior_recovery() {
    let fixture = support::Fixture::new("retired-unsaved-creation", 31);
    let service = service(&fixture, true);
    let prior = service.selected_identity().unwrap();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut retirement = service
        .retire_failed_thread_creation(source(prior), &markers)
        .ok()
        .unwrap();
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert!(
        !retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    let prior_retirement = retirement.into_prior_after_proven_noncommit().ok().unwrap();
    assert_eq!(prior_retirement.selection(), prior);
}

#[test]
fn committed_creation_retirement_requires_original_claim_and_disposes_each_session_once() {
    for disposal_cut in 0..4 {
        let fixture = support::Fixture::new("retired-committed-creation", 41 + disposal_cut);
        let service = service(&fixture, true);
        let saved = save(&fixture, &service);
        let prior = saved.selected();
        let committed = committed(&fixture, prior);
        let receipt = activated(&fixture, &service, &committed);
        let saved = saved.adopt(receipt, committed.selection).ok().unwrap();
        if disposal_cut != 0 {
            let mut slot = service.slot.lock().unwrap();
            slot.begin_committed_claim_disposal(
                &fixture.store,
                receipt,
                prior,
                support::operation_id(215),
            )
            .unwrap();
            if disposal_cut == 2 {
                fixture
                    .faults
                    .fail_next(FaultPoint::AfterCommitBeforePersist);
            }
            for _ in 0..if disposal_cut == 3 { 0 } else { 16 } {
                if matches!(
                    slot.advance_committed_claim_disposal(&fixture.store, receipt)
                        .unwrap(),
                    MainWindowComposerPublishAdvance::WidgetReleaseRequired(_)
                ) {
                    break;
                }
                if fixture.store.health().state() != HomeHealthState::Healthy {
                    break;
                }
            }
        }
        let seals = fixture.marker_seals();
        fail(&fixture);
        let markers = seals.capture_failed_home(&fixture.store).unwrap();
        let mut source = source(prior);
        source.saved = Some(saved);
        source.receipt = Some(receipt);
        source.committed_target = Some(committed.selection);
        let mut retirement = service
            .retire_failed_thread_creation(source, &markers)
            .ok()
            .unwrap();
        drop(markers);
        drop(seals);
        let (_directory, store, _) = fixture.into_store();
        let mut candidate = store.recover_same_home().unwrap();
        let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
        assert!(
            retirement
                .settle_predecessor_publication(&mut candidate, &storage)
                .unwrap()
        );
        assert!(
            retirement
                .settle_remaining_cleanup(&mut candidate, &storage)
                .is_err()
        );
        retirement
            .accept_committed_claim(&committed, &mut candidate, &state)
            .unwrap();
        retirement
            .accept_predecessor_widget_release(prior, &[])
            .unwrap();
        retirement
            .settle_remaining_cleanup(&mut candidate, &storage)
            .unwrap();
        let revision = candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap();
        retirement
            .settle_remaining_cleanup(&mut candidate, &storage)
            .unwrap();
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
        assert!(retirement.into_prior_after_proven_noncommit().is_err());
    }
}

#[test]
fn completed_predecessor_custody_survives_target_promotion_without_repeating_disposal() {
    let fixture = support::Fixture::new("retired-promoted-creation", 61);
    let service = service(&fixture, true);
    let saved = save(&fixture, &service);
    let prior = saved.selected();
    let committed = committed(&fixture, prior);
    let receipt = activated(&fixture, &service, &committed);
    let saved = saved.adopt(receipt, committed.selection).ok().unwrap();
    {
        let mut slot = service.slot.lock().unwrap();
        slot.begin_committed_claim_disposal(
            &fixture.store,
            receipt,
            prior,
            support::operation_id(215),
        )
        .unwrap();
        for _ in 0..16 {
            if matches!(
                slot.advance_committed_claim_disposal(&fixture.store, receipt)
                    .unwrap(),
                MainWindowComposerPublishAdvance::WidgetReleaseRequired(_)
            ) {
                break;
            }
        }
    }
    let completed = service
        .take_completed_thread_predecessor_disposal(receipt, prior)
        .unwrap();
    let release = crate::main_window::MainWindowComposerWidgetRelease::for_test(prior);
    {
        let mut slot = service.slot.lock().unwrap();
        slot.begin_final_publish(&fixture.store, receipt, prior)
            .unwrap();
        assert!(matches!(
            slot.complete_publish_after_widget_release(&fixture.store, receipt, &release)
                .unwrap(),
            MainWindowComposerPublishAdvance::Published(_)
        ));
    }
    let selected = service.selected_identity().unwrap();
    assert_ne!(selected.claim(), prior.claim());
    let seals = fixture.marker_seals();
    fail(&fixture);
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut source = source(prior);
    source.selected = selected;
    source.saved = Some(saved);
    source.receipt = Some(receipt);
    source.committed_target = Some(committed.selection);
    source.completed_predecessor = Some(completed);
    source.mounted_successor = Some(selected);
    source.release = Some(release);
    let mut retirement = service
        .retire_failed_thread_creation(source, &markers)
        .ok()
        .unwrap();
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    retirement
        .accept_committed_claim(&committed, &mut candidate, &state)
        .unwrap();
    retirement
        .accept_successor_widget_release(selected, &[])
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    retirement
        .settle_remaining_cleanup(&mut candidate, &storage)
        .unwrap();
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}
