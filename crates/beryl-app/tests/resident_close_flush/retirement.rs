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
    }
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
