use super::*;
use gpui::EntityInputHandler;
use std::sync::Mutex;

#[derive(Clone, Copy)]
enum Loss {
    EarlyWriter,
    Publication,
    Claim,
    ClaimReplacement,
    UncertainPublication,
}

#[test]
fn native_early_writer_refusal_preserves_original_text_selection_and_history() {
    run(Loss::EarlyWriter);
}

#[test]
fn native_final_writer_refuses_publication_loss_without_accepting_or_clearing() {
    run(Loss::Publication);
}

#[test]
fn native_final_writer_refuses_original_claim_loss_without_accepting_or_clearing() {
    run(Loss::Claim);
}

#[test]
fn native_final_writer_refuses_replaced_claim_on_same_window_and_thread() {
    run(Loss::ClaimReplacement);
}

#[test]
fn native_indeterminate_acceptance_settles_after_actual_weak_publication_loss() {
    run(Loss::UncertainPublication);
}

fn run(loss: Loss) {
    let faults = beryl_home_store::test_faults::FaultController::new();
    let fixture_faults = faults.clone();
    let (fixture, prepared, acquisition, service, mut runtime) = support::worker(move || {
        let mut fixture = syndic::Fixture::with_faults(199, fixture_faults);
        eprintln!(
            "native submission writer fixture: {}",
            fixture.home_path().display()
        );
        let theme = appearance::system_font_appearance(&fixture.state);
        let runtime::Prepared {
            shell: prepared,
            acquisition,
            composer: service,
            runtime,
        } = runtime::prepare(&mut fixture, theme, false);
        (fixture, prepared, acquisition, service, runtime)
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            gpui_text_input::ensure_text_input_bindings(app);
            let owner = beryl_app::theme_runtime::GpuiAppearanceWindowSet::new(
                prepared.appearance().clone(),
                std::num::NonZeroUsize::new(4).unwrap(),
                app,
            );
            let mut shell = GpuiMainWindowShellHost::new(app, owner)
                .construct_hidden(prepared)
                .unwrap_or_else(|_| panic!("native submission writer shell"));
            let window = shell.window();
            let publication = Arc::new(Mutex::new(Some(Arc::new(()))));
            let lifetime = Arc::downgrade(publication.lock().unwrap().as_ref().unwrap());
            window
                .update(app, |root, window, app| {
                    root.test_mount_exact_status_worker(
                        fixture.store.exact_stop_worker(),
                        lifetime,
                        fixture.state.session(),
                        window,
                        app,
                    )
                })
                .unwrap();
            app.spawn(async move |cx| {
                let deadline = Instant::now() + Duration::from_secs(15);
                while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                    assert!(Instant::now() < deadline, "native writer shell not ready");
                    window.update(cx, |_, window, _| window.refresh()).unwrap();
                    native::pump(cx).await;
                }
                cx.update(|app| shell.publish(app)).unwrap().unwrap();
                window
                    .update(cx, |_, window, _| window.minimize_window())
                    .unwrap();
                let (fixture, runtime) = join(support::worker(move || {
                    runtime.activate(&fixture);
                    (fixture, runtime)
                }), cx).await;
                submission::wait_availability(window, Ok(()), cx).await;
                let editor = window
                    .read_with(cx, |root, app| root.test_submission_composer(app).unwrap())
                    .unwrap();
                let input = editor
                    .read_with(cx, |editor, _| editor.gpui_input())
                    .unwrap();
                window
                    .update(cx, |_, window, app| {
                        input.update(app, |input, app| {
                            input.focus(window);
                            input.replace_and_mark_text_in_range(
                                None,
                                "retain writer draft",
                                None,
                                window,
                                app,
                            );
                        });
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                while editor
                    .read_with(cx, |editor, _| {
                        editor
                            .selection_identity()
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes()
                    })
                    .unwrap()
                    != 19
                {
                    assert!(Instant::now() < deadline, "native writer edit not settled");
                    native::pump(cx).await;
                }
                submission::wait_availability(window, Ok(()), cx).await;
                if matches!(loss, Loss::EarlyWriter) {
                    window.update(cx, |root, window, app| {
                        root.test_submission_mount().unwrap().update(app, |mount, app| {
                            mount.publish_autosave_interval(
                                1,
                                beryl_app::composer_host::ComposerHostAutosaveInterval::new(5).unwrap(),
                                window,
                                app,
                            )
                        }).unwrap();
                    }).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(10);
                    loop {
                        let saved = !service.test_with_selected_host(|host| host.is_dirty());
                        let synchronized = editor.read_with(cx, |editor, _| editor.selection_identity()).unwrap()
                            == service.selected_identity().unwrap();
                        let interactive = input.read_with(cx, |input, _| input.is_surface_current_and_interactive()).unwrap();
                        if saved && synchronized && interactive {
                            break;
                        }
                        assert!(Instant::now() < deadline, "early refusal baseline did not autosave and settle");
                        native::pump(cx).await;
                    }
                    submission::wait_availability(window, Ok(()), cx).await;
                }
                let retained = service.selected_identity().unwrap();
                let retained_selection = input
                    .read_with(cx, |input, _| {
                        let surface = input.surface().unwrap();
                        (surface.selection(), surface.caret(), input.history_frontier())
                    })
                    .unwrap();
                let early_refusal = Arc::new(AtomicBool::new(false));
                let early_snapshot = Arc::new(Mutex::new(None));
                let reached_writer = Arc::new(AtomicBool::new(false));
                let reached = reached_writer.clone();
                let writer_revision = Arc::new(Mutex::new(None));
                let revision_witness = writer_revision.clone();
                let retire = publication.clone();
                let settled_loss = publication.clone();
                let installed = reached_writer.clone();
                let installed_revision = writer_revision.clone();
                let acceptance_witness = Arc::new(Mutex::new(None));
                let witness = acceptance_witness.clone();
                let storage = fixture.storage.clone();
                let thread = fixture.thread;
                let session = fixture.state.session();
                service.test_with_selected_host(|host| {
                    if matches!(loss, Loss::EarlyWriter) {
                        let revision_witness = writer_revision.clone();
                        let snapshot = early_snapshot.clone();
                        host.test_arm_submission_materializer_before_execute_fault(move |home, storage| {
                            let current = storage.current_draft(home, thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                            *snapshot.lock().unwrap() = Some(current);
                            let mut command = HomeCommand::new(home.home_revision().unwrap());
                            command.add(storage.create_thread(
                                storage.revision(home).unwrap(),
                                syndic_storage::CreateThread::ordinary(
                                    SyndicThreadId::from_bytes([231; 16]),
                                    SyndicDraftId::from_bytes([232; 16]),
                                    syndic::execution_binding(),
                                    SyndicTimestamp::from_unix_millis(2),
                                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                                ),
                            )).unwrap();
                            assert!(matches!(home.execute(command), CommandOutcome::Committed { later_failure: None, .. }));
                            *revision_witness.lock().unwrap() = Some(home.home_revision().unwrap());
                        });
                        let refused = early_refusal.clone();
                        let snapshot = early_snapshot.clone();
                        let original_storage = storage.clone();
                        host.test_arm_submission_materializer_after_execute_fault(move |home, outcome| {
                            assert!(matches!(outcome, CommandOutcome::NotCommitted { evidence: beryl_home_store::CommandError::Conflict { .. } }), "original early writer outcome: {outcome:?}");
                            let current = original_storage.current_draft(home, thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                            assert_eq!(Some(current), *snapshot.lock().unwrap());
                            refused.store(true, Ordering::Release);
                        });
                    }
                    if matches!(loss, Loss::UncertainPublication) {
                        host.test_arm_submission_after_indeterminate_fault(move |home| {
                            assert_eq!(home.pending_reconciliations().len(), 1);
                            let gate = storage.input_gate(home, thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                            assert!(matches!(gate.state(), syndic_storage::InputGateState::PendingTurn(_)));
                            *witness.lock().unwrap() = Some(gate);
                            *installed_revision.lock().unwrap() = Some(home.home_revision().unwrap());
                            settled_loss.lock().unwrap().take();
                            installed.store(true, Ordering::Release);
                        });
                    }
                    host.test_arm_submission_before_execute_fault(move |home, _| {
                        if !matches!(loss, Loss::UncertainPublication) {
                            reached.store(true, Ordering::Release);
                        }
                        match loss {
                            Loss::EarlyWriter => panic!("early writer refusal reached final acceptance"),
                            Loss::UncertainPublication => {
                                faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
                            }
                            Loss::Publication => {
                                retire.lock().unwrap().take();
                            }
                            Loss::Claim | Loss::ClaimReplacement => {
                                let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
                                let record = bootstrap
                                    .windows()
                                    .iter()
                                    .find(|record| record.window_id() == retained.window_id())
                                    .unwrap();
                                let target = record.remembered_target().unwrap();
                                let placement = record.placement().clone();
                                let mut command = HomeCommand::new(home.home_revision().unwrap());
                                command
                                    .add(session.remove_window(
                                        session.revision(home).unwrap(),
                                        beryl_state::RemoveSessionWindow::new(
                                            bootstrap.header().revision(),
                                            record.window_id(),
                                            record.revision(),
                                            record.selected_thread(),
                                        ),
                                    ))
                                    .unwrap();
                                assert!(matches!(
                                    home.execute(command),
                                    CommandOutcome::Committed {
                                        later_failure: None,
                                        ..
                                    }
                                ));
                                if matches!(loss, Loss::ClaimReplacement) {
                                    let bootstrap =
                                        session.minimal_bootstrap(home).unwrap().unwrap();
                                    let mut command =
                                        HomeCommand::new(home.home_revision().unwrap());
                                    command
                                        .add(session.create_claimed_window(
                                            session.revision(home).unwrap(),
                                            beryl_state::CreateClaimedWindow::new(
                                                bootstrap.header().revision(),
                                                retained.window_id(),
                                                target,
                                                retained.claim().thread_id(),
                                                placement,
                                            ),
                                        ))
                                        .unwrap();
                                    assert!(matches!(
                                        home.execute(command),
                                        CommandOutcome::Committed {
                                            later_failure: None,
                                            ..
                                        }
                                    ));
                                    let source = session
                                        .thread_claim_catalog_source(
                                            home,
                                            retained.claim().thread_id(),
                                        )
                                        .unwrap();
                                    let replacement = source.claim().unwrap();
                                    assert_eq!(replacement.window_id(), retained.window_id());
                                    assert_eq!(
                                        replacement.thread_id(),
                                        retained.claim().thread_id()
                                    );
                                    assert_ne!(
                                        replacement.generation(),
                                        retained.claim().generation()
                                    );
                                }
                            }
                        }
                        *revision_witness.lock().unwrap() = Some(home.home_revision().unwrap());
                    });
                });
                cx.update_window(window.into(), |_, window, app| {
                    window.dispatch_input_for_test(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("enter").unwrap(),
                            is_held: false,
                        }),
                        app,
                    );
                })
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                let desired = if matches!(loss, Loss::UncertainPublication) {
                    MainWindowConversationComposerSubmissionStatus::Idle
                } else {
                    MainWindowConversationComposerSubmissionStatus::NotCommitted
                };
                loop {
                    let status = window
                        .read_with(cx, |root, app| {
                            root.test_submission_mount()
                                .unwrap()
                                .read(app)
                                .submission_status()
                        })
                        .unwrap();
                    let reached = if matches!(loss, Loss::EarlyWriter) {
                        early_refusal.load(Ordering::Acquire)
                    } else {
                        reached_writer.load(Ordering::Acquire)
                    };
                    if status == desired && reached {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native writer did not settle: {status:?}"
                    );
                    native::pump(cx).await;
                }
                assert_eq!(reached_writer.load(Ordering::Acquire), !matches!(loss, Loss::EarlyWriter));
                if matches!(loss, Loss::EarlyWriter) {
                    assert!(early_refusal.load(Ordering::Acquire));
                    assert_eq!(service.selected_identity().unwrap(), retained);
                    assert_eq!(input.read_with(cx, |input, _| {
                        let surface = input.surface().unwrap();
                        (surface.selection(), surface.caret(), input.history_frontier())
                    }).unwrap(), retained_selection);
                    let page = fixture.storage.candidate_draft_piece_text_demand(
                        &fixture.home(), retained.binding().candidate(),
                        syndic_storage::DraftPieceTextDemandV1::Forward(0), 65_536,
                    ).unwrap();
                    assert_eq!(page.value().bytes(), b"retain writer draft");
                    assert!(fixture.home().pending_reconciliations().is_empty());
                    let diagnostics = service.test_submission_diagnostics().unwrap().selected_submission().unwrap();
                    assert_eq!(diagnostics.retained_roots(), 0);
                    assert_eq!(diagnostics.retained_materializations(), 0);
                    assert!(!diagnostics.command_attempted());
                }
                if matches!(loss, Loss::UncertainPublication) {
                    assert!(publication.lock().unwrap().is_none());
                    let current = service.selected_identity().unwrap();
                    assert_ne!(current.binding().candidate().draft_id(), retained.binding().candidate().draft_id());
                    assert_eq!(current.binding().logical_extent().logical_utf8_bytes(), 0);
                    let home = fixture.home();
                    assert!(home.pending_reconciliations().is_empty());
                    let gate = fixture.storage.input_gate(&home, fixture.thread, syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()).unwrap().unwrap();
                    assert_eq!(gate, acceptance_witness.lock().unwrap().take().unwrap());
                    assert!(!service.test_submission_diagnostics().unwrap().selected_submission().unwrap().pending());
                    window.read_with(cx, |root, app| {
                        let diagnostics = root.test_submission_mount().unwrap().read(app).test_submission_diagnostics();
                        assert!(!diagnostics.active_ticket() && !diagnostics.active_task());
                    }).unwrap();
                } else {
                assert_eq!(
                    fixture.home().home_revision().unwrap(),
                    writer_revision.lock().unwrap().unwrap()
                );
                {
                    let current = service.selected_identity().unwrap();
                    assert_eq!(current.window_id(), retained.window_id());
                    assert_eq!(current.claim(), retained.claim());
                    assert_eq!(
                        current.binding().candidate().draft_id(),
                        retained.binding().candidate().draft_id()
                    );
                    assert_eq!(
                        current.binding().candidate().root(),
                        retained.binding().candidate().root()
                    );
                    assert_eq!(
                        current.binding().logical_extent(),
                        retained.binding().logical_extent()
                    );
                    assert_eq!(
                        editor
                            .read_with(cx, |editor, _| editor.submission_disabled_outcome())
                            .unwrap(),
                        Some(SubmissionUnavailable::AuthorityUnavailable)
                    );
                    cx.update_window(window.into(), |_, window, app| {
                        window.draw_and_present_for_test(app)
                    })
                    .unwrap();
                    let (reason, bounds) = editor
                        .read_with(cx, |editor, _| editor.test_submission_outcome_rendered())
                        .unwrap()
                        .expect("native final writer refusal painted");
                    assert_eq!(reason, SubmissionUnavailable::AuthorityUnavailable);
                    assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
                    while !input
                        .read_with(cx, |input, _| input.is_surface_current_and_interactive())
                        .unwrap()
                    {
                        assert!(
                            Instant::now() < deadline,
                            "original writer input did not settle after refusal: owner={:?}, realization={:?}",
                            editor.read_with(cx, |editor, _| (
                                editor.test_has_active_flight(),
                                editor.last_error().is_some(),
                            )),
                            input.read_with(cx, |input, _| input.realization_diagnostics()),
                        );
                        native::pump(cx).await;
                        cx.update_window(window.into(), |_, window, app| {
                            window.draw_and_present_for_test(app)
                        })
                        .unwrap();
                    }
                    assert!(
                        input
                            .read_with(cx, |input, _| input.is_surface_current_and_interactive())
                            .unwrap()
                    );
                    assert!(
                        !service
                            .test_submission_diagnostics()
                            .unwrap()
                            .selected_submission()
                            .unwrap()
                            .pending()
                    );
                    let gate = fixture
                        .storage
                        .input_gate(
                            &fixture.home(),
                            fixture.thread,
                            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
                        )
                        .unwrap()
                        .unwrap();
                    assert!(matches!(gate.state(), syndic_storage::InputGateState::Idle));
                    assert_eq!(gate.live_next_turn_count(), 0);
                    assert_eq!(gate.live_steering_count(), 0);
                    window
                        .read_with(cx, |root, app| {
                            let diagnostics = root
                                .test_submission_mount()
                                .unwrap()
                                .read(app)
                                .test_submission_diagnostics();
                            assert!(
                                !diagnostics.active_ticket()
                                    && !diagnostics.active_task()
                                    && !diagnostics.successor()
                            );
                        })
                        .unwrap();
                }
                }
                if matches!(loss, Loss::EarlyWriter) {
                    window.update(cx, |_, window, app| {
                        input.update(app, |input, app| {
                            input.replace_and_mark_text_in_range(None, "!", None, window, app);
                        });
                    }).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while service.selected_identity().unwrap().binding().logical_extent().logical_utf8_bytes() != 20 {
                        assert!(Instant::now() < deadline, "original input not writable after early refusal");
                        native::pump(cx).await;
                    }
                    let current = service.selected_identity().unwrap();
                    let page = fixture.storage.candidate_draft_piece_text_demand(
                        &fixture.home(), current.binding().candidate(),
                        syndic_storage::DraftPieceTextDemandV1::Forward(0), 65_536,
                    ).unwrap();
                    assert_eq!(page.value().bytes(), b"retain writer draft!");
                }
                window
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                drop(shell);
                drop(service);
                join(
                    support::worker(move || {
                        runtime.close();
                        drop(acquisition);
                        let (directory, service) = fixture.into_service();
                        assert!(matches!(
                            service.close().unwrap(),
                            ProjectionConnectionServiceCloseOutcome::Closed
                        ));
                        drop(directory);
                    }),
                    cx,
                )
                .await;
                result.store(true, Ordering::Release);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        });
    assert!(completed.load(Ordering::Acquire));
}
