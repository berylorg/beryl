use super::*;
use crate::{runtime_activity_enrollment::ActivityEnrollmentCommandOutcome, support};
use beryl_model::{RuntimeId, SyndicItemId};
use syndic_storage::{
    ActivityEnrollmentPreparation, ActivityEnrollmentRequest, ActivityQuerySource,
    SyndicPointReadLimit,
};

pub(super) fn installed() -> (tempfile::TempDir, ProcessServiceOwner, FaultController) {
    let (directory, candidate, state, syndic, faults) = fixture();
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner
        .graph()
        .unwrap()
        .handoff
        .as_ref()
        .unwrap()
        .test_completed_passes()
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "initial handoff scan did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    (directory, owner, faults)
}

pub(super) fn fail(owner: &ProcessServiceOwner, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(owner.graph().unwrap().home().home_revision().is_err());
    assert_eq!(
        owner.graph().unwrap().home().health().state(),
        HomeHealthState::Failed
    );
}

pub(super) fn marker_flight(
    owner: &ProcessServiceOwner,
) -> crate::composer_marker_seal::DraftMarkerSealFlight {
    use crate::composer_marker_seal::{DraftMarkerSealAdmission, DraftMarkerSealFlightRequest};
    use syndic_storage::{
        DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
        DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
        DraftEditorCurrentSelectorV1, DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1,
    };
    let graph = owner.graph().unwrap();
    let home = graph.home();
    let syndic = graph.syndic();
    support::seed_populated(home, syndic.clone());
    let current = syndic
        .current_draft(
            home,
            support::id(30),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let selector = DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    );
    let prepared = syndic
        .prepare_open_draft_editor_candidate_session(
            home,
            DraftEditorCandidateSessionOpenRequestV1::new(
                selector,
                DraftEditorCandidateSessionIdV1::from_bytes([201; 16]),
                DraftPieceOperationIdV1::from_bytes([202; 16]),
            ),
        )
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            syndic.open_draft_editor_candidate_session(
                syndic.revision(home).unwrap(),
                prepared.clone(),
            ),
        )
        .unwrap();
    let outcome = home.execute(command);
    let session = match syndic
        .reconcile_draft_editor_candidate_session_open(home, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("expected opened editor session: {other:?}"),
    };
    let request = DraftMarkerSealFlightRequest::new(
        DraftEditorCandidateActivationBindingV1::from_head(&session),
        DraftMarkerSealOperationIdV1::from_bytes([203; 16]),
        beryl_state::AssetReferenceSetStagingAuthority::new(
            beryl_model::AssetReferenceSetId::from_bytes([204; 16]),
            [204; 32],
        ),
    );
    match graph
        .marker()
        .admit(home, request, &CommandCancellation::new())
        .unwrap()
    {
        DraftMarkerSealAdmission::Admitted(flight) => flight,
        other => panic!("expected admitted marker flight: {other:?}"),
    }
}
pub(super) fn install_uncertain_enrollment(
    owner: &ProcessServiceOwner,
    home: &HomeStore,
    state: &beryl_state::BerylState,
    syndic: &SyndicStorage,
    faults: &FaultController,
) -> RuntimeId {
    let thread = support::id(30);
    register_populated_execution_source(home, state);
    support::seed_populated(home, syndic.clone());
    support::converge_and_release_terminal_history(
        home,
        syndic.clone(),
        thread,
        support::populated::source_turn(),
    );
    let turn = support::exact_cas::submit_current_draft(
        home,
        syndic.clone(),
        thread,
        support::draft_id(220),
        SyndicItemId::from_bytes([221; 16]),
        "pending enrollment",
        support::timestamp(100),
    );
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let execution = syndic
        .thread_execution(home, thread, limit)
        .unwrap()
        .unwrap();
    let runtime = execution.execution().runtime_id();
    let head = syndic
        .activity_query_head(home, thread, limit)
        .unwrap()
        .unwrap();
    let ActivityEnrollmentPreparation::Prepared(prepared) = syndic
        .prepare_activity_enrollment(
            home,
            ActivityEnrollmentRequest::first(
                ActivityQuerySource::new(thread, turn),
                execution.execution().clone(),
                head.revision(),
            ),
        )
        .unwrap()
    else {
        panic!("first real Activity enrollment must need publication")
    };
    let reservation = owner.enrollments.reserve(home, prepared).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        reservation.execute(),
        ActivityEnrollmentCommandOutcome::Pending { .. }
    ));
    assert_eq!(owner.enrollments.pending_count(), 1);
    assert_eq!(home.pending_reconciliations().len(), 1);
    runtime
}

fn register_populated_execution_source(home: &HomeStore, state: &beryl_state::BerylState) {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use beryl_model::{
        AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeLaunchForm, RuntimeMode,
        RuntimeNativePath,
    };
    use beryl_state::{
        AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
        UnixMillis,
    };
    let executable =
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap();
    let root =
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, r"C:\populated")
            .unwrap();
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let registration = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            RuntimeId::from_bytes([48; 16]),
            executable.clone(),
            root.mode().clone(),
            RuntimeLaunchForm::CodexCli,
            RuntimeNativePath::from_admitted(
                root.mode().clone(),
                executable.flavor(),
                executable.as_str(),
            )
            .unwrap(),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            RootId::from_bytes([49; 16]),
            root.clone(),
            AdmittedHostPath::from_admitted(root.flavor(), root.as_str()).unwrap(),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(home).unwrap(),
            registration,
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed { .. }
    ));
}
