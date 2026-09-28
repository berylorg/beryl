use beryl_app::composer_host::{
    ComposerHostFlushState, ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey,
    ComposerHostRequestKind, ComposerHostRequestPurpose,
};
use beryl_home_store::{HomeHealthState, test_faults::FaultPoint};
use syndic_storage::DraftPieceTextDemandV1;

use super::{composer, support};
use support::host_close::{begin_close, capture, ready};

#[test]
fn clean_host_retirement_preserves_saved_facts_after_home_failure() {
    for dirty in [false, true] {
        let mut fixture = support::host("retire-clean-host", 31 + u8::from(dirty) * 4);
        let initial = fixture.host.binding().unwrap();
        if dirty {
            composer::commit_text(
                &mut fixture.host,
                &fixture.store,
                initial,
                1,
                0,
                0,
                "saved",
                5,
                1,
            );
        }
        let close = begin_close(&mut fixture);
        ready(&mut fixture, close, 10);
        let saved = fixture.host.binding().unwrap();
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
        let retired = Box::new(fixture.host)
            .retire_clean_window_close(close)
            .unwrap_or_else(|_| panic!("ready host refused retirement"));
        assert_eq!(retired.binding(), saved);
        assert_eq!(retired.checkpoint(), saved.candidate());
        assert_eq!(retired.thread_id(), fixture.thread);
        assert_eq!(retired.close_ticket(), close);
        support::assert_history_preserved(retired.binding().history(), saved.history());
        assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
        let mut recovery = fixture.store.recover_same_home().unwrap();
        let fresh = syndic_storage::SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        let (retired, _) = retired
            .rebind_candidate(&access, fixture.storage.clone())
            .err()
            .unwrap();
        assert_eq!(retired.binding(), saved);
        assert!(
            retired
                .saved_checkpoint_matches_candidate(&access, &fixture.storage)
                .is_err()
        );
        for _ in 0..2 {
            assert!(
                retired
                    .saved_checkpoint_matches_candidate(&access, &fresh)
                    .unwrap()
            );
            assert_eq!(access.home_revision().unwrap(), revision);
        }
        let foreign = support::host("retired-checkpoint-foreign", 201);
        let (retired, _) = retired
            .rebind_candidate(&access, foreign.storage.clone())
            .err()
            .unwrap();
        assert_eq!(retired.binding(), saved);
        assert!(
            retired
                .saved_checkpoint_matches_candidate(&access, &foreign.storage)
                .is_err()
        );
        foreign.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(foreign.store.home_revision().is_err());
        let mut foreign_recovery = foreign.store.recover_same_home().unwrap();
        let foreign_storage =
            syndic_storage::SyndicStorage::reacquire_candidate(&foreign_recovery).unwrap();
        assert!(matches!(
            retired.saved_checkpoint_matches_candidate(
                &foreign_recovery.recovery_access().unwrap(),
                &foreign_storage
            ),
            Err(beryl_app::composer_host::ComposerHostError::ForeignHome { .. })
        ));
        foreign_recovery.abort().close().unwrap();
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        let (retired, _) = retired
            .rebind_candidate(&access, fresh.clone())
            .err()
            .unwrap();
        assert_eq!(retired.binding(), saved);
        for _ in 0..2 {
            assert!(
                retired
                    .saved_checkpoint_matches_candidate(&access, &fresh)
                    .is_err()
            );
        }
        assert!(recovery.publish().is_err());
    }
}

#[test]
fn retired_checkpoint_rejects_a_changed_durable_selector() {
    let mut fixture = support::host("retired-checkpoint-changed", 211);
    let close = begin_close(&mut fixture);
    ready(&mut fixture, close, 10);
    let retired = Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_or_else(|_| panic!("ready host refused retirement"));
    let (host, binding) = composer::activated(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        214,
        215,
    );
    fixture.host = host;
    composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        binding,
        1,
        0,
        0,
        "new",
        3,
        1,
    );
    let newer_close = begin_close(&mut fixture);
    ready(&mut fixture, newer_close, 10);
    let newer = Box::new(fixture.host)
        .retire_clean_window_close(newer_close)
        .unwrap_or_else(|_| panic!("newer ready host refused retirement"));
    assert_ne!(retired.selector(), newer.selector());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = syndic_storage::SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    let old_binding = retired.binding();
    let (retired, _) = retired
        .rebind_candidate(&access, fresh.clone())
        .err()
        .unwrap();
    assert_eq!(retired.binding(), old_binding);
    assert!(
        retired
            .saved_checkpoint_matches_candidate(&access, &fresh)
            .is_err()
    );
    assert!(
        newer
            .saved_checkpoint_matches_candidate(&access, &fresh)
            .unwrap()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    recovery.abort().close().unwrap();
}

#[test]
fn retirement_refuses_stale_and_unready_tickets_without_losing_the_host() {
    let mut fixture = support::host("retire-stale-close", 41);
    let old = begin_close(&mut fixture);
    ready(&mut fixture, old, 10);
    assert!(fixture.host.release_window_close(old).unwrap());
    let close = begin_close(&mut fixture);
    let binding = fixture.host.binding();
    let host = Box::new(fixture.host)
        .retire_clean_window_close(old)
        .unwrap_err();
    fixture.host = *host.retire_clean_window_close(close).unwrap_err();
    assert_eq!(fixture.host.binding(), binding);
    assert_eq!(fixture.host.window_close_ticket(), Some(close));
    ready(&mut fixture, close, 11);
    assert!(
        Box::new(fixture.host)
            .retire_clean_window_close(close)
            .is_ok()
    );
}

#[test]
fn authorized_disposal_cannot_be_reclassified_as_clean_retirement() {
    let mut fixture = support::host("retire-disposal", 51);
    let close = begin_close(&mut fixture);
    ready(&mut fixture, close, 10);
    fixture
        .host
        .authorize_window_close_disposal(&fixture.store, close)
        .unwrap();
    let host = Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    assert_eq!(host.window_close_ticket(), Some(close));
}

#[test]
fn pending_save_and_ambiguous_save_retain_the_complete_host() {
    let mut fixture = support::host("retire-pending-save", 61);
    let binding = fixture.host.binding().unwrap();
    composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        binding,
        1,
        0,
        0,
        "a",
        1,
        1,
    );
    let close = begin_close(&mut fixture);
    fixture.host = *Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    capture(&mut fixture, close, 10);
    fixture.host = *Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    let faults = fixture.faults.clone();
    fixture
        .host
        .test_arm_publication_before_execute_fault(move |_, _| {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        });
    fixture.host.advance_flush(&fixture.store, close).unwrap();
    let host = Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(host.window_close_ticket(), Some(close));
}

#[test]
fn ready_close_still_requires_pending_range_requests_to_drain() {
    let mut fixture = support::host("retire-range-request", 71);
    let close = begin_close(&mut fixture);
    ready(&mut fixture, close, 10);
    let key = ComposerHostRequestKey::new(
        fixture.host.binding().unwrap(),
        ComposerHostRequestId::new(std::num::NonZeroU64::new(100).unwrap()),
        ComposerHostRequestPurpose::Viewport,
    );
    fixture
        .host
        .begin_request(
            key,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 16,
            },
        )
        .unwrap();
    assert_eq!(
        fixture.host.flush_state(close).unwrap(),
        ComposerHostFlushState::CloseReady
    );
    let mut host = Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    assert!(host.cancel_request(key));
    assert!(host.retire_clean_window_close(close).is_ok());
}

#[test]
fn admitted_mutation_is_retained_on_retirement_refusal() {
    let mut fixture = support::host("retire-mutation", 81);
    let binding = fixture.host.binding().unwrap();
    composer::begin_text(&mut fixture.host, &fixture.store, binding, 1, 0).unwrap();
    let close = begin_close(&mut fixture);
    let host = Box::new(fixture.host)
        .retire_clean_window_close(close)
        .unwrap_err();
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.settlement_custody_in_use(), 1);
    assert_eq!(host.window_close_ticket(), Some(close));
}
