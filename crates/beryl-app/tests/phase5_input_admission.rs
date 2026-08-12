#![cfg(feature = "test-faults")]

use beryl_app::cas_projection::{
    DURABLE_START_ADMISSION_BUDGET_BYTES, MinimumTurnCaptureReserve, ProjectionConnectionService,
    ProjectionServiceConfig, ProjectionServiceConfigError, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_app::input_admission::{InputAdmissionBuildError, prepare_accepted_input_admission};
use beryl_home_store::test_faults::{FaultController, FreeSpaceTestObservation};
use beryl_home_store::{
    CommandError, CommandOutcome, FreeSpaceOutcome, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
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
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::NotCommitted { .. } => {
                panic!("expected committed thread setup, got {outcome:?}")
            }
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("expected no later failure, got {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("expected committed thread setup, got {outcome:?}")
            }
        }
        let store = ProjectionConnectionService::new(
            store,
            syndic,
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
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
        self.execute_one(
            self.syndic
                .begin_content(self.revision(), ContentBuild::from_prepared(&content)),
        );
        let mut manifest = content.building_manifest();
        while let Some(append) = ContentAppend::prepare(&manifest, &content).unwrap() {
            manifest = append.next_manifest().clone();
            self.execute_one(self.syndic.append_content(self.revision(), append));
        }
        let current = self.current_draft();
        let update =
            match DraftPayloadUpdate::prepare(&current, &content, time(updated_at)).unwrap() {
                DraftPayloadUpdateDecision::Update(update) => update,
                DraftPayloadUpdateDecision::NoChange => panic!("marker payload must be new"),
            };
        self.execute_one(self.syndic.update_draft_payload(self.revision(), update));
    }

    fn home(&self) -> beryl_app::cas_projection::LiveHomeCommand<'_> {
        self.store.live_home_command().unwrap()
    }

    fn revision(&self) -> beryl_model::DomainRevision {
        let home = self.home();
        self.syndic.revision(home.home()).unwrap()
    }

    fn current_draft(&self) -> syndic_storage::SyndicCurrentDraft {
        let home = self.home();
        self.syndic
            .current_draft(home.home(), self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    fn execute_one(&self, contribution: beryl_home_store::MutationContribution) {
        let home = self.home();
        execute_one(home.home(), contribution);
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
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed contribution, got {outcome:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed contribution, got {outcome:?}")
        }
    }
}

#[test]
fn idle_and_non_idle_admission_move_asset_ownership_in_the_same_commit() {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(1, faults.clone());
    let first_marker = SyndicDraftMarkerId::from_bytes([10; 16]);
    fixture.publish_marker(first_marker, 2);
    let first_draft = fixture.draft;
    let (_first_asset, first_set) =
        admit_asset(&mut fixture, first_marker, b"first image", first_draft, 20);
    let first_content = fixture.current_draft().draft().content();
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
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES + 1,
        total_free_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES + 1,
        total_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES + 1,
    });
    fixture
        .store
        .execute_idle_submission(fixture.state.assets(), submission)
        .unwrap();
    assert_eq!(faults.free_space_observation_count(), 1);

    let old_owner = AssetOwner::CurrentDraft(fixture.draft);
    let submitted_owner = AssetOwner::SubmittedTurnItem(first_item);
    let home = fixture.home();
    assert!(
        fixture
            .state
            .assets()
            .owner_head(home.home(), old_owner)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(home.home(), submitted_owner)
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
    let second_content = fixture.current_draft().draft().content();
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
    let home = fixture.home();
    let prepared = prepare_accepted_input_admission(
        home.home(),
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
    let home = fixture.home();
    assert_eq!(
        fixture
            .state
            .assets()
            .owner_head(home.home(), accepted_owner)
            .unwrap()
            .unwrap()
            .set(),
        second_set
    );
    fixture
        .store
        .live_home_command()
        .unwrap()
        .home()
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn turn_start_reserve_denials_leave_direct_draft_and_turn_history_unchanged() {
    for (seed, observation, expected) in [
        (
            101,
            FreeSpaceTestObservation::Observed {
                available_bytes: 0,
                total_free_bytes: 0,
                total_bytes: 1,
            },
            FreeSpaceOutcome::BelowReserve {
                available_bytes: 0,
                reserve_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES + 1,
            },
        ),
        (
            102,
            FreeSpaceTestObservation::Unavailable,
            FreeSpaceOutcome::Unavailable,
        ),
        (
            103,
            FreeSpaceTestObservation::Observed {
                available_bytes: 2,
                total_free_bytes: 1,
                total_bytes: 2,
            },
            FreeSpaceOutcome::Indeterminate,
        ),
    ] {
        let faults = FaultController::new();
        let fixture = Fixture::with_faults(seed, faults.clone());
        fixture.publish_text("direct admission reserve denial", 2);
        let before = fixture.current_draft();
        let submission = IdleSubmission::new(
            fixture.thread,
            before.thread().revision(),
            before.draft().id(),
            before.draft().revision(),
            before.draft().content(),
            InputGateRevision::new(1).unwrap(),
            SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(2); 16]),
            None,
            time(3),
        );
        let submitted_turn = submission.submitted_turn_id();
        faults.push_free_space_observation(observation);

        assert!(matches!(
            fixture
                .store
                .execute_idle_submission(fixture.state.assets(), submission),
            Err(beryl_app::cas_projection::IdleSubmissionExecutionError::FreeSpace(outcome))
                if outcome == expected
        ));
        assert_eq!(faults.free_space_observation_count(), 1);
        assert_eq!(fixture.current_draft(), before);
        assert!(
            fixture
                .syndic
                .turn(fixture.home().home(), submitted_turn, point_limit())
                .unwrap()
                .is_none(),
            "free-space denial must precede the direct durable/CAS turn transition"
        );
        assert_eq!(
            fixture
                .store
                .accepted_input_scheduler_diagnostics()
                .workers_started(),
            0
        );
    }
}

#[test]
fn marker_bearing_turn_start_denials_preserve_the_durable_draft_and_cas_boundary() {
    for (seed, observation, expected) in [
        (
            111,
            FreeSpaceTestObservation::Observed {
                available_bytes: 0,
                total_free_bytes: 0,
                total_bytes: 1,
            },
            FreeSpaceOutcome::BelowReserve {
                available_bytes: 0,
                reserve_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES + 1,
            },
        ),
        (
            112,
            FreeSpaceTestObservation::Unavailable,
            FreeSpaceOutcome::Unavailable,
        ),
        (
            113,
            FreeSpaceTestObservation::Observed {
                available_bytes: 2,
                total_free_bytes: 1,
                total_bytes: 2,
            },
            FreeSpaceOutcome::Indeterminate,
        ),
    ] {
        let faults = FaultController::new();
        let mut fixture = Fixture::with_faults(seed, faults.clone());
        let marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(10); 16]);
        fixture.publish_marker(marker, 2);
        let draft = fixture.draft;
        let (asset, proof) = admit_asset(
            &mut fixture,
            marker,
            b"phase105 retained marker-bearing image",
            draft,
            seed.wrapping_add(20),
        );
        let before_draft = fixture.current_draft();
        let home = fixture.home();
        let before_thread = fixture
            .syndic
            .thread(home.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let before_history = fixture
            .syndic
            .history_summary(home.home(), fixture.thread, point_limit())
            .unwrap();
        let before_binding = fixture
            .syndic
            .current_binding(home.home(), fixture.thread, point_limit())
            .unwrap();
        let before_owner = fixture
            .state
            .assets()
            .owner_head(home.home(), AssetOwner::CurrentDraft(fixture.draft))
            .unwrap();
        let before_asset = fixture.state.assets().metadata(home.home(), asset).unwrap();
        drop(home);

        let submission = IdleSubmission::new(
            fixture.thread,
            before_draft.thread().revision(),
            before_draft.draft().id(),
            before_draft.draft().revision(),
            before_draft.draft().content(),
            InputGateRevision::new(1).unwrap(),
            SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(2); 16]),
            Some(proof),
            time(3),
        );
        let submitted_turn = submission.submitted_turn_id();
        faults.push_free_space_observation(observation);

        assert!(matches!(
            fixture
                .store
                .execute_idle_submission(fixture.state.assets(), submission),
            Err(beryl_app::cas_projection::IdleSubmissionExecutionError::FreeSpace(outcome))
                if outcome == expected
        ));
        assert_eq!(faults.free_space_observation_count(), 1);

        let after_draft = fixture.current_draft();
        assert_eq!(after_draft, before_draft);
        assert_eq!(
            after_draft.draft().content().sealed_marker_summary(),
            before_draft.draft().content().sealed_marker_summary(),
            "the durable marker-bearing draft reference must remain unchanged"
        );
        let home = fixture.home();
        let after_thread = fixture
            .syndic
            .thread(home.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(after_thread, before_thread);
        assert_eq!(
            after_thread.committed_tail(),
            before_thread.committed_tail()
        );
        assert_eq!(
            after_thread.image_label_frontiers(),
            before_thread.image_label_frontiers(),
            "the durable image-label frontier must remain unchanged"
        );
        assert_eq!(
            fixture
                .syndic
                .history_summary(home.home(), fixture.thread, point_limit())
                .unwrap(),
            before_history
        );
        assert_eq!(
            fixture
                .syndic
                .current_binding(home.home(), fixture.thread, point_limit())
                .unwrap(),
            before_binding,
            "the CAS-visible durable binding must remain unchanged"
        );
        assert_eq!(
            fixture
                .state
                .assets()
                .owner_head(home.home(), AssetOwner::CurrentDraft(fixture.draft))
                .unwrap(),
            before_owner,
            "the draft asset owner head must remain unchanged"
        );
        assert_eq!(
            fixture.state.assets().metadata(home.home(), asset).unwrap(),
            before_asset
        );
        assert!(
            fixture
                .syndic
                .turn(home.home(), submitted_turn, point_limit())
                .unwrap()
                .is_none(),
            "free-space denial must not publish the new turn"
        );
        drop(home);
        assert_eq!(
            fixture
                .store
                .accepted_input_scheduler_diagnostics()
                .workers_started(),
            0,
            "direct denial must not make scheduler-visible progress"
        );
    }
}

#[test]
fn direct_turn_start_threshold_requires_the_composed_total() {
    let required = DURABLE_START_ADMISSION_BUDGET_BYTES + 1;
    let denied_faults = FaultController::new();
    let denied = Fixture::with_faults(121, denied_faults.clone());
    denied.publish_text("threshold denial", 2);
    let before = denied.current_draft();
    let denied_submission = IdleSubmission::new(
        denied.thread,
        before.thread().revision(),
        before.draft().id(),
        before.draft().revision(),
        before.draft().content(),
        InputGateRevision::new(1).unwrap(),
        SyndicDraftId::from_bytes([122; 16]),
        SyndicItemId::from_bytes([123; 16]),
        None,
        time(3),
    );
    denied_faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: required - 1,
        total_free_bytes: required - 1,
        total_bytes: required,
    });
    assert!(matches!(
        denied
            .store
            .execute_idle_submission(denied.state.assets(), denied_submission),
        Err(beryl_app::cas_projection::IdleSubmissionExecutionError::FreeSpace(
            FreeSpaceOutcome::BelowReserve { reserve_bytes, .. }
        )) if reserve_bytes == required
    ));
    assert_eq!(denied_faults.free_space_observation_count(), 1);

    let sufficient_faults = FaultController::new();
    let sufficient = Fixture::with_faults(124, sufficient_faults.clone());
    sufficient.publish_text("threshold sufficient", 2);
    let current = sufficient.current_draft();
    let sufficient_submission = IdleSubmission::new(
        sufficient.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        InputGateRevision::new(1).unwrap(),
        SyndicDraftId::from_bytes([127; 16]),
        SyndicItemId::from_bytes([128; 16]),
        None,
        time(3),
    );
    sufficient_faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: required,
        total_free_bytes: required,
        total_bytes: required,
    });
    sufficient
        .store
        .execute_idle_submission(sufficient.state.assets(), sufficient_submission)
        .unwrap();
    assert_eq!(sufficient_faults.free_space_observation_count(), 1);
}

#[test]
fn invalid_turn_start_configuration_performs_no_free_space_query() {
    let faults = FaultController::new();
    let directory = tempfile::tempdir().unwrap();
    let _home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();

    assert_eq!(
        MinimumTurnCaptureReserve::try_new(0),
        Err(beryl_home_store::TurnStartAdmissionRequirementError::ZeroMinimumTurnCaptureReserve)
    );
    assert_eq!(
        ProjectionServiceConfig::try_new(
            8,
            4,
            MinimumTurnCaptureReserve::try_new(u64::MAX).unwrap(),
        ),
        Err(ProjectionServiceConfigError::TurnStartAdmissionRequirement(
            beryl_home_store::TurnStartAdmissionRequirementError::ArithmeticOverflow {
                budget_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES,
                capture_reserve_bytes: u64::MAX,
            }
        ))
    );
    assert_eq!(faults.free_space_observation_count(), 0);
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
    let expected_content = fixture.current_draft().draft().content();
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
        fixture
            .store
            .execute_idle_submission(fixture.state.assets(), submission),
        Err(beryl_app::cas_projection::IdleSubmissionExecutionError::Build(
            InputAdmissionBuildError::MissingOwnerHead(owner)
        ))
            if owner == AssetOwner::CurrentDraft(fixture.draft)
    ));

    let current = fixture.current_draft();
    assert_eq!(current.draft().id(), fixture.draft);
    assert_eq!(current.draft().revision().get(), 2);
    assert!(
        fixture
            .syndic
            .turn(
                fixture.home().home(),
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
            .owner_head(
                fixture.home().home(),
                AssetOwner::SubmittedTurnItem(item_id)
            )
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .state
            .assets()
            .owner_head(
                fixture.home().home(),
                AssetOwner::CurrentDraft(wrong_owner_draft),
            )
            .unwrap()
            .is_some()
    );
}
