use super::*;

#[test]
fn native_mounted_image_renditions_paint_in_transcript_and_composer_and_release_on_close() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        |path| {
            super::image_markers::fixture::seed_png(path);
            seed_other_thread(path);
        },
        |owner, _, cx| {
            Box::pin(async move {
                let attachment = owner.borrow().media_attachment().unwrap();
                let media = owner.borrow().test_services().media_process().unwrap();
                let window = windows(&owner)[0];
                let root = cx.update(|app| window.entity(app).unwrap()).unwrap();
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["before", "after"]).await;
                settled_transcript(window, &panel, cx).await;
                let markers = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .snapshot()
                            .records
                            .iter()
                            .flat_map(|record| record.authored_parts())
                            .filter(|part| {
                                matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_))
                            })
                            .map(|part| part.id.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                assert_eq!(markers.len(), 2);
                for (index, marker) in markers.iter().enumerate() {
                    if index == 1 {
                        media.domain().lose_device_for_test();
                    }
                    with_native(window, cx, |native, app| {
                        let bounds = panel.read(app).image_marker_bounds(marker, native).unwrap();
                        click(native, app, bounds.center());
                    })
                    .unwrap();
                    if index == 1 {
                        wait_for_native_recovery(window, &media, cx).await;
                    }
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            window
                                .read_with(cx, |root, _| {
                                    root.test_image_marker_rendition()
                                        == Some((true, true, true, false))
                                })
                                .unwrap()
                        },
                        "the exact transcript marker must paint admitted Ready pixels",
                    )
                    .await;
                    with_native(window, cx, |_, app| {
                        assert_eq!(
                            root.read(app).test_image_marker_pixel().unwrap().unwrap(),
                            [255, 0, 0, 255]
                        );
                    })
                    .unwrap();
                    if index == 0 {
                        let decodes = media.diagnostics().unwrap().decodes;
                        let uploads = media.domain().diagnostics().uploads;
                        media.domain().lose_device_for_test();
                        with_native(window, cx, |native, _| native.refresh()).unwrap();
                        wait_for_native_recovery(window, &media, cx).await;
                        wait_until(window, cx, |cx| {
                            window.read_with(cx, |root, _| root.test_image_marker_rendition() == Some((true, true, true, false))).unwrap()
                                && media.domain().diagnostics().uploads > uploads
                        }, "the current visible inspection must reauthorize and reupload after generation change").await;
                        assert_eq!(media.diagnostics().unwrap().decodes, decodes);
                    }
                    with_native(window, cx, |native, app| {
                        assert_eq!(
                            root.read(app).test_image_marker_pixel().unwrap().unwrap(),
                            [255, 0, 0, 255]
                        );
                        assert_eq!(
                            root.read(app)
                                .test_image_marker_inspection()
                                .unwrap()
                                .target
                                .record
                                .record_id,
                            *marker
                        );
                        key(native, app, "escape");
                    })
                    .unwrap();
                    assert!(
                        window
                            .read_with(cx, |root, _| root.test_image_marker_rendition().is_none())
                            .unwrap()
                    );
                }
                with_native(window, cx, |native, app| {
                    let bounds = panel
                        .read(app)
                        .image_marker_bounds(&markers[0], native)
                        .unwrap();
                    click(native, app, bounds.center());
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_marker_inspection()
                                .is_some_and(|inspection| {
                                    inspection.target.record.record_id == markers[0]
                                })
                        })
                        .unwrap()
                    },
                    "the captured marker inspection must mount before closing its own request",
                )
                .await;
                with_native(window, cx, |native, app| key(native, app, "escape")).unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| root.test_image_marker_inspection().is_none())
                            .unwrap()
                    },
                    "Escape must close the mounted inspection before testing late completion",
                )
                .await;
                for _ in 0..5 {
                    with_native(window, cx, |native, app| {
                        native.draw_and_present_for_test(app)
                    })
                    .unwrap();
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_image_marker_rendition().is_none())
                        .unwrap()
                );
                edit(window, &panel, cx).await;
                let resident = composer(window, cx).await;
                replacement_ready(window, &resident, &panel, true, cx).await;
                let input = resident
                    .read_with(cx, |resident, _| resident.gpui_input())
                    .unwrap();
                open_composer_view(window, &resident, &input, 0, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |resident, _| {
                                resident.test_image_preview_rendition()
                                    == Some((true, true, true, false))
                            })
                            .unwrap()
                    },
                    "composer View must paint the exact current candidate marker",
                )
                .await;
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        resident
                            .read(app)
                            .test_image_preview_pixel()
                            .unwrap()
                            .unwrap(),
                        [255, 0, 0, 255]
                    );
                    key(native, app, "escape");
                })
                .unwrap();
                assert!(
                    resident
                        .read_with(cx, |resident, _| resident
                            .test_image_preview_rendition()
                            .is_none())
                        .unwrap()
                );
                open_composer_view(window, &resident, &input, 1, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |resident, _| {
                                resident.test_image_preview_rendition()
                                    == Some((true, true, true, false))
                            })
                            .unwrap()
                    },
                    "a new View must retain its own marker identity",
                )
                .await;
                with_native(window, cx, |_, app| {
                    assert_eq!(
                        resident
                            .read(app)
                            .test_image_preview_pixel()
                            .unwrap()
                            .unwrap(),
                        [255, 0, 0, 255]
                    );
                })
                .unwrap();
                activate(window, OTHER_THREAD, cx).await;
                assert!(
                    resident
                        .read_with(cx, |resident, _| resident
                            .test_image_preview_rendition()
                            .is_none())
                        .unwrap()
                );
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_image_marker_rendition().is_none())
                        .unwrap()
                );
                drain_media(window, &media, &attachment, cx).await;
                dispose(window, &panel, cx).await;
            })
        },
    );
}

async fn wait_for_native_recovery(
    window: WindowHandle<MainWindowShellRoot>,
    media: &crate::image_processing::ImageMediaProcess,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_image_marker_rendition() == Some((false, false, true, true))
                })
                .unwrap()
        },
        "native unavailability must retain one current failed interest for progress",
    )
    .await;
    let origin = window
        .read_with(cx, |root, _| root.test_image_marker_inspection().unwrap())
        .unwrap();
    let panel = window
        .read_with(cx, |root, _| root.test_transcript_panel())
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(350);
    let before = media.diagnostics().unwrap();
    while Instant::now() < deadline {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app)
        })
        .unwrap();
        let actual = window
            .read_with(cx, |root, _| root.test_image_marker_rendition())
            .unwrap();
        let source_currentness = window
            .read_with(cx, |root, _| {
                root.test_image_marker_inspection().map(|current| {
                    (
                        current.request == origin.request,
                        current.target.record.record_id == origin.target.record.record_id,
                        current.activation == origin.activation,
                        current.presentation == origin.presentation,
                    )
                })
            })
            .unwrap();
        let source_boundary = with_native(window, cx, |native, app| {
            let panel = panel.read(app);
            let snapshot = panel.snapshot();
            (
                snapshot.activation_revision == origin.activation,
                snapshot.presentation_revision == origin.presentation,
                panel.image_marker_bounds(&origin.target.record.record_id, native)
                    == Some(origin.bounds),
                panel.narrative_geometry_current_for_test(native),
                panel.measurement_diagnostics().0,
            )
        })
        .unwrap();
        assert!(
            matches!(actual, Some((false, false, true, _))),
            "unavailable native interest changed: actual={actual:?}, source_currentness={source_currentness:?}, source_boundary(activation,presentation,bounds,geometry,measurement_pending)={source_boundary:?}, before={before:?}, current={:?}, generation={:?}, domain_failure={:?}, domain={:?}",
            media.diagnostics().unwrap(),
            media.domain().generation(),
            media.domain().failure(),
            media.domain().diagnostics(),
        );
        window
            .read_with(cx, |root, _| {
                let current = root.test_image_marker_inspection().unwrap();
                assert_eq!(current.request, origin.request);
                assert_eq!(
                    current.target.record.record_id,
                    origin.target.record.record_id
                );
                assert_eq!(current.activation, origin.activation);
                assert_eq!(current.presentation, origin.presentation);
            })
            .unwrap();
        let progress = media.diagnostics().unwrap();
        assert!(
            progress.failures.saturating_sub(before.failures)
                <= progress
                    .native_revision
                    .saturating_sub(before.native_revision)
                    + 1,
            "a failed interest must not retry without bounded native progress"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    media.domain().recover_device_for_test();
    with_native(window, cx, |native, _| native.refresh()).unwrap();
}

pub(super) async fn open_composer_view(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &Entity<MainWindowConversationComposer>,
    input: &Entity<gpui_text_input::RangeTextInput>,
    index: usize,
    cx: &mut AsyncApp,
) {
    let expected = input
        .read_with(cx, |input, _| {
            input.surface().unwrap().realized_objects()[index].id()
        })
        .unwrap();
    with_native(window, cx, |native, app| {
        let bounds = native.debug_bounds_for_test("conversation-composer-root").unwrap();
        let position = {
            let input = input.read(app);
            let surface = input.surface().unwrap();
            let marker = surface.realized_objects()[index];
            let local = marker.hit_bounds().center();
            assert!(matches!(surface.hit_test_composite(local), Some(gpui_text_input::RangeSurfaceHit::Object(object)) if object.id() == expected));
            bounds.origin + local - point(px(0.0), surface.scroll_block())
        };
        assert!(bounds.contains(&position), "the selected marker must be visible inside the actual mounted composer: {position:?} in {bounds:?}");
        assert!(gpui::Bounds::new(point(px(0.0), px(0.0)), native.viewport_size()).contains(&position));
        click(native, app, position);
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            resident
                .read_with(cx, |resident, _| {
                    resident
                        .marker_menu()
                        .is_some_and(|menu| menu.anchor().object_id == expected)
                })
                .unwrap()
        },
        "the exact composer marker menu must mount before View is clicked",
    )
    .await;
    with_native(window, cx, |native, app| {
        assert_eq!(
            resident.read(app).marker_menu().unwrap().anchor().object_id,
            expected
        );
        let bounds = native
            .debug_bounds_for_test("conversation-composer-marker-view")
            .unwrap();
        click(native, app, bounds.center());
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            resident
                .read_with(cx, |resident, _| {
                    resident
                        .image_preview()
                        .is_some_and(|preview| preview.origin().object_id == expected)
                })
                .unwrap()
        },
        "View must mount inspection for the exact composer marker",
    )
    .await;
}

pub(super) async fn drain_media(
    window: WindowHandle<MainWindowShellRoot>,
    media: &crate::image_processing::ImageMediaProcess,
    attachment: &crate::image_processing::ImageMediaAttachment,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |root, _, _| root.close_image_media_window())
        .unwrap();
    attachment.retire();
    media.begin_shutdown();
    wait_until(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, app| {
                native.draw_and_present_for_test(app);
                app.poll_image_resource_retirement(media.domain()).unwrap();
            })
            .unwrap();
            let attachment_drained = attachment.retire() == gpui::ImageResourceDrain::Drained;
            media.poll_shutdown() == gpui::ImageResourceDrain::Drained && attachment_drained
        },
        "all media workers and native image owners must drain before fixture disposal",
    )
    .await;
    let resources = media.domain().diagnostics();
    assert_eq!(
        (
            resources.staging_buffers,
            resources.staging_bytes,
            resources.reserved_textures,
            resources.resident_textures,
            resources.texture_bytes,
            resources.unique_pins,
            resources.frame_uses
        ),
        (0, 0, 0, 0, 0, 0, 0)
    );
    let work = media.diagnostics().unwrap();
    assert_eq!(
        (
            work.queued,
            work.active_workers,
            work.flights,
            work.path_slots,
            work.verification_slots,
            work.cpu.entries,
            work.cpu.bytes
        ),
        (0, 0, 0, 0, 0, 0, 0)
    );
    assert!(media.domain().failure().is_none());
}
