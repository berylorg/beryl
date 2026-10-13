use super::super::super::narrative_native::{drag, require_fixture_ledger};
use super::*;
use crate::syndic_transcript::*;

async fn copy_context_both(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
    captured: &Rc<RefCell<Vec<String>>>,
    markdown: &str,
    stripped: &str,
) {
    let before = captured.borrow().len();
    with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
    wait_until(
        window,
        cx,
        |_| captured.borrow().len() == before + 1,
        "selected context Markdown must reach the checked writer",
    )
    .await;
    assert_eq!(captured.borrow()[before], markdown);
    with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
    wait_until(
        window,
        cx,
        |_| captured.borrow().len() == before + 2,
        "selected context stripped worker must reach the checked writer",
    )
    .await;
    assert_eq!(captured.borrow()[before + 1], stripped);
}

fn prepared_context(
    snapshot: &ResidentTranscriptSnapshot,
    narrative_kind_for: impl Fn(&ResidentPresentationRecordId) -> Option<TranscriptNarrativeKind>,
) -> PreparedTranscriptActivation {
    let view = snapshot
        .records
        .iter()
        .find_map(|record| match &record.provenance.source {
            ResidentRecordSource::Syndic(source) => Some(source.view_id.clone()),
            _ => None,
        })
        .unwrap();
    let mut views = Vec::new();
    let mut projections = Vec::new();
    for record in snapshot.records.iter() {
        let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
            continue;
        };
        let (kind, payload, narrative_kind) = match &record.kind {
            ResidentPresentationRecordKind::TextChunk {
                narrative_kind,
                text,
                plain_text,
            } => (
                ProjectionRecordKind::TextChunk,
                ProjectionPayload::Text {
                    text: text.clone(),
                    plain_text: plain_text.clone(),
                },
                narrative_kind.clone(),
            ),
            ResidentPresentationRecordKind::NarrativePending { source } => (
                ProjectionRecordKind::TextChunk,
                ProjectionPayload::NarrativePending {
                    source: source.clone(),
                },
                narrative_kind_for(&record.id).expect("genuine pending source classification"),
            ),
            ResidentPresentationRecordKind::NarrativeReady { prepared } => (
                ProjectionRecordKind::TextChunk,
                ProjectionPayload::NarrativeReady {
                    prepared: prepared.clone(),
                },
                narrative_kind_for(&record.id).expect("genuine prepared source classification"),
            ),
            ResidentPresentationRecordKind::NarrativeUnavailable { source, reason } => (
                ProjectionRecordKind::TextChunk,
                ProjectionPayload::NarrativeUnavailable {
                    source: source.clone(),
                    reason: *reason,
                },
                narrative_kind_for(&record.id).expect("genuine unavailable source classification"),
            ),
            _ => continue,
        };
        let id = record.provenance.projection_id.clone().unwrap();
        views.push(TranscriptViewRecord {
            id: TranscriptViewRecordId(record.id.0.clone()),
            position: source.position.unwrap(),
            projection_id: id.clone(),
            narrative_kind,
            provenance: source.clone(),
        });
        projections.push(ProjectionRecord {
            id,
            revision: record.provenance.projection_revision.unwrap(),
            kind,
            payload,
            provenance: source.clone(),
        });
    }
    let mut prepared = PreparedTranscriptActivation::new(
        view.clone(),
        TranscriptActivationPlacement::Tail,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view.clone(),
            revision: ProviderRevision(1),
            history_state: TranscriptProviderHistoryState::Complete,
            records: views,
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: view.clone(),
                revision: ProviderRevision(1),
                records: projections,
                rejections: Vec::new(),
            },
        )),
    );
    prepared.discussion_context = Some(SyntheticContextGroup {
        provenance: Arc::new(SyntheticContextProvenance {
            view_id: view,
            group_id: "native-context".into(),
            insertion_after: 1,
            insertion_parent: beryl_model::SyndicTurnId::from_bytes([12; 16]),
            descriptor: None,
            envelope_revision: 1,
            chunk_count: 1,
        }),
        chunks: vec![SyntheticContextChunk {
            range: 0..23,
            text: "# Context\n**selected**\n".into(),
            plain_text: "Context\nselected\n".into(),
        }],
    });
    prepared
}

#[test]
fn native_discussion_context_copy_is_exact_then_stripped_and_has_no_quote_or_menu_target() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("Authored source")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some()
                                    && !panel.snapshot().records.is_empty()
                            })
                            .unwrap()
                    },
                    "authored source admission",
                )
                .await;
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let (copy_admission, copy_workers) =
                    crate::main_window::WorkerAdmission::owned_for_test(1);
                assert!(copy_admission.available());
                panel
                    .update(cx, |panel, _| {
                        panel.set_code_save_admission(copy_admission.clone())
                    })
                    .unwrap();
                let prepared = panel
                    .read_with(cx, |panel, _| {
                        prepared_context(&panel.snapshot(), |id| panel.narrative_kind_for_test(id))
                    })
                    .unwrap();
                panel
                    .update(cx, |panel, cx| {
                        panel.stage_activation(&prepared, None);
                        cx.notify();
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| panel.measurement_ready())
                            .unwrap()
                    },
                    "context measured frame",
                )
                .await;
                panel
                    .update(cx, |panel, cx| {
                        panel.publish_coherent_activation(prepared);
                        cx.notify();
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some()
                                    && panel.snapshot().records.iter().any(|record| {
                                        matches!(
                                            record.provenance.source,
                                            ResidentRecordSource::LocalContext { .. }
                                        )
                                    })
                            })
                            .unwrap()
                    },
                    "context realized frame",
                )
                .await;
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let sink = captured.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_keyboard_copy_writer_for_test(move |item, _, _| {
                            sink.borrow_mut().push(item.text().unwrap());
                            Ok(())
                        })
                    })
                    .unwrap();
                let context_id = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .snapshot()
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
                            .clone()
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .literal_narrative_position_for_test(&context_id, 0)
                                    .is_some()
                                    && panel
                                        .literal_narrative_position_for_test(&context_id, 23)
                                        .is_some()
                            })
                            .unwrap()
                    },
                    "context pointer endpoints must come from painted literal geometry",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let id = panel
                        .read(app)
                        .snapshot()
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
                    panel.update(app, |panel, cx| {
                        assert!(matches!(
                            panel.apply_realized_quote_target_for_record_ids(&[id.clone()]),
                            ResidentQuoteOutcome::Unavailable(_)
                        ));
                        assert!(matches!(
                            panel.apply_realized_context_menu_target_for_record_id(&id),
                            ResidentContextMenuOutcome::Unavailable(_)
                        ));
                    });
                    let first = panel
                        .read(app)
                        .literal_narrative_position_for_test(&id, 0)
                        .unwrap();
                    let last = panel
                        .read(app)
                        .literal_narrative_position_for_test(&id, 23)
                        .unwrap();
                    drag(native, app, first, last);
                    key(native, app, "shift-f10");
                    assert!(!panel.read(app).quote_menu_open_for_test());
                })
                .unwrap();
                copy_context_both(
                    window,
                    cx,
                    &captured,
                    "# Context\n**selected**\n",
                    "Context\nselected\n",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let first = panel
                        .read(app)
                        .literal_narrative_position_for_test(&context_id, 12)
                        .unwrap();
                    let last = panel
                        .read(app)
                        .literal_narrative_position_for_test(&context_id, 20)
                        .unwrap();
                    drag(native, app, last, first);
                })
                .unwrap();
                copy_context_both(window, cx, &captured, "selected", "selected").await;
                assert!(
                    captured
                        .borrow()
                        .iter()
                        .all(|text| !text.contains("DISCUSSING"))
                );
                copy_workers.suspend();
                assert!(copy_admission.acquire().is_none());
                wait_until(
                    window,
                    cx,
                    |_| copy_workers.retained() == 0,
                    "context copy worker custody must drain before fixture retirement",
                )
                .await;
                retire_code_fixture(window, &panel, cx).await;
                panel
                    .read_with(cx, |panel, _| assert!(panel.snapshot().records.is_empty()))
                    .unwrap();
            })
        },
    );
}

#[test]
fn native_long_context_scrolls_away_and_back_with_bounded_frame_and_remeasures_theme_width() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("Source")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some()
                                    && !panel.snapshot().is_empty()
                            })
                            .unwrap()
                    },
                    "source measurement",
                )
                .await;
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let (copy_admission, copy_workers) =
                    crate::main_window::WorkerAdmission::owned_for_test(1);
                assert!(copy_admission.available());
                panel
                    .update(cx, |panel, _| {
                        panel.set_code_save_admission(copy_admission.clone())
                    })
                    .unwrap();
                let mut prepared = panel
                    .read_with(cx, |panel, _| {
                        prepared_context(&panel.snapshot(), |id| panel.narrative_kind_for_test(id))
                    })
                    .unwrap();
                let Some(TranscriptProviderResponseKind::ViewPage(page)) =
                    &prepared.view_page_response
                else {
                    panic!("context fixture preserves its genuine source view page");
                };
                let total_records = page.records.len() + 32;
                let group = prepared.discussion_context.as_mut().unwrap();
                let text = ("  repeated   spaces ".repeat(7) + "\n").repeat(14);
                assert!(text.len() <= CONTEXT_CHUNK_MAX_BYTES);
                group.chunks = (0..32)
                    .map(|index| SyntheticContextChunk {
                        range: (index * text.len()) as u64..((index + 1) * text.len()) as u64,
                        text: Arc::from(text.as_str()),
                        plain_text: Arc::from(text.as_str()),
                    })
                    .collect();
                Arc::make_mut(&mut group.provenance).chunk_count = 32;
                panel
                    .update(cx, |panel, cx| {
                        panel.stage_activation(&prepared, None);
                        cx.notify();
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| panel.measurement_ready())
                            .unwrap()
                    },
                    "long context staged frame",
                )
                .await;
                panel
                    .update(cx, |panel, cx| {
                        panel.publish_coherent_activation(prepared);
                        cx.notify();
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some_and(|frame| {
                                    frame
                                        .records
                                        .last()
                                        .is_some_and(|record| record.index + 1 == total_records)
                                })
                            })
                            .unwrap()
                    },
                    "long context tail",
                )
                .await;
                let (last_id, context_id) = panel
                    .read_with(cx, |panel, _| {
                        let snapshot = panel.snapshot();
                        let context = snapshot
                            .records
                            .iter()
                            .rev()
                            .find(|record| {
                                matches!(
                                    record.provenance.source,
                                    ResidentRecordSource::LocalContext { .. }
                                )
                            })
                            .unwrap();
                        (
                            snapshot.records.last().unwrap().id.clone(),
                            context.id.clone(),
                        )
                    })
                    .unwrap();
                let original = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let before = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .measured_frame_for_test()
                            .unwrap()
                            .records
                            .iter()
                            .find(|record| record.record_id == context_id)
                            .unwrap()
                            .height_px
                    })
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    native_appearance::system_font_appearance_with_context_size(
                        owner.test_services().graph().unwrap().state(),
                        28,
                    )
                };
                let large = Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    original.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                cx.update(|app| MainWindowShellRoot::test_apply_appearance(window, large, app))
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.measured_frame_for_test().is_some_and(|frame| {
                                    frame
                                        .records
                                        .iter()
                                        .find(|record| record.record_id == context_id)
                                        .is_some_and(|record| record.height_px > before * 1.5)
                                })
                            })
                            .unwrap()
                    },
                    "context theme measurement",
                )
                .await;
                with_native(window, cx, |native, app| {
                    let size = native.viewport_size();
                    native.resize(gpui::size(
                        px(f32::from(size.width).max(400.0) * 0.8),
                        size.height,
                    ));
                })
                .unwrap();
                for forward in [false, true, false, true] {
                    with_native(window, cx, |native, app| {
                        let position = panel.read(app).viewport_bounds_for_test().unwrap().center();
                        native.dispatch_input_for_test(
                            PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Pixels(point(
                                    px(0.0),
                                    px(if forward { -300_000.0 } else { 300_000.0 }),
                                )),
                                touch_phase: gpui::TouchPhase::Moved,
                                ..Default::default()
                            }),
                            app,
                        );
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    panel.measured_frame_for_test().is_some_and(|frame| {
                                        assert!(frame.records.len() <= 4);
                                        if forward {
                                            frame
                                                .records
                                                .last()
                                                .is_some_and(|record| record.record_id == last_id)
                                        } else {
                                            frame
                                                .records
                                                .first()
                                                .is_some_and(|record| record.index == 0)
                                        }
                                    })
                                })
                                .unwrap()
                        },
                        "context scroll completion",
                    )
                    .await;
                    panel
                        .read_with(cx, |panel, _| {
                            assert_eq!(panel.snapshot().record_count(), total_records);
                            assert_eq!(
                                panel
                                    .diagnostic_snapshot()
                                    .resident_data
                                    .synthetic_context_group_count,
                                1
                            );
                            assert!(
                                panel
                                    .diagnostic_snapshot()
                                    .resident_data
                                    .realized_context_chunk_count
                                    <= 4
                            );
                        })
                        .unwrap();
                }
                copy_workers.suspend();
                assert!(copy_admission.acquire().is_none());
                wait_until(
                    window,
                    cx,
                    |_| copy_workers.retained() == 0,
                    "long context worker custody must drain before fixture retirement",
                )
                .await;
                retire_code_fixture(window, &panel, cx).await;
                panel
                    .read_with(cx, |panel, _| assert!(panel.snapshot().is_empty()))
                    .unwrap();
            })
        },
    );
}
