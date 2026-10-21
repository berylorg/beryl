use super::*;

fn publish_recovered(
    candidate: beryl_home_store::HomeRecoveryCandidate,
    path: std::path::PathBuf,
    faults: beryl_home_store::test_faults::FaultController,
) -> Home {
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let peer = candidate.domain_handle::<Peer>().unwrap();
    let store = candidate.publish().unwrap();
    Home {
        path,
        store,
        state,
        peer,
        faults,
    }
}

#[test]
fn a_preparation_racing_absence_conflicts_instead_of_downgrading() {
    let home = open("raced-absence");
    let absence = home.state.assets().validate_generated_media_absence(
        home.revision(),
        target(),
        GeneratedMediaUnavailableReason::LostSourceAuthority,
    );
    home.prepared(target());
    let revision = home.revision();
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command.add_validation(absence).unwrap();
    command
        .add(home.peer.contribution(
            home.peer_revision(),
            PeerWrite {
                pair: Some(pair::<3>(
                    CommandParticipantKind::Validation,
                    unavailable_digest(
                        target(),
                        GeneratedMediaUnavailableReason::LostSourceAuthority,
                    ),
                )),
                value: 3,
            },
        ))
        .unwrap();
    assert!(matches!(
        rejected(home.store.execute(command)),
        CommandError::Conflict { .. }
    ));
    assert_eq!(home.revision(), revision);
    assert_eq!(home.peer_value(), Some(1));
    assert_eq!(
        home.admission(target()).lifecycle(),
        GeneratedMediaLifecycle::Prepared
    );
    home.store.close().unwrap();
}

#[test]
fn a_prepared_publication_cannot_ignore_a_later_asset_revision() {
    let home = open("stale-publication");
    let admission = home.prepared(target());
    let contribution = home
        .state
        .assets()
        .publish_generated_media(&home.store, home.revision(), &admission)
        .unwrap();
    let other = GeneratedMediaTarget::new(
        SyndicResourceId::from_bytes([11; 16]),
        target().thread(),
        target().turn(),
        target().item(),
        target().fingerprint(),
    );
    home.prepared(other);
    let mut command = HomeCommand::new(home.store.home_revision().unwrap());
    command.add(contribution).unwrap();
    command
        .add(home.peer.contribution(
            home.peer_revision(),
            PeerWrite {
                pair: Some(pair::<2>(
                    CommandParticipantKind::Mutation,
                    asset_digest(2, target(), admission.facts()),
                )),
                value: 2,
            },
        ))
        .unwrap();
    assert!(matches!(
        rejected(home.store.execute(command)),
        CommandError::Conflict { .. }
    ));
    assert_eq!(
        home.admission(target()).lifecycle(),
        GeneratedMediaLifecycle::Prepared
    );
    assert_eq!(home.peer_value(), Some(1));
    home.store.close().unwrap();
}

#[test]
fn a_before_commit_failure_keeps_metadata_and_prepared_evidence_absent() {
    let home = open("prepare-before-commit");
    let revision = home.revision();
    let command = home.preparation_command(target());
    home.faults.fail_next(FaultPoint::BeforeCommit);
    rejected(home.store.execute(command));
    let candidate = home.store.recover_same_home().unwrap();
    let home = publish_recovered(candidate, home.path, home.faults);
    assert_eq!(home.revision(), revision);
    assert!(
        home.state
            .assets()
            .generated_media_admission(&home.store, target())
            .unwrap()
            .is_none()
    );
    let token = sidecar(&home.store, BYTES, "images");
    let asset = facts(&token, None).asset();
    assert!(
        home.state
            .assets()
            .metadata(&home.store, asset)
            .unwrap()
            .is_none()
    );
    home.store.close().unwrap();
}

#[test]
fn preparation_and_publication_acknowledgement_loss_reconcile_the_original_custody() {
    for publishing in [false, true] {
        let home = open("acknowledgement-loss");
        let command = if publishing {
            let admission = home.prepared(target());
            home.publication_command(&admission)
        } else {
            home.preparation_command(target())
        };
        let old_revision = home.revision();
        let old_home_revision = home.store.home_revision().unwrap();
        let original_generation = home.store.health().generation().unwrap();
        home.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let handle = match home.store.execute(command) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected indeterminate: {other:?}"),
        };
        assert_eq!(home.store.pending_reconciliations().len(), 1);
        home.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(home.store.home_revision().is_err());
        let mut candidate = home.store.recover_same_home().unwrap();
        let receipt = match candidate
            .recovery_access()
            .unwrap()
            .reconcile(&handle)
            .unwrap()
        {
            ReconciliationResolution::ExactNew { receipt } => receipt,
            other => panic!("expected exact new: {other:?}"),
        };
        assert_eq!(
            receipt.home_revision(),
            old_home_revision.checked_next().unwrap()
        );
        assert_eq!(receipt.generation(), original_generation);
        let home = publish_recovered(candidate, home.path, home.faults);
        assert_eq!(home.revision(), old_revision.checked_next().unwrap());
        assert!(matches!(
            home.state
                .assets()
                .committed_revision(&home.store, &receipt),
            Err(beryl_home_store::CommitReceiptError::StaleOrForeign { .. })
        ));
        assert!(home.store.pending_reconciliations().is_empty());
        assert_eq!(
            home.admission(target()).lifecycle(),
            if publishing {
                GeneratedMediaLifecycle::Published
            } else {
                GeneratedMediaLifecycle::Prepared
            }
        );
        assert_eq!(home.peer_value(), Some(if publishing { 2 } else { 1 }));
        home.store.close().unwrap();
    }
}

#[test]
fn persisted_preparation_is_committed_even_when_later_acknowledgement_fails() {
    let home = open("persisted-preparation");
    let command = home.preparation_command(target());
    let before = home.store.home_revision().unwrap();
    let original_generation = home.store.health().generation().unwrap();
    home.faults.fail_next(FaultPoint::AfterPersist);
    let receipt = match home.store.execute(command) {
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(_),
            ..
        } => receipt,
        other => panic!("expected durable commit with later failure: {other:?}"),
    };
    assert_eq!(receipt.home_revision(), before.checked_next().unwrap());
    assert_eq!(receipt.generation(), original_generation);
    let candidate = home.store.recover_same_home().unwrap();
    let home = publish_recovered(candidate, home.path, home.faults);
    assert_eq!(home.store.home_revision().unwrap(), receipt.home_revision());
    assert!(matches!(
        home.state
            .assets()
            .committed_revision(&home.store, &receipt),
        Err(beryl_home_store::CommitReceiptError::StaleOrForeign { .. })
    ));
    assert_eq!(
        home.admission(target()).lifecycle(),
        GeneratedMediaLifecycle::Prepared
    );
    home.store.close().unwrap();
}

#[test]
fn reconciliation_binds_unchanged_metadata_as_well_as_the_changed_admission() {
    let home = open("reconciliation-metadata-collision");
    let admission = home.prepared(target());
    let command = home.publication_command(&admission);
    home.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match home.store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate: {other:?}"),
    };
    home.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.store.home_revision().is_err());
    let mut candidate = home.store.recover_same_home().unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let assets = state.assets();
    let mutation = assets.remove_metadata_for_test(
        assets.revision_candidate(&access).unwrap(),
        admission.facts().asset(),
    );
    let mut corruption = HomeCommand::new(access.home_revision().unwrap());
    corruption.add(mutation).unwrap();
    committed(access.execute(corruption));
    assert_eq!(
        access.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    drop(candidate);
}

#[test]
fn a_fresh_admission_read_rejects_missing_committed_metadata() {
    let home = open("read-metadata-corruption");
    let admission = home.prepared(target());
    let mutation = home
        .state
        .assets()
        .remove_metadata_for_test(home.revision(), admission.facts().asset());
    let mut corruption = HomeCommand::new(home.store.home_revision().unwrap());
    corruption.add(mutation).unwrap();
    committed(home.store.execute(corruption));
    assert!(matches!(
        home.state
            .assets()
            .generated_media_admission(&home.store, target()),
        Err(GeneratedMediaError::Rejected)
    ));
    home.store.close().unwrap();
}
