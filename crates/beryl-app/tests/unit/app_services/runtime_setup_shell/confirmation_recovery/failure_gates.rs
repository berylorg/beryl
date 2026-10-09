use super::*;
#[path = "failure_gates/support.rs"]
mod blocked_command;
use self::blocked_command::BlockedHomeCommand;

#[gpui::test]
fn published_creation_graph_retains_selection_fence_after_home_busy_then_retries_same_publication(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    cx.update(|app| RunningProcessOwner::mount_ordinary_commands(&owner, app))
        .unwrap();
    let native = window.window_id();
    let blocked = Rc::new(RefCell::new(None));
    let reached = blocked.clone();
    let blocker_faults = faults.clone();
    owner
        .borrow_mut()
        .test_before_thread_creation_reopen(Box::new(move |services| {
            let graph = services.graph().unwrap();
            let home = graph.home().service_reference();
            let generation = home.health().generation().unwrap();
            let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
            let executable = AdmittedHostPath::from_admitted(
                PathFlavor::Windows,
                r"C:\Codex\blocked-command.exe",
            )
            .unwrap();
            let native = |path| {
                RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path)
                    .unwrap()
            };
            let available =
                AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(4))
                    .unwrap();
            let registration = CreateRuntimeWithHomeRoot::new(
                RuntimeRegistration::new(
                    RuntimeId::from_bytes([153; 16]),
                    executable,
                    RuntimeMode::host(),
                    RuntimeLaunchForm::CodexCli,
                    native(r"C:\Codex\blocked-command.exe"),
                    UnixMillis::new(4),
                    available,
                )
                .unwrap(),
                RootRegistration::new(
                    RootId::from_bytes([154; 16]),
                    native(r"C:\work\beryl"),
                    AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\work\beryl").unwrap(),
                    UnixMillis::new(4),
                    available,
                ),
            )
            .unwrap();
            command
                .add(graph.state().runtime_roots().create_runtime_with_home_root(
                    graph.state().runtime_roots().revision(&home).unwrap(),
                    registration,
                ))
                .unwrap();
            command
                .add(
                    graph.syndic().create_thread(
                        graph.syndic().revision(&home).unwrap(),
                        syndic_storage::CreateThread::ordinary(
                            SyndicThreadId::from_bytes([151; 16]),
                            SyndicDraftId::from_bytes([152; 16]),
                            ExecutionBinding::new(
                                RuntimeId::from_bytes([153; 16]),
                                RootId::from_bytes([154; 16]),
                                RuntimeNativePath::from_admitted(
                                    RuntimeMode::host(),
                                    PathFlavor::Windows,
                                    r"C:\work\beryl",
                                )
                                .unwrap(),
                            ),
                            syndic_storage::SyndicTimestamp::from_unix_millis(4),
                            syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                        ),
                    ),
                )
                .unwrap();
            let pause = blocker_faults.block_next(FaultPoint::AfterCommitBeforePersist);
            let worker = std::thread::spawn(move || home.execute(command));
            let mut command = BlockedHomeCommand::new(pause, worker);
            assert!(
                command.reached(),
                "blocking Home command did not reach AfterCommitBeforePersist: {:?}",
                command.release_and_join(),
            );
            *reached.borrow_mut() = Some((command, generation));
        }));
    let target = interrupt_committed_creation(&owner, window, faults, cx);
    wait(
        cx,
        |cx| {
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Unavailable(_)) => true,
                _ => false,
            }
        },
        "published New Thread graph did not retain the refused coherent election",
    );
    let refusal = {
        let retained = owner.borrow();
        match retained.automatic_recovery_outcome().as_deref() {
            Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => error.clone(),
            _ => unreachable!(),
        }
    };
    let selection_pending = owner.borrow().test_services().running_selection_pending();
    let process_fenced = owner
        .borrow()
        .test_services()
        .test_recovery_process_is_fenced();
    let (mut command, generation) = blocked.borrow_mut().take().unwrap();
    let published = selection(window, cx);
    let bound = owner
        .borrow()
        .test_recovered_process_command_binding_count();
    assert_eq!(bound, Some(1));
    assert_eq!(published.binding().home_generation(), generation);
    assert_eq!(
        owner
            .borrow()
            .test_services()
            .graph()
            .unwrap()
            .home()
            .health()
            .generation(),
        Some(generation)
    );
    assert!(matches!(
        command.release_and_join().unwrap(),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(
        refusal.contains("home coherence is busy"),
        "unexpected publication refusal: {refusal}"
    );
    assert!(selection_pending && process_fenced);
    let result = Rc::new(RefCell::new(None));
    let returned = result.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |_| result.borrow().is_some(),
        "same published creation graph retry did not return",
    );
    result.borrow_mut().take().unwrap().unwrap();
    assert_eq!(selection(window, cx), published);
    let bound = owner
        .borrow()
        .test_recovered_process_command_binding_count();
    assert_eq!(bound, None);
    assert_eq!(window.window_id(), native);
    assert_eq!(cx.windows().len(), 1);
    assert_original_committed_target(&owner, window, target.lock().unwrap().as_ref().unwrap(), cx);
    dispose_recovered(owner, window, cx);
    directory.close().unwrap();
}

#[gpui::test]
fn noncommitted_creation_retains_prior_provenance_after_cancelled_editor_preparation(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    let native = window.window_id();
    edit_prior(window, "prior survives candidate retry", cx);
    let prior = selection(window, cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    owner.borrow_mut().test_enable_resident_frame_capture();
    owner
        .borrow_mut()
        .test_reject_ordinary_recovery_attachment_after(0);
    let home = owner
        .borrow()
        .test_services()
        .graph()
        .unwrap()
        .home()
        .service_reference();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                None,
                Some(Box::new(move |_| {
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(home.home_revision().is_err());
                })),
                None,
            );
        })
        .unwrap();
    confirm(&owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app);
                RunningProcessOwner::test_observe_running_home_failure(&owner, app);
            });
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                    assert!(error.contains("preserved ordinary resident attachment was refused"));
                    true
                }
                _ => false,
            }
        },
        "prior candidate preparation did not retain the refused attachment",
    );
    let result = Rc::new(RefCell::new(None));
    let returned = result.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            let settled =
                RunningProcessOwner::test_settle_running_home_recovery_cancellation(&retained, cx)
                    .await;
            *returned.borrow_mut() = Some(settled);
        })
        .detach()
    });
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app)
            });
            result.borrow().is_some()
        },
        "refused prior editor cleanup did not return",
    );
    result.borrow_mut().take().unwrap().unwrap();
    assert!(
        owner
            .borrow()
            .test_thread_creation_disposed_generation()
            .is_some()
    );
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let result = Rc::new(RefCell::new(None));
    let returned = result.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app)
            });
            result.borrow().is_some()
        },
        "original noncommitted creation continuation did not return",
    );
    result.borrow_mut().take().unwrap().unwrap();
    let restored = selection(window, cx);
    assert_eq!(restored.claim(), prior.claim());
    assert_eq!(restored.binding().root(), prior.binding().root());
    assert_eq!(
        restored.binding().logical_extent(),
        prior.binding().logical_extent()
    );
    assert_eq!(window.window_id(), native);
    assert_eq!(cx.windows().len(), 1);
    assert!(
        !owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    dispose_recovered(owner, window, cx);
    directory.close().unwrap();
}

fn interrupt_committed_creation(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    faults: FaultController,
    cx: &mut TestAppContext,
) -> Arc<Mutex<Option<beryl_state::SessionWindowRecord>>> {
    catalog::populate(owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let prior = selection(window, cx);
    let (home, state) = {
        let owner = owner.borrow();
        let graph = owner.test_services().graph().unwrap();
        (graph.home().service_reference(), graph.state().clone())
    };
    let captured = Arc::new(Mutex::new(None));
    let target = captured.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                None,
                None,
                Some(Box::new(move |_| {
                    let evidence = state
                        .session()
                        .capture_window_removal(&home, prior.window_id())
                        .unwrap();
                    assert_ne!(evidence.window().selected_thread(), Some(prior.claim()));
                    *target.lock().unwrap() = Some(evidence.window().clone());
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(home.home_revision().is_err());
                })),
            );
        })
        .unwrap();
    confirm(owner, window, &picker, RootId::from_bytes([1; 16]), cx);
    captured
}

fn assert_original_committed_target(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    expected: &beryl_state::SessionWindowRecord,
    cx: &mut TestAppContext,
) {
    let selected = selection(window, cx);
    assert_eq!(Some(selected.claim()), expected.selected_thread());
    let owner = owner.borrow();
    let graph = owner.test_services().graph().unwrap();
    let current = graph
        .state()
        .session()
        .capture_window_removal(graph.home(), selected.window_id())
        .unwrap();
    assert_eq!(current.window(), expected);
    assert!(!owner.test_services().running_selection_pending());
    assert!(!owner.test_services().test_recovery_process_is_fenced());
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_first_conversation_transcript_claim(selected.claim(), app))
            .unwrap()
    );
}

#[gpui::test]
fn dropped_ready_creation_graph_returns_same_editor_and_original_exclusion_before_recovery(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    let native = window.window_id();
    owner.borrow_mut().test_drop_ready_thread_creation_graph();
    let target = interrupt_committed_creation(&owner, window, faults, cx);
    recover_creation(&owner, window, cx);
    assert_eq!(owner.borrow().test_returned_claim_graphs(), 1);
    assert_eq!(window.window_id(), native);
    assert_eq!(cx.windows().len(), 1);
    assert_original_committed_target(&owner, window, target.lock().unwrap().as_ref().unwrap(), cx);
    dispose_recovered(owner, window, cx);
    directory.close().unwrap();
}

#[gpui::test]
fn refused_fresh_creation_mount_cancels_exact_editor_before_original_target_retry(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, faults) = selected_window(cx);
    let native = window.window_id();
    let original_mount = window
        .read_with(cx, |root, _| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .entity_id()
        })
        .unwrap();
    window
        .update(cx, |root, _, _| {
            root.test_reject_thread_creation_recovery_mount()
        })
        .unwrap();
    let target = interrupt_committed_creation(&owner, window, faults, cx);
    wait(
        cx,
        |cx| {
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app));
            match owner.borrow().automatic_recovery_outcome().as_deref() {
                Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                    assert!(error.contains("retained fresh New Thread mount was refused"));
                    true
                }
                Some(
                    InterruptedExitRecoveryOutcome::Completed
                    | InterruptedExitRecoveryOutcome::Cancelled,
                ) => panic!("refused fresh mount settled without exact cleanup"),
                _ => false,
            }
        },
        "fresh mount failure did not retain original recovery custody",
    );
    let (fresh_mount, fresh_close, gated) = window
        .read_with(cx, |root, app| {
            root.test_thread_creation_recovery_mount(app)
        })
        .unwrap()
        .unwrap();
    assert_ne!(fresh_mount, original_mount);
    assert!(gated);
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let settled = Rc::new(RefCell::new(None));
    let returned = settled.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_settle_running_home_recovery_cancellation(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |_| settled.borrow().is_some(),
        "retained fresh creation editor cleanup did not return",
    );
    settled.borrow_mut().take().unwrap().unwrap();
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_thread_creation_recovery_mount(app))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .entity_id())
            .unwrap(),
        original_mount
    );
    assert!(owner.borrow().test_services().running_selection_pending());
    assert!(
        owner
            .borrow()
            .test_services()
            .test_recovery_process_is_fenced()
    );
    let continued = Rc::new(RefCell::new(None));
    let returned = continued.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            let original_generation = retained
                .borrow()
                .test_thread_creation_disposed_generation()
                .unwrap();
            let cancelled = beryl_home_store::CommandCancellation::new();
            cancelled.cancel();
            let refusal = RunningProcessOwner::test_continue_retired_running_home_recovery_with(
                &retained, cancelled, cx,
            )
            .await;
            assert!(refusal.unwrap_err().contains("continuation was cancelled"));
            assert_eq!(
                retained.borrow().test_thread_creation_disposed_generation(),
                Some(original_generation)
            );
            let driver = retained
                .borrow_mut()
                .test_hold_running_home_recovery_driver()
                .unwrap();
            let refusal =
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await;
            assert!(refusal.unwrap_err().contains("already being driven"));
            assert_eq!(
                retained.borrow().test_thread_creation_disposed_generation(),
                Some(original_generation)
            );
            drop(driver);
            *returned.borrow_mut() = Some(
                RunningProcessOwner::test_continue_retired_running_home_recovery(&retained, cx)
                    .await,
            );
        })
        .detach()
    });
    wait(
        cx,
        |_| continued.borrow().is_some(),
        "exact original creation target continuation did not return",
    );
    continued.borrow_mut().take().unwrap().unwrap();
    assert_ne!(
        selection(window, cx).binding().home_generation(),
        fresh_close.binding().home_generation()
    );
    assert_eq!(window.window_id(), native);
    assert_eq!(cx.windows().len(), 1);
    assert_original_committed_target(&owner, window, target.lock().unwrap().as_ref().unwrap(), cx);
    dispose_recovered(owner, window, cx);
    directory.close().unwrap();
}
