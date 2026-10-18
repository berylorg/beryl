use super::*;
use crate::bootstrap::test_support::{Producer, ProducerHold};
use std::{cell::RefCell, rc::Rc};

#[path = "indexed_resources.rs"]
mod indexed_resources;

fn indexed_code_loaded(panel: &crate::syndic_transcript::SyndicTranscriptPanel) -> bool {
    panel.snapshot().resources.metadata.iter().any(|metadata| {
        metadata.kind == crate::syndic_transcript::ResourceKind::Code
            && panel
                .indexed_resource_response(&metadata.resource_id)
                .is_some_and(|response| {
                    !matches!(
                        response.value,
                        crate::syndic_transcript::IndexedResourceValue::Pending
                    ) && response.root.is_some()
                })
    }) && panel.code_painted_body_glyphs_for_test() > 0
}

#[test]
fn native_mounted_resource_failure_yields_to_selected_history_refresh() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```rust\n{}\n```",
                    "let payload = 1;\n".repeat(2000)
                )),
            )
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let mut pending = None;
                for _ in 0..150 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    pending = window
                        .update(cx, |root, _, app| {
                            root.test_pending_resource_read().then(|| {
                                let panel = root.test_transcript_panel();
                                let activation = panel.read(app).snapshot().activation_revision;
                                (panel, activation)
                            })
                        })
                        .unwrap();
                    if pending.is_some() {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                let (panel, prior_activation) =
                    pending.expect("mounted text range request before replacement");
                let mut fixture_committed = false;
                for _ in 0..32 {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let home = graph.home();
                    let storage = graph.syndic();
                    let home_revision = home.home_revision().unwrap();
                    let domain_revision = storage.revision(home).unwrap();
                    let thread = graph
                        .state()
                        .session()
                        .minimal_bootstrap(home)
                        .unwrap()
                        .unwrap()
                        .windows()[0]
                        .selected_thread()
                        .unwrap()
                        .thread_id();
                    let point = syndic_storage::SyndicPointReadLimit::new(65536).unwrap();
                    let head = storage
                        .transcript_view_head(home, thread, point)
                        .unwrap()
                        .unwrap();
                    let summary = storage
                        .history_summary(home, thread, point)
                        .unwrap()
                        .unwrap();
                    let current = storage.thread(home, thread, point).unwrap().unwrap();
                    let build = storage
                        .transcript_build(home, thread, head.generation(), point)
                        .unwrap()
                        .unwrap();
                    if home.home_revision().unwrap() != home_revision
                        || storage.revision(home).unwrap() != domain_revision
                    {
                        continue;
                    }
                    assert_eq!(
                        build.phase(),
                        syndic_storage::TranscriptBuildPhase::Complete
                    );
                    assert_eq!(build.revision(), head.revision());
                    assert_eq!(build.entry_count(), head.entry_count());
                    let revision = beryl_model::ProjectionRevision::new(
                        head.revision().get().max(summary.revision().get()) + 1,
                    )
                    .unwrap();
                    let mut batch = syndic_storage::test_faults::FixtureBatch::new();
                    batch
                        .put(syndic_storage::test_faults::FixtureRecord::TranscriptBuild(
                            syndic_storage::TranscriptBuildRecord::new(
                                build.thread_id(),
                                build.generation(),
                                revision,
                                build.source_thread_revision(),
                                build.committed_tail(),
                                build.selected_path_digest(),
                                build.path_turn_count(),
                                build.entry_count(),
                                build.entry_digest(),
                                build.history_complete(),
                                build.phase(),
                            ),
                        ))
                        .unwrap();
                    batch
                        .put(
                            syndic_storage::test_faults::FixtureRecord::TranscriptViewHead(
                                syndic_storage::TranscriptViewHeadRecord::new(
                                    thread,
                                    head.generation(),
                                    revision,
                                    head.entry_count(),
                                    current.committed_tail(),
                                    current.selected_path_digest(),
                                    syndic_storage::ProjectionLifecycle::Current,
                                ),
                            ),
                        )
                        .unwrap();
                    batch
                        .put(syndic_storage::test_faults::FixtureRecord::HistorySummary(
                            syndic_storage::HistorySummaryRecord::new(
                                thread,
                                revision,
                                current.revision(),
                                current.committed_tail(),
                                current.selected_path_digest(),
                                build.history_complete(),
                                syndic_storage::SyndicTimestamp::from_unix_millis(2000),
                            ),
                        ))
                        .unwrap();
                    let mut command = beryl_home_store::HomeCommand::new(home_revision);
                    command
                        .add(storage.fixture_contribution(domain_revision, batch))
                        .unwrap();
                    let outcome = home.execute(command);
                    if matches!(&outcome,
                        beryl_home_store::CommandOutcome::NotCommitted { evidence }
                            if evidence.conflicts().is_some_and(|conflicts| {
                                !conflicts.is_empty() && conflicts.iter().all(|conflict| matches!(
                                    conflict,
                                    beryl_home_store::RevisionConflict::Home { .. }
                                        | beryl_home_store::RevisionConflict::Domain { domain: "syndic", .. }
                                ))
                            })
                    ) {
                        continue;
                    }
                    assert!(
                        matches!(
                            &outcome,
                            beryl_home_store::CommandOutcome::Committed {
                                later_failure: None,
                                ..
                            }
                        ),
                        "resource refresh fixture command failed: {outcome:?}"
                    );
                    assert!(revision.get() > summary.revision().get());
                    let refreshed = storage
                        .transcript_build(home, thread, head.generation(), point)
                        .unwrap()
                        .unwrap();
                    let refreshed_head = storage
                        .transcript_view_head(home, thread, point)
                        .unwrap()
                        .unwrap();
                    let refreshed_summary = storage
                        .history_summary(home, thread, point)
                        .unwrap()
                        .unwrap();
                    assert_eq!(refreshed.revision(), refreshed_head.revision());
                    assert_eq!(refreshed.entry_count(), refreshed_head.entry_count());
                    assert_eq!(refreshed.committed_tail(), refreshed_head.committed_tail());
                    assert_eq!(
                        refreshed.selected_path_digest(),
                        refreshed_head.selected_path_digest()
                    );
                    assert_eq!(refreshed.history_complete(), refreshed_summary.complete());
                    fixture_committed = true;
                    break;
                }
                assert!(
                    fixture_committed,
                    "resource refresh fixture admission remained contended"
                );
                let mut refreshed = false;
                for _ in 0..200 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    refreshed = window
                        .update(cx, |root, _, app| {
                            let snapshot = panel.read(app).snapshot();
                            root.coherent_selected_thread_title(app).is_some()
                                && snapshot.activation_revision > prior_activation
                                && !snapshot.resources.metadata.is_empty()
                                && indexed_code_loaded(panel.read(app))
                        })
                        .unwrap();
                    if refreshed {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(
                    refreshed,
                    "resource failure must yield to current history and resume resource loading"
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

#[test]
fn native_mounted_text_resources_load_after_selection_and_dispose_pending_ranges() {
    for dispose_pending in [false, true] {
        let metadata_hold = Rc::new(RefCell::new(None));
        let prepared_hold = metadata_hold.clone();
        run_mounted_with_prepared_home_retained(
            1,
            0,
            move |path| {
                seed_history_text(
                    path,
                    1,
                    Some(&format!(
                        "```rust\n{}\n```",
                        "let payload = 1;\n".repeat(2000)
                    )),
                );
                let StartupHomeOpen::Ready {
                    candidate,
                    state,
                    syndic,
                } = support::open(path)
                else {
                    panic!("resource fixture Home")
                };
                let home = candidate.publish().unwrap();
                let thread = state
                    .session()
                    .minimal_bootstrap(&home)
                    .unwrap()
                    .unwrap()
                    .windows()[0]
                    .selected_thread()
                    .unwrap()
                    .thread_id();
                drop((state, syndic));
                home.close().unwrap();
                *prepared_hold.borrow_mut() = Some((
                    ProducerHold::install(Producer::ResourceMetadata, *thread.as_bytes()),
                    thread,
                ));
            },
            move |owner, _, cx| {
                Box::pin(async move {
                    let (metadata_hold, selected_thread) =
                        metadata_hold.borrow_mut().take().unwrap();
                    let metadata = metadata_hold.gate();
                    let window = windows(&owner)[0];
                    let mut published = None;
                    for _ in 0..150 {
                        cx.update(|app| {
                            app.update_window(window.into(), |_, native, app| {
                                native.draw_and_present_for_test(app)
                            })
                            .unwrap()
                        })
                        .unwrap();
                        published = window
                            .update(cx, |root, _, app| {
                                if metadata.entered() == 0 {
                                    return None;
                                }
                                let title = root.coherent_selected_thread_title(app)?;
                                let panel = root.test_transcript_panel();
                                let snapshot = panel.read(app).snapshot();
                                if snapshot.records.is_empty() {
                                    return None;
                                }
                                let anchor = panel.read(app).measured_frame_for_test()?.anchor?;
                                Some((panel, title, snapshot.activation_revision, anchor))
                            })
                            .unwrap();
                        if published.is_some() {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    let (panel, title, activation, _) =
                        published.expect("selected text resource publication");
                    assert_eq!(title.0.thread_id(), selected_thread);
                    let range_gate = window
                        .update(cx, |root, _, app| {
                            if !dispose_pending {
                                return None;
                            }
                            let demand = panel.read(app).resource_demand().unwrap();
                            assert!(demand.indexed.is_none());
                            assert!(demand.metadata.is_none());
                            assert!(demand.range.is_none());
                            let gate = crate::transcript_provider::IndexedReadGate::code_range(
                                demand.revision,
                                0..16,
                            );
                            root.test_diagnostic_resource_gate(gate.clone());
                            Some(gate)
                        })
                        .unwrap();
                    let anchor = cx
                        .update(|app| {
                            assert!(
                                panel.read(app).snapshot().resources.metadata.is_empty(),
                                "selection must precede resource metadata publication"
                            );
                            assert!(panel.read(app).snapshot().resources.slices.is_empty());
                            let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
                            app.update_window(window.into(), |_, native, app| {
                                native.dispatch_input_for_test(
                                    gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                        position: bounds.center(),
                                        delta: gpui::ScrollDelta::Pixels(gpui::point(
                                            gpui::px(0.0),
                                            gpui::px(1.25),
                                        )),
                                        modifiers: Default::default(),
                                        touch_phase: gpui::TouchPhase::Moved,
                                    }),
                                    app,
                                );
                                native.draw_and_present_for_test(app);
                            })
                            .unwrap();
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .anchor
                                .unwrap()
                        })
                        .unwrap();
                    metadata.release();
                    drop(metadata_hold);
                    let mut settled = false;
                    for _ in 0..150 {
                        cx.update(|app| {
                            app.update_window(window.into(), |_, native, app| {
                                native.draw_and_present_for_test(app)
                            })
                            .unwrap()
                        })
                        .unwrap();
                        settled = window
                            .update(cx, |root, native, app| {
                                assert_eq!(
                                    root.coherent_selected_thread_title(app),
                                    Some(title.clone())
                                );
                                assert_eq!(
                                    panel.read(app).snapshot().activation_revision,
                                    activation
                                );
                                if dispose_pending {
                                    range_gate.as_ref().unwrap().entered()
                                        && root.test_pending_resource_read()
                                        && root.test_fence_pending_resource_read(native, app)
                                } else {
                                    indexed_code_loaded(panel.read(app))
                                }
                            })
                            .unwrap();
                        if settled {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    assert!(
                        settled,
                        "mounted resource must load or expose its pending range to disposal"
                    );
                    if let Some(gate) = &range_gate {
                        gate.release();
                    }
                    let before_disposal = cx
                        .update(|app| {
                            let current = panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .anchor
                                .unwrap();
                            assert_eq!(current.record_id, anchor.record_id);
                            assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
                            panel.read(app).snapshot()
                        })
                        .unwrap();
                    let before_indexed = cx
                        .update(|app| {
                            before_disposal
                                .resources
                                .metadata
                                .iter()
                                .filter_map(|metadata| {
                                    panel
                                        .read(app)
                                        .indexed_resource_response(&metadata.resource_id)
                                        .map(|response| (metadata.resource_id.clone(), response))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap();
                    if dispose_pending {
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
                        assert!(
                            drained,
                            "pending resource worker and returned output must drain"
                        );
                        window
                            .update(cx, |root, native, app| {
                                assert!(root.running_thread_reads_drained());
                                root.retire_running_thread_views(native, app).unwrap();
                            })
                            .unwrap();
                        cx.update(|app| {
                            assert!(Arc::ptr_eq(
                                &before_disposal.records,
                                &panel.read(app).snapshot().records
                            ));
                            assert!(Arc::ptr_eq(
                                &before_disposal.resources.metadata,
                                &panel.read(app).snapshot().resources.metadata
                            ));
                            for (resource, before) in &before_indexed {
                                let retained = panel
                                    .read(app)
                                    .indexed_resource_response(resource)
                                    .expect("inert indexed custody retained until view retirement");
                                assert!(Arc::ptr_eq(before, &retained));
                            }
                            assert!(Arc::ptr_eq(
                                &before_disposal.resources.slices,
                                &panel.read(app).snapshot().resources.slices
                            ));
                            panel.update(app, |panel, _| panel.retire());
                        })
                        .unwrap();
                        for _ in 0..4 {
                            cx.background_executor()
                                .timer(Duration::from_millis(20))
                                .await;
                            cx.update(|app| {
                                assert!(panel.read(app).snapshot().resources.slices.is_empty());
                                assert!(panel.read(app).snapshot().resources.metadata.is_empty());
                                for (resource, _) in &before_indexed {
                                    assert!(
                                        panel
                                            .read(app)
                                            .indexed_resource_response(resource)
                                            .is_none()
                                    );
                                }
                            })
                            .unwrap();
                        }
                    }
                    cx.update(|app| app.quit()).unwrap();
                })
            },
        );
    }
}
