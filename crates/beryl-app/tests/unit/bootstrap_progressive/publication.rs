use super::*;
use std::{
    fmt::Write,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Publish,
    Exit,
    ChangedSource,
}

#[derive(Debug, PartialEq, Eq)]
struct NativeIdentity {
    window: gpui::WindowId,
    root: gpui::EntityId,
    selected: (beryl_model::WindowId, Option<beryl_model::SyndicThreadId>),
    native: usize,
    composer: gpui::EntityId,
    input: gpui::EntityId,
}

#[derive(Default)]
struct Progress {
    worker_entered: AtomicBool,
    worker_released: AtomicBool,
    validation: AtomicUsize,
    admission: AtomicUsize,
    controlled_validation: AtomicUsize,
    controlled_frame_generation: AtomicU64,
    controlled_pending: AtomicUsize,
    natural_pending: AtomicUsize,
    ready_samples_after_pending: AtomicUsize,
    last_ready_sample: AtomicBool,
    ready_sample_count: AtomicUsize,
    publication: AtomicUsize,
    input_wakes_during_validation: AtomicUsize,
    input_wakes_after_pending: AtomicUsize,
    ready_input_wakes_after_pending: AtomicUsize,
    source_change_entered: AtomicBool,
    source_change_completed: AtomicBool,
    retirement_started: AtomicUsize,
}

fn identities(app: &mut App) -> Vec<NativeIdentity> {
    let mut identities = support::shells(app)
        .into_iter()
        .map(|window| {
            let (composer, input) = window
                .read_with(app, |root, app| {
                    let composer = root.test_submission_composer(app).unwrap();
                    let input = composer.read(app).gpui_input();
                    (composer.entity_id(), input.entity_id())
                })
                .unwrap();
            NativeIdentity {
                window: window.window_id(),
                root: window.entity(app).unwrap().entity_id(),
                selected: identity(window, app),
                native: support::native(window, app).0 as usize,
                composer,
                input,
            }
        })
        .collect::<Vec<_>>();
    identities.sort_by_key(|identity| identity.native);
    identities
}

fn assert_hidden(app: &mut App) {
    assert!(!running(app));
    for window in support::shells(app) {
        assert!(
            window
                .read_with(app, |root, _| root.startup_interaction_gated())
                .unwrap()
        );
        assert!(
            !unsafe {
                windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(support::native(
                    window, app,
                ))
            }
            .as_bool()
        );
    }
}

fn pending_layout(window: WindowHandle<MainWindowShellRoot>, app: &mut App) -> u64 {
    let composer = window
        .read_with(app, |root, app| root.test_submission_composer(app).unwrap())
        .unwrap();
    let input = composer.read(app).gpui_input();
    assert!(composer.read(app).selected_first_presentable(app));
    input.update(app, |input, cx| {
        let layout = input.resident_layout_snapshot();
        let diagnostics = input.realization_diagnostics();
        assert!(diagnostics.frame.remaining <= diagnostics.max_realization_work_per_frame);
        let requests = diagnostics.frame.remaining.checked_add(1).unwrap();
        for _ in 0..requests {
            input
                .set_layout(layout.layout.clone(), layout.style.clone(), cx)
                .unwrap();
        }
        assert_eq!(input.realization_diagnostics().frame.remaining, 0);
        assert!(!input.is_surface_current_and_interactive());
    });
    assert!(!composer.read(app).selected_first_presentable(app));
    input.read(app).realization_diagnostics().frame_generation
}

fn selected_ready(app: &App) -> bool {
    let windows = support::shells(app);
    windows.len() == 2
        && windows.iter().all(|window| {
            window
                .read_with(app, |root, app| {
                    root.test_submission_composer(app)
                        .unwrap()
                        .read(app)
                        .selected_first_presentable(app)
                })
                .unwrap()
        })
}

async fn wait_for_publication(
    cx: &mut gpui::AsyncApp,
    progress: &Progress,
    original: &Mutex<Option<Vec<NativeIdentity>>>,
) {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if cx.update(|app| running(app)).unwrap() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "same-flight input completion did not publish the original set: {}",
            cx.update(|app| timeout_snapshot(app, progress, original))
                .unwrap_or_else(|_| "GUI snapshot unavailable".to_owned()),
        );
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

async fn wait_for_source_refusal(
    cx: &mut gpui::AsyncApp,
    progress: &Progress,
    original: &Mutex<Option<Vec<NativeIdentity>>>,
) {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if cx
            .update(|app| {
                assert!(!running(app));
                app.windows().iter().any(|window| {
                    window
                        .downcast::<crate::startup_surface::StartupSurface>()
                        .is_some()
                })
            })
            .unwrap()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "original invalidation did not fail the renewed publication flight: {}",
            cx.update(|app| timeout_snapshot(app, progress, original))
                .unwrap_or_else(|_| "GUI snapshot unavailable".to_owned()),
        );
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
}

fn timeout_snapshot(
    app: &App,
    progress: &Progress,
    original: &Mutex<Option<Vec<NativeIdentity>>>,
) -> String {
    let mut snapshot = String::new();
    let lifetime = app.global::<BootstrapLifetime>();
    let _ = writeln!(
        snapshot,
        "running={} unavailable={} windows={} worker_entered={} worker_released={} validation={} admission={} controlled_validation={} controlled_frame_generation={} controlled_pending={} natural_pending={} last_ready_sample={} ready_sample_count={} ready_samples_after_pending={} publication={} input_wakes_during_validation={} input_wakes_after_pending={} ready_input_wakes_after_pending={} source_change_entered={} source_change_completed={} retirement_started={}",
        running(app),
        lifetime.unavailable.is_some(),
        app.windows().len(),
        progress.worker_entered.load(Ordering::SeqCst),
        progress.worker_released.load(Ordering::SeqCst),
        progress.validation.load(Ordering::SeqCst),
        progress.admission.load(Ordering::SeqCst),
        progress.controlled_validation.load(Ordering::SeqCst),
        progress.controlled_frame_generation.load(Ordering::SeqCst),
        progress.controlled_pending.load(Ordering::SeqCst),
        progress.natural_pending.load(Ordering::SeqCst),
        progress.last_ready_sample.load(Ordering::SeqCst),
        progress.ready_sample_count.load(Ordering::SeqCst),
        progress.ready_samples_after_pending.load(Ordering::SeqCst),
        progress.publication.load(Ordering::SeqCst),
        progress
            .input_wakes_during_validation
            .load(Ordering::SeqCst),
        progress.input_wakes_after_pending.load(Ordering::SeqCst),
        progress
            .ready_input_wakes_after_pending
            .load(Ordering::SeqCst),
        progress.source_change_entered.load(Ordering::SeqCst),
        progress.source_change_completed.load(Ordering::SeqCst),
        progress.retirement_started.load(Ordering::SeqCst)
    );
    for window in app.windows() {
        if let Some(surface) = window.downcast::<crate::startup_surface::StartupSurface>() {
            let state = surface
                .read(app)
                .map(|surface| surface.diagnostic_home_state());
            let _ = writeln!(
                snapshot,
                "startup_surface={:?} state={state:?}",
                window.window_id()
            );
        }
    }
    match original.try_lock() {
        Ok(original) => {
            if let Some(original) = original.as_ref() {
                for expected in original.iter().take(2) {
                    let _ = writeln!(snapshot, "original={expected:?}");
                    let window = app.windows().into_iter().find_map(|window| {
                        (window.window_id() == expected.window)
                            .then(|| window.downcast::<MainWindowShellRoot>())
                            .flatten()
                    });
                    let Some(window) = window else {
                        let _ = writeln!(snapshot, "current_window=absent");
                        continue;
                    };
                    let root_id = window.entity(app).map(|root| root.entity_id());
                    let read = window.read_with(app, |root, app| {
                        let member = root.controller().map(|controller| controller.window_id());
                        let _ = writeln!(snapshot, "current_window={:?} root={root_id:?} member={member:?} gated={}",
                            window.window_id(), root.startup_interaction_gated());
                        let Some(composer) = root.test_submission_composer(app) else {
                            let _ = writeln!(snapshot, "composer=absent");
                            return;
                        };
                        let composer_id = composer.entity_id();
                        let composer = composer.read(app);
                        let error = composer.last_error().map(|error| error.chars().take(160).collect::<String>());
                        let input = composer.gpui_input();
                        let _ = writeln!(snapshot, "composer={composer_id:?} input={:?} composer_error={error:?}",
                            input.entity_id());
                        let input = input.read(app);
                        let diag = input.realization_diagnostics();
                        let current = diag.current;
                        let _ = writeln!(snapshot, "input_current={} quiescent={} frame_generation={} frame={:?} continuation={} capacity={:?} config_items={} candidates={} pending_intents(target,layout,presentation,rebind,index)={:?} geometry(jobs,pages,objects,resident_page_waits,deferred_responses)={:?} requests(queued,pending_pages,pending_objects,dispatched_pages,dispatched_objects,response_custody)={:?} response_rejections={} last_rejection={:?} rejection_stage={:?}",
                            input.is_surface_current_and_interactive(), input.is_quiescent(), diag.frame_generation, diag.frame,
                            diag.continuation_scheduled, diag.capacity, current.pending_configuration_items, current.candidates,
                            (current.pending_target_intents, current.pending_layout_intents, current.pending_presentation_intents, current.pending_rebind_intents, current.pending_index_intents),
                            (current.active_geometry_jobs, current.pending_geometry_pages, current.pending_geometry_objects, current.resident_geometry_page_waits, current.deferred_geometry_responses),
                            (current.queued_requests, current.pending_page_requests, current.pending_object_requests, current.dispatched_page_requests, current.dispatched_object_requests, current.response_custody_count),
                            diag.response_rejection_count, diag.last_response_rejection, diag.last_response_rejection_stage);
                    });
                    if let Err(error) = read {
                        let _ = writeln!(snapshot, "current_window_read={error}");
                    }
                }
            } else {
                let _ = writeln!(snapshot, "original_identities=absent");
            }
        }
        Err(std::sync::TryLockError::Poisoned(_)) => {
            let _ = writeln!(snapshot, "original_identities=poisoned");
        }
        Err(std::sync::TryLockError::WouldBlock) => {
            let _ = writeln!(snapshot, "original_identities=busy");
        }
    }
    let mut end = snapshot.len().min(8 * 1024 - 256);
    while !snapshot.is_char_boundary(end) {
        end -= 1;
    }
    snapshot.truncate(end);
    snapshot
}

#[test]
fn native_publication_continues_original_hidden_set_after_input_layout_becomes_pending() {
    continuation(Completion::Publish);
}

#[test]
fn native_exit_during_renewed_publication_readiness_disposes_original_set() {
    continuation(Completion::Exit);
}

#[test]
fn native_renewed_publication_validation_refuses_changed_original_source() {
    continuation(Completion::ChangedSource);
}

fn continuation(completion: Completion) {
    let fixture = support::selected(2, false, false);
    let runtime_hold = ProducerHold::install(Producer::RuntimeLaunch, *fixture.runtime.as_bytes());
    let runtime = runtime_hold.gate();
    let catalog_hold =
        ProducerHold::install(Producer::CatalogCertification, *fixture.home_id.as_bytes());
    let catalog = catalog_hold.gate();
    let history_holds = fixture
        .members
        .iter()
        .map(|(_, thread, _)| ProducerHold::install(Producer::SelectedHistory, *thread.as_bytes()))
        .collect::<Vec<_>>();
    let progress = Arc::new(Progress::default());
    let original = Arc::new(Mutex::new(None));
    let source_change = Arc::new(Mutex::new(None::<Box<dyn FnOnce() + Send>>));
    let (release, held) = std::sync::mpsc::channel();
    let held = Arc::new(Mutex::new(held));
    let mut configuration = support::configuration(&fixture);
    let native_progress = progress.clone();
    let native_original = original.clone();
    let native_source_change = source_change.clone();
    let native_runtime = runtime.clone();
    configuration.startup.native_hook = Some(Arc::new(move |native| {
        let progress = native_progress.clone();
        let held = held.clone();
        native.test_before_final_validation(move || {
            progress.worker_entered.store(true, Ordering::SeqCst);
            match held.lock().unwrap().recv_timeout(Duration::from_secs(25)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("first validation worker was not released before its deadline")
                }
            }
        });
        let progress = native_progress.clone();
        let source_change = native_source_change.clone();
        native.test_each_final_validation(move || {
            let pass = progress.validation.fetch_add(1, Ordering::SeqCst) + 1;
            let controlled = progress.controlled_validation.load(Ordering::SeqCst);
            if controlled > 0 && pass > controlled && completion == Completion::ChangedSource {
                progress.source_change_entered.store(true, Ordering::SeqCst);
                source_change.lock().unwrap().take().unwrap()();
                progress
                    .source_change_completed
                    .store(true, Ordering::SeqCst);
            }
        });
        if completion == Completion::ChangedSource {
            let progress = native_progress.clone();
            native.test_before_retirement(move |_, _| {
                progress.retirement_started.fetch_add(1, Ordering::SeqCst);
            });
        }
        let progress = native_progress.clone();
        native.test_observe_input_wake(move |ready| {
            if progress.worker_entered.load(Ordering::SeqCst)
                && !progress.worker_released.load(Ordering::SeqCst)
            {
                progress
                    .input_wakes_during_validation
                    .fetch_add(1, Ordering::SeqCst);
            }
            if progress.controlled_pending.load(Ordering::SeqCst) > 0 {
                progress
                    .input_wakes_after_pending
                    .fetch_add(1, Ordering::SeqCst);
                if ready {
                    progress
                        .ready_input_wakes_after_pending
                        .fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        let progress = native_progress.clone();
        native.test_observe_readiness(move |ready| {
            progress.last_ready_sample.store(ready, Ordering::SeqCst);
            progress.ready_sample_count.fetch_add(1, Ordering::SeqCst);
            if ready && progress.controlled_pending.load(Ordering::SeqCst) > 0 {
                progress
                    .ready_samples_after_pending
                    .fetch_add(1, Ordering::SeqCst);
            }
            if !ready {
                if progress.controlled_validation.load(Ordering::SeqCst) > 0 {
                    progress.controlled_pending.fetch_add(1, Ordering::SeqCst);
                } else {
                    progress.natural_pending.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        let progress = native_progress.clone();
        let original = native_original.clone();
        let runtime = native_runtime.clone();
        native.test_before_admission(move |shells, app| {
            assert_eq!(shells.len(), 2);
            assert_hidden(app);
            assert_eq!(runtime.entered(), 0);
            assert_eq!(identities(app), *original.lock().unwrap().as_ref().unwrap());
            progress.admission.fetch_add(1, Ordering::SeqCst);
            let controlled = progress.controlled_validation.load(Ordering::SeqCst);
            if controlled == 0 {
                if selected_ready(app) {
                    let generation = pending_layout(support::shells(app)[0], app);
                    progress
                        .controlled_frame_generation
                        .store(generation, Ordering::SeqCst);
                    progress
                        .controlled_validation
                        .store(progress.validation.load(Ordering::SeqCst), Ordering::SeqCst);
                    if completion == Completion::Exit {
                        let progress = progress.clone();
                        app.defer(move |app| {
                            assert!(progress.controlled_pending.load(Ordering::SeqCst) > 0);
                            assert_hidden(app);
                            assert_eq!(progress.publication.load(Ordering::SeqCst), 0);
                            exit(app);
                        });
                    }
                }
            } else {
                assert!(progress.validation.load(Ordering::SeqCst) > controlled);
                assert!(
                    progress
                        .ready_input_wakes_after_pending
                        .load(Ordering::SeqCst)
                        > 0
                );
            }
        });
        let progress = native_progress.clone();
        native.test_after_admission_pending(move |app| {
            assert_hidden(app);
            assert_eq!(progress.publication.load(Ordering::SeqCst), 0);
        });
        let progress = native_progress.clone();
        native.test_before_publication(move |_, _, _| {
            let controlled = progress.controlled_validation.load(Ordering::SeqCst);
            assert!(controlled > 0);
            assert!(progress.controlled_pending.load(Ordering::SeqCst) > 0);
            assert!(progress.validation.load(Ordering::SeqCst) > controlled);
            assert!(progress.validation.load(Ordering::SeqCst) >= 2);
            progress.publication.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
    }));
    let app_progress = progress.clone();
    let app_original = original.clone();
    let app_runtime = runtime.clone();
    let checked = Arc::new(AtomicBool::new(false));
    let receipt = checked.clone();
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            support::wait(
                cx,
                |_| app_progress.worker_entered.load(Ordering::SeqCst),
                "first real validation worker did not enter",
            )
            .await;
            cx.update(|app| {
                assert_hidden(app);
                assert_eq!(support::shells(app).len(), 2);
                *app_original.lock().unwrap() = Some(identities(app));
                let window = support::shells(app)[0];
                if completion == Completion::ChangedSource {
                    let reference = window
                        .read_with(app, |root, app| {
                            root.test_submission_composer(app)
                                .unwrap()
                                .read(app)
                                .test_startup_home_reference()
                        })
                        .unwrap();
                    *source_change.lock().unwrap() = Some(Box::new(move || {
                        support::change_saved_placement(&reference);
                    }));
                }
            })
            .unwrap();
            support::wait(
                cx,
                |app| {
                    assert_hidden(app);
                    if !selected_ready(app) {
                        return false;
                    }
                    app_progress
                        .input_wakes_during_validation
                        .store(0, Ordering::SeqCst);
                    pending_layout(support::shells(app)[0], app);
                    true
                },
                "original editors did not become ready for held-validation layout",
            )
            .await;
            support::wait(
                cx,
                |app| {
                    assert_hidden(app);
                    app_progress
                        .input_wakes_during_validation
                        .load(Ordering::SeqCst)
                        > 0
                        && selected_ready(app)
                },
                "input-local work did not notify the retained flight during validation",
            )
            .await;
            app_progress.worker_released.store(true, Ordering::SeqCst);
            release.send(()).unwrap();
            if completion == Completion::Exit {
                receipt.store(true, Ordering::SeqCst);
                return;
            }
            if completion == Completion::ChangedSource {
                wait_for_source_refusal(cx, &app_progress, &app_original).await;
                cx.update(|app| {
                    assert!(support::shells(app).is_empty());
                    for identity in app_original.lock().unwrap().as_ref().unwrap() {
                        assert!(
                            !unsafe {
                                windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(
                                    windows::Win32::Foundation::HWND(identity.native as *mut _),
                                ))
                            }
                            .as_bool()
                        );
                    }
                })
                .unwrap();
                assert_eq!(app_runtime.entered(), 0);
                receipt.store(true, Ordering::SeqCst);
                cx.update(exit).unwrap();
                return;
            }
            wait_for_publication(cx, &app_progress, &app_original).await;
            cx.update(|app| {
                assert_eq!(
                    identities(app),
                    *app_original.lock().unwrap().as_ref().unwrap()
                );
                for window in support::shells(app) {
                    assert!(
                        !window
                            .read_with(app, |root, _| root.startup_interaction_gated())
                            .unwrap()
                    );
                    assert!(
                        unsafe {
                            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                                support::native(window, app),
                            )
                        }
                        .as_bool()
                    );
                }
            })
            .unwrap();
            catalog.release();
            app_runtime.release();
            for hold in &history_holds {
                hold.gate().release();
            }
            support::wait(
                cx,
                |_| catalog.returned() > 0 && app_runtime.returned() > 0,
                "original progressive producers did not settle",
            )
            .await;
            receipt.store(true, Ordering::SeqCst);
            cx.update(exit).unwrap();
        })
        .detach();
    }));
    let result = run(configuration);
    if completion == Completion::Publish {
        result.unwrap();
        assert_eq!(progress.publication.load(Ordering::SeqCst), 2);
        assert!(progress.admission.load(Ordering::SeqCst) >= 2);
    } else {
        assert!(matches!(result, Err(BootstrapError::Startup)));
        assert_eq!(progress.publication.load(Ordering::SeqCst), 0);
        if completion == Completion::ChangedSource {
            assert_eq!(
                progress.validation.load(Ordering::SeqCst),
                progress.controlled_validation.load(Ordering::SeqCst) + 1,
            );
        }
    }
    assert!(checked.load(Ordering::SeqCst));
    assert!(progress.controlled_validation.load(Ordering::SeqCst) > 0);
    assert!(progress.controlled_pending.load(Ordering::SeqCst) > 0);
    assert!(
        progress
            .input_wakes_during_validation
            .load(Ordering::SeqCst)
            > 0
    );
    if matches!(completion, Completion::Publish | Completion::ChangedSource) {
        assert!(
            progress
                .ready_input_wakes_after_pending
                .load(Ordering::SeqCst)
                > 0
        );
    }
    support::assert_saved(&fixture, 0);
    if completion == Completion::ChangedSource {
        support::assert_changed_placement(&fixture);
    }
    drop((runtime_hold, catalog_hold));
    fixture.directory.close().unwrap();
}
