use std::num::NonZeroU64;

use beryl_app::input_admission::{accepted_input_command, idle_submission_command};
use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore, SidecarByteLimit,
    SidecarNamespace,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AssetId, DraftRevision, InputGateRevision, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId,
    SyndicThreadId, ThreadRevision,
};
use beryl_state::{
    AssetMediaType, AssetReferenceOwner, BerylState, CreateAssetWithReference, UnixMillis,
};
use syndic_storage::{
    AcceptedInputAdmission, AdmissionMarkers, ComposerAtom, ComposerPayload, ContentAppend,
    ContentBuild, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, IdleSubmission,
    ImageLabelOrdinal, InputAdmissionStatus, PreparedContent, ResolvedImageMarker,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
};

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    syndic: SyndicStorage,
    state: BerylState,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
}

impl Fixture {
    fn new(name: u8) -> Self {
        Self::open(name, None)
    }

    fn with_faults(name: u8, faults: FaultController) -> Self {
        Self::open(name, Some(faults))
    }

    fn open(name: u8, faults: Option<FaultController>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let options = HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT);
        let mut store = match faults {
            Some(faults) => HomeStore::open_with_faults(options, faults).unwrap(),
            None => HomeStore::open(options).unwrap(),
        };
        let state = BerylState::register(&mut store).unwrap();
        let syndic = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([name; 16]);
        let draft = SyndicDraftId::from_bytes([name.wrapping_add(1); 16]);
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(syndic.create_thread(
                syndic.revision(&store).unwrap(),
                CreateThread::ordinary(thread, draft, time(1)),
            ))
            .unwrap();
        store.execute(command).unwrap();
        Self {
            _directory: directory,
            store,
            syndic,
            state,
            thread,
            draft,
        }
    }

    fn publish_marker(&self, marker_id: SyndicDraftMarkerId, updated_at: u64) {
        let payload = ComposerPayload::new(vec![ComposerAtom::image_marker(
            marker_id,
            ImageLabelOrdinal::FIRST,
        )])
        .unwrap();
        let content = PreparedContent::composer(&payload).unwrap();
        execute_one(
            &self.store,
            self.syndic.begin_content(
                self.syndic.revision(&self.store).unwrap(),
                ContentBuild::from_prepared(&content),
            ),
        );
        let mut manifest = content.building_manifest();
        while let Some(append) = ContentAppend::prepare(&manifest, &content).unwrap() {
            manifest = append.next_manifest().clone();
            execute_one(
                &self.store,
                self.syndic
                    .append_content(self.syndic.revision(&self.store).unwrap(), append),
            );
        }
        let current = self
            .syndic
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let update =
            match DraftPayloadUpdate::prepare(&current, &content, time(updated_at)).unwrap() {
                DraftPayloadUpdateDecision::Update(update) => update,
                DraftPayloadUpdateDecision::NoChange => panic!("marker payload must be new"),
            };
        execute_one(
            &self.store,
            self.syndic
                .update_draft_payload(self.syndic.revision(&self.store).unwrap(), update),
        );
    }
}

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn admit_asset(
    fixture: &mut Fixture,
    marker_id: SyndicDraftMarkerId,
    bytes: &[u8],
    owner_draft: SyndicDraftId,
) -> AssetId {
    let sidecar = fixture
        .store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap()),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = fixture.state.assets();
    let revision = assets.revision(&fixture.store).unwrap();
    let creation = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        AssetReferenceOwner::CurrentDraftMarker {
            draft_id: owner_draft,
            marker_id,
        },
        UnixMillis::new(2),
    )
    .unwrap();
    let first = assets
        .create_with_reference(revision, sidecar, creation)
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    fixture.store.execute(command).unwrap();
    asset_id
}

fn markers(marker_id: SyndicDraftMarkerId, asset_id: AssetId) -> AdmissionMarkers {
    AdmissionMarkers::new(vec![ResolvedImageMarker::new(
        marker_id,
        ImageLabelOrdinal::FIRST,
        asset_id,
    )])
    .unwrap()
}

#[test]
fn idle_and_non_idle_admission_move_asset_ownership_in_the_same_commit() {
    let mut fixture = Fixture::new(1);
    let first_marker = SyndicDraftMarkerId::from_bytes([10; 16]);
    fixture.publish_marker(first_marker, 2);
    let first_draft = fixture.draft;
    let first_asset = admit_asset(&mut fixture, first_marker, b"first image", first_draft);
    let first_content = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let first_item = SyndicItemId::from_bytes([11; 16]);
    let second_draft = SyndicDraftId::from_bytes([12; 16]);
    let submission = IdleSubmission::new(
        fixture.thread,
        ThreadRevision::new(1).unwrap(),
        fixture.draft,
        DraftRevision::new(2).unwrap(),
        first_content,
        InputGateRevision::new(1).unwrap(),
        second_draft,
        first_item,
        markers(first_marker, first_asset),
        time(3),
    );
    let command = idle_submission_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        submission,
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    let old_owner = AssetReferenceOwner::CurrentDraftMarker {
        draft_id: fixture.draft,
        marker_id: first_marker,
    };
    let submitted_owner = AssetReferenceOwner::SubmittedTurnItemMarker {
        item_id: first_item,
        marker_id: first_marker,
    };
    assert!(
        fixture
            .state
            .assets()
            .reference(&fixture.store, old_owner)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .state
            .assets()
            .reference(&fixture.store, submitted_owner)
            .unwrap()
            .unwrap()
            .asset_id(),
        first_asset
    );

    let second_marker = SyndicDraftMarkerId::from_bytes([13; 16]);
    fixture.publish_marker(second_marker, 4);
    let second_asset = admit_asset(&mut fixture, second_marker, b"second image", second_draft);
    let second_content = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
        ThreadRevision::new(2).unwrap(),
        second_draft,
        DraftRevision::new(2).unwrap(),
        second_content,
        InputGateRevision::new(2).unwrap(),
        SyndicDraftId::from_bytes([14; 16]),
        markers(second_marker, second_asset),
        time(5),
    );
    let input_id = admission.accepted_input_id();
    let command = accepted_input_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        admission,
    )
    .unwrap();
    fixture.store.execute(command).unwrap();

    let accepted_owner = AssetReferenceOwner::AcceptedInputMarker {
        input_id,
        marker_id: second_marker,
    };
    assert_eq!(
        fixture
            .state
            .assets()
            .reference(&fixture.store, accepted_owner)
            .unwrap()
            .unwrap()
            .asset_id(),
        second_asset
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn missing_draft_asset_owner_rejects_both_domains_without_consuming_the_draft() {
    let mut fixture = Fixture::new(30);
    let marker_id = SyndicDraftMarkerId::from_bytes([31; 16]);
    fixture.publish_marker(marker_id, 2);
    let wrong_owner_draft = SyndicDraftId::from_bytes([32; 16]);
    let asset_id = admit_asset(
        &mut fixture,
        marker_id,
        b"mismatched owner",
        wrong_owner_draft,
    );
    let item_id = SyndicItemId::from_bytes([33; 16]);
    let expected_content = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let submission = IdleSubmission::new(
        fixture.thread,
        ThreadRevision::new(1).unwrap(),
        fixture.draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(1).unwrap(),
        SyndicDraftId::from_bytes([34; 16]),
        item_id,
        markers(marker_id, asset_id),
        time(3),
    );
    let command = idle_submission_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        submission,
    )
    .unwrap();
    assert!(fixture.store.execute(command).is_err());

    let current = fixture
        .syndic
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), fixture.draft);
    assert_eq!(current.draft().revision().get(), 2);
    assert!(
        fixture
            .syndic
            .turn(
                &fixture.store,
                fixture.draft.submitted_turn_id(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .reference(
                &fixture.store,
                AssetReferenceOwner::SubmittedTurnItemMarker { item_id, marker_id },
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .reference(
                &fixture.store,
                AssetReferenceOwner::CurrentDraftMarker {
                    draft_id: wrong_owner_draft,
                    marker_id,
                },
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn persistence_cuts_keep_syndic_and_asset_ownership_on_the_same_side() {
    for (name, point, status, moved) in [
        (
            50,
            FaultPoint::BeforeCommit,
            InputAdmissionStatus::Absent,
            false,
        ),
        (
            60,
            FaultPoint::AfterPersist,
            InputAdmissionStatus::ExactSubmitted,
            true,
        ),
        (
            70,
            FaultPoint::AfterCommitBeforePersist,
            InputAdmissionStatus::ExactSubmitted,
            true,
        ),
    ] {
        let faults = FaultController::new();
        let mut fixture = Fixture::with_faults(name, faults.clone());
        let marker_id = SyndicDraftMarkerId::from_bytes([name.wrapping_add(2); 16]);
        fixture.publish_marker(marker_id, 2);
        let draft_id = fixture.draft;
        let asset_id = admit_asset(&mut fixture, marker_id, b"fault image", draft_id);
        let content = fixture
            .syndic
            .current_draft(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .content();
        let item_id = SyndicItemId::from_bytes([name.wrapping_add(3); 16]);
        let submission = IdleSubmission::new(
            fixture.thread,
            ThreadRevision::new(1).unwrap(),
            draft_id,
            DraftRevision::new(2).unwrap(),
            content,
            InputGateRevision::new(1).unwrap(),
            SyndicDraftId::from_bytes([name.wrapping_add(4); 16]),
            item_id,
            markers(marker_id, asset_id),
            time(3),
        );
        let command = idle_submission_command(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            submission.clone(),
        )
        .unwrap();
        faults.fail_next(point);
        assert!(fixture.store.execute(command).is_err());
        assert_eq!(fixture.store.health().state(), HomeHealthState::Verifying);
        fixture.store.verify_health().unwrap();
        assert_eq!(
            fixture
                .syndic
                .idle_submission_status(&fixture.store, &submission, point_limit())
                .unwrap(),
            status
        );
        let source = fixture
            .state
            .assets()
            .reference(
                &fixture.store,
                AssetReferenceOwner::CurrentDraftMarker {
                    draft_id,
                    marker_id,
                },
            )
            .unwrap();
        let target = fixture
            .state
            .assets()
            .reference(
                &fixture.store,
                AssetReferenceOwner::SubmittedTurnItemMarker { item_id, marker_id },
            )
            .unwrap();
        assert_eq!(source.is_none(), moved);
        assert_eq!(target.is_some(), moved);
        fixture.store.validate_registered_domains().unwrap();
    }
}
