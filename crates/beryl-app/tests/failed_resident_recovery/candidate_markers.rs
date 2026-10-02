use super::{composer, host, publication, support};
use beryl_app::{
    composer_host::{ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture},
    composer_marker_seal::{
        DraftMarkerSealAdmission, DraftMarkerSealDriveOutcome, DraftMarkerSealFlightRequest,
    },
};
use beryl_home_store::CommandCancellation;
use beryl_model::AssetReferenceSetId;
use beryl_state::{AssetOwner, AssetReferenceSetStagingAuthority, BerylState};
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1 as Evidence, DraftMarkerSealOperationIdV1,
    SyndicStorage, SyndicTimestamp,
};

#[test]
fn failed_marker_resident_prepares_fresh_and_resumed_evidence_before_exact_save() {
    for unfinished in [false, true] {
        let mut fixture = support::host("candidate-marker-resident-save", 141);
        let asset = publication::publish_image_asset(
            &fixture.store,
            fixture.assets.clone(),
            b"candidate-prepared actual resident asset",
        );
        let empty = fixture.host.binding().unwrap();
        let dirty = publication::insert_two_markers_with_readiness(
            &mut fixture.host,
            &fixture.store,
            &fixture.storage,
            empty,
            1301,
            [asset; 2],
        );
        if unfinished {
            let timer = fixture.host.autosave_timer().unwrap();
            let ticket = match fixture
                .host
                .fire_autosave(
                    &fixture.store,
                    timer,
                    fixture.assets.clone(),
                    &fixture.seals,
                    composer::operation_id(1302),
                    Some(publication::authority(142)),
                    SyndicTimestamp::from_unix_millis(1302),
                    &CommandCancellation::new(),
                )
                .unwrap()
            {
                ComposerHostAutosaveCapture::Captured(ticket) => ticket,
                other => panic!("original marker autosave was not captured: {other:?}"),
            };
            assert_eq!(
                fixture
                    .host
                    .advance_autosave(&fixture.store, ticket)
                    .unwrap(),
                ComposerHostAutosaveAdvance::Progress
            );
            assert_eq!(fixture.seals.diagnostics().current_flights(), 1);
        }
        host::fail(&fixture);
        if unfinished {
            let foreign = support::host("foreign-marker-resident-custody", 161);
            host::fail(&foreign);
            let foreign_custody = foreign.seals.capture_failed_home(&foreign.store).unwrap();
            fixture.host = *Box::new(fixture.host)
                .retire_failed_resident_with_marker_custody(&fixture.store, &foreign_custody)
                .err()
                .expect("foreign capture cannot release original host marker custody");
            assert_eq!(fixture.host.binding(), Some(dirty));
        }
        let mut captured = fixture.seals.capture_failed_home(&fixture.store).unwrap();
        let retained = Box::new(fixture.host)
            .retire_failed_resident_with_marker_custody(&fixture.store, &captured)
            .ok()
            .expect("exact captured marker custody permits host retirement");
        let mut retained = retained;
        let mut candidate = fixture.store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        captured
            .bind_candidate(&mut candidate, storage.clone(), state.assets())
            .unwrap();
        assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
        let original = captured.captured_flights().next();
        assert_eq!(original.is_some(), unfinished);
        let flight = if let Some(flight) = original {
            flight
        } else {
            let request = DraftMarkerSealFlightRequest::new(
                retained
                    .qualified_publication_checkpoint(&mut candidate, &storage)
                    .unwrap(),
                DraftMarkerSealOperationIdV1::from_bytes([143; 16]),
                AssetReferenceSetStagingAuthority::new(
                    AssetReferenceSetId::from_bytes([144; 16]),
                    [145; 32],
                ),
            );
            match captured
                .admit_candidate(&mut candidate, request, &CommandCancellation::new())
                .unwrap()
            {
                DraftMarkerSealAdmission::Admitted(flight) => flight,
                other => panic!("fresh candidate marker flight was not admitted: {other:?}"),
            }
        };
        let evidence = loop {
            match captured.drive_candidate(&mut candidate, flight).unwrap() {
                DraftMarkerSealDriveOutcome::Progress => {}
                DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets } => {
                    break Evidence::ChangedNonempty {
                        seal_proof: syndic,
                        asset_proof: assets,
                    };
                }
                other => panic!("candidate marker preparation did not complete: {other:?}"),
            }
        };
        host::save(
            &mut retained,
            &mut candidate,
            &storage,
            &state,
            evidence,
            1303,
        );
        let restored = retained
            .reconstruct(&mut candidate, storage.clone())
            .unwrap();
        let binding = restored.binding().unwrap();
        assert_eq!(
            binding.candidate().session_id(),
            dirty.candidate().session_id()
        );
        assert_eq!(binding.root(), dirty.root());
        support::assert_history_preserved(binding.history(), dirty.history());
        let Evidence::ChangedNonempty { asset_proof, .. } = evidence else {
            unreachable!()
        };
        let access = candidate.recovery_access().unwrap();
        let head = state
            .assets()
            .owner_head_candidate(
                &access,
                AssetOwner::CurrentDraft(dirty.candidate().draft_id()),
            )
            .unwrap()
            .unwrap();
        assert_eq!(head.set(), asset_proof);
        assert!(
            state
                .assets()
                .sealed_reference_set_manifest_candidate(&access, asset_proof)
                .is_ok()
        );
        drop(restored);
        drop(captured);
        candidate.publish().unwrap().close().unwrap();
    }
}

#[test]
fn captured_zero_flight_collision_retains_authority_without_save_eligibility() {
    let mut fixture = support::host("candidate-marker-collision-custody", 151);
    let asset = publication::publish_image_asset(
        &fixture.store,
        fixture.assets.clone(),
        b"collision-staged actual resident asset",
    );
    let empty = fixture.host.binding().unwrap();
    let dirty = publication::insert_two_markers_with_readiness(
        &mut fixture.host,
        &fixture.store,
        &fixture.storage,
        empty,
        1311,
        [asset; 2],
    );
    let timer = fixture.host.autosave_timer().unwrap();
    let ticket = match fixture
        .host
        .fire_autosave(
            &fixture.store,
            timer,
            fixture.assets.clone(),
            &fixture.seals,
            composer::operation_id(1312),
            Some(publication::authority(152)),
            SyndicTimestamp::from_unix_millis(1312),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("original marker autosave was not captured: {other:?}"),
    };
    assert_eq!(
        fixture
            .host
            .advance_autosave(&fixture.store, ticket)
            .unwrap(),
        ComposerHostAutosaveAdvance::Progress
    );
    fixture.seals.test_fail_next_drive_as_collision();
    assert!(matches!(
        fixture
            .host
            .advance_autosave(&fixture.store, ticket)
            .unwrap(),
        ComposerHostAutosaveAdvance::Unsatisfied(_)
    ));
    assert_eq!(fixture.seals.diagnostics().current_flights(), 0);
    host::fail(&fixture);
    let foreign = support::host("foreign-terminal-marker-custody", 171);
    host::fail(&foreign);
    let foreign_custody = foreign.seals.capture_failed_home(&foreign.store).unwrap();
    fixture.host = *Box::new(fixture.host)
        .retire_failed_resident_with_marker_custody(&fixture.store, &foreign_custody)
        .err()
        .expect("foreign capture cannot release terminal staging authority");
    assert_eq!(fixture.host.binding(), Some(dirty));
    let mut captured = fixture.seals.capture_failed_home(&fixture.store).unwrap();
    let flight = captured
        .captured_flights()
        .next()
        .expect("zero live-flight count still retains orphan authority");
    let mut retained = Box::new(fixture.host)
        .retire_failed_resident_with_marker_custody(&fixture.store, &captured)
        .ok()
        .expect("exact orphan capture permits retirement without discarding outcome");
    assert_eq!(retained.checkpoint(), dirty.candidate());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    captured
        .bind_candidate(&mut candidate, storage.clone(), state.assets())
        .unwrap();
    let before = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(captured.drive_candidate(&mut candidate, flight).is_err());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        before
    );
    assert_eq!(retained.recovery_known_commit(), None);
    assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
    assert_eq!(retained.checkpoint(), dirty.candidate());
}
