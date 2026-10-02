use crate::main_window::*;
use resident_recovery::resident_fixture;

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    abandon: bool,
    cx: &mut AsyncApp,
) {
    let (window, home, retired, original) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            retained.test_process().windows.shells()[0].window(),
            graph.home().home_id(),
            graph.home().health().generation().unwrap(),
            selected_preparation::publication_evidence(
                &retained.interrupted_exit_session().unwrap(),
            ),
        )
    };
    let (mount, composer, input, selection, close) = window
        .update(cx, |root, _, app| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let composer = mount.read(app).contribution().unwrap();
            let input = composer.read(app).gpui_input();
            let selection = composer.read(app).selection_identity().claim();
            let close = owner
                .borrow()
                .test_captured_recovery_ticket(composer.entity_id())
                .unwrap();
            (mount, composer, input, selection, close)
        })
        .unwrap();
    let mut preparation = None;
    let mut appearance = None;
    let mut adapters = None;
    let layout = input
        .read_with(cx, |input, _| input.resident_layout_snapshot())
        .unwrap();
    let mut configurator: Option<MainWindowConversationComposerConfigurator> = Some(Box::new({
        let layout = layout.clone();
        move |selection| resident_fixture::configure_current(selection, &layout)
    }));
    let mut retirement = None;
    let current = Rc::new(RefCell::new(None));
    let admitted = std::cell::Cell::new(None);
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (attempt, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::recover_interrupted_exit_resident_window(
                &owner,
                attempt,
                retired,
                window,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                &mut preparation,
                |_, _| panic!("refused entry admitted resident"),
                &mut appearance,
                &mut adapters,
                &mut configurator,
                |_, _| panic!("refused entry attached resident"),
                cancellation,
                |_| panic!("refused entry attempted recovery"),
                cx,
            )
            .await
            .is_err()
        );
        assert!(preparation.is_none() && appearance.is_none() && adapters.is_none());
        assert!(configurator.is_some());
        assert!(owner.borrow().test_services().graph().is_some());
    }
    let captured = current.clone();
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(
        RunningProcessOwner::recover_interrupted_exit_resident_window(
            &owner,
            request,
            retired,
            window,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            &mut preparation,
            |generation, app| {
                assert_ne!(generation, retired);
                assert_eq!(
                    original,
                    selected_preparation::publication_evidence(
                        &owner.borrow().interrupted_exit_session().unwrap()
                    )
                );
                assert!(owner.borrow().exit_requested());
                assert!(owner.borrow().test_services().graph().is_none());
                assert!(!input.read(app).is_enabled());
                retirement = mount
                    .update(app, |mount, cx| {
                        mount.take_interrupted_exit_retirement(close, cx)
                    })
                    .unwrap();
                assert!(retirement.is_some());
                let key = RunningProcessOwner::prepare_interrupted_exit_resident(
                    &owner,
                    request,
                    &composer,
                    close,
                    window.into(),
                    generation,
                    &mut retirement,
                    move |seed, selection, window| {
                        let (environment, capacity) = resident_fixture::environment_with_layout(
                            seed,
                            selection,
                            window,
                            Some(&layout),
                        )?;
                        *captured.borrow_mut() =
                            Some(gpui_text_input::RangePrepublicationCurrent {
                                binding: seed.binding,
                                history: seed.history,
                                available_capacity: gpui_text_input::RangeSurfaceCharge {
                                    bytes: capacity.bytes / 2,
                                    items: capacity.items / 2,
                                },
                            });
                        Ok((environment, capacity))
                    },
                    app,
                    |_, _| {},
                )?;
                admitted.set(Some(generation));
                Ok(key)
            },
            &mut appearance,
            &mut adapters,
            &mut configurator,
            |_, _| Ok(current.borrow_mut().take().unwrap()),
            CommandCancellation::new(),
            |_| panic!("unexpected recovery failure"),
            &mut drive_cx,
        ),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let completed = loop {
        let polled = {
            use std::future::Future;
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            drive.as_mut().poll(&mut context)
        };
        if let std::task::Poll::Ready(result) = polled {
            assert!(!abandon, "abandoned case must stop during preparation");
            break Some(result.unwrap());
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
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        if abandon && admitted.get().is_some() {
            break None;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    };
    drop(drive);
    let appearance = appearance.unwrap();
    let generation = admitted.get().unwrap();
    let (renewed, record) = if let Some(completed) = completed {
        completed
    } else {
        assert!(preparation.is_some() && adapters.is_some() && configurator.is_some());
        RunningProcessOwner::prepare_and_complete_interrupted_exit_resident_window(
            &owner,
            request,
            retired,
            generation,
            &mut preparation,
            |_| panic!("retained preparation must not be admitted again"),
            window,
            &appearance,
            &mut adapters,
            &mut configurator,
            |_, _| Ok(current.borrow_mut().take().unwrap()),
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap()
    };
    assert_ne!(renewed, close);
    assert!(
        preparation.is_none()
            && retirement.is_none()
            && adapters.is_none()
            && configurator.is_none()
    );
    assert!(!owner.borrow().exit_requested());
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(owner.borrow().shutdown_status().is_none());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
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
    }
    window
        .update(cx, |root, _, app| {
            assert_eq!(root.test_exit_presentation().0, "Exit");
            assert!(!root.test_notices_inert());
            assert_eq!(
                root.controller().unwrap().composer_mount().as_ref(),
                Some(&mount)
            );
            assert_eq!(mount.read(app).contribution().as_ref(), Some(&composer));
            assert_eq!(composer.read(app).gpui_input(), input);
            assert_eq!(composer.read(app).selection_identity().claim(), selection);
            assert!(input.read(app).is_enabled());
            assert_eq!(root.controller().unwrap().window_id(), record.window_id());
        })
        .unwrap();
    drop((mount, composer, input));
    selected_publication::dispose_recovered(owner, vec![window], appearance, cx).await;
}
