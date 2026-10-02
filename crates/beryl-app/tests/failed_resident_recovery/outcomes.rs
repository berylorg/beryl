use super::{composer, support};
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};
use beryl_state::BerylState;
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1 as Evidence, SyndicStorage, SyndicTimestamp,
};

#[test]
fn recovery_save_outcomes_survive_fresh_graph_and_only_proven_noncommit_can_retry() {
    for cut in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let mut fixture = support::host("failed-recovery-save-outcome", 141);
        let empty = fixture.host.binding().unwrap();
        let dirty = composer::commit_text(
            &mut fixture.host,
            &fixture.store,
            empty,
            601,
            0,
            0,
            "outcome custody",
            15,
            1,
        );
        let faults = fixture.faults.clone();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let support::Host {
            host,
            store,
            seals,
            directory,
            ..
        } = fixture;
        let mut retained = Box::new(host).retire_failed_resident(&store).ok().unwrap();
        drop(seals);
        let mut candidate = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        faults.fail_next(cut);
        retained
            .publish_candidate(
                &mut candidate,
                &storage,
                &state.assets(),
                composer::operation_id(602),
                SyndicTimestamp::from_unix_millis(602),
                Evidence::UnchangedEmpty,
                CommandCancellation::new(),
            )
            .unwrap();
        assert!(matches!(
            candidate.service_reference().health().state(),
            HomeHealthState::Reopening | HomeHealthState::Failed
        ));
        assert_eq!(retained.checkpoint(), dirty.candidate());
        let mut fresh = candidate.abort().recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&fresh).unwrap();
        let state = BerylState::reacquire_candidate(&fresh).unwrap();
        let committed = cut != FaultPoint::BeforeCommit;
        assert_eq!(
            retained.qualify_saved(&mut fresh, &storage).unwrap(),
            committed
        );
        assert_eq!(retained.recovery_known_commit(), Some(committed));
        if committed {
            assert!(retained.retry_publication(&mut fresh).is_err());
        } else {
            retained.retry_publication(&mut fresh).unwrap();
            retained
                .publish_candidate(
                    &mut fresh,
                    &storage,
                    &state.assets(),
                    composer::operation_id(603),
                    SyndicTimestamp::from_unix_millis(603),
                    Evidence::UnchangedEmpty,
                    CommandCancellation::new(),
                )
                .unwrap();
        }
        assert!(retained.qualify_saved(&mut fresh, &storage).unwrap());
        let host = retained.reconstruct(&mut fresh, storage.clone()).unwrap();
        assert_eq!(host.binding().unwrap().root(), dirty.root());
        drop(host);
        fresh.publish().unwrap().close().unwrap();
        drop(directory);
    }
}

#[test]
fn failed_retirement_refuses_healthy_home_and_undrained_mutation_without_losing_host() {
    let mut fixture = support::host("failed-retirement-admission", 151);
    let binding = fixture.host.binding().unwrap();
    fixture.host = *Box::new(fixture.host)
        .retire_failed_resident(&fixture.store)
        .err()
        .expect("healthy host cannot be labelled failed retirement");
    assert_eq!(fixture.host.binding(), Some(binding));
    composer::begin_text(&mut fixture.host, &fixture.store, binding, 701, 0).unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    fixture.host = *Box::new(fixture.host)
        .retire_failed_resident(&fixture.store)
        .err()
        .expect("actual unfinished mutation must preserve old host custody");
    assert_eq!(fixture.host.binding(), Some(binding));
}

#[test]
fn another_canonical_home_candidate_cannot_qualify_or_write_retained_candidate() {
    let fixture = support::host("failed-exact-home-source", 161);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut retained = Box::new(fixture.host)
        .retire_failed_resident(&fixture.store)
        .ok()
        .unwrap();
    let other = support::host("failed-other-home-source", 171);
    other.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(other.store.home_revision().is_err());
    let support::Host {
        host,
        store,
        seals,
        directory,
        ..
    } = other;
    drop((host, seals));
    let mut candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retained.qualify_saved(&mut candidate, &storage).is_err());
    assert!(
        retained
            .publish_candidate(
                &mut candidate,
                &storage,
                &state.assets(),
                composer::operation_id(702),
                SyndicTimestamp::from_unix_millis(702),
                Evidence::UnchangedEmpty,
                CommandCancellation::new()
            )
            .is_err()
    );
    assert_eq!(retained.recovery_known_commit(), None);
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    candidate.publish().unwrap().close().unwrap();
    drop(directory);
}
