use crate::theme_runtime::GpuiAppearanceWindowSet;
use resident_recovery::resident_fixture;

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    delivery: SelectedWindowsDelivery,
    cx: &mut AsyncApp,
) {
    use crate::running_owner::ResidentRecoveryWindow;
    use std::{future::Future, task::Poll};

    let windows: Vec<_> = owner
        .borrow()
        .test_process()
        .windows
        .shells()
        .iter()
        .map(|shell| shell.window())
        .collect();
    assert_eq!(windows.len(), 2);
    let original = selected_preparation::publication_evidence(
        &owner.borrow().interrupted_exit_session().unwrap(),
    );
    let prepared = owner.borrow().interrupted_exit_appearance(request).unwrap();
    let home = prepared.prepared().home().home_id();
    let generation = prepared.prepared().home().home_generation();
    let previous = owner.borrow().test_process_appearance();
    let appearance = cx
        .update(|app| {
            previous.update(app, |set, _| set.retire());
            GpuiAppearanceWindowSet::new(prepared, std::num::NonZeroUsize::new(4).unwrap(), app)
        })
        .unwrap();
    let configured_counts = Rc::new(RefCell::new([0, 0]));
    let refuse_configuration = matches!(delivery, SelectedWindowsDelivery::ConfigurationRefused);
    let make_entry = |window| {
        ResidentRecoveryWindow::new(
            window,
            owner
                .borrow()
                .interrupted_exit_composer_adapters(
                    request,
                    home,
                    generation,
                    configuration()
                        .projection
                        .turn_start_admission_requirement(),
                )
                .unwrap(),
            Box::new({
                let counts = configured_counts.clone();
                let index = windows
                    .iter()
                    .position(|candidate| *candidate == window)
                    .unwrap();
                move |selection| {
                    counts.borrow_mut()[index] += 1;
                    if refuse_configuration && index == 0 && counts.borrow()[index] == 2 {
                        return Err("current configuration refused".into());
                    }
                    resident_fixture::recovery_configuration(selection)
                }
            }),
        )
    };
    let mut entries: Vec<_> = windows.iter().copied().map(make_entry).collect();
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (attempt, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
                &owner,
                attempt,
                retired,
                generation,
                &mut entries,
                &appearance,
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
    }
    for mut invalid in [
        Vec::new(),
        vec![make_entry(windows[0])],
        vec![make_entry(windows[0]), make_entry(windows[0])],
    ] {
        assert_eq!(
            RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
                &owner,
                request,
                retired,
                generation,
                &mut invalid,
                &appearance,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit requires the complete retained selected window set"
        );
    }
    let mut residents = Vec::new();
    assert_eq!(*configured_counts.borrow(), [0, 0]);
    for window in &windows {
        let resident = window
            .update(cx, |root, _, app| {
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                let input = composer.read(app).gpui_input();
                let selection = composer.read(app).selection_identity().claim();
                let close = composer
                    .read(app)
                    .recovery_snapshot()
                    .unwrap()
                    .close_ticket();
                (
                    mount,
                    composer,
                    input,
                    selection,
                    close,
                    root.controller().unwrap().window_id(),
                )
            })
            .unwrap();
        residents.push(resident);
    }
    assert!(
        RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            retired,
            &mut entries,
            &appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
    assert_eq!(*configured_counts.borrow(), [0, 0]);
    let mut drive_cx = cx.clone();
    let refuse = matches!(delivery, SelectedWindowsDelivery::AppearanceRefused);
    let mut drive = Box::pin(
        RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            generation,
            &mut entries,
            if refuse { &previous } else { &appearance },
            CommandCancellation::new(),
            &mut drive_cx,
        ),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut finished = false;
    loop {
        let polled = drive
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
        if let Poll::Ready(result) = polled {
            if refuse {
                assert_eq!(
                    result.unwrap_err(),
                    "Recovery appearance candidate identity changed"
                );
            } else if refuse_configuration {
                assert_eq!(result.unwrap_err(), "current configuration refused");
            } else {
                result.unwrap();
                finished = true;
            }
            break;
        }
        assert!(std::time::Instant::now() < deadline);
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
        assert_eq!(
            RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
                &owner,
                request,
                retired,
                generation,
                &mut competing,
                &appearance,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err(),
            "Interrupted Exit recovery is already being driven"
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        if matches!(delivery, SelectedWindowsDelivery::Dropped)
            && configured_counts.borrow()[1] == 1
        {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    drop(drive);
    if !finished {
        assert_eq!(configured_counts.borrow()[0], 2);
        assert_eq!(
            configured_counts.borrow()[1],
            usize::from(!refuse && !refuse_configuration)
        );
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
                let root = window.read(app).unwrap();
                assert_eq!(root.test_exit_presentation().0, "Exiting…");
                assert!(!residents[index].2.read(app).is_enabled());
                let ticket = owner
                    .borrow()
                    .test_captured_recovery_ticket(residents[index].1.entity_id())
                    .unwrap();
                assert_eq!(
                    ticket == residents[index].4,
                    index == 1 || refuse_configuration
                );
            }
        })
        .unwrap();
        RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_windows(
            &owner,
            request,
            retired,
            generation,
            &mut entries,
            &appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap();
    }
    assert_eq!(
        *configured_counts.borrow(),
        [2 + usize::from(refuse_configuration), 2]
    );
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
    for (window, (mount, composer, input, selection, _close, window_id)) in
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
            })
            .unwrap();
    }
    drop(entries);
    drop(residents);
    selected_publication::dispose_recovered(owner, windows, appearance, cx).await;
}
