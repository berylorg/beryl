use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::SyndicItemId;
use syndic_storage::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    test_faults::{
        FixtureBatch, FixtureDelete, terminal_history_evidence_candidate_with_confirmation_hook,
    },
};

use crate::{
    finalizing_history_support::{
        completion_request, converge_first_item, finish_transcript, terminal_home,
    },
    recovery_support::{RecoveryHome, execute, ordered_id, pending_home, point_limit},
    support::open,
};

fn terminal_fixture(value: u64, complete: bool, settled: bool) -> RecoveryHome {
    let fixture = terminal_home("candidate-terminal-evidence", value, complete);
    if settled {
        if complete {
            converge_first_item(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
                SyndicItemId::from_bytes(*ordered_id(value + 30_000).as_bytes()),
            );
        }
        finish_transcript(&fixture.store, fixture.storage.clone(), fixture.thread);
        execute(
            &fixture.store,
            fixture.storage.complete_terminal_history(
                fixture.storage.revision(&fixture.store).unwrap(),
                completion_request(
                    &fixture.store,
                    fixture.storage.clone(),
                    fixture.thread,
                    fixture.turn,
                ),
            ),
        );
    }
    fixture
}

#[test]
fn candidate_terminal_evidence_preserves_fixed_point_identity_and_publication_parity() {
    let foreign = pending_home("foreign-terminal-evidence", 1000);
    for complete in [false, true] {
        for settled in [false, true] {
            let fixture = terminal_fixture(
                1001 + u64::from(complete) * 2 + u64::from(settled),
                complete,
                settled,
            );
            let original = fixture
                .storage
                .terminal_history_evidence(
                    &fixture.store,
                    fixture.thread,
                    fixture.turn,
                    point_limit(),
                )
                .unwrap();
            assert_eq!(original.is_some(), settled);
            fixture.store.close().unwrap();
            let faults = FaultController::new();
            let mut candidate = HomeOpenCandidate::open_with_faults(
                HomeOpenOptions::new(fixture.home.path(), HomeSchemaVersion::CURRENT),
                faults.clone(),
            )
            .unwrap();
            let storage = SyndicStorage::register(&mut candidate).unwrap();
            let mut publication = candidate
                .prepare_publication(SyndicStorage::required_domains().unwrap())
                .unwrap();
            let access = publication.recovery_access().unwrap();
            syndic_storage::test_faults::reset_syndic_point_read_count();
            let evidence = storage
                .terminal_history_evidence_candidate(
                    &access,
                    fixture.thread,
                    fixture.turn,
                    point_limit(),
                )
                .unwrap();
            assert!(syndic_storage::test_faults::syndic_point_read_count() <= 48);
            assert_eq!(
                evidence.map(|e| (
                    e.home_id(),
                    e.source_revision(),
                    e.selected_path(),
                    e.gate_revision(),
                    e.state_revision(),
                    e.lifecycle()
                )),
                original.map(|e| (
                    e.home_id(),
                    e.source_revision(),
                    e.selected_path(),
                    e.gate_revision(),
                    e.state_revision(),
                    e.lifecycle()
                ))
            );
            for stale in [&fixture.storage, &foreign.storage] {
                assert!(matches!(
                    stale.terminal_history_evidence_candidate(
                        &access,
                        fixture.thread,
                        fixture.turn,
                        point_limit()
                    ),
                    Err(SyndicReadError::Read(_))
                ));
            }
            assert!(
                storage
                    .terminal_history_evidence_candidate(
                        &access,
                        fixture.thread,
                        fixture.turn,
                        SyndicPointReadLimit::new(1).unwrap()
                    )
                    .is_err()
            );
            beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
                assert!(
                    storage
                        .terminal_history_evidence(
                            store,
                            fixture.thread,
                            fixture.turn,
                            point_limit()
                        )
                        .is_err()
                );
            });
            let store = publication.publish().unwrap();
            assert_eq!(
                storage
                    .terminal_history_evidence(&store, fixture.thread, fixture.turn, point_limit())
                    .unwrap(),
                evidence
            );
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
            let access = recovered.recovery_access().unwrap();
            assert!(matches!(
                storage.terminal_history_evidence_candidate(
                    &access,
                    fixture.thread,
                    fixture.turn,
                    point_limit()
                ),
                Err(SyndicReadError::Read(_))
            ));
            let fresh_evidence = fresh
                .terminal_history_evidence_candidate(
                    &access,
                    fixture.thread,
                    fixture.turn,
                    point_limit(),
                )
                .unwrap();
            assert_eq!(
                fresh_evidence.map(|e| (
                    e.home_id(),
                    e.source_revision(),
                    e.selected_path(),
                    e.lifecycle()
                )),
                evidence.map(|e| (
                    e.home_id(),
                    e.source_revision(),
                    e.selected_path(),
                    e.lifecycle()
                ))
            );
            if let (Some(fresh), Some(old)) = (fresh_evidence, evidence) {
                assert_ne!(fresh.home_generation(), old.home_generation());
            }
            let store = recovered.publish().unwrap();
            assert_eq!(
                fresh
                    .terminal_history_evidence(&store, fixture.thread, fixture.turn, point_limit())
                    .unwrap(),
                fresh_evidence
            );
            store.close().unwrap();
        }
    }
    foreign.store.close().unwrap();
}

#[test]
fn candidate_terminal_evidence_distinguishes_drift_missing_authority_and_unfinished_projection() {
    for case in 0..3 {
        let fixture = terminal_fixture(1010 + case, false, true);
        let revision = fixture.storage.revision(&fixture.store).unwrap();
        fixture.store.close().unwrap();
        let mut candidate = open(fixture.home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        let remove = || {
            let mut batch = FixtureBatch::new();
            batch
                .delete(if case == 1 {
                    FixtureDelete::TurnState(fixture.turn)
                } else {
                    FixtureDelete::TranscriptViewHead(fixture.thread)
                })
                .unwrap();
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command
                .add(storage.clone().fixture_contribution(revision, batch))
                .unwrap();
            assert!(matches!(
                access.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        };
        let result = if case == 0 {
            terminal_history_evidence_candidate_with_confirmation_hook(
                &storage,
                &access,
                fixture.thread,
                fixture.turn,
                point_limit(),
                remove,
            )
        } else {
            remove();
            storage.terminal_history_evidence_candidate(
                &access,
                fixture.thread,
                fixture.turn,
                point_limit(),
            )
        };
        match case {
            0 => assert!(
                matches!(result, Err(SyndicReadError::ConcurrentChange { .. })),
                "{result:?}"
            ),
            1 => assert!(
                matches!(result, Err(SyndicReadError::Invariant(_))),
                "{result:?}"
            ),
            _ => assert_eq!(result.unwrap(), None),
        }
        publication.close().unwrap();
    }
}
