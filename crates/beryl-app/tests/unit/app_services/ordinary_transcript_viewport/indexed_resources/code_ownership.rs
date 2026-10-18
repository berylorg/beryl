use super::*;

#[test]
fn native_code_panels_transfer_wheel_ownership_and_release_retired_models() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            let block = format!("```rust\n{}\n```", "let payload = 1;\n".repeat(2000));
            seed_history_text(path, 1, Some(&format!("{block}\n\n{block}")));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .update(cx, |root, _, _| root.test_transcript_panel())
                    .unwrap();
                let mut ready = false;
                for frame in 0..300 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app);
                            if frame == 0 {
                                native.minimize_window();
                            }
                        })
                        .unwrap();
                    })
                    .unwrap();
                    ready = panel.read_with(cx, |panel, _| {
                        let viewports = panel.code_viewports_for_test();
                        viewports.len() == 2 && viewports.iter().all(|(resource, bounds, _, _)| {
                            bounds.size.height > gpui::px(0.0)
                                && panel.indexed_resource_response(resource).is_some_and(|response| {
                                    matches!(&response.value, IndexedResourceValue::Range(range)
                                        if !range.items.is_empty() && range.items.len() <= 16)
                                })
                        })
                    }).unwrap();
                    if ready {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(ready, "two provider-backed Code panels must mount");
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let viewports = panel.read(app).code_viewports_for_test();
                        assert_ne!(viewports[0].0, viewports[1].0);
                        assert!(
                            viewports
                                .iter()
                                .all(|(_, _, top, selected)| *top == 0 && !selected)
                        );
                        let outer = panel.read(app).viewport_bounds_for_test().unwrap();
                        let before = panel
                            .read(app)
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px;
                        let keys: Vec<_> = panel
                            .read(app)
                            .diagnostic_snapshot()
                            .code_panels
                            .iter()
                            .map(|code| code.key)
                            .collect();
                        assert!(keys.iter().all(Option::is_some));
                        assert_ne!(keys[0], keys[1]);
                        let positions: Vec<_> = viewports
                            .iter()
                            .map(|(_, bounds, _, _)| {
                                let top = f32::from(bounds.top()).max(f32::from(outer.top()));
                                let bottom =
                                    f32::from(bounds.bottom()).min(f32::from(outer.bottom()));
                                assert!(
                                    bottom > top,
                                    "both Code text viewports must intersect the outer viewport"
                                );
                                let position =
                                    gpui::point(outer.center().x, gpui::px((top + bottom) / 2.0));
                                assert!(bounds.contains(&position) && outer.contains(&position));
                                position
                            })
                            .collect();
                        for (selected, expected) in [(0, [1, 0]), (1, [1, 1]), (0, [2, 1])] {
                            code_interaction::press(native, app, positions[selected]);
                            code_interaction::release(native, app, positions[selected]);
                            let transferred = panel.read(app).code_viewports_for_test();
                            for (index, (_, _, _, owned)) in transferred.iter().enumerate() {
                                assert_eq!(*owned, index == selected);
                            }
                            code_interaction::wheel(native, app, positions[selected]);
                            let after = panel.read(app).code_viewports_for_test();
                            assert_eq!([after[0].2, after[1].2], expected);
                            assert_eq!(
                                panel
                                    .read(app)
                                    .measured_frame_for_test()
                                    .unwrap()
                                    .manual_scroll_total_px,
                                before
                            );
                            let diagnostics = panel.read(app).diagnostic_snapshot();
                            assert_eq!(
                                diagnostics
                                    .code_panels
                                    .iter()
                                    .filter(|code| code.selected_for_wheel)
                                    .count(),
                                1
                            );
                            assert_eq!(
                                diagnostics
                                    .code_panels
                                    .iter()
                                    .map(|code| code.key)
                                    .collect::<Vec<_>>(),
                                keys
                            );
                        }
                    })
                    .unwrap();
                })
                .unwrap();
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let mut drained = false;
                for _ in 0..100 {
                    drained = window
                        .update(cx, |root, _, _| {
                            root.release_suspended_running_thread_sources()
                        })
                        .unwrap();
                    if drained {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(drained);
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let original = panel.read(app).code_viewports_for_test();
                        let selected_key = panel.read(app).diagnostic_snapshot().code_panels[0].key;
                        for (resource, remaining) in [(&original[1].0, 1), (&original[0].0, 0)] {
                            let released = panel.update(app, |panel, _| {
                                panel.override_code_owner_for_test(resource)
                            });
                            for _ in 0..5 {
                                native.draw_and_present_for_test(app);
                            }
                            assert!(
                                released.upgrade().is_none(),
                                "retirement must release the scroll model"
                            );
                            let viewports = panel.read(app).code_viewports_for_test();
                            let diagnostics = panel.read(app).diagnostic_snapshot();
                            assert_eq!(viewports.len(), remaining);
                            assert_eq!(diagnostics.code_panels.len(), remaining);
                            if remaining == 1 {
                                assert_eq!(viewports[0].0, original[0].0);
                                assert_eq!(viewports[0].2, 2);
                                assert!(viewports[0].3);
                                assert_eq!(diagnostics.code_panels[0].key, selected_key);
                                assert!(diagnostics.code_panels[0].selected_for_wheel);
                                let bounds = viewports[0].1;
                                let outer_bounds =
                                    panel.read(app).viewport_bounds_for_test().unwrap();
                                let top =
                                    f32::from(bounds.top()).max(f32::from(outer_bounds.top()));
                                let bottom = f32::from(bounds.bottom())
                                    .min(f32::from(outer_bounds.bottom()));
                                assert!(
                                    bottom > top,
                                    "remaining Code text viewport must intersect the outer viewport: code={bounds:?}, outer={outer_bounds:?}"
                                );
                                let position = gpui::point(
                                    outer_bounds.center().x,
                                    gpui::px((top + bottom) / 2.0),
                                );
                                assert!(bounds.contains(&position));
                                assert!(outer_bounds.contains(&position));
                                let outer = panel
                                    .read(app)
                                    .measured_frame_for_test()
                                    .unwrap()
                                    .manual_scroll_total_px;
                                code_interaction::wheel(native, app, position);
                                assert_eq!(panel.read(app).code_viewports_for_test()[0].2, 3);
                                assert_eq!(
                                    panel
                                        .read(app)
                                        .measured_frame_for_test()
                                        .unwrap()
                                        .manual_scroll_total_px,
                                    outer
                                );
                            }
                        }
                    })
                    .unwrap();
                })
                .unwrap();
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        panel.update(app, |panel, _| panel.retire());
                        native.draw_and_present_for_test(app);
                        assert!(panel.read(app).code_viewports_for_test().is_empty());
                        assert!(panel.read(app).diagnostic_snapshot().code_panels.is_empty());
                    })
                    .unwrap();
                    app.quit();
                })
                .unwrap();
            })
        },
    );
}
