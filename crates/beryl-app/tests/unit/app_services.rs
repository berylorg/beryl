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

#[path = "app_services/committed_first_conversation.rs"]
#[cfg(target_os = "windows")]
mod committed_first_conversation;
#[path = "app_services/recovery_graph.rs"]
mod recovery_graph;
#[path = "app_services/recovery_preparation.rs"]
mod recovery_preparation;
#[path = "app_services/recovery_publication.rs"]
mod recovery_publication;
#[path = "app_services/recovery_retirement.rs"]
mod recovery_retirement;
#[path = "app_services/recovery_support.rs"]
mod recovery_support;
#[path = "app_services/runtime_setup_services.rs"]
#[cfg(target_os = "windows")]
mod runtime_setup_services;
#[path = "app_services/runtime_setup_shell.rs"]
#[cfg(target_os = "windows")]
mod runtime_setup_shell;

mod custody {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/custody.rs"
    ));
}

mod initial_disposal {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/initial_disposal.rs"
    ));
}

mod reopening {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/reopening.rs"
    ));
}

#[path = "app_services/observed_shutdown.rs"]
mod observed_shutdown;
#[path = "app_services/runtime_cleanup_support.rs"]
mod runtime_cleanup_support;

mod failed_retirement {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/failed_retirement.rs"
    ));
}

mod window_services {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/window_services.rs"
    ));
}

#[cfg(target_os = "windows")]
mod startup_owner {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/startup_owner.rs"
    ));

    mod ordinary_running_home {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/app_services/ordinary_running_home.rs"
        ));
    }
}

pub(super) fn fixture() -> (
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

pub(super) fn configuration() -> AppServiceConfiguration {
    let one = NonZeroUsize::new(1).unwrap();
    AppServiceConfiguration {
        wsl_supervisor_artifact: None,
        paste_resources: crate::main_window::MainWindowComposerPasteResources::new(1, 4096)
            .unwrap(),
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
        token_directory: crate::cas_projection::RuntimeTokenDirectory::from_admitted(
            beryl_model::AdmittedHostPath::from_admitted(
                beryl_model::PathFlavor::Windows,
                r"C:\tokens",
            )
            .unwrap(),
        ),
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
    let private_clipboard = graph.private_clipboard.clone();
    assert!(!private_clipboard.is_retired());
    let restore_attempt = graph.restored_window_attempt().unwrap();
    assert!(restore_attempt.validate_lifetime().is_ok());
    assert_eq!(graph.home().home_id(), home_id);
    assert_eq!(graph.home().health().generation(), Some(generation));
    assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
    assert!(graph.handoff.is_some() && graph.activity.is_some() && graph.marker.is_some());
    assert_eq!(themes.diagnostics().active_subscriptions(), 1);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    graph.release_theme().unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert!(graph.theme().unwrap().current().is_some());
    let (_other_directory, other, other_state, other_syndic, _) = fixture();
    let rejected = owner
        .open_initial(
            other,
            other_state,
            other_syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::AlreadyInstalled
    ));
    rejected.rejected_candidate.unwrap().close().unwrap();
    close(&mut owner);
    assert!(restore_attempt.validate_lifetime().is_err());
    assert!(private_clipboard.is_retired());
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
    let failure = result.unwrap_err();
    assert!(matches!(failure.error, AppServiceOpenError::Theme(_)));
    assert!(failure.rejected_candidate.is_none());
    assert!(owner.retained_close().is_none());
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
        let mut owner = owner(&candidate);
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
        let failure = match prepared.publish(&cancellation) {
            Err(failure) => owner.dispose_initial_failure(failure),
            Ok(_) => panic!("rejected preparation unexpectedly published"),
        };
        if cancel {
            assert!(matches!(failure.error, AppServiceOpenError::Cancelled));
        } else {
            assert!(matches!(failure.error, AppServiceOpenError::Candidate(_)));
        }
        assert!(failure.rejected_candidate.is_none());
        assert!(owner.retained_close().is_none());
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
    let private_clipboard = owner.graph().unwrap().private_clipboard.clone();
    assert!(!private_clipboard.is_retired());
    drop(owner.graph.take());
    assert!(private_clipboard.is_retired());
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
