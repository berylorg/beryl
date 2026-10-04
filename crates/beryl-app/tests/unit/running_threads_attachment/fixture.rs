use super::*;

pub(super) fn activate(cx: &mut gpui::TestAppContext, seed: u8, mode: CompletionMode) {
    let mounted = mount(cx, seed);
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.notice_safe_focus(cx).focus(window)
        })
        .unwrap();
    for (index, character) in "retained prior draft".chars().enumerate() {
        gpui::VisualTestContext::from_window(mounted.window.into(), cx)
            .simulate_input(&character.to_string());
        drive(cx, |cx| {
            mounted
                .window
                .read_with(cx, |root, app| {
                    root.cached_running_selection(app)
                        .is_some_and(|(current, _)| {
                            current.binding().logical_extent().logical_utf8_bytes()
                                == (index + 1) as u64
                        })
                })
                .unwrap()
        });
    }
    let source = source(&mounted, cx);
    let edited_prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    install_source(&mounted, &source, cx);
    let target = SyndicThreadId::from_bytes([seed.wrapping_add(20); 16]);
    let store = mounted.fixture.store.clone();
    let storage = mounted.fixture.storage.clone();
    let execution = mounted.fixture.execution();
    let gate = home_support::join(
        home_support::worker(move || {
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.create_thread(
                    storage.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        target,
                        SyndicDraftId::from_bytes([seed.wrapping_add(21); 16]),
                        execution,
                        SyndicTimestamp::from_unix_millis(9999),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            let before = storage
                .input_gate(&store, target, SyndicPointReadLimit::new(65_536).unwrap())
                .unwrap()
                .unwrap();
            let gate = InputGateRecord::new(
                target,
                InputGateRevision::new(before.revision().get() + 1).unwrap(),
                InputGateState::FinalizingHistory(SyndicTurnId::from_bytes(
                    [seed.wrapping_add(22); 16],
                )),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap();
            let mut batch = FixtureBatch::new();
            batch.put(FixtureRecord::InputGate(gate.clone())).unwrap();
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.fixture_contribution(storage.revision(&store).unwrap(), batch))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            gate
        }),
        cx,
    );
    let (source, mut live) = if mode == CompletionMode::LiveCheckedOut {
        let (source, live) = live_session::LiveSession::install(
            source,
            mounted.fixture.execution(),
            target,
            mounted.fixture.state.assets(),
            cx,
        );
        (source, Some(live))
    } else {
        (source, None)
    };
    let storage = mounted.fixture.storage.clone();
    let home = source.home.clone();
    let execution_before = home_support::join(
        home_support::worker(move || {
            storage
                .thread_execution(&home, target, SyndicPointReadLimit::new(65_536).unwrap())
                .unwrap()
        }),
        cx,
    );
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| root.running_threads.count == Some(2))
            .unwrap()
    });
    mounted
        .window
        .update(cx, |root, window, _| {
            root.running_threads.focus.focus(window)
        })
        .unwrap();
    support::draw(cx);
    gpui::VisualTestContext::from_window(mounted.window.into(), cx).simulate_keystrokes("enter");
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, app| {
                root.running_threads
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.read(app).diagnostics().resident_row_count == 2)
            })
            .unwrap()
    });
    let picker = mounted
        .window
        .read_with(cx, |root, _| root.running_threads.picker.clone().unwrap())
        .unwrap();
    let lease = mounted
        .fixture
        .process
        .admit_selection(&[prior.window_id()], prior.window_id())
        .unwrap();
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.running_threads.selection_lease = Some(Arc::new(lease));
            picker.update(cx, |picker, cx| picker.focus_row(0, window, cx));
        })
        .unwrap();
    let faults = mounted.fixture.faults.clone();
    let saved_prior = Arc::new(std::sync::Mutex::new(None));
    let commit_prior = saved_prior.clone();
    let composer_service = mounted
        .window
        .read_with(cx, |root, app| {
            root.controller
                .as_ref()
                .unwrap()
                .composer_mount
                .as_ref()
                .unwrap()
                .read(app)
                .claim_publication_service()
                .unwrap()
        })
        .unwrap();
    let save_faults = mounted.fixture.faults.clone();
    let disposal_faults = mounted.fixture.faults.clone();
    let hooks = RunningActivationFixtureHooks {
        commit: Some(Box::new(move |cancellation| {
            *commit_prior.lock().unwrap() = composer_service.selected_identity();
            match mode {
                CompletionMode::CancelClaim => cancellation.cancel(),
                CompletionMode::ReconcileClaim => {
                    faults.fail_next(FaultPoint::AfterCommitBeforePersist)
                }
                _ => {}
            }
        })),
        save: match mode {
            CompletionMode::CancelSave => Some(Box::new(|cancellation| cancellation.cancel())),
            CompletionMode::ReconcileSave => Some(Box::new(move |_| {
                save_faults.fail_next(FaultPoint::AfterCommitBeforePersist)
            })),
            _ => None,
        },
        disposal: (mode == CompletionMode::RefuseCommittedDisposal).then(|| {
            Box::new(move |_: &beryl_home_store::CommandCancellation| {
                disposal_faults.fail_next(FaultPoint::BeforeCommit)
            }) as _
        }),
    };
    mounted
        .window
        .update(cx, |root, _, _| {
            root.running_threads.fixture_activation_hooks = Some(hooks)
        })
        .unwrap();
    let connection_revision = source.service.connection_work_revision().unwrap();
    let connection_records = source
        .service
        .connection_work_page(
            &connection_revision,
            None,
            crate::cas_projection::ConnectionWorkPageLimits::new(8, 65_536).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec();
    let session_revision = source._sessions.work_revision().unwrap();
    let observe_workers = source.service.worker_pool_observer_for_test();
    let workers_before = observe_workers();
    let sessions_before = source
        .service
        .required_session_work_for_test(&source._sessions, &ProjectionCancellationToken::new())
        .unwrap();
    if live.is_some() {
        assert_eq!(sessions_before.len(), 1);
        assert_eq!(sessions_before[0].0, target);
        assert_eq!(
            sessions_before[0].1.state(),
            crate::cas_projection::ScheduledSessionWorkState::CheckedOut
        );
        assert_eq!(source._sessions.diagnostics().checked_out, 1);
    } else {
        assert!(sessions_before.is_empty());
    }
    support::draw(cx);
    gpui::VisualTestContext::from_window(mounted.window.into(), cx).simulate_keystrokes("enter");
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| {
                root.running_threads.activation_operation.is_some()
            })
            .unwrap()
    });
    mounted
        .window
        .update(cx, |root, _, _| root.running_threads.read_drain_task = None)
        .unwrap();
    if mode == CompletionMode::ReconcileClaim {
        drive(cx, |cx| {
            mounted
                .window
                .read_with(cx, |root, _| {
                    root.running_threads
                        .activation_operation
                        .as_ref()
                        .is_some_and(|operation| {
                            operation.diagnostics().starts_with("stage=Reconcile ")
                        })
                })
                .unwrap()
        });
        mounted
            .window
            .update(cx, |root, _, cx| {
                assert_fenced_prior(root, cx, prior);
                assert!(root.running_threads.activation_operation.is_some());
                assert!(!root.running_thread_reads_drained());
            })
            .unwrap();
        assert!(
            mounted
                .fixture
                .process
                .reserve_main_window(WindowId::from_bytes([seed.wrapping_add(30); 16]))
                .is_err()
        );
        assert_eq!(cx.windows().len(), 1);
    }
    if mode == CompletionMode::ReconcileSave {
        drive(cx, |cx| {
            mounted
                .window
                .read_with(cx, |root, _| {
                    root.running_threads.failure.as_ref().is_some_and(|error| {
                        error.contains("Prior editor save is awaiting reconciliation")
                    })
                })
                .unwrap()
        });
        mounted
            .window
            .read_with(cx, |root, app| {
                assert_fenced_prior(root, app, prior);
                assert!(root.running_threads.has_activation_custody());
                assert!(root.running_threads.failure_notice.is_some());
            })
            .unwrap();
    }
    if mode == CompletionMode::RefuseCommittedDisposal {
        drive(cx, |cx| {
            mounted
                .window
                .read_with(cx, |root, _| {
                    root.running_threads
                        .activation_operation
                        .as_ref()
                        .is_some_and(|operation| {
                            operation
                                .diagnostics()
                                .contains("committed predecessor disposal advance failed")
                        })
                })
                .unwrap()
        });
        mounted
            .window
            .read_with(cx, |root, app| {
                assert_fenced_prior(root, app, prior);
                assert!(root.running_threads.has_activation_custody());
                assert!(!root.running_thread_reads_drained());
                assert!(root.running_threads.failure_notice.is_some());
            })
            .unwrap();
        let record = mounted
            .window
            .read_with(cx, |root, _| {
                root.running_threads
                    .activation_operation
                    .as_ref()
                    .unwrap()
                    .committed_window_for_test()
                    .unwrap()
            })
            .unwrap();
        assert_eq!(record.selected_thread().unwrap().thread_id(), target);
        assert_eq!(
            source.home.health().state(),
            beryl_home_store::HomeHealthState::Failed
        );
        assert!(
            mounted
                .fixture
                .process
                .reserve_main_window(WindowId::from_bytes([seed.wrapping_add(30); 16]))
                .is_err()
        );
        mounted
            .window
            .update(cx, |root, _, _| {
                root.running_threads
                    .activation_operation
                    .as_ref()
                    .unwrap()
                    .suspend()
            })
            .unwrap();
        eprintln!(
            "retained committed fixture home: {}",
            mounted.fixture.directory.path().display()
        );
        return;
    }
    let mut last_diagnostic = String::new();
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| {
                let diagnostic = root
                    .running_threads
                    .activation_operation
                    .as_ref()
                    .map_or_else(
                        || {
                            format!(
                                "failure={:?} pending={}",
                                root.running_threads.failure,
                                root.running_threads.pending_activation.is_some()
                            )
                        },
                        |operation| operation.diagnostics(),
                    );
                if last_diagnostic != diagnostic {
                    eprintln!("activation operation {diagnostic}");
                    last_diagnostic = diagnostic;
                }
                !root.running_threads.has_activation_custody()
            })
            .unwrap()
    });
    mounted
        .window
        .read_with(cx, |root, app| {
            let expected = if matches!(
                mode,
                CompletionMode::CancelClaim | CompletionMode::CancelSave
            ) {
                prior.claim().thread_id()
            } else {
                target
            };
            assert_eq!(
                root.cached_running_selection(app)
                    .map(|(selection, _)| selection.claim().thread_id()),
                Some(expected)
            );
            assert_eq!(
                root.cached_running_selection(app)
                    .unwrap()
                    .0
                    .claim()
                    .thread_id(),
                expected
            );
            if !matches!(
                mode,
                CompletionMode::CancelClaim | CompletionMode::CancelSave
            ) {
                assert_eq!(
                    root.running_threads.transcript_claim.unwrap().thread_id(),
                    target
                );
                assert!(root.running_threads.failure.is_none());
                assert!(root.running_threads.failure_notice.is_none());
            }
            if matches!(
                mode,
                CompletionMode::CancelClaim | CompletionMode::CancelSave
            ) {
                let resumed = root.cached_running_selection(app).unwrap().0;
                assert_eq!(
                    resumed.binding().candidate().session_id(),
                    edited_prior.binding().candidate().session_id()
                );
                assert_eq!(resumed.binding().root(), edited_prior.binding().root());
                assert_eq!(
                    resumed.binding().history().availability(),
                    edited_prior.binding().history().availability()
                );
                if mode == CompletionMode::CancelClaim {
                    assert_eq!(
                        resumed.binding(),
                        saved_prior.lock().unwrap().unwrap().binding()
                    );
                }
            }
        })
        .unwrap();
    assert_eq!(cx.windows().len(), 1);
    if let Some(live) = live.as_mut() {
        live.verify(target);
    }
    let current_connection_revision = source.service.connection_work_revision().unwrap();
    assert_eq!(
        current_connection_revision.home_id(),
        connection_revision.home_id()
    );
    assert_eq!(
        current_connection_revision.home_generation(),
        connection_revision.home_generation()
    );
    assert_eq!(
        current_connection_revision.service_generation(),
        connection_revision.service_generation()
    );
    assert_eq!(
        source
            .service
            .connection_work_page(
                &current_connection_revision,
                None,
                crate::cas_projection::ConnectionWorkPageLimits::new(8, 65_536).unwrap(),
            )
            .unwrap()
            .records(),
        connection_records
    );
    if live.is_none() {
        assert!(
            source
                .service
                .validate_connection_work_revision(&connection_revision)
                .is_ok()
        );
    }
    assert_eq!(source._sessions.work_revision().unwrap(), session_revision);
    assert_eq!(observe_workers(), workers_before);
    assert_eq!(
        source
            .service
            .required_session_work_for_test(&source._sessions, &ProjectionCancellationToken::new())
            .unwrap(),
        sessions_before
    );
    let storage = mounted.fixture.storage.clone();
    let home = source.home.clone();
    let after = home_support::join(
        home_support::worker(move || {
            (
                storage
                    .input_gate(&home, target, SyndicPointReadLimit::new(65_536).unwrap())
                    .unwrap()
                    .unwrap(),
                storage
                    .thread_execution(&home, target, SyndicPointReadLimit::new(65_536).unwrap())
                    .unwrap(),
            )
        }),
        cx,
    );
    assert_eq!(after.0, gate);
    assert_eq!(after.1, execution_before);
    if let Some(live) = live.as_mut() {
        live.verify(target);
    }
    let storage = mounted.fixture.storage.clone();
    let home = source.home.clone();
    let prior_text = home_support::join(
        home_support::worker(move || {
            storage
                .current_draft_piece_text_demand(
                    &home,
                    prior.claim().thread_id(),
                    syndic_storage::DraftPieceTextDemandV1::Forward(0),
                    4096,
                )
                .unwrap()
                .unwrap()
                .value()
                .bytes()
                .to_vec()
        }),
        cx,
    );
    if mode != CompletionMode::CancelSave {
        assert_eq!(prior_text, b"retained prior draft");
    }
    if let Some(live) = live {
        live.close(&source);
    }
    finish(mounted, source, cx);
}

fn assert_fenced_prior(
    root: &MainWindowShellRoot,
    app: &gpui::App,
    prior: crate::main_window::MainWindowComposerSelectionIdentity,
) {
    let controller = root.controller.as_ref().unwrap();
    let selection = match &controller.content {
        ShellContent::Acquired { selection, .. }
        | ShellContent::Restored { selection, .. }
        | ShellContent::Selected { selection, .. } => *selection,
        _ => panic!("prior selection custody was lost"),
    };
    assert_eq!(selection.window_id(), prior.window_id());
    assert_eq!(selection.claim(), prior.claim());
    let composer = controller
        .composer_mount
        .as_ref()
        .unwrap()
        .read(app)
        .contribution()
        .unwrap();
    let resident = composer.read(app).selection_identity();
    assert_eq!(resident.window_id(), prior.window_id());
    assert_eq!(resident.claim(), prior.claim());
    assert_eq!(
        resident.binding().candidate().session_id(),
        prior.binding().candidate().session_id()
    );
    assert!(!composer.read(app).is_live());
}
