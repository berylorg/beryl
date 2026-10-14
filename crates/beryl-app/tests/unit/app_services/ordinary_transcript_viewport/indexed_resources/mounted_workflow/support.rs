use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::SyndicDraftId;
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicTimestamp, TurnEndStatus};

pub(super) const OTHER_THREAD: SyndicThreadId = SyndicThreadId::from_bytes([0xD8; 16]);

pub(super) fn seed_other_thread(path: &std::path::Path) {
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("combined workflow Home");
    };
    let home = candidate.publish().unwrap();
    let selected = state
        .session()
        .minimal_bootstrap(&home)
        .unwrap()
        .unwrap()
        .windows()[0]
        .selected_thread()
        .unwrap()
        .thread_id();
    let execution = syndic
        .thread_execution(
            &home,
            selected,
            super::super::branch_discussion::fixture::limit(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(syndic.create_thread(
            syndic.revision(&home).unwrap(),
            CreateThread::ordinary(
                OTHER_THREAD,
                SyndicDraftId::from_bytes([0xD9; 16]),
                execution,
                SyndicTimestamp::from_unix_millis(2000),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    super::super::branch_discussion::fixture::append(
        &home,
        &syndic,
        OTHER_THREAD,
        super::super::branch_discussion::SOURCE,
        Some(TurnEndStatus::complete()),
    );
    drop((state, syndic));
    home.close().unwrap();
}

pub(super) async fn activate(
    window: WindowHandle<MainWindowShellRoot>,
    target: SyndicThreadId,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |root, native, cx| {
            root.request_ordinary_thread_activation(target, native, cx, |result, _, _| {
                assert!(
                    result.is_ok(),
                    "ordinary activation admission failed: {result:?}"
                );
            })
            .unwrap()
        })
        .unwrap();
    let mut observations = 0_u32;
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    let failure = root.test_running_activation_failure();
                    assert!(failure.is_none(), "ordinary activation failed: {failure:?}");
                    if observations % 50 == 0 {
                        eprintln!(
                            "mounted activation publication: target={target:?}, viewed={:?}, pending={}, ordinary={}",
                            root.coherent_viewed_thread(app),
                            root.test_running_thread_activation_pending(),
                            root.test_ordinary_thread_activation_diagnostics(),
                        );
                    }
                    observations += 1;
                    root.coherent_viewed_thread(app) == Some(target)
                        && !root.test_running_thread_activation_pending()
                })
                .unwrap()
        },
        "exact ordinary thread activation must publish coherently",
    )
    .await;
}

pub(super) fn wheel(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
    x: f32,
    y: f32,
) {
    pointer(
        native,
        app,
        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase: gpui::TouchPhase::Moved,
            ..Default::default()
        }),
    );
}

pub(super) fn click(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
) {
    pointer(
        native,
        app,
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
    pointer(
        native,
        app,
        PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
}

pub(super) async fn leading(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| panel.viewport_bounds_for_test().is_some())
                .unwrap()
        },
        "published transcript viewport",
    )
    .await;
    with_native(window, cx, |native, app| {
        let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
        wheel(
            native,
            app,
            point(bounds.left() + px(1.0), bounds.center().y),
            0.0,
            100_000.0,
        );
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, app| {
                let panel = panel.read(app);
                panel.narrative_geometry_current_for_test(native)
                    && panel.measured_frame_for_test().is_some_and(|frame| {
                        frame
                            .records
                            .first()
                            .is_some_and(|row| row.index == 0 && row.top_px == 0.0)
                    })
            })
            .unwrap()
        },
        "native outer wheel must reach the exact leading record",
    )
    .await;
}

pub(super) async fn place(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
    location: impl Fn(&SyndicTranscriptPanel) -> Option<gpui::Point<gpui::Pixels>>,
) {
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| location(panel).is_some())
                .unwrap()
        },
        "nested geometry must become available",
    )
    .await;
    with_native(window, cx, |native, app| {
        let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
        let target = location(panel.read(app)).unwrap();
        wheel(
            native,
            app,
            point(bounds.left() + px(1.0), bounds.center().y),
            0.0,
            f32::from(bounds.center().y - target.y),
        );
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, app| {
                let panel = panel.read(app);
                panel.narrative_geometry_current_for_test(native)
                    && location(panel)
                        .zip(panel.viewport_bounds_for_test())
                        .is_some_and(|(target, bounds)| bounds.contains(&target))
            })
            .unwrap()
        },
        "nested command must become visible under current outer geometry",
    )
    .await;
}

pub(super) fn select_words(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    with_native(window, cx, |native, app| {
        let (id, _, display) = record(panel.read(app), "before");
        let offset = display.find("café 🌸 words").unwrap();
        drag(
            native,
            app,
            point_at(panel.read(app), &id, offset),
            point_at(panel.read(app), &id, offset + "café 🌸 words".len()),
        );
    })
    .unwrap();
}

pub(super) async fn settled_transcript(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    wait_until(window, cx, |cx| {
        window.read_with(cx, |root, _| !root.test_transcript_delivery_pending()).unwrap()
            && with_native(window, cx, |native, app| {
                let panel = panel.read(app);
                !panel.narrative_states_for_test().5
                    && !panel.measurement_diagnostics().0
                    && panel.narrative_geometry_current_for_test(native)
                    && panel.resource_demand().is_none()
                    && panel.narrative_demand().is_none()
            }).unwrap()
    }, "mixed transcript preparation and queued delivery must settle before a new captured command").await;
}

pub(super) fn words_width(
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) -> gpui::Pixels {
    panel
        .read_with(cx, |panel, _| {
            let (id, _, display) = record(panel, "before");
            let start = display.find("café 🌸 words").unwrap();
            point_at(panel, &id, start + "café 🌸 words".len()).x - point_at(panel, &id, start).x
        })
        .unwrap()
}

pub(super) fn bounded(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    panel
        .read_with(cx, |panel, _| {
            let counts = panel.owned_presentation_counts_for_test();
            assert!(
                counts.0 <= 128 && counts.1 <= 2 && counts.2 <= 4 && counts.3 <= 128,
                "bounded presentation owners: {counts:?}"
            );
            let diagnostics = panel.diagnostic_snapshot();
            assert!(diagnostics.resident_data.resident_record_count <= 512);
            assert!(diagnostics.resident_data.active_pin_count <= 16);
            assert!(diagnostics.resident_data.pending_provider_requests <= 9);
            assert_eq!(diagnostics.resident_data.decoded_or_uploaded_media_bytes, 0);
            assert!(
                diagnostics
                    .table_panels
                    .iter()
                    .all(|table| table.cells <= 128)
            );
            assert!(
                diagnostics
                    .code_panels
                    .iter()
                    .all(|code| code.layout_fragments <= 32)
            );
        })
        .unwrap();
    assert!(
        window
            .read_with(cx, |root, _| root.test_transcript_worker_retained())
            .unwrap()
            <= 9
    );
}

pub(super) async fn context_copy(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    let length = super::super::branch_discussion::SELECTED.len();
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    panel
                        .literal_narrative_text_for_test()
                        .iter()
                        .any(|(id, text)| {
                            text == super::super::branch_discussion::SELECTED
                                && panel
                                    .literal_narrative_position_for_test(id, length)
                                    .is_some()
                        })
                })
                .unwrap()
        },
        "created discussion context must paint exact literal source",
    )
    .await;
    let (copied, _) = writer(panel, cx);
    with_native(window, cx, |native, app| {
        let snapshot = panel.read(app).snapshot();
        let id = snapshot
            .records
            .iter()
            .find(|record| {
                matches!(
                    record.provenance.source,
                    ResidentRecordSource::LocalContext { .. }
                )
            })
            .unwrap()
            .id
            .clone();
        assert_eq!(
            panel
                .read(app)
                .diagnostic_snapshot()
                .resident_data
                .synthetic_context_group_count,
            1
        );
        panel.update(app, |panel, _| {
            assert!(matches!(
                panel.apply_realized_quote_target_for_record_ids(&[id.clone()]),
                crate::syndic_transcript::ResidentQuoteOutcome::Unavailable(_)
            ));
            assert!(matches!(
                panel.apply_realized_context_menu_target_for_record_id(&id),
                crate::syndic_transcript::ResidentContextMenuOutcome::Unavailable(_)
            ));
        });
        let first = panel
            .read(app)
            .literal_narrative_position_for_test(&id, 0)
            .unwrap();
        let last = panel
            .read(app)
            .literal_narrative_position_for_test(&id, length)
            .unwrap();
        drag(native, app, first, last);
        key(native, app, "shift-f10");
        assert!(!panel.read(app).quote_menu_open_for_test());
        pointer(
            native,
            app,
            PlatformInput::MouseDown(gpui::MouseDownEvent {
                position: first,
                button: gpui::MouseButton::Right,
                click_count: 1,
                ..Default::default()
            }),
        );
        assert!(panel.read(app).resident_context_menu_target().is_none());
        assert!(!panel.read(app).quote_menu_open_for_test());
    })
    .unwrap();
    for (index, expected) in [super::super::branch_discussion::SELECTED, "café 🌸 words"]
        .into_iter()
        .enumerate()
    {
        with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
        wait_until(
            window,
            cx,
            |_| copied.borrow().len() == index + 1,
            "context paired Copy must settle through the ordinary worker",
        )
        .await;
        assert_eq!(copied.borrow()[index], expected);
    }
}

pub(super) async fn dispose(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |root, native, app| {
            root.set_ordinary_close_interaction_gated(true, app);
            root.suspend_running_thread_reads(native, app);
        })
        .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            window
                .update(cx, |root, _, _| {
                    root.release_suspended_running_thread_sources()
                })
                .unwrap()
        },
        "owned transcript workers must drain before disposal",
    )
    .await;
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_transcript_worker_retained())
            .unwrap(),
        0
    );
    window
        .update(cx, |root, native, app| {
            root.retire_running_thread_views(native, app).unwrap()
        })
        .unwrap();
    panel.update(cx, |panel, _| panel.retire()).unwrap();
    with_native(window, cx, |native, app| {
        native.draw_and_present_for_test(app)
    })
    .unwrap();
    panel
        .read_with(cx, |panel, _| {
            let diagnostics = panel.diagnostic_snapshot();
            assert!(diagnostics.code_panels.is_empty() && diagnostics.table_panels.is_empty());
            assert_eq!(diagnostics.resident_data.active_pin_count, 0);
            assert_eq!(diagnostics.resident_data.pending_provider_requests, 0);
            assert_eq!(diagnostics.resident_data.resident_record_count, 0);
            assert_eq!(
                (
                    diagnostics.resident_data.resident_view_record_count,
                    diagnostics.resident_data.resident_projection_record_count,
                    diagnostics.resident_data.resident_resource_metadata_count,
                    diagnostics.resident_data.resident_resource_slice_count
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(
                (
                    diagnostics.resident_data.projection_bytes,
                    diagnostics.resident_data.presentation_bytes,
                    diagnostics.resident_data.resource_bytes,
                    diagnostics.resident_data.resource_slice_bytes,
                    diagnostics.resident_data.geometry_bytes,
                    diagnostics.resident_data.pin_bytes
                ),
                (0, 0, 0, 0, 0, 0)
            );
            assert!(panel.narrative_selection_ranges_for_test().is_none());
            assert!(!panel.quote_menu_open_for_test());
            assert_eq!(panel.owned_presentation_counts_for_test(), (0, 0, 0, 0));
        })
        .unwrap();
    cx.update(|app| app.quit()).unwrap();
}
