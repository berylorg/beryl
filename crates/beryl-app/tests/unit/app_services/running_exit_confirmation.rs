use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{FindWindowW, GW_ENABLEDPOPUP, GetWindow, IDOK, PostMessageW},
        },
    },
    core::PCWSTR,
};

pub(super) async fn exercise(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &mut startup_owner::RunningExitRequest,
    stale: crate::cas_projection::ShutdownWorkObservation,
    confirm: bool,
    replace_request: Option<bool>,
    cx: &mut AsyncApp,
) {
    let (invoking, main, permit) = {
        let owner = owner.borrow();
        (
            owner.test_process().windows.window_ids()[0],
            owner.test_process().windows.shells()[0].window(),
            owner.test_services().process.execution_permit(),
        )
    };
    let changed = crate::cas_projection::test_faults::retain_projection_work(
        owner.borrow().test_services().graph().unwrap().cas(),
        beryl_model::SyndicThreadId::from_bytes([246; 16]),
    );
    assert!(matches!(
        cx.update(|app| RunningProcessOwner::route_exit_work(
            owner,
            request,
            Ok(stale),
            app,
            |_, _| panic!("stale work cannot schedule confirmation"),
        ))
        .unwrap(),
        Err(ExitWorkError::Confirmation(_))
    ));
    permit.commit(|| ()).unwrap();
    assert!(
        owner
            .borrow_mut()
            .take_shutdown_confirmation()
            .unwrap()
            .is_none()
    );
    drop(changed);
    let title = format!("exit-work-routing-{}", std::process::id());
    main.update(cx, |_, window, _| window.set_window_title(&title))
        .unwrap();
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let native = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap();
    let observation = observe(owner, ProjectionCancellationToken::new(), cx)
        .await
        .unwrap();
    let duplicate = observation.clone();
    let delivered = Rc::new(RefCell::new(None));
    let completion = delivered.clone();
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    let gui_thread = std::thread::current().id();
    assert_eq!(
        cx.update(|app| RunningProcessOwner::route_exit_work(
            owner,
            request,
            Ok(observation),
            app,
            move |owner, _| {
                assert_eq!(std::thread::current().id(), gui_thread);
                callback_calls.set(callback_calls.get() + 1);
                assert!(owner.borrow().exit_requested());
                assert!(owner.borrow().shutdown_status().is_none());
                *completion.borrow_mut() = Some(());
            },
        ))
        .unwrap()
        .unwrap(),
        ExitWorkRoute::Confirming
    );
    assert!(delivered.borrow().is_none());
    assert_eq!(
        cx.update(|app| owner.borrow_mut().consume_exit_confirmation(request, app))
            .unwrap()
            .unwrap(),
        None
    );
    assert!(owner.borrow().shutdown_status().is_none());
    permit.commit(|| ()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let dialog = loop {
        if let Ok(dialog) = unsafe { GetWindow(native, GW_ENABLEDPOPUP) } {
            if dialog != native {
                break dialog;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Exit confirmation did not appear"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    };
    let command = cx
        .update(|app| owner.borrow().window_exit_command(invoking, app))
        .unwrap()
        .unwrap();
    command.request_exit();
    assert!(matches!(
        cx.update(|app| RunningProcessOwner::route_exit_work(
            owner,
            request,
            Ok(duplicate),
            app,
            |_, _| panic!("duplicate routing cannot replace callback"),
        ))
        .unwrap(),
        Err(ExitWorkError::IntentBusy)
    ));
    assert_eq!(
        unsafe { GetWindow(native, GW_ENABLEDPOPUP) }.unwrap(),
        dialog
    );
    if confirm {
        unsafe {
            PostMessageW(
                Some(dialog),
                TDM_CLICK_BUTTON.0 as u32,
                WPARAM(IDOK.0 as usize),
                LPARAM(0),
            )
        }
        .unwrap();
    } else {
        RunningProcessOwner::cancel_shutdown_confirmation(owner).unwrap();
    }
    wait(&delivered, cx).await;
    assert_eq!(calls.get(), 1);
    if let Some(replace) = replace_request {
        if replace {
            assert!(RunningProcessOwner::finish_exit(owner, request));
            command.request_exit();
            let mut successor = next_request(owner, cx).await;
            assert!(matches!(
                cx.update(|app| owner.borrow_mut().consume_exit_confirmation(request, app))
                    .unwrap(),
                Err(ExitConfirmationError::Request(_))
            ));
            assert!(matches!(
                cx.update(|app| owner
                    .borrow_mut()
                    .consume_exit_confirmation(&mut successor, app))
                    .unwrap(),
                Err(ExitConfirmationError::Unrelated)
            ));
            assert!(matches!(
                owner.borrow_mut().take_shutdown_confirmation().unwrap(),
                Some(ShutdownConfirmationResult::Confirmed(_))
            ));
            *request = successor;
        } else {
            let result = cx
                .update(|app| owner.borrow_mut().consume_exit_confirmation(request, app))
                .unwrap()
                .unwrap();
            assert_eq!(
                result,
                Some(if confirm {
                    ExitConfirmationRoute::AwaitingObservation
                } else {
                    ExitConfirmationRoute::Cancelled
                })
            );
            assert_eq!(
                cx.update(|app| owner.borrow_mut().consume_exit_confirmation(request, app))
                    .unwrap()
                    .unwrap(),
                None
            );
            permit.commit(|| ()).unwrap();
            if confirm {
                assert_eq!(
                    owner.borrow().shutdown_status(),
                    Some((
                        invoking,
                        ShutdownIntent::ApplicationExit,
                        RunningShutdownStatus::AwaitingObservation
                    ))
                );
                owner.borrow_mut().end_unadmitted_shutdown().unwrap();
            }
        }
    } else {
        match owner
            .borrow_mut()
            .take_shutdown_confirmation()
            .unwrap()
            .unwrap()
        {
            ShutdownConfirmationResult::Confirmed(context) if confirm => {
                assert_eq!(context.invoking(), invoking);
                assert_eq!(context.intent(), ShutdownIntent::ApplicationExit);
                assert_eq!(context.observation().running_threads(), 1);
                assert!(context.observation().has_work());
            }
            ShutdownConfirmationResult::Cancelled if !confirm => {}
            _ => panic!("unexpected Exit confirmation outcome"),
        }
    }
    cx.update(|app| {
        assert_eq!(
            owner.borrow().resolve_exit_window(request, app).unwrap(),
            invoking
        )
    })
    .unwrap();
    assert!(owner.borrow().shutdown_status().is_none());
    permit.commit(|| ()).unwrap();
}
