#[test]
fn startup_converges_persisted_terminal_history_before_returning_the_graph() {
    use crate::support;
    use beryl_model::{
        AdmittedHostPath, Availability, PathFlavor, RuntimeLaunchForm, RuntimeNativePath,
    };
    use beryl_state::{
        AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
        UnixMillis,
    };
    use syndic_storage::{InputGateState, SyndicPointReadLimit};
    let (directory, candidate, state, syndic, _) = fixture();
    let home = candidate.publish().unwrap();
    support::seed_populated(&home, syndic.clone());
    let thread = support::id(30);
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let execution = syndic
        .thread_execution(&home, thread, limit)
        .unwrap()
        .unwrap();
    let binding = execution.execution();
    let executable =
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap();
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let registration = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            binding.runtime_id(),
            executable.clone(),
            binding.root_path().mode().clone(),
            RuntimeLaunchForm::CodexCli,
            RuntimeNativePath::from_admitted(
                binding.root_path().mode().clone(),
                executable.flavor(),
                executable.as_str(),
            )
            .unwrap(),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            binding.root_id(),
            binding.root_path().clone(),
            AdmittedHostPath::from_admitted(
                binding.root_path().flavor(),
                binding.root_path().as_str(),
            )
            .unwrap(),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let mut registration_command =
        beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    registration_command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&home).unwrap(),
            registration,
        ))
        .unwrap();
    assert!(matches!(
        home.execute(registration_command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
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
