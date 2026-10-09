use super::*;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::VK_RETURN,
    WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP},
};

#[test]
fn native_switcher_elsewhere_pointer_and_enter_preserve_both_selected_windows() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, _, cx| {
            Box::pin(async move {
                let original_windows = windows(&owner);
                let invoking = original_windows[0];
                let other = original_windows[1];
                let before = snapshot(&owner);
                for actual in &original_windows {
                    let resident = composer(*actual, cx).await;
                    let deadline = Instant::now() + Duration::from_secs(10);
                    loop {
                        if cx
                            .update(|app| {
                                let resident = resident.read(app);
                                let input = resident.gpui_input();
                                let input = input.read(app);
                                input.is_quiescent()
                                    && input.is_surface_current_and_interactive()
                                    && !resident.test_has_active_flight()
                            })
                            .unwrap()
                        {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "native selected editor did not become coherent"
                        );
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                    }
                }
                let views = cx
                    .update(|app| {
                        original_windows
                            .iter()
                            .map(|window| {
                                let root = window.read(app).unwrap();
                                let mount = root.controller().unwrap().composer_mount().unwrap();
                                let resident = mount.read(app).contribution().unwrap();
                                let composer = resident.read(app);
                                let input = composer.gpui_input();
                                let surface = input.read(app).surface().unwrap();
                                let identity = composer.selection_identity();
                                assert_eq!(
                                    identity.binding().logical_extent().logical_utf8_bytes(),
                                    0
                                );
                                assert_eq!(surface.binding(), identity.binding().range_binding());
                                assert!(surface.pages().iter().all(|page| {
                                    page.range().start().get() == 0
                                        && page.range().end().get() == 0
                                        && page.text().is_empty()
                                }));
                                (
                                    *window,
                                    root.controller().unwrap().window_id(),
                                    mount.clone(),
                                    resident.clone(),
                                    input.clone(),
                                    identity,
                                    surface.selection(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                let other_thread = views
                    .iter()
                    .find(|view| view.0 == other)
                    .unwrap()
                    .5
                    .claim()
                    .thread_id();
                let invoking_identity = views.iter().find(|view| view.0 == invoking).unwrap().5;
                let handles = vec![native(invoking, cx).await, native(other, cx).await];
                let placements = vec![capture(invoking, cx).await, capture(other, cx).await];
                wait_for_unviewed_catalog_source(&owner, other_thread, cx).await;
                invoking
                    .update(cx, |_, window, _| window.activate_window())
                    .unwrap();
                let (picker, key) = open(invoking, other_thread, cx).await;
                let history = invoking
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                let (actual_key, point, reason) = invoking
                    .read_with(cx, |root, _| {
                        root.test_thread_switcher_visible_row_point(other_thread)
                            .unwrap()
                    })
                    .unwrap();
                assert_eq!(actual_key, key);
                assert!(
                    reason
                        .as_deref()
                        .is_some_and(|reason| reason.contains("another window"))
                );
                let scale = invoking
                    .update(cx, |_, window, _| window.scale_factor())
                    .unwrap();
                let x = (f32::from(point.x) * scale).round() as i32;
                let y = (f32::from(point.y) * scale).round() as i32;
                assert!((0..=i16::MAX as i32).contains(&x));
                assert!((0..=i16::MAX as i32).contains(&y));
                let client = LPARAM((((y as u32) << 16) | x as u32) as isize);
                unsafe {
                    PostMessageW(Some(handles[0]), WM_LBUTTONDOWN, WPARAM(1), client).unwrap();
                    PostMessageW(Some(handles[0]), WM_LBUTTONUP, WPARAM(0), client).unwrap();
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                while !cx
                    .update(|app| picker.read(app).focused_key() == Some(&key))
                    .unwrap()
                {
                    assert!(
                        Instant::now() < deadline,
                        "native pointer did not focus the exact unavailable row"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                unsafe {
                    PostMessageW(
                        Some(handles[0]),
                        WM_KEYDOWN,
                        WPARAM(VK_RETURN.0 as usize),
                        LPARAM(1),
                    )
                    .unwrap();
                    PostMessageW(
                        Some(handles[0]),
                        WM_KEYUP,
                        WPARAM(VK_RETURN.0 as usize),
                        LPARAM(0xc0000001u32 as isize),
                    )
                    .unwrap();
                }
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                assert_eq!(snapshot(&owner).windows(), before.windows());
                assert_eq!(windows(&owner), original_windows);
                cx.update(|app| {
                    assert!(app.active_window() == Some(invoking.into()));
                    let root = invoking.read(app).unwrap();
                    assert_eq!(root.test_thread_switcher_picker().as_ref(), Some(&picker));
                    assert!(root.test_thread_switcher_failure().is_none());
                    assert_eq!(root.test_thread_navigation_history(), history);
                    for (window, id, mount, resident, input, identity, selection) in &views {
                        let root = window.read(app).unwrap();
                        assert_eq!(root.controller().unwrap().window_id(), *id);
                        assert_eq!(
                            root.controller().unwrap().composer_mount().as_ref(),
                            Some(mount)
                        );
                        let composer = resident.read(app);
                        assert_eq!(composer.selection_identity(), *identity);
                        assert_eq!(&composer.gpui_input(), input);
                        let surface = input.read(app).surface().unwrap();
                        assert_eq!(surface.binding(), identity.binding().range_binding());
                        assert_eq!(surface.selection(), *selection);
                        assert_eq!(identity.binding().logical_extent().logical_utf8_bytes(), 0);
                        assert!(surface.pages().iter().all(|page| {
                            page.range().start().get() == 0
                                && page.range().end().get() == 0
                                && page.text().is_empty()
                        }));
                    }
                })
                .unwrap();
                for (index, window) in original_windows.iter().enumerate() {
                    assert_eq!(native(*window, cx).await, handles[index]);
                    assert!(unsafe { IsWindow(Some(handles[index])).as_bool() });
                    assert_eq!(capture(*window, cx).await, placements[index]);
                }
                invoking
                    .update(cx, |root, window, app| {
                        root.test_thread_switcher_dismiss(window, app)
                    })
                    .unwrap();
                drop((picker, views));
                *qualified.borrow_mut() = Some((
                    invoking_identity.window_id(),
                    invoking_identity.claim().thread_id(),
                ));
                activate_exit(invoking, cx);
            })
        },
        true,
        2,
    );
}
