use super::*;
use beryl_app::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
use gpui::{AppContext, Entity, Focusable, TestAppContext, WindowHandle};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Mounted {
    fixture: Fixture,
    services: Arc<MainWindowCreationServices>,
    owner: Entity<MainWindowCreationOwner>,
    source: WindowHandle<MainWindowShellRoot>,
    appearance: Entity<GpuiAppearanceWindowSet>,
}

fn mount(
    cx: &mut TestAppContext,
    seed: u8,
    configure: impl FnOnce(&mut MainWindowCreationServices, &Fixture) + Send + 'static,
) -> Mounted {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, services, appearance, prepared) = home_support::join(
        home_support::worker(move || {
            let fixture = Fixture::new(seed);
            let (mut services, appearance) = services(&fixture);
            configure(Arc::get_mut(&mut services).unwrap(), &fixture);
            let mut initial = fixture.begin(seed.wrapping_add(1));
            assert_eq!(
                initial.advance(&CommandCancellation::new()).unwrap(),
                MainWindowInitialComposerProgress::Activated
            );
            let prepared = initial
                .prepare(&mut config)
                .unwrap_or_else(|failure| panic!("{}", failure.error));
            let prepared = prepared
                .into_shell(
                    Box::new(config),
                    services.marker_seals.clone(),
                    MainWindowComposerSubmissionRequestSource::new(services.turn_start_requirement),
                    appearance.clone(),
                )
                .unwrap_or_else(|failure| panic!("{}", failure.error));
            (fixture, services, appearance, prepared)
        }),
        cx,
    );
    let appearance_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app)
    });
    let owner = cx.update(|app| {
        MainWindowCreationOwner::install(
            services.clone(),
            appearance_owner.clone(),
            MainWindowCreationGate::Ready,
            app,
        )
        .unwrap()
    });
    let mut shell = cx
        .update(|app| {
            GpuiMainWindowShellHost::new(app, appearance_owner.clone()).construct_hidden(prepared)
        })
        .unwrap_or_else(|_| panic!("source hidden shell"));
    cx.update(|app| shell.attach_creation(owner.clone(), app));
    drive_until(cx, |cx| cx.update(|app| shell.ready_to_publish(app)));
    cx.update(|app| shell.publish(app).unwrap());
    let source = shell.window();
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("source published"));
    Mounted {
        fixture,
        services,
        owner,
        source,
        appearance: appearance_owner,
    }
}

fn finish(mounted: Mounted, cx: &mut TestAppContext) {
    let Mounted {
        fixture,
        services,
        owner,
        appearance,
        ..
    } = mounted;
    cx.update(|app| owner.update(app, |owner, cx| owner.fence(cx)));
    drive_until(cx, |cx| {
        owner.read_with(cx, |owner, _| owner.pending_count()) == 0
    });
    for window in cx.windows() {
        cx.update(|app| {
            app.update_window(window, |_, window, _| window.remove_window())
                .unwrap()
        });
    }
    draw(cx);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cx.update(MainWindowCreationOwner::test_uninstall);
    drop((owner, services, appearance));
    for _ in 0..32 {
        draw(cx);
    }
    home_support::join(home_support::worker(move || cleanup(fixture)), cx);
}

fn activate(mounted: &Mounted, cx: &mut TestAppContext) -> WindowId {
    cx.update(|app| {
        let root = mounted.source.entity(app).unwrap();
        mounted
            .owner
            .update(app, |owner, cx| owner.activate(&root, cx))
            .unwrap()
    })
}

fn selected(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &TestAppContext,
) -> MainWindowComposerSelectionIdentity {
    window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap()
}

#[gpui::test]
fn pointer_and_accelerator_create_independent_windows_and_preserve_invoker(
    cx: &mut TestAppContext,
) {
    let mounted = mount(cx, 71, |_, _| {});
    let original = selected(mounted.source, cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.source.into(), cx);
    let button = visual.debug_bounds("main-window-new-window").unwrap();
    visual.simulate_click(button.center(), gpui::Modifiers::none());
    drive_until(cx, |cx| {
        cx.windows().len() == 2
            && mounted
                .owner
                .read_with(cx, |owner, _| owner.pending_count())
                == 0
    });
    let second = cx
        .windows()
        .into_iter()
        .find(|window| *window != mounted.source.into())
        .unwrap()
        .downcast::<MainWindowShellRoot>()
        .unwrap();
    assert!(cx.window_visibility(second.into()).is_visible);
    let new = selected(second, cx);
    assert_ne!(original.window_id(), new.window_id());
    assert_ne!(original.claim().thread_id(), new.claim().thread_id());
    assert_eq!(
        second
            .read_with(cx, |root, _| root
                .controller()
                .unwrap()
                .acquisition()
                .target())
            .unwrap(),
        target(&mounted.fixture)
    );
    assert_eq!(selected(mounted.source, cx), original);
    let (first_input, second_input) = cx.update(|app| {
        let input = |window: WindowHandle<MainWindowShellRoot>| {
            window
                .read_with(app, |root, app| {
                    root.controller()
                        .unwrap()
                        .composer_mount()
                        .unwrap()
                        .read(app)
                        .contribution()
                        .unwrap()
                        .read(app)
                        .gpui_input()
                })
                .unwrap()
        };
        (input(mounted.source), input(second))
    });
    assert_ne!(first_input.entity_id(), second_input.entity_id());
    let (first_focus, second_focus) = cx.update(|app| {
        (
            first_input.read(app).focus_handle(app),
            second_input.read(app).focus_handle(app),
        )
    });
    assert_ne!(first_focus, second_focus);
    mounted
        .source
        .update(cx, |_, window, app| {
            first_input.update(app, |input, _| input.focus(window))
        })
        .unwrap();
    cx.simulate_keystrokes(mounted.source.into(), "ctrl-shift-n");
    drive_until(cx, |cx| {
        cx.windows().len() == 3
            && mounted
                .owner
                .read_with(cx, |owner, _| owner.pending_count())
                == 0
    });
    assert_eq!(selected(mounted.source, cx), original);
    second
        .update(cx, |_, window, _| second_focus.focus(window))
        .unwrap();
    assert!(
        mounted
            .source
            .update(cx, |_, window, _| first_focus.is_focused(window))
            .unwrap()
    );
    drop((visual, first_input, second_input));
    finish(mounted, cx);
}

#[gpui::test]
fn native_publication_failure_abandons_only_new_hidden_window(cx: &mut TestAppContext) {
    let mounted = mount(cx, 111, |_, _| {});
    let original = selected(mounted.source, cx);
    cx.update(|app| {
        mounted
            .owner
            .update(app, |owner, _| owner.test_hold_next_publication())
    });
    let id = activate(&mounted, cx);
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.test_hidden_window(id).is_some())
    });
    let hidden = mounted
        .owner
        .read_with(cx, |owner, _| owner.test_hidden_window(id).unwrap());
    assert!(!cx.window_visibility(hidden.into()).is_visible);
    cx.fail_next_window_visibility_change(hidden.into());
    cx.update(|app| {
        mounted
            .owner
            .update(app, |owner, cx| owner.test_release_publication(id, cx))
    });
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
    });
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(selected(mounted.source, cx), original);
    assert_eq!(mounted.fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        mounted
            .appearance
            .read_with(cx, |owner, _| owner.target().snapshot().count),
        1
    );
    assert!(
        mounted
            .owner
            .read_with(cx, |owner, _| owner.last_error().is_some())
    );
    finish(mounted, cx);
}

#[gpui::test]
fn preparation_deadline_retires_hidden_window_without_publishing_late(cx: &mut TestAppContext) {
    let mounted = mount(cx, 121, |_, _| {});
    let original = selected(mounted.source, cx);
    cx.update(|app| {
        mounted
            .owner
            .update(app, |owner, _| owner.test_hold_next_publication())
    });
    let id = activate(&mounted, cx);
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.test_hidden_window(id).is_some())
    });
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(31));
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
    });
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(selected(mounted.source, cx), original);
    assert_eq!(mounted.fixture.process.main_window_occupancy(), 1);
    finish(mounted, cx);
}

#[gpui::test]
fn repeated_publication_and_test_os_release_return_registry_and_appearance_to_baseline(
    cx: &mut TestAppContext,
) {
    let mounted = mount(cx, 131, |_, _| {});
    for _ in 0..4 {
        activate(&mounted, cx);
        drive_until(cx, |cx| {
            cx.windows().len() == 2
                && mounted
                    .owner
                    .read_with(cx, |owner, _| owner.pending_count())
                    == 0
        });
        let additional = cx
            .windows()
            .into_iter()
            .find(|window| *window != mounted.source.into())
            .unwrap();
        assert!(cx.window_visibility(additional).is_visible);
        assert_eq!(mounted.fixture.process.main_window_occupancy(), 2);
        assert_eq!(
            mounted
                .appearance
                .read_with(cx, |owner, _| owner.target().snapshot().count),
            2
        );
        cx.update(|app| {
            app.update_window(additional, |_, window, _| window.remove_window())
                .unwrap()
        });
        draw(cx);
        assert_eq!(mounted.fixture.process.main_window_occupancy(), 1);
        assert_eq!(
            mounted
                .appearance
                .read_with(cx, |owner, _| owner.target().snapshot().count),
            1
        );
    }
    finish(mounted, cx);
}

#[gpui::test]
fn disabled_gates_capacity_and_duplicate_admission_have_no_durable_effect(cx: &mut TestAppContext) {
    let mounted = mount(cx, 81, |_, _| {});
    for gate in [
        MainWindowCreationGate::NoRuntimes,
        MainWindowCreationGate::ExitWaiting,
    ] {
        cx.update(|app| {
            mounted
                .owner
                .update(app, |owner, cx| owner.set_gate(gate.clone(), cx))
        });
        draw(cx);
        let reason = mounted
            .source
            .read_with(cx, |root, app| root.new_window_disabled_reason(app))
            .unwrap()
            .unwrap();
        assert!(if matches!(gate, MainWindowCreationGate::NoRuntimes) {
            reason.contains("ellipsis")
        } else {
            reason.contains("Exit")
        });
        let revision = mounted.fixture.store.home_revision().unwrap();
        cx.dispatch_action(mounted.source.into(), NewWindow);
        draw(cx);
        assert_eq!(mounted.fixture.store.home_revision().unwrap(), revision);
        assert_eq!(
            mounted
                .owner
                .read_with(cx, |owner, _| owner.pending_count()),
            0
        );
    }
    cx.update(|app| {
        mounted.owner.update(app, |owner, cx| {
            owner.set_gate(MainWindowCreationGate::Ready, cx)
        })
    });
    let reservations = (0..255u16)
        .map(|value| {
            let mut bytes = [0; 16];
            bytes[..2].copy_from_slice(&value.to_le_bytes());
            mounted
                .fixture
                .process
                .reserve_main_window(WindowId::from_bytes(bytes))
                .unwrap()
        })
        .collect::<Vec<_>>();
    draw(cx);
    assert!(
        mounted
            .source
            .read_with(cx, |root, app| root.new_window_disabled_reason(app))
            .unwrap()
            .unwrap()
            .contains("256")
    );
    let revision = mounted.fixture.store.home_revision().unwrap();
    cx.dispatch_action(mounted.source.into(), NewWindow);
    draw(cx);
    assert_eq!(mounted.fixture.store.home_revision().unwrap(), revision);
    drop(reservations);
    let id = activate(&mounted, cx);
    let refused = cx.update(|app| {
        let root = mounted.source.entity(app).unwrap();
        mounted
            .owner
            .update(app, |owner, cx| owner.activate(&root, cx))
    });
    assert!(refused.is_err());
    assert_eq!(
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count()),
        1
    );
    cx.update(|app| mounted.owner.update(app, |owner, cx| owner.cancel(id, cx)));
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
    });
    assert_eq!(mounted.fixture.process.main_window_occupancy(), 1);
    finish(mounted, cx);
}

#[gpui::test]
fn candidate_open_progress_survives_more_than_four_real_revision_conflicts(
    cx: &mut TestAppContext,
) {
    let visits = Arc::new(AtomicUsize::new(0));
    let seen = visits.clone();
    let mounted = mount(cx, 91, move |services, fixture| {
        let state = fixture.state.clone();
        let source = WindowId::from_bytes([92; 16]);
        services.test_before_initial_advance = Some(Arc::new(move |initial| {
            let visit = seen.fetch_add(1, Ordering::SeqCst);
            if visit >= 6 {
                return;
            }
            let state = state.clone();
            initial.test_arm_before_open(move |store, _| {
                let session = state.session();
                let bootstrap = session.minimal_bootstrap(store).unwrap().unwrap();
                let record = bootstrap
                    .windows()
                    .iter()
                    .find(|record| record.window_id() == source)
                    .unwrap();
                let mut command =
                    beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
                command
                    .add(session.update_placement(
                        session.revision(store).unwrap(),
                        beryl_state::UpdateWindowPlacement::new(
                            bootstrap.header().revision(),
                            source,
                            record.revision(),
                            WindowPlacement::new(
                                WindowBounds::new(30 + visit as i32, 20, 900, 700).unwrap(),
                                WindowDisplayState::Normal,
                                None,
                                None,
                            ),
                        ),
                    ))
                    .unwrap();
                let outcome = store.execute(command);
                assert!(
                    matches!(outcome, beryl_home_store::CommandOutcome::Committed { .. }),
                    "source contention mutation: {outcome:?}"
                );
            });
        }));
    });
    let original = selected(mounted.source, cx);
    activate(&mounted, cx);
    for _ in 0..32 {
        draw(cx);
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(850));
        if mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
        {
            break;
        }
    }
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
    });
    assert!(visits.load(Ordering::SeqCst) >= 7);
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(selected(mounted.source, cx), original);
    finish(mounted, cx);
}

#[gpui::test]
fn source_release_during_worker_preparation_abandons_late_completion(cx: &mut TestAppContext) {
    let mounted = mount(cx, 101, |_, _| {});
    cx.update(|app| {
        mounted.owner.update(app, |owner, _| {
            owner.test_delay_next_completion(std::time::Duration::from_secs(1))
        })
    });
    let id = activate(&mounted, cx);
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.test_completion_is_waiting(id))
    });
    assert_eq!(mounted.fixture.process.main_window_occupancy(), 2);
    mounted
        .source
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    draw(cx);
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(1));
    drive_until(cx, |cx| {
        mounted
            .owner
            .read_with(cx, |owner, _| owner.pending_count())
            == 0
    });
    assert!(cx.windows().is_empty());
    assert_eq!(mounted.fixture.process.main_window_occupancy(), 0);
    finish(mounted, cx);
}
