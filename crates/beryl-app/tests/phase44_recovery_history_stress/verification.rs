//! Component-local recovery capacity and progress assertions.

use beryl_app::cas_projection::{LoadedCasProjection, RecoveryReplayDiagnosticsSnapshot};
use syndic_storage::{
    BindingState, CasLineageProof, RecoveredInjectionProof, RecoveryProjection,
    test_faults::recovery_residency_metrics,
};

use crate::{
    history::{HistorySpec, InstalledHistory},
    syndic::{Fixture, point_limit},
};

pub fn assert_recovered_lineage(
    fixture: &Fixture,
    installed: InstalledHistory,
    history: &HistorySpec,
    expected_projection: RecoveryProjection,
    projection: &LoadedCasProjection,
    expected_target: &str,
) -> RecoveredInjectionProof {
    assert_eq!(projection.cas_thread_id().as_str(), expected_target);
    let CasLineageProof::RecoveredInjection(proof) = projection.lineage_proof() else {
        panic!("large recovery did not publish recovered-injection lineage")
    };
    assert_eq!(u64::from(proof.item_count().get()), history.item_count());
    assert_eq!(proof.utf8_bytes().get(), history.utf8_bytes());
    assert_eq!(proof.sequence_digest(), history.sequence_digest());
    assert_eq!(proof.version(), expected_projection.version());
    assert_eq!(
        proof.established_prefix(),
        expected_projection.represented_prefix()
    );
    assert_eq!(proof.item_count(), expected_projection.item_count());
    assert_eq!(proof.utf8_bytes(), expected_projection.utf8_bytes());
    assert_eq!(
        proof.sequence_digest(),
        expected_projection.sequence_digest()
    );
    assert_eq!(
        proof.established_prefix().tail(),
        installed.completed_prefix.tail()
    );
    assert_eq!(
        proof.established_prefix().digest(),
        installed.completed_prefix.digest()
    );
    assert_eq!(
        proof.established_prefix().source_thread_revision(),
        installed.selected_path.thread_revision()
    );

    let binding = fixture
        .storage
        .current_binding(&*fixture.home(), installed.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("successful recovery did not publish a valid durable binding")
    };
    assert_eq!(usable.cas_thread_id(), projection.cas_thread_id());
    assert_eq!(usable.lineage(), CasLineageProof::RecoveredInjection(proof));
    assert_eq!(usable.represented_prefix(), proof.established_prefix());
    proof
}

pub fn assert_failed_recovery_is_stale(
    fixture: &Fixture,
    installed: InstalledHistory,
    expected_target: &str,
) {
    let binding = fixture
        .storage
        .current_binding(&*fixture.home(), installed.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(binding.binding().selected_path(), installed.selected_path);
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("failed recovery unexpectedly left an authorizing binding")
    };
    assert_eq!(stale.cas_thread_id().as_str(), expected_target);
    assert_eq!(stale.observed_lineage(), None);
}

pub fn assert_live_capacity_one(
    snapshot: RecoveryReplayDiagnosticsSnapshot,
    logical_pages: u64,
    logical_items: u64,
    logical_utf8_bytes: u64,
) {
    assert!(!snapshot.released());
    assert!(snapshot.final_capacity().is_none());
    assert_eq!(snapshot.logical_pages(), logical_pages);
    assert_eq!(snapshot.logical_items(), logical_items);
    assert_eq!(snapshot.logical_utf8_bytes(), logical_utf8_bytes);

    let capacity = snapshot
        .live_capacity()
        .expect("recovery page and capacity-one rings are live");
    let pages = capacity.pages();
    assert_eq!(pages.page_capacity, 65_536);
    assert_eq!(pages.page_count, 1);
    assert_eq!(pages.available, 0);
    assert_eq!(pages.leased, 1);
    assert_eq!(pages.high_water, 1);
    assert_eq!(pages.exhausted, 0);

    for channel in [capacity.requests(), capacity.replies()] {
        assert_eq!(channel.capacity, 1);
        assert_eq!(channel.len, 0);
        assert!(channel.high_water <= 1);
        assert!(channel.sender_open && channel.receiver_open);
    }
}

pub fn assert_recovery_released(
    snapshot: RecoveryReplayDiagnosticsSnapshot,
    expected_pages: u64,
    expected_items: u64,
    expected_utf8_bytes: u64,
) {
    assert!(snapshot.released());
    assert!(snapshot.live_capacity().is_none());
    assert_eq!(snapshot.logical_pages(), expected_pages);
    assert_eq!(snapshot.logical_items(), expected_items);
    assert_eq!(snapshot.logical_utf8_bytes(), expected_utf8_bytes);

    let capacity = snapshot
        .final_capacity()
        .expect("recovery captures final local ownership facts");
    let pages = capacity.pages();
    assert_eq!(pages.page_capacity, 65_536);
    assert_eq!(pages.page_count, 1);
    assert_eq!(pages.available, 1);
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.high_water, 1);
    assert!(pages.total_leases >= expected_pages);
    assert_eq!(pages.exhausted, 0);

    for channel in [capacity.requests(), capacity.replies()] {
        assert_eq!(channel.capacity, 1);
        assert_eq!(channel.len, 0);
        assert_eq!(channel.high_water, 1);
    }
}

pub fn assert_recovery_plateau(
    first: RecoveryReplayDiagnosticsSnapshot,
    second: RecoveryReplayDiagnosticsSnapshot,
) {
    let first = first.final_capacity().unwrap();
    let second = second.final_capacity().unwrap();
    assert_eq!(second.pages().page_capacity, first.pages().page_capacity);
    assert_eq!(second.pages().page_count, first.pages().page_count);
    assert_eq!(second.pages().high_water, first.pages().high_water);
    assert_eq!(second.requests().capacity, first.requests().capacity);
    assert_eq!(second.requests().high_water, first.requests().high_water);
    assert_eq!(second.replies().capacity, first.replies().capacity);
    assert_eq!(second.replies().high_water, first.replies().high_water);
}

pub fn assert_syndic_constant_residency(expected_pages: u64, expected_items: u64) {
    let metrics = recovery_residency_metrics();
    assert_eq!(metrics.max_resident_turns(), 1);
    assert_eq!(metrics.max_resident_items(), 1);
    assert!(
        metrics.turn_item_read_attempts() >= usize::try_from(expected_items).unwrap(),
        "Syndic must traverse every recovery item through bounded index reads"
    );
    assert_eq!(
        metrics.cursor_page_count(),
        usize::try_from(expected_pages).unwrap()
    );
    assert!(metrics.max_cursor_page_bytes() <= 65_536);
}
