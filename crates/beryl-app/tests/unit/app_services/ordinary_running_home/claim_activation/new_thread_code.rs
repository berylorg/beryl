use super::*;
use crate::thread_root_picker::{PickerCommand, PickerRowKey};
use beryl_model::{
    AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeLaunchForm, RuntimeMode,
    RuntimeNativePath,
};
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
    UnixMillis,
};
use std::sync::Mutex;

#[test]
fn native_recovered_new_thread_submits_code_and_routes_fresh_inner_ranges() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        1,
        0,
        prepare_root,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let prior_panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let prior = composer(window, cx)
                    .await
                    .read_with(cx, |composer, _| composer.selection_identity())
                    .unwrap();
                native_code::draw(window, cx);
                wait_for_services(&owner, cx).await;
                assert!(owner.borrow().runtime_setup_services().is_some());
                owner.borrow_mut().test_enable_resident_frame_capture();
                let target = Arc::new(Mutex::new(None));
                let captured = target.clone();
                let (home, state) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (graph.home().service_reference(), graph.state().clone())
                };
                window.update(cx, |root, native, app| {
                root.test_thread_confirmation_hooks(None, None, Some(Box::new(move |_| {
                    let evidence = state.session().capture_window_removal(&home, prior.window_id()).unwrap();
                    let exact = evidence.window().selected_thread().unwrap();
                    assert_ne!(exact, prior.claim());
                    *captured.lock().unwrap() = Some(exact);
                    faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
                    assert!(home.home_revision().is_err());
                })));
                root.test_open_runtime_setup(native, app);
            }).unwrap();
                native_code::wait(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.test_runtime_setup_picker().is_some_and(|picker| {
                                    let diagnostics = picker.read(app).diagnostics();
                                    diagnostics.total_count == 2
                                        && diagnostics.pending_page_count == 0
                                        && !diagnostics.collection_failed
                                })
                            })
                            .unwrap()
                    },
                    "native New Thread root page",
                )
                .await;
                let picker = window
                    .read_with(cx, |root, _| root.test_runtime_setup_picker().unwrap())
                    .unwrap();
                let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
                cx.update(|app| picker.update(app, |picker, cx| picker.activate(&key, cx)))
                    .unwrap();
                native_code::wait(
                    window,
                    cx,
                    |cx| {
                        picker
                            .read_with(cx, |picker, _| {
                                picker
                                    .command_state(&PickerCommand::Confirm(key.clone()))
                                    .is_some_and(|state| state.can_dispatch())
                            })
                            .unwrap()
                    },
                    "native New Thread confirmation admission",
                )
                .await;
                cx.update(|app| {
                    picker.update(app, |picker, cx| {
                        picker.dispatch_command(PickerCommand::Confirm(key), cx)
                    })
                })
                .unwrap();
                native_code::wait(
                    window,
                    cx,
                    |cx| {
                        cx.update(|app| {
                            RunningProcessOwner::test_dispatch_scheduled_resident_frame(
                                &owner, app,
                            );
                            RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                        })
                        .unwrap();
                        let recovered = matches!(
                            owner.borrow().automatic_recovery_outcome().as_deref(),
                            Some(InterruptedExitRecoveryOutcome::Completed)
                        );
                        if let Some(InterruptedExitRecoveryOutcome::Unavailable(error)) =
                            owner.borrow().automatic_recovery_outcome().as_deref()
                        {
                            panic!("native creation recovery: {error}");
                        }
                        recovered
                            && window
                                .read_with(cx, |root, app| {
                                    root.controller().unwrap().composer_mount().is_some_and(
                                        |mount| {
                                            mount.read(app).contribution().is_some_and(|composer| {
                                                let current =
                                                    composer.read(app).selection_identity();
                                                Some(current.claim()) == *target.lock().unwrap()
                                                    && current.binding().home_generation()
                                                        != prior.binding().home_generation()
                                            })
                                        },
                                    )
                                })
                                .unwrap()
                    },
                    "native committed New Thread recovery",
                )
                .await;
                let current = composer(window, cx)
                    .await
                    .read_with(cx, |composer, _| composer.selection_identity())
                    .unwrap();
                assert_eq!(current.window_id(), prior.window_id());
                assert_eq!(windows(&owner), vec![window]);
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                assert_ne!(panel.entity_id(), prior_panel.entity_id());
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.snapshot().records.is_empty())
                        .unwrap()
                );
                native_code::submit_code(&owner, window, cx).await;
                native_code::qualify(window, &panel, cx).await;
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_transcript_panel().entity_id())
                        .unwrap(),
                    panel.entity_id()
                );
                *qualified.borrow_mut() = Some((current.window_id(), current.claim().thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        1,
    );
}

fn prepare_root(path: &std::path::Path) {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let host = |path: &str| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let native = |path: &str| {
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
    };
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let creation = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            RuntimeId::from_bytes([1; 16]),
            host(r"C:\Codex\runtime.exe"),
            RuntimeMode::host(),
            RuntimeLaunchForm::CodexCli,
            native(r"C:\Codex\runtime.exe"),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            RootId::from_bytes([1; 16]),
            native(r"C:\Work\root"),
            host(r"C:\Work\root"),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&home).unwrap(),
            creation,
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    drop((storage, state));
    home.close().unwrap();
}
