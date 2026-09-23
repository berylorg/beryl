#![cfg(feature = "test-faults")]

mod support;
#[path = "discussion_handoff/queue.rs"]
mod queue;
#[path = "discussion_handoff/child_settlement.rs"]
mod child_settlement;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasTurnId, InputGateRevision, JobId, ResolutionIntentId, ThreadRevision};
use support::{TestHome, id, seed_populated, timestamp};
use syndic_storage::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn seeded(
    home: &TestHome,
    faults: FaultController,
) -> (HomeStore, SyndicStorage, AdmitDiscussionHandoff) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let request = support::discussion_handoff::active_request(
        &store,
        &storage,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
    );
    (store, storage, request)
}

fn command(store: &HomeStore, prepared: PreparedDiscussionHandoff) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    command
}

fn execute(store: &HomeStore, prepared: PreparedDiscussionHandoff) {
    assert!(matches!(
        store.execute(command(store, prepared)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn admission_rejects_stale_proofs_and_failure_release_preserves_unrelated_records() {
    let home = TestHome::new("handoff-admission");
    let (store, storage, request) = seeded(&home, FaultController::new());
    let before_thread = storage.thread(&store, id(36), limit()).unwrap();
    let before_attributes = storage.thread_attributes(&store, id(36), limit()).unwrap();
    let before_input = storage.input_gate(&store, id(36), limit()).unwrap();
    let before_parent = storage.input_gate(&store, id(30), limit()).unwrap();
    let mut variants = Vec::new();
    let mut changed = request.clone();
    changed.thread_revision = ThreadRevision::new(999).unwrap();
    variants.push(changed);
    let mut changed = request.clone();
    changed.input_gate_revision = InputGateRevision::new(999).unwrap();
    variants.push(changed);
    let mut changed = request.clone();
    changed.attributes_revision = ThreadAttributesRevision::new(999).unwrap();
    variants.push(changed);
    let mut changed = request.clone();
    changed.turn_state_revision = TurnStateRevision::new(999).unwrap();
    variants.push(changed);
    let mut changed = request.clone();
    changed.parent.accepted_high_water += 1;
    variants.push(changed);
    let mut changed = request.clone();
    changed.parent.thread_id = id(36);
    variants.push(changed);
    let mut changed = request.clone();
    changed.parent.input_gate_revision = InputGateRevision::new(999).unwrap();
    variants.push(changed);
    let mut changed = request.clone();
    changed.resolving_target = SteeringTargetProof::new(
        changed.resolving_target.pending().clone(),
        CasTurnId::new("foreign-turn").unwrap(),
    );
    variants.push(changed);
    let mut changed = request.clone();
    changed.context_digest = beryl_model::DiscussionContextDigest::from_bytes([255; 32]);
    variants.push(changed);
    for changed in variants {
        let prepared = storage
            .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(changed))
            .unwrap();
        let intent = prepared.intent().clone();
        assert!(matches!(
            store.execute(command(&store, prepared)),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(
            storage.discussion_handoff_status(&store, &intent).unwrap(),
            DiscussionHandoffStatus::ExactOld
        );
    }
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let intent = prepared.intent().clone();
    execute(&store, prepared);
    assert_eq!(
        storage.discussion_handoff_status(&store, &intent).unwrap(),
        DiscussionHandoffStatus::ExactNew
    );
    let prepared = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::Release {
                expected: intent.new_gate(),
            },
        )
        .unwrap();
    let release = prepared.intent().clone();
    execute(&store, prepared);
    assert_eq!(
        storage.discussion_handoff_status(&store, &release).unwrap(),
        DiscussionHandoffStatus::ExactNew
    );
    assert_eq!(
        storage.thread(&store, id(36), limit()).unwrap(),
        before_thread
    );
    assert_eq!(
        storage.thread_attributes(&store, id(36), limit()).unwrap(),
        before_attributes
    );
    assert_eq!(
        storage.input_gate(&store, id(36), limit()).unwrap(),
        before_input
    );
    assert_eq!(
        storage.input_gate(&store, id(30), limit()).unwrap(),
        before_parent
    );
    assert!(
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::Release {
                    expected: intent.new_gate()
                }
            )
            .is_err()
    );
    store.close().unwrap();
}

#[test]
fn success_recovery_verifies_both_records_and_fences_old_handles() {
    for (point, expected) in [
        (FaultPoint::BeforeCommit, DiscussionHandoffStatus::ExactOld),
        (
            FaultPoint::AfterCommitBeforePersist,
            DiscussionHandoffStatus::ExactNew,
        ),
    ] {
        let home = TestHome::new("handoff-recovery");
        let faults = FaultController::new();
        let (store, storage, request) = seeded(&home, faults.clone());
        let admit = storage
            .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
            .unwrap();
        let pending = admit.intent().new_gate();
        execute(&store, admit);
        let prepared = storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::ReleaseAndArchive {
                    expected: pending,
                    attributes_revision: ThreadAttributesRevision::FIRST,
                    archived_at: timestamp(20),
                },
            )
            .unwrap();
        let intent = prepared.intent().clone();
        let cmd = command(&store, prepared);
        faults.fail_next(point);
        match store.execute(cmd) {
            CommandOutcome::NotCommitted { .. } if point == FaultPoint::BeforeCommit => {}
            CommandOutcome::Indeterminate { reconciliation, .. }
                if point == FaultPoint::AfterCommitBeforePersist =>
            {
                reconciliation.install();
            }
            other => panic!("unexpected handoff fault: {other:?}"),
        }
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            storage
                .discussion_handoff_status_candidate(&access, &intent)
                .is_err()
        );
        assert_eq!(
            fresh
                .discussion_handoff_status_candidate(&access, &intent)
                .unwrap(),
            expected
        );
        for pending in access.pending_reconciliations() {
            assert!(matches!(
                access.reconcile(&pending).unwrap(),
                beryl_home_store::ReconciliationResolution::ExactNew { .. }
            ));
        }
        let store = recovery.publish().unwrap();
        assert_eq!(
            fresh.discussion_handoff_status(&store, &intent).unwrap(),
            expected
        );
        store.close().unwrap();
    }
}

#[test]
fn mixed_archive_outcome_and_exhausted_gate_cannot_report_success() {
    use syndic_storage::test_faults::FixtureRecord;
    let home = TestHome::new("handoff-collision");
    let (store, storage, request) = seeded(&home, FaultController::new());
    let admit = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = admit.intent().new_gate();
    execute(&store, admit);
    let old_attributes = storage
        .thread_attributes(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let thread = storage.thread(&store, id(36), limit()).unwrap().unwrap();
    let canonical = storage
        .canonical_item(
            &store,
            beryl_model::SyndicItemId::from_bytes([202; 16]),
            limit(),
        )
        .unwrap()
        .unwrap();
    let title = GeneratedThreadTitle::new(
        "Discussion result",
        thread.committed_tail().unwrap(),
        canonical.presentation_content().unwrap(),
        thread.selected_path_digest(),
        thread.revision(),
        timestamp(12),
    )
    .unwrap();
    let stale = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: pending,
                attributes_revision: old_attributes.revision(),
                archived_at: timestamp(20),
            },
        )
        .unwrap();
    let mut title_command = HomeCommand::new(store.home_revision().unwrap());
    title_command
        .add(storage.accept_generated_thread_title(
            storage.revision(&store).unwrap(),
            AcceptGeneratedThreadTitle::new(id(36), old_attributes.revision(), title.clone()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(title_command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        store.execute(command(&store, stale)),
        CommandOutcome::NotCommitted { .. }
    ));
    let old_attributes = storage
        .thread_attributes(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let prepared = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: pending,
                attributes_revision: old_attributes.revision(),
                archived_at: timestamp(20),
            },
        )
        .unwrap();
    let intent = prepared.intent().clone();
    execute(&store, prepared);
    assert_eq!(
        storage
            .thread_attributes(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .generated_title(),
        Some(&title)
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::ThreadAttributes(old_attributes)]),
    );
    assert_eq!(
        storage.discussion_handoff_status(&store, &intent).unwrap(),
        DiscussionHandoffStatus::Collision
    );
    let exhausted = DiscussionHandoffGateRecord::new(
        id(36),
        DiscussionHandoffGateRevision::new(u64::MAX).unwrap(),
        pending.state(),
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::DiscussionHandoffGate(exhausted)]),
    );
    assert!(
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::Release {
                    expected: exhausted
                }
            )
            .is_err()
    );
    store.close().unwrap();
}
