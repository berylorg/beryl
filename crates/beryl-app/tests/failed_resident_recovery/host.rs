use super::{composer, publication, support};
use beryl_app::composer_host::ComposerHostFailedResident;
use beryl_app::composer_marker_seal::{
    DraftMarkerSealAdmission, DraftMarkerSealDriveOutcome, DraftMarkerSealFlightRequest,
};
use beryl_home_store::{
    CommandCancellation, HomeHealthState, HomeRecoveryCandidate, test_faults::FaultPoint,
};
use beryl_state::{AssetOwner, BerylState};
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1 as Evidence, SyndicStorage, SyndicTimestamp,
};

pub(super) fn fail(fixture: &support::Host) {
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
}

pub(super) fn retire(
    fixture: support::Host,
) -> (
    tempfile::TempDir,
    HomeRecoveryCandidate,
    SyndicStorage,
    BerylState,
    ComposerHostFailedResident,
) {
    let support::Host {
        host,
        store,
        seals,
        directory,
        ..
    } = fixture;
    let retained = Box::new(host)
        .retire_failed_resident(&store)
        .ok()
        .expect("drained failed host retains its actual candidate");
    drop(seals);
    let candidate = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    (directory, candidate, storage, state, retained)
}

pub(super) fn save(
    retained: &mut ComposerHostFailedResident,
    candidate: &mut HomeRecoveryCandidate,
    storage: &SyndicStorage,
    state: &BerylState,
    evidence: Evidence,
    operation: u64,
) {
    retained
        .publish_candidate(
            candidate,
            storage,
            &state.assets(),
            composer::operation_id(operation),
            SyndicTimestamp::from_unix_millis(operation),
            evidence,
            CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(retained.recovery_known_commit(), Some(true));
    assert!(retained.qualify_saved(candidate, storage).unwrap());
}

#[test]
fn saved_failed_host_reconstruction_has_no_write_and_retains_same_session() {
    let fixture = support::host("failed-saved-resident", 51);
    let old = fixture.host.binding().unwrap();
    fail(&fixture);
    let (_directory, mut candidate, storage, _state, mut retained) = retire(fixture);
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(retained.qualify_saved(&mut candidate, &storage).unwrap());
    let host = retained
        .reconstruct(&mut candidate, storage.clone())
        .unwrap();
    let fresh = host.binding().unwrap();
    assert_eq!(fresh.candidate(), old.candidate());
    assert_ne!(fresh.home_generation(), old.home_generation());
    assert_ne!(fresh.host_generation(), old.host_generation());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    assert_eq!(retained.original_known_commit(), None);
    assert_eq!(retained.recovery_known_commit(), None);
    drop(host);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn unsaved_failed_host_saves_exact_candidate_and_preserves_undo_redo() {
    let mut fixture = support::host("failed-unsaved-resident", 61);
    let old = fixture.host.binding().unwrap();
    let dirty = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        old,
        101,
        0,
        0,
        "é日 retained",
        14,
        1,
    );
    assert!(fixture.host.is_dirty());
    fail(&fixture);
    let (_directory, mut candidate, storage, state, mut retained) = retire(fixture);
    assert_eq!(retained.checkpoint(), dirty.candidate());
    assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
    save(
        &mut retained,
        &mut candidate,
        &storage,
        &state,
        Evidence::UnchangedEmpty,
        102,
    );
    let mut host = retained
        .reconstruct(&mut candidate, storage.clone())
        .unwrap();
    let fresh = host.binding().unwrap();
    assert_eq!(fresh.root(), dirty.root());
    assert_eq!(
        fresh.candidate().session_id(),
        dirty.candidate().session_id()
    );
    support::assert_history_preserved(fresh.history(), dirty.history());
    let store = candidate.publish().unwrap();
    assert_eq!(
        composer::candidate_text(storage.clone(), &store, fresh),
        "é日 retained".as_bytes()
    );
    let undone = composer::select_history(
        &mut host,
        &store,
        fresh,
        103,
        gpui_text_input::MutationKind::Undo,
    );
    assert!(composer::candidate_text(storage.clone(), &store, undone).is_empty());
    let redone = composer::select_history(
        &mut host,
        &store,
        undone,
        104,
        gpui_text_input::MutationKind::Redo,
    );
    assert_eq!(
        composer::candidate_text(storage, &store, redone),
        "é日 retained".as_bytes()
    );
    drop(host);
    store.close().unwrap();
}

#[test]
fn retained_marker_candidate_save_uses_real_sealed_asset_and_syndic_evidence() {
    for wrong_evidence in [false, true] {
        let mut fixture = support::host("failed-marker-resident", 81);
        let asset = publication::publish_image_asset(
            &fixture.store,
            fixture.assets.clone(),
            b"retained real asset",
        );
        let empty = fixture.host.binding().unwrap();
        let dirty = publication::insert_two_markers_with_readiness(
            &mut fixture.host,
            &fixture.store,
            &fixture.storage,
            empty,
            301,
            [asset; 2],
        );
        let request = DraftMarkerSealFlightRequest::new(
            dirty.candidate(),
            syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([85; 16]),
            beryl_state::AssetReferenceSetStagingAuthority::new(
                beryl_model::AssetReferenceSetId::from_bytes([86; 16]),
                [87; 32],
            ),
        );
        let flight = match fixture
            .seals
            .admit(&fixture.store, request, &CommandCancellation::new())
            .unwrap()
        {
            DraftMarkerSealAdmission::Admitted(flight)
            | DraftMarkerSealAdmission::Coalesced(flight) => flight,
            other => panic!("marker proof preparation was not admitted: {other:?}"),
        };
        let evidence = loop {
            match fixture.seals.drive(&fixture.store, flight).unwrap() {
                DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets } => {
                    break Evidence::ChangedNonempty {
                        seal_proof: syndic,
                        asset_proof: assets,
                    };
                }
                DraftMarkerSealDriveOutcome::Progress => {}
                other => panic!("unexpected marker proof outcome: {other:?}"),
            }
        };
        fail(&fixture);
        let (_directory, mut candidate, storage, state, mut retained) = retire(fixture);
        assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
        if wrong_evidence {
            let revision = candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap();
            assert!(
                retained
                    .publish_candidate(
                        &mut candidate,
                        &storage,
                        &state.assets(),
                        composer::operation_id(302),
                        SyndicTimestamp::from_unix_millis(302),
                        Evidence::UnchangedEmpty,
                        CommandCancellation::new()
                    )
                    .is_err()
            );
            assert_eq!(retained.checkpoint(), dirty.candidate());
            assert_eq!(retained.recovery_known_commit(), None);
            assert_eq!(
                candidate
                    .recovery_access()
                    .unwrap()
                    .home_revision()
                    .unwrap(),
                revision
            );
            assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
        } else {
            save(
                &mut retained,
                &mut candidate,
                &storage,
                &state,
                evidence,
                302,
            );
            let host = retained
                .reconstruct(&mut candidate, storage.clone())
                .unwrap();
            let access = candidate.recovery_access().unwrap();
            let head = state
                .assets()
                .owner_head_candidate(
                    &access,
                    AssetOwner::CurrentDraft(dirty.candidate().draft_id()),
                )
                .unwrap()
                .unwrap();
            let Evidence::ChangedNonempty { asset_proof, .. } = evidence else {
                unreachable!()
            };
            assert_eq!(head.set(), asset_proof);
            assert!(
                state
                    .assets()
                    .sealed_reference_set_manifest_candidate(&access, asset_proof)
                    .is_ok()
            );
            drop(host);
        }
        candidate.publish().unwrap().close().unwrap();
    }
}

#[test]
fn cancelled_before_preparation_preserves_unsaved_candidate_without_publication() {
    let mut fixture = support::host("failed-cancelled-resident", 91);
    let empty = fixture.host.binding().unwrap();
    let dirty = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        empty,
        401,
        0,
        0,
        "cancelled",
        9,
        1,
    );
    fail(&fixture);
    let (_directory, mut candidate, storage, state, mut retained) = retire(fixture);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(
        retained
            .publish_candidate(
                &mut candidate,
                &storage,
                &state.assets(),
                composer::operation_id(402),
                SyndicTimestamp::from_unix_millis(402),
                Evidence::UnchangedEmpty,
                cancellation
            )
            .is_err()
    );
    assert_eq!(retained.checkpoint(), dirty.candidate());
    assert_eq!(retained.recovery_known_commit(), None);
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    save(
        &mut retained,
        &mut candidate,
        &storage,
        &state,
        Evidence::UnchangedEmpty,
        403,
    );
    candidate.publish().unwrap().close().unwrap();
}
