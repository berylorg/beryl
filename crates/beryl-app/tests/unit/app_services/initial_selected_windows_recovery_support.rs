use crate::running_owner::ResidentRecoveryWindow;
use resident_recovery::resident_fixture;
use std::{future::Future, task::Poll};

mod publication_continuation {
    use super::*;
    include!("selected_publication_continuation_support.rs");
}

mod prepared_continuation {
    use super::*;
    include!("selected_prepared_continuation_support.rs");
}

mod retired_continuation {
    use super::*;
    include!("selected_retired_continuation_support.rs");
}

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    abandon: bool,
    cx: &mut AsyncApp,
) {
    let (windows, home, retired, original) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            retained
                .test_process()
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>(),
            graph.home().home_id(),
            graph.home().health().generation().unwrap(),
            selected_preparation::publication_evidence(
                &retained.interrupted_exit_session().unwrap(),
            ),
        )
    };
    assert_eq!(windows.len(), 2);
    let layouts = windows
        .iter()
        .map(|window| {
            window
                .read_with(cx, |root, app| {
                    let mount = root.controller().unwrap().composer_mount().unwrap();
                    let composer = mount.read(app).contribution().unwrap();
                    composer
                        .read(app)
                        .gpui_input()
                        .read(app)
                        .resident_layout_snapshot()
                })
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut residents = Vec::new();
    for window in &windows {
        residents.push(
            window
                .update(cx, |root, _, app| {
                    let mount = root.controller().unwrap().composer_mount().unwrap();
                    let composer = mount.read(app).contribution().unwrap();
                    let input = composer.read(app).gpui_input();
                    let selection = composer.read(app).selection_identity().claim();
                    let close = owner
                        .borrow()
                        .test_captured_recovery_ticket(composer.entity_id())
                        .unwrap();
                    (
                        mount,
                        composer,
                        input,
                        selection,
                        close,
                        root.controller().unwrap().window_id(),
                    )
                })
                .unwrap(),
        );
    }
    let make_entry = |window| {
        let layout = layouts[windows
            .iter()
            .position(|candidate| *candidate == window)
            .unwrap()]
        .clone();
        ResidentRecoveryWindow::unprepared(
            window,
            Box::new(move |selection| {
                resident_fixture::recovery_configuration_current(selection, &layout)
            }),
        )
    };
    let mut appearance = None;
    let foreign = request.test_foreign();
    let configured_counts = Rc::new(RefCell::new([0, 0]));
    let generation = Rc::new(std::cell::Cell::new(None));
    let mut configured = Vec::new();
    let mut dropped = std::rc::Weak::new();
    assert_eq!(
        RunningProcessOwner::recover_interrupted_exit(
            &owner,
            request,
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            |window| {
                configured.push(window);
                if configured.len() == 2 {
                    return Err("configuration refused".into());
                }
                let capture = Rc::new(());
                dropped = Rc::downgrade(&capture);
                Ok(Box::new(move |selection| {
                    let _retained = &capture;
                    resident_fixture::recovery_configuration(selection)
                }))
            },
            |_| panic!("unconfigured recovery"),
            cx,
        )
        .await
        .unwrap_err(),
        "configuration refused"
    );
    assert_eq!(configured, windows);
    assert!(dropped.upgrade().is_none());
    assert!(owner.borrow().test_services().graph().is_some());
    let mut entries = windows.iter().copied().map(make_entry).collect::<Vec<_>>();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    publication_continuation::assert_refused(&owner, request, cx).await;
    for (attempt, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::recover_interrupted_exit_resident_windows(
                &owner,
                attempt,
                retired,
                &mut entries,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                &mut appearance,
                cancellation,
                |_| panic!("refused recovery"),
                cx,
            )
            .await
            .is_err()
        );
        assert!(appearance.is_none());
        assert!(owner.borrow().test_services().graph().is_some());
    }
    for mut invalid in [
        Vec::new(),
        vec![make_entry(windows[0])],
        vec![make_entry(windows[0]), make_entry(windows[0])],
    ] {
        assert_eq!(
            RunningProcessOwner::recover_interrupted_exit_resident_windows(
                &owner,
                request,
                retired,
                &mut invalid,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                &mut appearance,
                CommandCancellation::new(),
                |_| panic!("invalid recovery"),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit requires the complete retained selected window set"
        );
        assert!(appearance.is_none());
        assert!(owner.borrow().test_services().graph().is_some());
    }
    let mut occupied = Some(owner.borrow().test_process_appearance());
    assert_eq!(
        RunningProcessOwner::recover_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            &mut entries,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            &mut occupied,
            CommandCancellation::new(),
            |_| panic!("occupied recovery"),
            cx,
        )
        .await
        .unwrap_err(),
        "Interrupted Exit selected recovery inputs are already retained"
    );
    assert!(owner.borrow().test_services().graph().is_some());
    drop(occupied);
    assert_eq!(*configured_counts.borrow(), [0, 0]);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (attempt, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                attempt,
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                |_| panic!("unexpected configuration factory"),
                |_| panic!("refused owned recovery"),
                cx,
            )
            .await
            .is_err()
        );
        assert!(owner.borrow().test_selected_recovery_appearance().is_none());
        assert!(owner.borrow().test_services().graph().is_some());
    }
    assert_eq!(*configured_counts.borrow(), [0, 0]);
    prepared_continuation::assert_refused(&owner, request, cx).await;
    retired_continuation::assert_refused(&owner, request, cx).await;
    let factory_calls = std::cell::Cell::new(0);
    let mut configure = |window| {
        let index = factory_calls.get();
        assert_eq!(window, windows[index]);
        factory_calls.set(index + 1);
        let counts = configured_counts.clone();
        let generation = generation.clone();
        let layout = layouts[index].clone();
        Ok(Box::new(
            move |selection: crate::main_window::MainWindowComposerSelectionIdentity| {
                counts.borrow_mut()[index] += 1;
                let fresh = selection.binding().home_generation();
                assert_ne!(fresh, retired);
                if let Some(previous) = generation.replace(Some(fresh)) {
                    assert_eq!(previous, fresh);
                }
                resident_fixture::recovery_configuration_current(selection, &layout)
            },
        ) as Box<dyn FnMut(_) -> _>)
    };
    if abandon {
        let cancellation = CommandCancellation::new();
        assert!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                cancellation.clone(),
                |window| {
                    let result = configure(window);
                    if factory_calls.get() == windows.len() {
                        cancellation.cancel();
                    }
                    result
                },
                |_| panic!("cancelled initial configuration"),
                cx,
            )
            .await
            .is_err()
        );
        assert_eq!(factory_calls.get(), windows.len());
        assert_eq!(*configured_counts.borrow(), [0, 0]);
        assert!(owner.borrow().test_services().graph().is_some());
    }
    let mut drive_cx = cx.clone();
    let mut drive: std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + '_>> =
        Box::pin(RunningProcessOwner::recover_interrupted_exit(
            &owner,
            request,
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            &mut configure,
            |_| panic!("unexpected recovery failure"),
            &mut drive_cx,
        ));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut prepared_continued = false;
    let mut retired_continued = false;
    loop {
        if !abandon
            && !retired_continued
            && owner
                .borrow()
                .interrupted_exit_graph_retirement_result(request)
                .is_ok()
        {
            drop(drive);
            retired_continuation::verify_refusals(&owner, request, cx).await;
            assert_eq!(*configured_counts.borrow(), [0, 0]);
            assert_eq!(
                original,
                selected_preparation::publication_evidence(
                    &owner.borrow().interrupted_exit_session().unwrap()
                )
            );
            drive = Box::pin(RunningProcessOwner::recover_retired_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                |_| panic!("unexpected continued recovery failure"),
                &mut drive_cx,
            ));
            retired_continued = true;
        }
        if !abandon
            && !prepared_continued
            && owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_ok()
        {
            drop(drive);
            prepared_continuation::verify_refusals(&owner, request, retired, cx).await;
            assert_eq!(*configured_counts.borrow(), [0, 0]);
            assert_eq!(
                original,
                selected_preparation::publication_evidence(
                    &owner.borrow().interrupted_exit_session().unwrap()
                )
            );
            drive = Box::pin(RunningProcessOwner::recover_prepared_interrupted_exit(
                &owner,
                request,
                CommandCancellation::new(),
                &mut drive_cx,
            ));
            prepared_continued = true;
        }
        if let Poll::Ready(result) = drive
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            assert!(!abandon);
            result.unwrap();
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        prepared_continuation::assert_refused(&owner, request, cx).await;
        retired_continuation::assert_refused(&owner, request, cx).await;
        cx.update(|app| {
            let result = owner
                .borrow_mut()
                .interrupted_exit_resident_windows(request, app, |_| {
                    panic!("competing window configuration")
                });
            assert_eq!(
                result.err().unwrap(),
                "Interrupted Exit recovery is already being driven"
            );
        })
        .unwrap();
        assert_eq!(
            RunningProcessOwner::await_interrupted_exit_completion(
                &owner,
                request,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit recovery is already being driven"
        );
        assert_eq!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                |_| panic!("unexpected configuration factory"),
                |_| panic!("competing recovery"),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit recovery is already being driven"
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        if abandon && configured_counts.borrow()[1] == 1 {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    drop(drive);
    assert_eq!(factory_calls.get(), windows.len());
    assert_eq!(prepared_continued, !abandon);
    assert_eq!(retired_continued, !abandon);
    let appearance = if abandon {
        owner.borrow().test_selected_recovery_appearance().unwrap()
    } else {
        owner.borrow().test_process_appearance()
    };
    let generation = generation.get().unwrap();
    if abandon {
        retired_continuation::assert_refused(&owner, request, cx).await;
        assert_eq!(*configured_counts.borrow(), [2, 1]);
        assert_eq!(
            original,
            selected_preparation::publication_evidence(
                &owner.borrow().interrupted_exit_session().unwrap()
            )
        );
        cx.update(|app| {
            assert!(owner.borrow().test_services().graph().is_none());
            assert!(
                owner
                    .borrow()
                    .validate_interrupted_exit_bindings(request, &appearance, app)
                    .is_err()
            );
            for (index, window) in windows.iter().enumerate() {
                assert_eq!(
                    window.read(app).unwrap().test_exit_presentation().0,
                    "Exiting…"
                );
                assert!(!residents[index].2.read(app).is_enabled());
                let ticket = owner
                    .borrow()
                    .test_captured_recovery_ticket(residents[index].1.entity_id())
                    .unwrap();
                assert_eq!(ticket == residents[index].4, index == 1);
            }
        })
        .unwrap();
        assert_eq!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                |_| panic!("unexpected configuration factory"),
                |_| panic!("repeated initial recovery"),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit original service graph is unavailable"
        );
        assert!(
            RunningProcessOwner::recover_prepared_interrupted_exit(
                &owner,
                &foreign,
                CommandCancellation::new(),
                cx,
            )
            .await
            .is_err()
        );
        publication_continuation::verify(&owner, request, cx).await;
    }

    assert_eq!(*configured_counts.borrow(), [2, 2]);
    assert!(!owner.borrow().exit_requested());
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(owner.borrow().shutdown_status().is_none());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    assert_eq!(owner.borrow().test_process_appearance(), appearance);
    {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        assert_eq!(graph.home().home_id(), home);
        assert_eq!(graph.home().health().generation().unwrap(), generation);
        drop(
            retained
                .test_services()
                .process
                .execution_permit()
                .reserve()
                .unwrap(),
        );
        assert_eq!(
            retained
                .test_process()
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>(),
            windows
        );
    }
    for (window, (mount, composer, input, selection, close, window_id)) in
        windows.iter().zip(&residents)
    {
        window
            .update(cx, |root, _, app| {
                assert_eq!(root.test_exit_presentation().0, "Exit");
                assert!(!root.test_notices_inert());
                assert_eq!(
                    root.controller().unwrap().composer_mount().as_ref(),
                    Some(mount)
                );
                assert_eq!(mount.read(app).contribution().as_ref(), Some(composer));
                assert_eq!(&composer.read(app).gpui_input(), input);
                assert_eq!(&composer.read(app).selection_identity().claim(), selection);
                assert!(input.read(app).is_enabled());
                assert_eq!(&root.controller().unwrap().window_id(), window_id);
                assert_ne!(
                    owner
                        .borrow()
                        .test_captured_recovery_ticket(composer.entity_id()),
                    Some(*close)
                );
            })
            .unwrap();
    }
    drop(entries);
    drop(residents);
    selected_publication::dispose_recovered(owner, windows, appearance, cx).await;
}
