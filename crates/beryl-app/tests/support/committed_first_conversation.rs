use super::*;

pub(crate) fn commit_onboarding(
    process: &ProcessServiceOwner,
    transient_reconciliation: Option<&FaultController>,
) -> (
    SyndicThreadId,
    SyndicDraftId,
    beryl_state::SessionWindowRecord,
) {
    let graph = process.graph().unwrap();
    let home = graph.home();
    let state = graph.state();
    let syndic = graph.syndic();
    let window = state
        .session()
        .minimal_bootstrap(home)
        .unwrap()
        .unwrap()
        .windows()[0]
        .window_id();
    let runtime_id = RuntimeId::from_bytes([74; 16]);
    let root_id = RootId::from_bytes([75; 16]);
    let thread_id = SyndicThreadId::from_bytes([76; 16]);
    let draft_id = SyndicDraftId::from_bytes([77; 16]);
    let host = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let native = |path| {
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
    };
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let creation = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            runtime_id,
            host(r"C:\Codex\codex.exe"),
            RuntimeMode::host(),
            RuntimeLaunchForm::CodexCli,
            native(r"C:\Codex\codex.exe"),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            root_id,
            native(r"C:\Work\Beryl"),
            host(r"C:\Work\Beryl"),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let registry_source = creation.initial_catalog_source();
    let thread = CreateThread::ordinary(
        thread_id,
        draft_id,
        ExecutionBinding::new(runtime_id, root_id, native(r"C:\Work\Beryl")),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1).unwrap(),
    );
    let replacement = match state
        .session()
        .prepare_window_claim_replacement(
            home,
            window,
            None,
            RememberedTarget::new(runtime_id, root_id),
            thread_id,
        )
        .unwrap()
    {
        WindowClaimReplacementPreparation::Prepared(prepared) => prepared,
        _ => panic!("first conversation should be unclaimed"),
    };
    let summary = thread.initial_catalog_summary();
    let facts =
        crate::window_acquisition::project_unclaimed_facts(&summary, &registry_source).unwrap();
    let exact_window = replacement.future_window().clone();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(home).unwrap(),
                creation,
            ),
        )
        .unwrap();
    command
        .add(state.catalog().publish_claim(
            state.catalog().revision(home).unwrap(),
            PublishCatalogClaim::initial(
                thread_id,
                CatalogSourceRevisions::new(
                    summary.revision(),
                    RecordRevision::INITIAL,
                    RecordRevision::INITIAL,
                    None,
                ),
                facts,
                replacement.catalog_claim(),
            ),
        ))
        .unwrap();
    command
        .add(syndic.create_thread(syndic.revision(home).unwrap(), thread))
        .unwrap();
    command
        .add(replacement.contribution(&state.session(), home).unwrap())
        .unwrap();
    if let Some(faults) = transient_reconciliation {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    }
    graph.runtime_setup().test_execute_first(
        command,
        runtime_id,
        root_id,
        OnboardingFacts::test_committed(thread_id, draft_id, replacement),
    );
    if let Some(faults) = transient_reconciliation {
        let original = home.pending_reconciliations();
        assert_eq!(original.len(), 1);
        faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
        assert!(home.reconcile(&original[0]).is_err());
        assert!(
            home.reconcile(&original[0]).is_err(),
            "original handle must retain the transient cached error"
        );
    }
    (thread_id, draft_id, exact_window)
}
