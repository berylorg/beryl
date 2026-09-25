use super::*;
use crate::{
    cas_projection::{
        MinimumTurnCaptureReserve, OrdinaryTurnExecutionRequest, ProjectionCancellationToken,
    },
    discussion_handoff_limits::HandoffScanConfiguration,
    theme_runtime::AppearanceCoordinatorConfig,
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{
    CursorReadLimits, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{ThemeManifestReadLimits, ThemePageLimits};
use std::{
    num::NonZeroU64,
    time::{Duration, Instant},
};

mod custody {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/custody.rs"
    ));
}

fn fixture() -> (
    tempfile::TempDir,
    HomeOpenPublication,
    BerylState,
    SyndicStorage,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    (directory, candidate, state, syndic, faults)
}

fn owner(candidate: &HomeOpenPublication) -> ProcessServiceOwner {
    ProcessServiceOwner::new(
        candidate.home_id(),
        NonZeroUsize::new(1).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
}

fn configuration() -> AppServiceConfiguration {
    let one = NonZeroUsize::new(1).unwrap();
    AppServiceConfiguration {
        projection: ProjectionServiceConfig::try_new(
            8,
            4,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap(),
        runtime_interest: RuntimeInterestConfig::new(one, one, Duration::from_secs(1)).unwrap(),
        session_policy: ScheduledOrdinaryRequestPolicy::new(
            ThreadStartOptions::persistent(),
            None,
            Duration::from_secs(1),
            OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), Duration::from_secs(1)),
        ),
        token_directories: Vec::new(),
        handoff: HandoffScanLimits::try_from(HandoffScanConfiguration {
            handoff_recovery_page_items: 1,
            handoff_recovery_page_encoded_bytes: beryl_state::HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
            handoff_job_record_encoded_bytes: beryl_state::HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
            handoff_reconcile_slots: 1,
            handoff_ready_job_items: 1,
        })
        .unwrap(),
        marker: DraftMarkerSealServiceLimits::new(NonZeroUsize::new(2).unwrap(), one).unwrap(),
        activity: ActivityServiceLimits::new(one, one, CursorReadLimits::new(8, 65_536).unwrap()),
        theme: ThemeRuntimeConfig::new(
            AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
            NonZeroU64::new(1024 * 1024).unwrap(),
            ThemeManifestReadLimits::new(
                NonZeroUsize::new(4096).unwrap(),
                NonZeroUsize::new(16 * 1024).unwrap(),
                NonZeroUsize::new(256 * 1024).unwrap(),
            )
            .unwrap(),
            ThemePageLimits::new(
                NonZeroUsize::new(8).unwrap(),
                NonZeroUsize::new(4096).unwrap(),
            )
            .unwrap(),
            Duration::from_millis(10),
            NonZeroUsize::new(8).unwrap(),
            NonZeroUsize::new(32).unwrap(),
            NonZeroU64::new(1024 * 1024).unwrap(),
            NonZeroUsize::new(4).unwrap(),
        ),
    }
}

fn assert_reopens(directory: &tempfile::TempDir) {
    HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .close()
    .unwrap();
}

fn close(owner: &mut ProcessServiceOwner) {
    owner.begin_shutdown().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match owner
            .poll_shutdown(&ProjectionCancellationToken::new())
            .unwrap()
        {
            AppServiceShutdownProgress::Ready => break,
            AppServiceShutdownProgress::Waiting => {
                assert!(Instant::now() < deadline, "shutdown did not drain");
                std::thread::sleep(Duration::from_millis(10));
            }
            progress => panic!("unexpected shutdown: {progress:?}"),
        }
    }
    owner.finish_shutdown().unwrap();
}

#[test]
fn complete_graph_publishes_once_and_theme_loading_waits_for_explicit_startup() {
    let (directory, candidate, state, syndic, faults) = fixture();
    let home_id = candidate.home_id();
    let generation = candidate.generation();
    let themes = state.themes();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let graph = owner.graph_mut().unwrap();
    let restore_attempt = graph.restored_window_attempt().unwrap();
    assert!(restore_attempt.validate_lifetime().is_ok());
    assert_eq!(graph.home().home_id(), home_id);
    assert_eq!(graph.home().health().generation(), Some(generation));
    assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
    assert!(graph.handoff.is_some() && graph.activity.is_some() && graph.marker.is_some());
    assert_eq!(themes.diagnostics().active_subscriptions(), 1);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let revision = graph.state().settings().revision(graph.home()).unwrap();
    graph.load_theme(revision, None).unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert!(graph.theme().unwrap().current().is_some());
    let (_other_directory, other, other_state, other_syndic, _) = fixture();
    assert!(matches!(
        owner.open_initial(
            other,
            other_state,
            other_syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new()
        ),
        Err(AppServiceOpenError::AlreadyInstalled)
    ));
    close(&mut owner);
    assert!(restore_attempt.validate_lifetime().is_err());
    assert!(owner.graph().is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert_reopens(&directory);
}

#[test]
fn last_constructor_failure_disposes_every_component_before_releasing_home() {
    let (directory, candidate, state, syndic, faults) = fixture();
    let themes = state.themes();
    let mut owner = owner(&candidate);
    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    let result = owner.open_initial(
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(1),
        CommandCancellation::new(),
    );
    assert!(matches!(result, Err(AppServiceOpenError::Theme(_))));
    assert!(owner.graph().is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    assert_reopens(&directory);
}

#[test]
fn cancellation_and_publication_rejection_dispose_the_complete_private_graph() {
    for cancel in [false, true] {
        let (directory, candidate, state, syndic, faults) = fixture();
        let themes = state.themes();
        let owner = owner(&candidate);
        let cancellation = CommandCancellation::new();
        let mut prepared = preparation::PreparedAppServices::prepare(
            &owner,
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            &cancellation,
        )
        .unwrap();
        assert_eq!(themes.diagnostics().active_subscriptions(), 1);
        if cancel {
            cancellation.cancel();
        } else {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            let access = prepared
                .candidate
                .as_mut()
                .unwrap()
                .recovery_access()
                .unwrap();
            assert!(prepared.syndic.revision_candidate(&access).is_err());
        }
        let result = prepared.publish(&cancellation);
        if cancel {
            assert!(matches!(result, Err(AppServiceOpenError::Cancelled)));
        } else {
            assert!(matches!(result, Err(AppServiceOpenError::Candidate(_))));
        }
        assert!(owner.graph().is_none());
        assert_eq!(themes.diagnostics().active_subscriptions(), 0);
        assert_reopens(&directory);
    }
}

#[test]
fn dropping_published_graph_joins_services_before_releasing_home() {
    let (directory, candidate, state, syndic, _) = fixture();
    let themes = state.themes();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let marker = owner.graph().unwrap().marker();
    drop(owner.graph.take());
    assert_eq!(marker.diagnostics().current_flights(), 0);
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    assert_reopens(&directory);
}

#[test]
fn retained_attention_clones_cannot_report_or_track_after_graph_retirement() {
    use crate::lifecycle_attention::{LifecycleAttentionAdmission, LifecycleAttentionRejection};
    use beryl_model::{SyndicThreadId, SyndicTurnId};
    for implicit in [false, true] {
        let (directory, candidate, state, syndic, _) = fixture();
        let home_id = candidate.home_id();
        let mut owner = owner(&candidate);
        owner
            .open_initial(
                candidate,
                state,
                syndic,
                configuration(),
                SyndicTimestamp::from_unix_millis(1),
                CommandCancellation::new(),
            )
            .unwrap();
        let pool = Arc::clone(owner.graph().unwrap().attention());
        let thread = SyndicThreadId::from_bytes([1; 16]);
        let turn = SyndicTurnId::from_bytes([2; 16]);
        let attempt = pool
            .track_accepted_yield(
                home_id,
                thread,
                turn,
                crate::LifecycleYieldOutcome::PlanComplete,
            )
            .unwrap();
        if implicit {
            drop(owner.graph.take());
        } else {
            close(&mut owner);
        }
        assert!(matches!(
            pool.report_terminal(&attempt),
            LifecycleAttentionAdmission::Rejected(LifecycleAttentionRejection::Closed)
        ));
        assert!(
            pool.track_accepted_yield(
                home_id,
                thread,
                turn,
                crate::LifecycleYieldOutcome::PlanComplete
            )
            .is_none()
        );
        assert!(pool.snapshot().is_empty());
        assert_reopens(&directory);
    }
}

#[test]
fn startup_converges_persisted_terminal_history_before_returning_the_graph() {
    use crate::support;
    use syndic_storage::{InputGateState, SyndicPointReadLimit};
    let (directory, candidate, _, syndic, _) = fixture();
    let home = candidate.publish().unwrap();
    support::seed_populated(&home, syndic.clone());
    let thread = support::id(30);
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    assert_ne!(
        syndic
            .input_gate(&home, thread, limit)
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    home.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            support::timestamp(1000),
            CommandCancellation::new(),
        )
        .unwrap();
    let graph = owner.graph().unwrap();
    assert_eq!(
        graph
            .syndic()
            .input_gate(graph.home(), thread, limit)
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    assert!(
        graph
            .cas()
            .accepted_input_scheduler_diagnostics()
            .startup_terminal_convergences()
            >= 1
    );
    close(&mut owner);
    assert_reopens(&directory);
}
