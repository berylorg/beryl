#![cfg(feature = "test-faults")]

#[path = "activity_window/fixture.rs"]
mod fixture;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;

use beryl_app::{ActivityWindowHarness, ActivityWindowTestStep};
use beryl_home_store::HomeStore;
use beryl_model::BerylHomeId;
use fixture::Fixture;
use std::{num::NonZeroUsize, sync::Arc};
use support::id;

fn harness(f: &Fixture, rows: usize, pages: usize) -> ActivityWindowHarness {
    let (window, claim) = f.window();
    ActivityWindowHarness::new(
        &f.home, &f.storage, &f.state, &f.owner, window, claim, rows, pages,
    )
}

fn ready(reader: &mut ActivityWindowHarness) {
    for _ in 0..4096 {
        match reader.step() {
            ActivityWindowTestStep::Ready => return,
            ActivityWindowTestStep::Pending => {}
            state => panic!(
                "unexpected Activity state {state:?}: {:?}",
                reader.failure()
            ),
        }
        let (checkpoints, pages, _, raw) = reader.diagnostics();
        assert!(checkpoints <= 8 && pages <= 4 && raw <= 1);
    }
    panic!("bounded Activity test did not finish");
}

#[test]
fn empty_and_active_collections_publish_exact_visibility_and_bounded_rows() {
    let f = Fixture::new(true);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let empty = reader.prepared().unwrap();
    let initial = empty.publish().unwrap();
    assert_eq!(initial.total, 0);
    assert!(initial.visible);
    let source = f.activate();
    for index in 230..235 {
        f.add_command(&source, index);
    }
    assert!(empty.publish().is_err());
    ready(&mut reader);
    let current = reader.prepared().unwrap().publish().unwrap();
    assert_eq!(current.total, 5);
    assert!(current.visible);
    assert_eq!(current.rows.len(), 2);
    assert!(
        current
            .rows
            .iter()
            .all(|(_, _, agent, value)| agent == "Main" && value.starts_with("command-"))
    );
    assert_eq!(reader.diagnostics().3, 0);
}

#[test]
fn distant_and_backward_demands_bound_checkpoints_and_revoke_old_publications() {
    let f = Fixture::new(true);
    let source = f.activate();
    for index in 230..255 {
        f.add_command(&source, index);
    }
    let mut reader = harness(&f, 2, 1);
    reader.demand(24..25);
    ready(&mut reader);
    let old = reader.prepared().unwrap();
    assert_eq!(old.publish().unwrap().rows[0].0, 24);
    assert_eq!(reader.diagnostics().0, 8);
    reader.demand(0..1);
    assert!(old.publish().is_err());
    ready(&mut reader);
    assert_eq!(
        reader
            .prepared()
            .unwrap()
            .publish()
            .unwrap()
            .rows
            .last()
            .unwrap()
            .0,
        1
    );
    reader.demand(20..21);
    assert_eq!(reader.step(), ActivityWindowTestStep::Pending);
    reader.demand(4..5);
    ready(&mut reader);
    assert!(
        reader
            .prepared()
            .unwrap()
            .publish()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.0 == 4)
    );
}

#[test]
fn failed_initial_query_keeps_auto_eligibility_and_only_explicit_retry_reopens() {
    let f = Fixture::new(false);
    let mut reader = harness(&f, 2, 1);
    assert_eq!(reader.step(), ActivityWindowTestStep::Failed);
    assert!(reader.eligibility().unwrap());
    assert!(reader.failure().is_some());
    assert_eq!(reader.step(), ActivityWindowTestStep::Failed);
    f.interest
        .as_ref()
        .unwrap()
        .enroll_activity_for_test(&f.home, &f.storage, id(30), f.turn)
        .unwrap();
    let source = f.activate();
    f.add_command(&source, 230);
    assert!(reader.retry());
    assert!(!reader.retry());
    ready(&mut reader);
    assert_eq!(reader.prepared().unwrap().publish().unwrap().total, 1);
}

#[test]
fn query_replacement_and_cancellation_release_raw_capacity_and_make_old_models_inert() {
    let f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let old = reader.prepared().unwrap();
    f.add_command(&source, 231);
    assert!(old.publish().is_err());
    ready(&mut reader);
    assert_eq!(reader.prepared().unwrap().publish().unwrap().total, 2);
    let current = reader.prepared().unwrap();
    reader.cancel();
    assert_eq!(reader.step(), ActivityWindowTestStep::Retired);
    assert!(current.publish().is_err());
    assert_eq!(reader.diagnostics(), (0, 0, false, 0));
}

#[test]
fn graph_window_and_service_retirement_reject_late_publications() {
    for kind in 0..3 {
        let f = Fixture::new(true);
        let mut reader = harness(&f, 2, 1);
        ready(&mut reader);
        let old = reader.prepared().unwrap();
        match kind {
            0 => reader.retire_graph(),
            1 => reader.retire_window(),
            _ => reader.retire_service(),
        }
        assert!(old.publish().is_err());
        assert_ne!(reader.step(), ActivityWindowTestStep::Ready);
        assert_eq!(reader.diagnostics().3, 0);
    }
}

#[test]
fn page_failures_retry_the_same_position_after_the_initial_request_completed() {
    for start in [0, 2] {
        let f = Fixture::new(true);
        let source = f.activate();
        for index in 230..234 {
            f.add_command(&source, index);
        }
        let mut reader = harness(&f, 2, 1);
        if start == 0 {
            assert_eq!(reader.step(), ActivityWindowTestStep::Pending);
        } else {
            ready(&mut reader);
            reader.demand(2..3);
        }
        assert_eq!(reader.request_state(), (false, true, start));
        reader.with_page_capacity_held(|reader| {
            assert_eq!(reader.step(), ActivityWindowTestStep::Failed);
            assert!(reader.failure().unwrap().contains("page capacity"));
            assert_eq!(reader.request_state(), (false, true, start));
        });
        assert_eq!(reader.step(), ActivityWindowTestStep::Failed);
        assert!(reader.retry());
        assert!(reader.failure().is_some());
        assert!(!reader.retry());
        ready(&mut reader);
        assert!(reader.failure().is_none());
        assert!(
            reader
                .prepared()
                .unwrap()
                .publish()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.0 == start)
        );
        assert_eq!(reader.diagnostics().3, 0);
    }
}

#[test]
fn busy_publication_returns_without_waiting_and_can_publish_after_release() {
    let f = Fixture::new(true);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let prepared = reader.prepared().unwrap();
    reader.with_publication_gate_held(|_| {
        assert!(prepared.publish().unwrap_err().contains("busy"));
    });
    assert_eq!(prepared.publish().unwrap().total, 0);
}

#[test]
fn runtime_replacement_rejects_persisted_rows_and_old_prepared_models() {
    let mut f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let prepared = reader.prepared().unwrap();
    f.interest.take();
    assert!(f.probe.wait_for_disposal(fixture::TIMEOUT));
    assert!(prepared.publish().is_err());
    f.acquire(false);
    assert_ne!(reader.step(), ActivityWindowTestStep::Ready);
    assert!(prepared.publish().is_err());
}

#[test]
fn demand_larger_than_four_resident_pages_fails_with_bounded_retention() {
    let f = Fixture::new(true);
    let source = f.activate();
    for index in 230..242 {
        f.add_command(&source, index);
    }
    let mut reader = harness(&f, 2, 1);
    reader.demand(0..12);
    for _ in 0..4096 {
        match reader.step() {
            ActivityWindowTestStep::Pending => {}
            ActivityWindowTestStep::Failed => {
                assert!(reader.failure().unwrap().contains("resident page capacity"));
                assert_eq!(reader.diagnostics().1, 4);
                assert_eq!(reader.diagnostics().3, 0);
                return;
            }
            state => panic!("unexpected state {state:?}"),
        }
    }
    panic!("capacity failure did not finish");
}

#[test]
fn cached_pages_do_not_prevent_a_new_multi_page_demand() {
    let f = Fixture::new(true);
    let source = f.activate();
    for index in 230..242 {
        f.add_command(&source, index);
    }
    let mut reader = harness(&f, 2, 1);
    for start in [0, 2, 4] {
        reader.demand(start..start + 1);
        ready(&mut reader);
    }
    reader.demand(8..12);
    ready(&mut reader);
    let current = reader.prepared().unwrap().publish().unwrap();
    assert_eq!(
        current.rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![8, 9, 10, 11]
    );
    reader.demand(9..12);
    ready(&mut reader);
    assert_eq!(reader.diagnostics().1, 2);
    reader.demand(0..4);
    ready(&mut reader);
    reader.demand(1..6);
    ready(&mut reader);
    assert_eq!(reader.diagnostics().1, 3);
}

#[test]
fn metadata_completion_keeps_its_original_query_demand_and_runtime_authority() {
    for invalidation in 0..4 {
        let mut f = Fixture::new(true);
        let source = f.activate();
        f.add_command(&source, 230);
        let mut reader = harness(&f, 2, 1);
        ready(&mut reader);
        let prepared = reader.prepared().unwrap();
        let thread = prepared.first_thread().unwrap();
        let request = prepared.metadata_request(&thread).unwrap();
        let metadata = beryl_backend::lifecycle_test_support::thread_read_metadata(
            thread.clone(),
            beryl_backend::ThreadStatus::Idle,
            "provider-is-not-model",
            Some("Scout"),
        );
        match invalidation {
            0 => {
                let wrong = beryl_backend::lifecycle_test_support::thread_read_metadata(
                    beryl_model::CasThreadId::new("wrong-thread").unwrap(),
                    beryl_backend::ThreadStatus::Idle,
                    "provider",
                    Some("Wrong"),
                );
                assert!(
                    prepared
                        .metadata_request(&thread)
                        .unwrap()
                        .complete(&wrong)
                        .is_none()
                );
                let completion = request.complete(&metadata).unwrap();
                let duplicate = prepared
                    .metadata_request(&thread)
                    .unwrap()
                    .complete(&metadata)
                    .unwrap();
                let before = prepared.publish().unwrap();
                reader.with_publication_gate_held(|reader| {
                    assert!(request.complete(&metadata).is_none());
                    assert!(!reader.apply_metadata(&completion));
                });
                assert!(request.complete(&metadata).is_some());
                assert!(reader.apply_metadata(&completion));
                assert!(prepared.publish().is_err());
                assert!(!reader.apply_metadata(&duplicate));
                let after = reader.prepared().unwrap().publish().unwrap();
                assert_eq!(before.rows, after.rows);
                assert_eq!(after.rows[0].2, "Main");
                continue;
            }
            1 => reader.demand(0..1),
            2 => f.add_command(&source, 231),
            _ => {
                f.interest.take();
                assert!(f.probe.wait_for_disposal(fixture::TIMEOUT));
                f.acquire(false);
            }
        }
        assert!(request.complete(&metadata).is_none());
    }
}

#[gpui::test]
fn child_nickname_scheduling_retains_busy_completion_and_renews_after_query_revision(
    cx: &mut gpui::TestAppContext,
) {
    let (f, mut reader, source, child) = std::thread::spawn(|| {
        let f = Fixture::new(true);
        let source = f.activate();
        let child = f.child_answer();
        let mut reader = harness(&f, 2, 1);
        ready(&mut reader);
        (f, reader, source, child)
    })
    .join()
    .unwrap();
    let mut scheduler = beryl_app::main_window::ActivityMetadataSchedulingHarness::default();
    let response = |nickname| {
        beryl_backend::lifecycle_test_support::thread_read_metadata(
            child.clone(),
            beryl_backend::ThreadStatus::Idle,
            "provider-is-not-model",
            Some(nickname),
        )
    };
    assert_eq!(reader.prepared().unwrap().publish().unwrap().rows[0].2, "");
    assert!(scheduler.enqueue(&mut reader, response("Scout")));
    reader.with_publication_gate_held(|reader| scheduler.step(reader, &cx.background_executor));
    assert_eq!(reader.prepared().unwrap().publish().unwrap().rows[0].2, "");
    scheduler.step(&mut reader, &cx.background_executor);
    assert_eq!(
        reader.prepared().unwrap().publish().unwrap().rows[0].2,
        "Scout"
    );
    assert!(!scheduler.enqueue(&mut reader, response("duplicate")));
    let (f, mut reader) = std::thread::spawn(move || {
        f.add_command(&source, 231);
        ready(&mut reader);
        (f, reader)
    })
    .join()
    .unwrap();
    assert!(scheduler.enqueue(&mut reader, response("Scout 2")));
    scheduler.step(&mut reader, &cx.background_executor);
    let rows = reader.prepared().unwrap().publish().unwrap().rows;
    assert!(rows.iter().any(|row| row.2 == "Scout 2"));
    assert!(rows.iter().any(|row| row.2 == "Main"));
    drop((reader, f));
}

#[test]
fn a_changed_query_replaces_a_failed_page_without_retrying_its_old_scope() {
    let f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let mut reader = harness(&f, 2, 1);
    assert_eq!(reader.step(), ActivityWindowTestStep::Pending);
    reader.with_page_capacity_held(|reader| {
        assert_eq!(reader.step(), ActivityWindowTestStep::Failed);
    });
    f.add_command(&source, 231);
    assert_eq!(reader.step(), ActivityWindowTestStep::Pending);
    assert!(reader.failure().is_none());
    ready(&mut reader);
    assert_eq!(reader.prepared().unwrap().publish().unwrap().total, 2);
}

#[test]
fn height_save_outcomes_remain_owned_after_window_read_retirement() {
    use beryl_home_store::test_faults::FaultPoint;
    for (fault, expected) in [
        (None, ("committed", false)),
        (Some(FaultPoint::BeforeCommit), ("not_committed", false)),
        (
            Some(FaultPoint::AfterCommitBeforePersist),
            ("indeterminate", true),
        ),
        (Some(FaultPoint::AfterPersist), ("committed", true)),
    ] {
        let f = Fixture::new(true);
        let mut reader = harness(&f, 2, 1);
        if let Some(fault) = fault {
            f.faults.fail_next(fault);
        }
        reader.save_height(202.25).unwrap();
        reader.retire_window();
        assert_eq!(reader.retained_height_outcome(), Some(expected));
        assert_eq!(reader.step(), ActivityWindowTestStep::Retired);
        assert_eq!(reader.retained_height_outcome(), Some(expected));
        assert!(reader.save_height(222.).is_err());
        assert_eq!(reader.retained_height_outcome(), Some(expected));
    }
}

#[test]
fn metadata_request_and_completion_do_not_pin_retired_collection_capacity() {
    let f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let prepared = reader.prepared().unwrap();
    let thread = prepared.first_thread().unwrap();
    let request = prepared.metadata_request(&thread).unwrap();
    let metadata = beryl_backend::lifecycle_test_support::thread_read_metadata(
        thread,
        beryl_backend::ThreadStatus::Idle,
        "provider",
        Some("Scout"),
    );
    let completion = request.complete(&metadata).unwrap();
    drop(prepared);
    assert_eq!(reader.collection_count(), 1);
    reader.cancel();
    assert_eq!(reader.step(), ActivityWindowTestStep::Retired);
    assert_eq!(reader.collection_count(), 0);
    assert!(request.complete(&metadata).is_none());
    assert!(!reader.apply_metadata(&completion));
}

#[test]
fn renewed_claim_rejects_old_publications_and_pending_metadata() {
    let f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let mut reader = harness(&f, 2, 1);
    ready(&mut reader);
    let prepared = reader.prepared().unwrap();
    let thread = prepared.first_thread().unwrap();
    let request = prepared.metadata_request(&thread).unwrap();
    f.renew_window_claim();
    assert!(prepared.publish().is_err());
    let metadata = beryl_backend::lifecycle_test_support::thread_read_metadata(
        thread,
        beryl_backend::ThreadStatus::Idle,
        "provider",
        Some("Scout"),
    );
    assert!(request.complete(&metadata).is_none());
    assert_ne!(reader.step(), ActivityWindowTestStep::Ready);
    assert!(reader.eligibility().is_err());
}
