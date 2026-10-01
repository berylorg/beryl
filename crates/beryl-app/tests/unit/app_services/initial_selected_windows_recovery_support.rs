use crate::running_owner::ResidentRecoveryWindow;
use resident_recovery::resident_fixture;
use std::{future::Future, task::Poll};

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
    let make_entry =
        |window| ResidentRecoveryWindow::unprepared(window, Box::new(resident_fixture::configure));
    let mut appearance = None;
    let foreign = request.test_foreign();
    let mut entries = cx
        .update(|app| {
            let mut retained = owner.borrow_mut();
            assert!(
                retained
                    .interrupted_exit_resident_windows(&foreign, app, |_| {
                        panic!("foreign window configuration")
                    })
                    .is_err()
            );
            let mut configured = Vec::new();
            let mut dropped = std::rc::Weak::new();
            assert!(
                retained
                    .interrupted_exit_resident_windows(request, app, |window| {
                        configured.push(window);
                        if configured.len() == 2 {
                            return Err("configuration refused".into());
                        }
                        let capture = Rc::new(());
                        dropped = Rc::downgrade(&capture);
                        Ok(Box::new(move |selection| {
                            let _retained = &capture;
                            resident_fixture::configure(selection)
                        }))
                    })
                    .is_err()
            );
            assert_eq!(configured, windows);
            assert!(dropped.upgrade().is_none());
            assert!(retained.test_services().graph().is_some());
            configured.clear();
            let entries = retained
                .interrupted_exit_resident_windows(request, app, |window| {
                    configured.push(window);
                    Ok(Box::new(resident_fixture::configure))
                })
                .unwrap();
            assert_eq!(configured, windows);
            entries
        })
        .unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
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
                |_, _, _| panic!("refused admission"),
                |_, _, _| panic!("refused attachment"),
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
                |_, _, _| panic!("invalid admission"),
                |_, _, _| panic!("invalid attachment"),
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
            |_, _, _| panic!("occupied admission"),
            |_, _, _| panic!("occupied attachment"),
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
    let currents = Rc::new(RefCell::new(vec![None, None]));
    let admitted = std::cell::Cell::new([0, 0]);
    let generation = std::cell::Cell::new(None);
    let mut retirements = vec![None, None];
    let mut admit = |index: usize, fresh, app: &mut gpui::App| {
        assert_ne!(fresh, retired);
        if let Some(previous) = generation.replace(Some(fresh)) {
            assert_eq!(previous, fresh);
        }
        let mut counts = admitted.get();
        assert_eq!(counts[index], 0);
        counts[index] += 1;
        admitted.set(counts);
        assert_eq!(
            original,
            selected_preparation::publication_evidence(
                &owner.borrow().interrupted_exit_session().unwrap()
            )
        );
        assert!(owner.borrow().exit_requested());
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(!residents[index].2.read(app).is_enabled());
        assert!(retirements[index].is_none());
        assert!(
            RunningProcessOwner::prepare_interrupted_exit_window_resident(
                &owner,
                &foreign,
                windows[index],
                fresh,
                &mut retirements[index],
                |_, _, _| panic!("foreign resident environment"),
                app,
                |_, _| panic!("foreign resident completion"),
            )
            .is_err()
        );
        assert!(retirements[index].is_none());
        assert!(
            RunningProcessOwner::prepare_interrupted_exit_window_resident(
                &owner,
                request,
                windows[index],
                retired,
                &mut retirements[index],
                |_, _, _| panic!("retired generation environment"),
                app,
                |_, _| panic!("retired generation completion"),
            )
            .is_err()
        );
        assert_eq!(
            retirements[index].as_ref().unwrap().close_ticket(),
            residents[index].4
        );
        if index == 0 {
            assert!(
                RunningProcessOwner::prepare_interrupted_exit_window_resident(
                    &owner,
                    request,
                    windows[1],
                    fresh,
                    &mut retirements[index],
                    |_, _, _| panic!("foreign retirement environment"),
                    app,
                    |_, _| panic!("foreign retirement completion"),
                )
                .is_err()
            );
            assert_eq!(
                retirements[index].as_ref().unwrap().close_ticket(),
                residents[index].4
            );
        }
        let captured = currents.clone();
        RunningProcessOwner::prepare_interrupted_exit_window_resident(
            &owner,
            request,
            windows[index],
            fresh,
            &mut retirements[index],
            move |seed, selection, window| {
                let config = resident_fixture::configure(selection)?;
                let mut invalid = seed;
                invalid.history = None;
                assert!(
                    config
                        .resident_recovery_environment(invalid, window)
                        .is_err()
                );
                let current = config.native_lineage_current();
                let capacity = gpui_text_input::RangeSurfaceCharge {
                    bytes: current.available_capacity.bytes * 2,
                    items: current.available_capacity.items * 2,
                };
                captured.borrow_mut()[index] = Some(current);
                Ok((config, capacity))
            },
            app,
            |_, _| {},
        )
    };
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(
        RunningProcessOwner::recover_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            &mut entries,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            &mut appearance,
            &mut admit,
            |index, _, _| Ok(currents.borrow_mut()[index].take().unwrap()),
            CommandCancellation::new(),
            |_| panic!("unexpected recovery failure"),
            &mut drive_cx,
        ),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Poll::Ready(result) = drive
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            assert!(!abandon);
            result.unwrap();
            break;
        }
        assert!(std::time::Instant::now() < deadline);
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
        let mut competing = Vec::new();
        let mut competing_appearance = None;
        assert_eq!(
            RunningProcessOwner::recover_interrupted_exit_resident_windows(
                &owner,
                request,
                retired,
                &mut competing,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                &mut competing_appearance,
                |_, _, _| panic!("competing admission"),
                |_, _, _| panic!("competing attachment"),
                CommandCancellation::new(),
                |_| panic!("competing recovery"),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit recovery is already being driven"
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        if abandon && admitted.get()[1] == 1 {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    drop(drive);
    let appearance = appearance.unwrap();
    let generation = generation.get().unwrap();
    if abandon {
        assert_eq!(admitted.get(), [1, 1]);
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
        let mut absent_appearance = None;
        assert_eq!(
            RunningProcessOwner::recover_interrupted_exit_resident_windows(
                &owner,
                request,
                retired,
                &mut entries,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                &mut absent_appearance,
                |_, _, _| panic!("repeated initial admission"),
                |_, _, _| panic!("repeated initial attachment"),
                CommandCancellation::new(),
                |_| panic!("repeated initial recovery"),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit selected recovery inputs are already retained"
        );
        RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            generation,
            &mut entries,
            &appearance,
            |_, _| panic!("retained preparation must not be admitted again"),
            |index, _, _| Ok(currents.borrow_mut()[index].take().unwrap()),
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap();
    }
    drop(admit);
    assert_eq!(admitted.get(), [1, 1]);
    assert!(retirements.iter().all(Option::is_none));
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
