use super::*;

fn first_page_configuration(fixture: &support::Fixture) -> Configuration {
    let mut configuration = support::configuration(fixture);
    let activation_source = configuration
        .startup
        .windows
        .restored_activation_source
        .clone();
    configuration.startup.windows.restored_activation_source = Arc::new(move |record| {
        let (request, operation) = activation_source(record)?;
        Ok((
            crate::composer_host::ComposerHostActivationRequest::new(
                request.thread_id(),
                request.session_id(),
                request.operation_id(),
                request.presentation_generation(),
                request.restoration().cloned(),
                Vec::new().into_boxed_slice(),
            ),
            operation,
        ))
    });
    configuration
}

#[test]
fn native_complete_restore_waits_for_real_draft_loading_and_editor_notification() {
    let fixture = support::selected(2, true, false);
    let activation_holds = fixture
        .members
        .iter()
        .map(|(_, thread, _)| ProducerHold::install(Producer::DraftActivation, *thread.as_bytes()))
        .collect::<Vec<_>>();
    let activation_gates = activation_holds
        .iter()
        .map(ProducerHold::gate)
        .collect::<Vec<_>>();
    let page_holds = fixture
        .members
        .iter()
        .map(|(_, thread, _)| ProducerHold::install(Producer::DraftPage, *thread.as_bytes()))
        .collect::<Vec<_>>();
    let page_gates = page_holds
        .iter()
        .map(ProducerHold::gate)
        .collect::<Vec<_>>();
    let catalog_hold =
        ProducerHold::install(Producer::CatalogCertification, *fixture.home_id.as_bytes());
    let catalog = catalog_hold.gate();
    let runtime_hold = ProducerHold::install(Producer::RuntimeLaunch, *fixture.runtime.as_bytes());
    let runtime = runtime_hold.gate();
    let waiting = Arc::new(AtomicUsize::new(0));
    let ready = Arc::new(AtomicUsize::new(0));
    let waiting_observer = waiting.clone();
    let ready_observer = ready.clone();
    let mut configuration = first_page_configuration(&fixture);
    configuration.startup.native_hook = Some(Arc::new(move |native| {
        let waiting = waiting_observer.clone();
        let ready = ready_observer.clone();
        native.test_observe_readiness(move |current| {
            if current {
                ready.fetch_add(1, Ordering::SeqCst);
            } else {
                waiting.fetch_add(1, Ordering::SeqCst);
            }
        });
    }));
    let expected = fixture.members.clone();
    let checked = Rc::new(Cell::new(false));
    let receipt = checked.clone();
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            for gate in &activation_gates {
                support::wait(
                    cx,
                    |_| gate.entered() > 0,
                    "real selected activation never reached loading gate",
                )
                .await;
                cx.update(|app| {
                    assert!(!running(app));
                    assert!(support::shells(app).is_empty());
                })
                .unwrap();
                assert_eq!(gate.returned(), 0);
                gate.release();
            }
            support::wait(
                cx,
                |app| {
                    assert!(!running(app));
                    page_gates.iter().all(|gate| gate.entered() > 0)
                        && waiting.load(Ordering::SeqCst) > 0
                        && support::shells(app).len() == expected.len()
                },
                "real native first-editor page loading and readiness observation never overlapped",
            )
            .await;
            let native_ids = cx
                .update(|app| {
                    support::shells(app)
                        .into_iter()
                        .map(|window| {
                            assert!(window.read_with(app, |root, app| {
                                !root.test_submission_composer(app).unwrap().read(app).selected_first_presentable(app)
                            }).unwrap());
                            assert!(
                                window
                                    .read_with(app, |root, _| root.startup_interaction_gated())
                                    .unwrap()
                            );
                            let handle = support::native(window, app);
                            assert!(
                                !unsafe {
                                    windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(handle)
                                }
                                .as_bool()
                            );
                            (window.window_id(), identity(window, app), handle.0 as usize)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap();
            assert_eq!(ready.load(Ordering::SeqCst), 0);
            assert_eq!(runtime.entered(), 0, "unpublished selected views do not launch a backend");
            page_gates[0].release();
            support::wait(
                cx,
                |_| page_gates[0].returned() > 0,
                "released real first editor work did not return",
            )
            .await;
            support::wait(cx, |app| {
                assert!(!running(app));
                assert_eq!(runtime.entered(), 0);
                support::shells(app).iter().any(|window| window.read_with(app, |root, app| {
                    root.diagnostic_selected_thread(app) == Some(expected[0].1)
                        && root.test_submission_composer(app).is_some_and(|composer| composer.read(app).selected_first_presentable(app))
                }).unwrap())
            }, "released first member did not become independently presentable while its peer Page remained held").await;
            cx.update(|app| {
                assert!(!running(app));
                for window in support::shells(app) {
                    window.read_with(app, |root, app| {
                        if root.diagnostic_selected_thread(app) == Some(expected[1].1) {
                            assert!(!root.test_submission_composer(app).unwrap().read(app).selected_first_presentable(app));
                        }
                    }).unwrap();
                    assert!(
                        window
                            .read_with(app, |root, _| root.startup_interaction_gated())
                            .unwrap()
                    );
                    assert!(
                        !unsafe {
                            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                                support::native(window, app),
                            )
                        }
                        .as_bool()
                    );
                }
            })
            .unwrap();
            assert_eq!(page_gates[1].returned(), 0);
            page_gates[1].release();
            support::wait(
                cx,
                |app| running(app),
                "real readiness notification did not complete native publication",
            )
            .await;
            assert!(ready.load(Ordering::SeqCst) > 0);
            for (window_id, selected, raw) in native_ids {
                let window = cx
                    .update(|app| {
                        let window = support::shells(app)
                            .into_iter()
                            .find(|window| window.window_id() == window_id)
                            .unwrap();
                        assert_eq!(identity(window, app), selected);
                        assert_eq!(support::native(window, app).0 as usize, raw);
                        assert!(
                            unsafe {
                                windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                                    support::native(window, app),
                                )
                            }
                            .as_bool()
                        );
                        window
                    })
                    .unwrap();
                let text = &expected
                    .iter()
                    .find(|(id, _, _)| *id == selected.0)
                    .unwrap()
                    .2;
                let restored_frame = Rc::new(Cell::new(false));
                let handle = cx.update(|app| {
                    support::configure_autosave(window, app, 1);
                    let handle = support::prepare_native_text_entry_at(window, app, 0);
                    let restored_frame = restored_frame.clone();
                    window.update(app, |_, native, _| {
                        native.on_next_frame(move |native, _| {
                            native.on_next_frame(move |_, _| restored_frame.set(true));
                        });
                    }).unwrap();
                    handle
                }).unwrap();
                support::wait(
                    cx,
                    |app| restored_frame.get() && assert_local_editor(window, app, text.len()),
                    "first saved editor never became interactive",
                )
                .await;
                cx.update(|app| {
                    assert_eq!(handle.0 as usize, raw);
                    assert_eq!(support::native(window, app), handle);
                    window.update(app, |root, native, app| {
                        let input = root.test_submission_composer(app).unwrap().read(app).gpui_input();
                        let input = input.read(app);
                        let surface = input.surface().unwrap();
                        assert_eq!(surface.source_caret().byte_offset.get(), 0,
                            "normal insertion stays at its exact original source position");
                        assert_eq!(surface.source_selection().anchor, surface.source_selection().head);
                        assert!(gpui::Focusable::focus_handle(input, app).is_focused(native));
                    }).unwrap();
                    support::post_native_character(handle);
                }).unwrap();
                support::wait_with_failure_context(
                    cx,
                    |app| assert_local_editor(window, app, text.len() + 1),
                    "normal native text input did not settle",
                    |app| native_text_failure_snapshot(window, app, text.len() + 1),
                )
                .await;
                support::wait(
                    cx,
                    |app| assert_saved_editor(window, app, text.len() + 1),
                    "original autosave did not durably publish the native edit",
                )
                .await;
                cx.update(|app| support::minimize(window, app)).unwrap();
            }
            catalog.release();
            runtime.release();
            support::wait(cx, |app| catalog.returned() > 0 && runtime.returned() > 0 && support::shells(app).iter().all(|window| window.read_with(app, |root, app| {
                root.test_progressive_readiness(app).0 && root.test_runtime_failure_snapshot().is_some()
            }).unwrap()), "released original catalog and runtime producers did not settle before Exit").await;
            receipt.set(true);
            cx.update(exit).unwrap();
        })
        .detach();
    }));
    run(configuration).unwrap();
    assert!(checked.get());
    support::assert_saved(&fixture, 1);
    drop((activation_holds, page_holds, catalog_hold, runtime_hold));
    fixture.directory.close().unwrap();
}

#[test]
fn native_exit_during_real_first_editor_loading_joins_original_attempt() {
    interrupted_editor(false);
}

#[test]
fn native_saved_source_change_during_real_first_editor_loading_refuses_publication() {
    interrupted_editor(true);
}

fn interrupted_editor(source_change: bool) {
    let fixture = support::selected(1, true, false);
    let page_hold = ProducerHold::install(Producer::DraftPage, *fixture.members[0].1.as_bytes());
    let page = page_hold.gate();
    let runtime_hold = ProducerHold::install(Producer::RuntimeLaunch, *fixture.runtime.as_bytes());
    let checked = Rc::new(Cell::new(false));
    let receipt = checked.clone();
    let mut configuration = first_page_configuration(&fixture);
    let validation = Arc::new(AtomicUsize::new(0));
    let publication = Arc::new(AtomicUsize::new(0));
    let validation_hook = validation.clone();
    let publication_hook = publication.clone();
    configuration.startup.native_hook = Some(Arc::new(move |native| {
        let validation = validation_hook.clone();
        native.test_before_final_validation(move || {
            validation.fetch_add(1, Ordering::SeqCst);
        });
        let publication = publication_hook.clone();
        native.test_before_publication(move |_, _, _| {
            publication.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
    }));
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            support::wait(
                cx,
                |app| page.entered() > 0 && support::shells(app).len() == 1,
                "interruption requires an actual native editor Page producer",
            )
            .await;
            let (window, raw) = cx
                .update(|app| {
                    assert!(!running(app));
                    let window = support::shells(app)[0];
                    assert!(
                        window
                            .read_with(app, |root, app| {
                                !root
                                    .test_submission_composer(app)
                                    .unwrap()
                                    .read(app)
                                    .selected_first_presentable(app)
                            })
                            .unwrap()
                    );
                    assert!(
                        window
                            .read_with(app, |root, _| root.startup_interaction_gated())
                            .unwrap()
                    );
                    let handle = support::native(window, app);
                    assert!(
                        !unsafe {
                            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(handle)
                        }
                        .as_bool()
                    );
                    if !source_change {
                        unsafe {
                            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                                handle,
                                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                                None,
                                None,
                            );
                        }
                        assert!(
                            window.read(app).is_ok(),
                            "reentrant close cannot retire a gated restore member"
                        );
                    }
                    (window, handle.0 as usize)
                })
                .unwrap();
            assert_eq!(page.returned(), 0);
            if source_change {
                let reference = window
                    .read_with(cx, |root, app| {
                        root.test_submission_composer(app)
                            .unwrap()
                            .read(app)
                            .test_startup_home_reference()
                    })
                    .unwrap();
                cx.background_executor()
                    .spawn(async move {
                        support::change_saved_placement(&reference);
                    })
                    .await;
            } else {
                cx.update(exit).unwrap();
            }
            cx.background_executor()
                .timer(Duration::from_millis(30))
                .await;
            cx.update(|app| {
                assert!(!running(app));
                assert_eq!(
                    support::shells(app).len(),
                    1,
                    "original attempt remains owned until outstanding editor work settles"
                );
                assert!(
                    app.windows().into_iter().all(|window| window
                        .downcast::<crate::startup_surface::StartupSurface>()
                        .is_none()),
                    "outstanding cleanup grants no Retry surface"
                );
                assert!(
                    !unsafe {
                        windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                            windows::Win32::Foundation::HWND(raw as *mut _),
                        )
                    }
                    .as_bool()
                );
            })
            .unwrap();
            assert_eq!(page.returned(), 0);
            page.release();
            if source_change {
                support::wait(
                    cx,
                    |app| {
                        assert!(!running(app));
                        app.windows().iter().any(|window| {
                            window
                                .downcast::<crate::startup_surface::StartupSurface>()
                                .is_some()
                        })
                    },
                    "original final validation did not refuse the changed saved member",
                )
                .await;
                cx.update(|app| {
                    assert!(
                        support::shells(app).is_empty(),
                        "settled failure disposes the complete attempted native set"
                    );
                    let surface = app
                        .windows()
                        .into_iter()
                        .find_map(|window| {
                            window.downcast::<crate::startup_surface::StartupSurface>()
                        })
                        .unwrap();
                    assert_eq!(
                        surface.read(app).unwrap().diagnostic_home_state(),
                        "open_failed",
                        "Retry authority follows proven complete cleanup"
                    );
                })
                .unwrap();
                assert!(
                    !unsafe {
                        windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(
                            windows::Win32::Foundation::HWND(raw as *mut _),
                        ))
                    }
                    .as_bool()
                );
                receipt.set(true);
                cx.update(exit).unwrap();
            } else {
                receipt.set(true);
            }
        })
        .detach();
    }));
    assert!(matches!(run(configuration), Err(BootstrapError::Startup)));
    assert!(checked.get());
    assert_eq!(
        validation.load(Ordering::SeqCst),
        usize::from(source_change),
        "source-change refusal reaches original final validation; cancellation does not"
    );
    assert_eq!(
        publication.load(Ordering::SeqCst),
        0,
        "no native member entered publication"
    );
    assert!(
        page_hold.gate().returned() > 0,
        "original loading worker settles under cancellation"
    );
    support::assert_saved(&fixture, 0);
    if source_change {
        support::assert_changed_placement(&fixture);
    }
    drop((page_hold, runtime_hold));
    fixture.directory.close().unwrap();
}
