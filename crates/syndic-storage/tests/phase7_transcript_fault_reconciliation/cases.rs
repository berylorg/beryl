use super::*;

#[test]
fn final_transcript_publication_cuts_reconcile_as_one_atomic_state() {
    for (name, point, expected) in [
        (
            "phase7-transcript-final-before-commit",
            FaultPoint::BeforeCommit,
            ExpectedState::Unpublished,
        ),
        (
            "phase7-transcript-final-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            ExpectedState::Either,
        ),
        (
            "phase7-transcript-final-after-persist",
            FaultPoint::AfterPersist,
            ExpectedState::Published,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let target = prepare_final_publication(&store, storage);
        let unpublished = observe(&store, storage, &target);
        assert!(unpublished.entries.is_empty() && unpublished.build.entry_count() == 0);
        assert!(
            unpublished.head.entry_count() == 0
                && unpublished.head.lifecycle() == ProjectionLifecycle::Stale
        );
        assert!(!unpublished.summary.complete());
        assert!(unpublished.build.history_complete());
        let published = expected_published(&unpublished, &target);

        let contribution = storage.advance_transcript_build(
            storage.revision(&store).unwrap(),
            AdvanceTranscriptBuild::new(
                target.thread,
                target.generation,
                unpublished.build.revision(),
            ),
        );
        let command = command(&store, contribution);
        faults.fail_next(point);
        match (point, store.execute(command)) {
            (
                FaultPoint::BeforeCommit,
                beryl_home_store::CommandOutcome::NotCommitted {
                    evidence: CommandError::Commit { .. },
                },
            )
            | (
                FaultPoint::AfterPersist,
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => {}
            (
                FaultPoint::AfterCommitBeforePersist,
                outcome @ beryl_home_store::CommandOutcome::Indeterminate {
                    failure: CommandError::Persistence { .. },
                    ..
                },
            ) => assert!(format!("{outcome:?}").contains("Indeterminate")),
            (_, outcome) => panic!("unexpected transcript fault outcome: {outcome:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Verifying);

        store.verify_health().unwrap();
        let recovered = observe(&store, storage, &target);
        assert_state(&recovered, &unpublished, &published, expected);
        store.validate_registered_domains().unwrap();
        store.close().unwrap();

        let mut reopened = open(home.path());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        let durable = observe(&reopened, reopened_storage, &target);
        assert_eq!(durable, recovered);
        assert_state(&durable, &unpublished, &published, expected);
        reopened.validate_registered_domains().unwrap();
        reopened.close().unwrap();
    }
}
