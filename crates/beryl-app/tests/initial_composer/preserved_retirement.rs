use super::*;
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::theme_runtime::{AppearanceCoordinator, AppearanceCoordinatorConfig};
use std::num::NonZeroUsize;

fn prepared_shell(
    fixture: &Fixture,
    custody: MainWindowInitialComposer,
) -> MainWindowShellPrepared {
    let prepared = custody
        .prepare(&mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    let appearance = AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        beryl_state::PreparedThemeAppearance::fallback(
            fixture
                .state
                .themes()
                .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        ),
    )
    .current();
    let seals = DraftMarkerSealService::test_new(
        &fixture.store,
        fixture.store.health().generation().unwrap(),
        fixture.storage.clone(),
        fixture.state.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let submission = MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    );
    prepared
        .into_shell(Box::new(config), seals, submission, appearance)
        .unwrap_or_else(|failure| panic!("{}", failure.error))
}

#[test]
fn acquired_transient_retirement_preserves_records_through_cancellation_and_uncertain_commit() {
    for case in 0..3 {
        let fixture = Fixture::new(61);
        let mut initial = fixture.begin(62);
        let draft = initial.acquisition().draft_id();
        let thread = initial.acquisition().thread_id();
        let window = initial.acquisition().window_id();
        assert_eq!(
            initial.advance(&CommandCancellation::new()).unwrap(),
            MainWindowInitialComposerProgress::Activated
        );
        if case == 2 {
            let faults = fixture.faults.clone();
            initial.test_arm_before_retirement(move |_, _| {
                faults.fail_next(FaultPoint::AfterCommitBeforePersist)
            });
        }
        let before = restoration_support::snapshot(&fixture);
        let claim = restoration_support::paired_claim(&fixture, window);
        let mut custody = prepared_shell(&fixture, initial)
            .into_unpublished()
            .preserve_records();
        assert_eq!(custody.window_id(), window);
        let cancellation = CommandCancellation::new();
        if case == 1 {
            cancellation.cancel();
        }
        if case != 0 {
            let MainWindowShellRecordPreservingRetirementOutcome::Pending {
                custody: retained,
                error,
            } = custody.retire(cancellation)
            else {
                panic!("unsettled retirement retains original custody")
            };
            assert!(!error.is_empty());
            custody = retained;
            assert_eq!(custody.window_id(), window);
            assert_eq!(fixture.process.main_window_occupancy(), 1);
            assert_eq!(restoration_support::snapshot(&fixture), before);
        }
        assert!(matches!(
            custody.retire(CommandCancellation::new()),
            MainWindowShellRecordPreservingRetirementOutcome::Retired
        ));
        assert_eq!(fixture.process.main_window_occupancy(), 0);
        assert_eq!(restoration_support::snapshot(&fixture), before);
        assert_eq!(restoration_support::paired_claim(&fixture, window), claim);
        assert!(matches!(
            fixture.session(draft, 62),
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
        ));
        assert!(
            fixture
                .storage
                .audit_pristine_thread(&fixture.store, thread, &fixture.execution())
                .is_ok()
        );
        restoration_support::begin_restore(&fixture);
        let restoring = restoration_support::snapshot(&fixture);
        let (attempt, _service) = restoration_support::attempt(&fixture);
        let mut restored = restoration_support::begin(&fixture, &attempt, 63);
        restoration_support::open(&mut restored, &attempt);
        restoration_support::retire(restored);
        assert_eq!(restoration_support::snapshot(&fixture), restoring);
    }
}

#[test]
fn acquired_record_preservation_without_remaining_candidate_releases_only_reservation() {
    let fixture = Fixture::new(71);
    let mut initial = fixture.begin(72);
    initial.advance(&CommandCancellation::new()).unwrap();
    let before = restoration_support::snapshot(&fixture);
    let MainWindowInitialComposerRetirement::Retired(unpublished) =
        initial.retire(CommandCancellation::new())
    else {
        panic!("candidate settles before acquired record preservation")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(matches!(
        unpublished
            .preserve_records()
            .retire(CommandCancellation::new()),
        MainWindowShellRecordPreservingRetirementOutcome::Retired
    ));
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert_eq!(restoration_support::snapshot(&fixture), before);
}
