use super::*;

pub(super) fn configure(
    selection: MainWindowComposerSelectionIdentity,
) -> Result<MainWindowConversationComposerConfig, String> {
    MainWindowConversationComposerConfig::new(
        selection,
        widget_support::widget_config(
            selection.binding().range_binding(),
            selection.binding().presentation_generation(),
        ),
    )
    .map_err(|e| e.to_string())
}

pub(super) fn prepared_process() -> (
    tempfile::TempDir,
    ProcessServiceOwner,
    PreparedMainWindowRestoreSet,
    Arc<crate::theme_runtime::AppearanceGeneration>,
    FaultController,
) {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut process = owner(&candidate);
    process
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    process.graph_mut().unwrap().release_theme().unwrap();
    let appearance = process
        .graph_mut()
        .unwrap()
        .theme()
        .unwrap()
        .current()
        .unwrap();
    let mut inputs = window_services::inputs();
    inputs.configurator_source = Arc::new(|| Box::new(configure));
    let bundle = process.window_services(inputs).unwrap();
    let mut work = bundle
        .into_restore_set(
            appearance.clone(),
            WindowId::from_bytes([73; 16]),
            window_services::placement(),
        )
        .unwrap();
    for _ in 0..32 {
        match work.advance() {
            MainWindowRestoreSetOutcome::Pending(next) => work = next,
            MainWindowRestoreSetOutcome::Prepared(prepared) => {
                return (directory, process, prepared, appearance, faults);
            }
            MainWindowRestoreSetOutcome::Retained { reason, .. } => {
                panic!("first conversation restore retained: {reason:?}")
            }
            MainWindowRestoreSetOutcome::Failed { error } => {
                panic!("first conversation restore failed: {error}")
            }
        }
    }
    panic!("first conversation restore did not settle")
}

#[path = "../../../support/committed_first_conversation.rs"]
mod onboarding;
pub(super) use onboarding::commit_onboarding;

pub(super) fn wait(
    cx: &mut TestAppContext,
    mut ready: impl FnMut(&mut TestAppContext) -> bool,
    detail: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready(cx) {
        assert!(Instant::now() < deadline, "{detail}");
        cx.run_until_parked();
        for window in cx.windows() {
            cx.update(|app| {
                app.update_window(window, |_, window, app| window.draw(app).clear())
                    .unwrap()
            });
        }
        cx.executor().advance_clock(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.run_until_parked();
}

pub(super) fn recovery_stage(owner: &Rc<RefCell<RunningProcessOwner>>) -> String {
    let owner = owner.borrow();
    let stage = if owner.test_services_on_worker() {
        "service owner on worker".into()
    } else {
        owner.test_services().test_recovery_retirement_stage()
    };
    format!(
        "capture={}, automatic={}, {stage}",
        owner.test_running_home_recovery_identity().is_some(),
        owner.automatic_recovery_outcome().is_some()
    )
}

pub(super) fn dispose_recovered(
    owner: Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) {
    stop_observers(&owner, cx);
    let draft = window
        .update(cx, |root, window, cx| {
            root.set_shutdown_interaction_gated(true, cx).unwrap();
            root.begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .update(cx, |root, window, cx| {
                    let advance = root.advance_shutdown_draft(&draft, window, cx).unwrap();
                    match advance {
                        MainWindowShutdownDraftAdvance::Resident(
                            MainWindowConversationComposerCloseAdvance::Ready,
                        ) => true,
                        MainWindowShutdownDraftAdvance::Resident(
                            MainWindowConversationComposerCloseAdvance::Unsatisfied(_)
                            | MainWindowConversationComposerCloseAdvance::Stale,
                        ) => panic!("fresh resident close refused: {advance:?}"),
                        _ => false,
                    }
                })
                .unwrap()
        },
        "fresh resident close did not settle",
    );
    wait(
        cx,
        |cx| {
            window
                .update(cx, |root, window, cx| {
                    root.test_dispose_and_remove_published_first_conversation(&draft, window, cx)
                        .unwrap()
                })
                .unwrap()
        },
        "fresh resident final disposal did not settle",
    );
    drop(draft);
    retire_virtual_bindings(&owner, cx);
    let mut process = owner.borrow_mut().test_take_services();
    std::thread::spawn(move || close(&mut process))
        .join()
        .unwrap();
}

pub(super) fn stop_observers(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &mut TestAppContext) {
    let stopped = Rc::new(std::cell::Cell::new(false));
    let done = stopped.clone();
    let retained = owner.clone();
    cx.update(|app| {
        app.spawn(async move |cx| {
            RunningProcessOwner::test_stop_running_observers(&retained, cx)
                .await
                .unwrap();
            done.set(true);
        })
        .detach();
    });
    wait(
        cx,
        |_| stopped.get(),
        "running observer stop did not settle",
    );
}

pub(super) fn retire_virtual_bindings(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut TestAppContext,
) {
    cx.update(|app| {
        let mut owner = owner.borrow_mut();
        owner.test_process_mut().windows.release_retired_shells();
        owner
            .test_process_appearance()
            .update(app, |appearance, _| appearance.retire());
    });
}

pub(super) fn dispose_refused(
    owner: Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) {
    stop_observers(&owner, cx);
    window
        .update(cx, |root, window, cx| {
            root.test_remove_retired_first_conversation(window, cx)
                .unwrap()
        })
        .unwrap();
    retire_virtual_bindings(&owner, cx);
    let home = owner.borrow_mut().test_take_retired_recovery_home();
    std::thread::spawn(move || home.close().unwrap())
        .join()
        .unwrap();
}
