use beryl_app::cas_projection::{
    ProjectionConnectionService, ProjectionServiceConfig, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_app::input_admission::{
    InputAdmissionBuildError, idle_submission_command, prepare_accepted_input_admission,
};
use beryl_home_store::{CommandError, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
#[cfg(feature = "test-faults")]
use beryl_home_store::{
    HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    DraftRevision, ExecutionBinding, InputGateRevision, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
    ThreadRevision,
};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, BerylState, UpdateAssetOwnerHeads};
use syndic_storage::{
    AcceptedInputAdmission, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild,
    CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, IdleSubmission,
    ImageLabelOrdinal, InputAdmissionStatus, PreparedContent, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

#[path = "phase5_input_admission/assets.rs"]
mod assets;
#[path = "phase5_input_admission/historical_labels.rs"]
mod historical_labels;
#[path = "phase5_input_admission/marker_free.rs"]
mod marker_free;

use assets::{admit_asset, admit_asset_at_label};

struct UnavailableScheduledOrdinaryProvider;

impl ScheduledOrdinaryExecutionProvider for UnavailableScheduledOrdinaryProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: ProjectionConnectionService,
    syndic: SyndicStorage,
    state: BerylState,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
}

impl Fixture {
    fn new(name: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(name, directory, store)
    }

    #[cfg(feature = "test-faults")]
    fn with_faults(name: u8, faults: FaultController) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(name, directory, store)
    }

    fn from_store(name: u8, directory: tempfile::TempDir, mut store: HomeStore) -> Self {
        let state = BerylState::register(&mut store).unwrap();
        let syndic = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([name; 16]);
        let draft = SyndicDraftId::from_bytes([name.wrapping_add(1); 16]);
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(syndic.create_thread(
                syndic.revision(&store).unwrap(),
                CreateThread::ordinary(thread, draft, execution_binding(name), time(1)),
            ))
            .unwrap();
        match store.execute(command) {
            CommandOutcome::Committed { later_failure: None, .. } => {}
            outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed thread setup, got {outcome:?}"),
            outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed thread setup, got {outcome:?}"),
        }
        let store = ProjectionConnectionService::new(
            store,
            syndic,
            ProjectionServiceConfig::try_new(8, 4).unwrap(),
            Box::new(UnavailableScheduledOrdinaryProvider),
        )
        .unwrap();
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
        self.publish_marker_at(marker_id, ImageLabelOrdinal::FIRST, updated_at);
    }

    fn publish_marker_at(
        &self,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        updated_at: u64,
    ) {
        let payload =
            ComposerPayload::new(vec![ComposerAtom::image_marker(marker_id, label)]).unwrap();
        self.publish_payload(&payload, updated_at);
    }

    fn publish_marker_free(&self, updated_at: u64) {
        self.publish_text("marker free", updated_at);
    }

    fn publish_text(&self, text: &str, updated_at: u64) {
        let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
        self.publish_payload(&payload, updated_at);
    }

    fn publish_payload(&self, payload: &ComposerPayload, updated_at: u64) {
        let content = PreparedContent::composer(payload).unwrap();
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

fn execution_binding(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.wrapping_add(2); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-phase5",
        )
        .unwrap(),
    )
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed contribution, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed contribution, got {outcome:?}"),
    }
}

#[test]
fn idle_and_non_idle_admission_move_asset_ownership_in_the_same_commit() {
    let mut fixture = Fixture::new(1);
    let first_marker = SyndicDraftMarkerId::from_bytes([10; 16]);
    fixture.publish_marker(first_marker, 2);
    let first_draft = fixture.draft;
    let (_first_asset, first_set) =
        admit_asset(&mut fixture, first_marker, b"first image", first_draft, 20);
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
        Some(first_set),
        time(3),
    );
    let command = idle_submission_command(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        submission,
    )
    .unwrap();
    match fixture.store.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed submission, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed submission, got {outcome:?}"),
    }

    let old_owner = AssetOwner::CurrentDraft(fixture.draft);
    let submitted_owner = AssetOwner::SubmittedTurnItem(first_item);
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, old_owner)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, submitted_owner)
            .unwrap()
            .unwrap()
            .set(),
        first_set
    );

    let second_marker = SyndicDraftMarkerId::from_bytes([13; 16]);
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    fixture.publish_marker_at(second_marker, second_label, 4);
    let (_second_asset, second_set) = admit_asset_at_label(
        &mut fixture,
        second_marker,
        second_label,
        b"second image",
        second_draft,
        21,
    );
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
        Some(second_set),
        time(5),
    );
    let input_id = admission.accepted_input_id();
    let prepared = prepare_accepted_input_admission(
        &fixture.store,
        fixture.syndic,
        fixture.state.assets(),
        admission,
    )
    .unwrap();
    fixture
        .store
        .execute_accepted_input_admission(prepared)
        .unwrap();

    let accepted_owner = AssetOwner::AcceptedInput(input_id);
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, accepted_owner)
            .unwrap()
            .unwrap()
            .set(),
        second_set
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn missing_draft_asset_owner_rejects_both_domains_without_consuming_the_draft() {
    let mut fixture = Fixture::new(30);
    let marker_id = SyndicDraftMarkerId::from_bytes([31; 16]);
    fixture.publish_marker(marker_id, 2);
    let wrong_owner_draft = SyndicDraftId::from_bytes([32; 16]);
    let (_asset_id, proof) = admit_asset(
        &mut fixture,
        marker_id,
        b"mismatched owner",
        wrong_owner_draft,
        40,
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
        Some(proof),
        time(3),
    );
    assert!(matches!(
        idle_submission_command(
            &fixture.store,
            fixture.syndic,
            fixture.state.assets(),
            submission,
        ),
        Err(InputAdmissionBuildError::MissingOwnerHead(owner))
            if owner == AssetOwner::CurrentDraft(fixture.draft)
    ));

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
            .owner_head(&fixture.store, AssetOwner::SubmittedTurnItem(item_id))
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::CurrentDraft(wrong_owner_draft))
            .unwrap()
            .is_some()
    );
}

#[cfg(feature = "test-faults")]
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
        let (_asset_id, proof) =
            admit_asset(&mut fixture, marker_id, b"fault image", draft_id, name);
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
            Some(proof),
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
        match fixture.store.execute(command) {
            CommandOutcome::NotCommitted {
                evidence: CommandError::Commit { .. },
            } if point == FaultPoint::BeforeCommit => {}
            CommandOutcome::Committed {
                later_failure: Some(CommandError::Persistence { .. }),
                ..
            } if point == FaultPoint::AfterPersist => {}
            CommandOutcome::Indeterminate {
                failure: CommandError::Persistence { .. },
                reconciliation: _,
            } if point == FaultPoint::AfterCommitBeforePersist => {}
            outcome => panic!("unexpected input-admission fault outcome at {point:?}: {outcome:?}"),
        }
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
            .owner_head(&fixture.store, AssetOwner::CurrentDraft(draft_id))
            .unwrap();
        let target = fixture
            .state
            .assets()
            .owner_head(&fixture.store, AssetOwner::SubmittedTurnItem(item_id))
            .unwrap();
        assert_eq!(source.is_none(), moved);
        assert_eq!(target.is_some(), moved);
        fixture.store.validate_registered_domains().unwrap();
    }
}
