use super::*;

#[test]
fn native_lineage_parent_open_elsewhere_is_represented_and_never_reveals_other_window() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home_selections(
        2,
        0,
        lineage_fixture::prepare_home,
        final_selection,
        move |owner, _, cx| {
            Box::pin(async move {
                let windows_before = windows(&owner);
                let invoking = windows_before[0];
                let other = windows_before[1];
                prepare(invoking, cx).await;
                let parent = SyndicThreadId::from_bytes([242; 16]);
                let (picker, key) = switcher_entry::open(other, parent, cx).await;
                other
                    .update(cx, |_, _, app| {
                        picker.update(app, |picker, cx| picker.activate(&key, cx))
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    if invoking
                        .update(cx, |root, window, app| {
                            let (other_claim, _) = other
                                .read(app)
                                .unwrap()
                                .coherent_selected_thread_title(app)?;
                            if other_claim.thread_id() != parent {
                                return None;
                            }
                            let widget = root.test_thread_lineage_view()?;
                            let (point, _, available) =
                                widget.read(app).test_parent_input(parent, window)?;
                            (!available && !widget.read(app).test_inert()).then_some(point)
                        })
                        .unwrap()
                        .is_some()
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "occupied exact parent never became an inert represented breadcrumb"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                let before = snapshot(&owner);
                let history = invoking
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                let native = native(invoking, cx).await;
                invoking
                    .update(cx, |_, window, _| window.activate_window())
                    .unwrap();
                assert!(unsafe { IsWindow(Some(native)).as_bool() });
                focus(invoking, parent, cx).await;
                let point = loop {
                    let focused = invoking
                        .update(cx, |root, window, app| {
                            if app.active_window() != Some(invoking.into())
                                || !root.test_thread_lineage_focus(
                                    lineage_fixture::child(),
                                    parent,
                                    window,
                                    app,
                                )
                            {
                                return None;
                            }
                            let widget = root.test_thread_lineage_view()?;
                            let (point, _, available) =
                                widget.read(app).test_parent_input(parent, window)?;
                            (!available && !widget.read(app).test_inert()).then_some(point)
                        })
                        .unwrap();
                    if let Some(point) = focused {
                        break point;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "the actual child lineage owner did not acquire focus before input"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                };
                let scale = invoking
                    .update(cx, |_, window, _| window.scale_factor())
                    .unwrap();
                let x = (f32::from(point.x) * scale).round() as u32;
                let y = (f32::from(point.y) * scale).round() as u32;
                let client = LPARAM(((y << 16) | (x & 0xffff)) as isize);
                let foreground_before = unsafe { GetForegroundWindow() };
                eprintln!(
                    "native lineage unavailable input: native_valid=true gpui_child_focus=true physical_child_foreground={}",
                    foreground_before == native,
                );
                unsafe {
                    PostMessageW(Some(native), WM_LBUTTONDOWN, WPARAM(1), client).unwrap();
                }
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
                unsafe {
                    PostMessageW(Some(native), WM_LBUTTONUP, WPARAM(0), client).unwrap();
                }
                for key in [VK_RETURN.0, VK_SPACE.0] {
                    unsafe {
                        PostMessageW(Some(native), WM_KEYDOWN, WPARAM(key as usize), LPARAM(1))
                            .unwrap();
                        PostMessageW(
                            Some(native),
                            WM_KEYUP,
                            WPARAM(key as usize),
                            LPARAM(0xc0000001u32 as isize),
                        )
                        .unwrap();
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                assert_eq!(windows(&owner), windows_before);
                assert_eq!(snapshot(&owner).windows(), before.windows());
                invoking
                    .update(cx, |root, window, app| {
                        assert!(unsafe { IsWindow(Some(native)).as_bool() });
                        assert_eq!(unsafe { GetForegroundWindow() }, foreground_before);
                        assert!(app.active_window() == Some(invoking.into()));
                        assert_eq!(root.test_thread_navigation_history(), history);
                        assert!(!root.test_running_thread_activation_pending());
                        assert!(root.test_thread_lineage_focus(
                            lineage_fixture::child(),
                            parent,
                            window,
                            app
                        ));
                        assert_eq!(
                            root.test_thread_confirmation_visible_transcript_claim()
                                .unwrap()
                                .thread_id(),
                            lineage_fixture::child()
                        );
                    })
                    .unwrap();
                let id = invoking
                    .read_with(cx, |root, _| root.controller().unwrap().window_id())
                    .unwrap();
                let other_id = other
                    .read_with(cx, |root, _| root.controller().unwrap().window_id())
                    .unwrap();
                *qualified.borrow_mut() =
                    Some(vec![(id, lineage_fixture::child()), (other_id, parent)]);
                activate_exit(invoking, cx);
            })
        },
        true,
        2,
    );
}
