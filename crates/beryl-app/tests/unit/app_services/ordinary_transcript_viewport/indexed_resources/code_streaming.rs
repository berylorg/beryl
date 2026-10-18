use super::*;
use crate::support::exact_cas;
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId};
use syndic_storage::{
    ProviderAgentMessageV1, ProviderFrameOrdinalV1, ProviderItemDeltaV1, ProviderItemFrameV1,
    ProviderItemLifecycle, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderTextV1, SourceEventPayload, SyndicPointReadLimit,
    SyndicTimestamp, TurnEndStatus, TurnLifecycle,
};

fn agent(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: None,
        memory_citation: None,
    })
}

#[test]
fn native_saved_streaming_code_preserves_mount_and_manual_anchor_through_growth_and_completion() {
    streaming_code(None);
}

#[test]
fn native_ordinary_driver_streams_saved_code_and_preserves_manual_anchor() {
    streaming_code_with_deadline(
        Some(OpeningFence {
            marker: "```",
            delimiter_fragments: &[],
            fragments: &["rust", "\n"],
            line_ending: "\n",
        }),
        Duration::from_secs(60),
    );
}

#[test]
fn native_ordinary_driver_preserves_backtick_code_across_split_crlf() {
    streaming_code(Some(OpeningFence {
        marker: "```",
        delimiter_fragments: &[],
        fragments: &["rust", "\r", "\n"],
        line_ending: "\r\n",
    }));
}

#[test]
fn native_ordinary_driver_preserves_tilde_code_across_split_crlf() {
    streaming_code(Some(OpeningFence {
        marker: "~~~",
        delimiter_fragments: &[],
        fragments: &["rust", "\r", "\n"],
        line_ending: "\r\n",
    }));
}

#[test]
fn native_ordinary_driver_recognizes_fragmented_backtick_delimiter() {
    streaming_code(Some(OpeningFence {
        marker: "```",
        delimiter_fragments: &["`", "`", "`"],
        fragments: &["rust", "\n"],
        line_ending: "\n",
    }));
}

#[test]
fn native_ordinary_driver_recognizes_fragmented_tilde_delimiter() {
    streaming_code(Some(OpeningFence {
        marker: "~~~",
        delimiter_fragments: &["~", "~", "~"],
        fragments: &["rust", "\r", "\n"],
        line_ending: "\r\n",
    }));
}

#[derive(Clone, Copy)]
struct OpeningFence {
    marker: &'static str,
    delimiter_fragments: &'static [&'static str],
    fragments: &'static [&'static str],
    line_ending: &'static str,
}

mod live_capture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_code_capture.rs"
    ));
}

type StreamingRuntimeFixture = (
    live_capture::server::NormalTerminalServer,
    crate::cas_projection::AdmittedProjectionSession,
    beryl_model::ExecutionBinding,
);

fn configure_streaming_runtime(
    services: &crate::app_services::ProcessServiceOwner,
    readiness_deadline: Instant,
) -> StreamingRuntimeFixture {
    let server = live_capture::server::NormalTerminalServer::spawn_controlled_connection_loss();
    let connector = beryl_backend::ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        live_capture::server::AUTHORIZATION,
    );
    let graph = services.graph().unwrap();
    let selected = graph
        .state()
        .session()
        .minimal_bootstrap(graph.home())
        .unwrap()
        .unwrap();
    let thread = selected.windows()[0].selected_thread().unwrap().thread_id();
    let binding = graph
        .syndic()
        .thread_execution(
            graph.home(),
            thread,
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let spec = loop {
        assert!(
            Instant::now() < readiness_deadline,
            "streaming runtime fixture did not admit a launch specification before its original readiness deadline"
        );
        match graph.sessions().test_launch_spec_for(thread, &binding) {
            Ok(spec) => break spec,
            Err(crate::cas_projection::RuntimeInterestError::AdmissionUnavailable) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                panic!("streaming runtime fixture launch specification failed: {error:?}")
            }
        }
    };
    assert!(
        Instant::now() < readiness_deadline,
        "streaming runtime fixture launch specification arrived after its original readiness deadline"
    );
    let session = graph
        .cas()
        .admit_configured_runtime_lifecycle_test_candidate(
            &connector,
            spec,
            binding.clone(),
            beryl_model::CasProcessGeneration::new(37_144).unwrap(),
            std::path::Path::new(binding.root_path().as_str()),
            live_capture::server::TIMEOUT,
        )
        .unwrap();
    assert!(!graph.cas().gui_runtime_submission_observer_for_test()(
        &binding
    ));
    (server, session, binding)
}

fn streaming_code(opening: Option<OpeningFence>) {
    streaming_code_with_deadline(opening, Duration::from_secs(60));
}

fn streaming_code_with_deadline(opening: Option<OpeningFence>, completion_deadline: Duration) {
    let live = opening.is_some();
    let opening = opening.unwrap_or(OpeningFence {
        marker: "```",
        delimiter_fragments: &[],
        fragments: &["rust", "\n"],
        line_ending: "\n",
    });
    let configured = Arc::new(std::sync::Mutex::new(None));
    let output = configured.clone();
    let runtime_configuration: Option<RuntimeFixtureConfiguration> = live.then(|| {
        Box::new(
            move |services: &crate::app_services::ProcessServiceOwner, readiness_deadline| {
                *output.lock().unwrap() =
                    Some(configure_streaming_runtime(services, readiness_deadline));
            },
        ) as RuntimeFixtureConfiguration
    });
    run_mounted_retained_with_runtime_configuration(
        1,
        0,
        if live { 5 } else { 4 },
        completion_deadline,
        runtime_configuration,
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                code_history_revision::draw(window, cx);
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let (home, storage, thread) = {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    let thread = graph
                        .state()
                        .session()
                        .minimal_bootstrap(graph.home())
                        .unwrap()
                        .unwrap()
                        .windows()[0]
                        .selected_thread()
                        .unwrap()
                        .thread_id();
                    (
                        graph.home().service_reference(),
                        graph.syndic().clone(),
                        thread,
                    )
                };
                let time = SyndicTimestamp::from_unix_millis;
                let point = SyndicPointReadLimit::new(1_000_000).unwrap();
                let user = SyndicItemId::from_bytes([241; 16]);
                let assistant = SyndicItemId::from_bytes([242; 16]);
                let turn = exact_cas::submit_current_draft(
                    &home,
                    storage.clone(),
                    thread,
                    SyndicDraftId::from_bytes([243; 16]),
                    user,
                    if live {
                        live_capture::server::SUBMITTED_TEXT
                    } else {
                        "show code"
                    },
                    time(2001),
                );
                let cas_item = CasItemId::new("native-streaming-code").unwrap();
                let line = "let payload = 1;".repeat(16) + opening.line_ending;
                let initial = format!(
                    "{}{}{}",
                    opening.marker,
                    opening.fragments.concat(),
                    line.repeat(2000),
                );
                let mut capture = if live {
                    let (server, session, binding) = configured.lock().unwrap().take().unwrap();
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    assert_eq!(
                        &binding,
                        graph
                            .syndic()
                            .thread_execution(&home, thread, point)
                            .unwrap()
                            .unwrap()
                            .execution(),
                        "live capture keeps the original configured execution binding"
                    );
                    let driver = live_capture::Capture::start(
                        server,
                        session,
                        home.clone(),
                        storage.clone(),
                        graph.state().assets(),
                        thread,
                        binding,
                    );
                    driver.started(
                        opening
                            .delimiter_fragments
                            .first()
                            .copied()
                            .unwrap_or(opening.marker),
                    );
                    Some(driver)
                } else {
                    None
                };
                let source = if live {
                    None
                } else {
                    let source =
                        exact_cas::establish_turn(&home, storage.clone(), thread, turn, time(2002));
                    exact_cas::admit_event(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        &source,
                        SourceEventPayload::TurnActivated,
                        time(2003),
                    );
                    exact_cas::correlate_user_item(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        user,
                        &source,
                        time(2004),
                    );
                    Some(source)
                };
                if !live {
                    let source = source.as_ref().unwrap();
                    exact_cas::admit_item_frame(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        assistant,
                        source,
                        ProviderItemFrameV1::new(
                            ProviderFrameOrdinalV1::FIRST,
                            cas_item.clone(),
                            ProviderItemObservationV1::Started {
                                observed_at: ProviderLifecycleTimestampMsV1::new(2005),
                                item: agent(&initial),
                            },
                        ),
                        time(2005),
                    );
                    exact_cas::project_item_if_needed(&home, storage.clone(), user);
                    exact_cas::project_item_if_needed(&home, storage.clone(), assistant);
                    exact_cas::converge_transcript(&home, storage.clone(), thread);
                }
                let assert_live = || {
                    let state = storage.turn_state(&home, turn, point).unwrap().unwrap();
                    assert_eq!(state.lifecycle(), TurnLifecycle::Active);
                    assert_eq!(state.finalized_item_count(), 0);
                    if !live {
                        assert_eq!(
                            storage
                                .canonical_item(&home, assistant, point)
                                .unwrap()
                                .unwrap()
                                .provider_lifecycle(),
                            ProviderItemLifecycle::Started
                        );
                    }
                    assert!(
                        !storage
                            .history_summary(&home, thread, point)
                            .unwrap()
                            .unwrap()
                            .complete()
                    );
                };
                if live {
                    let mut prefix = String::new();
                    for (step, fragment) in opening.delimiter_fragments.iter().enumerate() {
                        prefix.push_str(fragment);
                        if prefix == opening.marker {
                            break;
                        }
                        let mut delivered = false;
                        for _ in 0..300 {
                            code_history_revision::draw(window, cx);
                            delivered = panel.read_with(cx, |panel, _| {
                                let snapshot = panel.snapshot();
                                let prefix_visible = snapshot.records.iter().any(|record| {
                                    matches!(
                                        &record.kind,
                                        crate::syndic_transcript::ResidentPresentationRecordKind::TextChunk { text, .. }
                                            if text.as_ref() == prefix
                                    ) || match &record.kind {
                                        crate::syndic_transcript::ResidentPresentationRecordKind::NarrativePending { source }
                                        | crate::syndic_transcript::ResidentPresentationRecordKind::NarrativeUnavailable { source, .. } => {
                                            matches!(source.as_ref(), syndic_storage::ProjectionPayload::InlineMarkdown { source, .. } if source.as_ref() == prefix)
                                        }
                                        crate::syndic_transcript::ResidentPresentationRecordKind::NarrativeReady { prepared } => prepared.source() == prefix,
                                        _ => false,
                                    }
                                });
                                prefix_visible && panel.diagnostic_snapshot().code_panels.is_empty()
                            }).unwrap();
                            if delivered {
                                break;
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(20))
                                .await;
                        }
                        assert!(
                            delivered,
                            "saved delimiter prefix did not mount as ordinary text"
                        );
                        assert_live();
                        capture
                            .as_ref()
                            .unwrap()
                            .append(opening.delimiter_fragments[step + 1]);
                    }
                    let mut initial_identity = None;
                    let mut body_start = opening.marker.len() as u64;
                    for step in 0..opening.fragments.len() + 3 {
                        let expected_lines = if step <= opening.fragments.len() {
                            0
                        } else if step == opening.fragments.len() + 1 {
                            1
                        } else {
                            2000
                        };
                        let mut mounted = None;
                        let mut last_facts = None;
                        for _ in 0..300 {
                            code_history_revision::draw(window, cx);
                            mounted = panel
                                .read_with(cx, |panel, _| {
                                    let snapshot = panel.diagnostic_snapshot();
                                    let code = snapshot.code_panels.first();
                                    let resource =
                                        panel.code_viewport_for_test().map(|view| view.0);
                                    let response = resource.as_ref().and_then(|resource| {
                                        panel.code_foreground_response_for_test(resource)
                                    });
                                    last_facts = Some((
                                        code.map(|code| {
                                            (
                                                code.logical_lines,
                                                code.source_bytes,
                                                code.realized_visual_lines,
                                                code.layout_state.to_owned(),
                                            )
                                        }),
                                        response.as_ref().map(|response| match &response.value {
                                            IndexedResourceValue::Pending => ("pending", None),
                                            IndexedResourceValue::Range(range) => (
                                                "range",
                                                Some((
                                                    range.binding.range.start,
                                                    range.binding.range.end,
                                                    range.items.len(),
                                                    range.request_complete,
                                                )),
                                            ),
                                            _ => ("other", None),
                                        }),
                                    ));
                                    let code = code?;
                                    let source_bytes = expected_lines * line.len() as u64;
                                    let response = response?;
                                    let IndexedResourceValue::Range(range) = &response.value else {
                                        return None;
                                    };
                                    (code.logical_lines == expected_lines
                                        && code.source_bytes == Some(source_bytes)
                                        && range.binding.range.start == body_start
                                        && range.binding.range.end == body_start + source_bytes
                                        && (expected_lines == 0 || !range.items.is_empty())
                                        && range.request_complete
                                        && code.realized_visual_lines
                                            == expected_lines.min(16) as usize)
                                        .then(|| panel.code_presentation_for_test().unwrap().0)
                                })
                                .unwrap();
                            if mounted.is_some() {
                                break;
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(20))
                                .await;
                        }
                        let identity = mounted.unwrap_or_else(|| panic!(
                            "saved fenced Code did not mount at its expected extent: step={step}, expected_lines={expected_lines}, body_start={body_start}, facts={last_facts:?}, capture_worker_finished={:?}",
                            capture.as_ref().map(|capture| capture.worker_finished()),
                        ));
                        if let Some(previous) = initial_identity.as_ref() {
                            assert_eq!(previous, &identity);
                        } else {
                            initial_identity = Some(identity);
                        }
                        assert_live();
                        if let Some(fragment) = opening.fragments.get(step) {
                            capture.as_ref().unwrap().append(fragment);
                            body_start += fragment.len() as u64;
                        } else if step == opening.fragments.len() {
                            capture.as_ref().unwrap().append(&line);
                        } else if step == opening.fragments.len() + 1 {
                            capture.as_ref().unwrap().append(&line.repeat(1999));
                        }
                    }
                }
                if live {
                    panel.read_with(cx, |panel, _| {
                        let resource = panel.code_viewport_for_test().unwrap().0;
                        let response = panel.code_foreground_response_for_test(&resource).unwrap();
                        let IndexedResourceValue::Range(range) = &response.value else {
                            panic!("grown opening fence must load an ordinary Code range");
                        };
                        assert!(!range.items.is_empty() && range.items.len() <= 16);
                        assert!(matches!(range.cursor.request, index::Request::Code { lines } if lines.start == 0));
                    }).unwrap();
                } else {
                    code_history_revision::wait_range(window, &panel, None, 0, cx).await;
                }
                assert_live();
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                        code_interaction::press(native, app, bounds.center());
                        code_interaction::release(native, app, bounds.center());
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.center(),
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(-45.5),
                                    gpui::px(-18010.5),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                code_history_revision::wait_painted_range(window, &panel, None, 900, cx).await;
                let (identity, key, resource, revision, anchor, outer) = panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.painted_glyphs_for_test() > 50);
                        assert_eq!(
                            panel.diagnostic_snapshot().code_panels[0].logical_lines,
                            2000
                        );
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel.code_viewport_for_test().unwrap().0,
                            panel.code_provider_revision_for_test().unwrap(),
                            panel.code_anchor_for_test().unwrap().position,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                        )
                    })
                    .unwrap();
                let delta = line.repeat(1000);
                if let Some(driver) = &capture {
                    driver.append(&delta);
                } else {
                    exact_cas::admit_item_frame(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        assistant,
                        source.as_ref().unwrap(),
                        ProviderItemFrameV1::new(
                            ProviderFrameOrdinalV1::new(2).unwrap(),
                            cas_item.clone(),
                            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                                delta: ProviderTextV1::inline(&delta),
                            }),
                        ),
                        time(2006),
                    );
                    exact_cas::project_item_if_needed(&home, storage.clone(), assistant);
                    exact_cas::converge_transcript(&home, storage.clone(), thread);
                }
                for _ in 0..300 {
                    code_history_revision::draw(window, cx);
                    let current_revision = storage
                        .transcript_view_head(&home, thread, point)
                        .unwrap()
                        .unwrap()
                        .revision()
                        .get();
                    let grown = panel
                        .read_with(cx, |panel, _| {
                            current_revision > revision.0
                                && panel.code_provider_revision_for_test()
                                    == Some(crate::syndic_transcript::ProviderRevision(
                                        current_revision,
                                    ))
                                && panel
                                    .diagnostic_snapshot()
                                    .code_panels
                                    .first()
                                    .is_some_and(|code| code.logical_lines == 3000)
                        })
                        .unwrap();
                    if grown {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert_live();
                let grown_revision = crate::syndic_transcript::ProviderRevision(
                    storage
                        .transcript_view_head(&home, thread, point)
                        .unwrap()
                        .unwrap()
                        .revision()
                        .get(),
                );
                assert!(grown_revision > revision);
                code_history_revision::wait_range(window, &panel, Some(grown_revision), 900, cx)
                    .await;
                let assert_continuity =
                    |panel: &crate::syndic_transcript::SyndicTranscriptPanel| {
                        assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                        assert_eq!(panel.code_presentation_for_test().unwrap().2, 10.5);
                        assert_eq!(panel.code_horizontal_offset_for_test(), Some(45.5));
                        assert_eq!(panel.code_anchor_for_test().unwrap().position, anchor);
                        assert!(panel.code_viewport_for_test().unwrap().3);
                        let diagnostics = panel.diagnostic_snapshot();
                        let code = &diagnostics.code_panels[0];
                        assert_eq!(code.key, key);
                        assert_eq!(code.logical_lines, 3000);
                        assert!(code.realized_visual_lines <= 16);
                        assert_eq!(
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer
                        );
                    };
                let grown_resource = panel
                    .read_with(cx, |panel, _| {
                        assert_continuity(panel);
                        panel.code_viewport_for_test().unwrap().0
                    })
                    .unwrap();
                assert_ne!(grown_resource, resource);
                if let Some(driver) = capture.take() {
                    driver.complete(&format!("{initial}{delta}"));
                } else {
                    exact_cas::admit_item_frame(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        assistant,
                        source.as_ref().unwrap(),
                        ProviderItemFrameV1::new(
                            ProviderFrameOrdinalV1::new(3).unwrap(),
                            cas_item,
                            ProviderItemObservationV1::Completed {
                                observed_at: ProviderLifecycleTimestampMsV1::new(2007),
                                item: agent(&format!("{initial}{delta}")),
                            },
                        ),
                        time(2007),
                    );
                    exact_cas::admit_event(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                        source.as_ref().unwrap(),
                        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
                        time(2008),
                    );
                    exact_cas::converge_and_release_terminal_history(
                        &home,
                        storage.clone(),
                        thread,
                        turn,
                    );
                }
                let complete_revision = crate::syndic_transcript::ProviderRevision(
                    storage
                        .transcript_view_head(&home, thread, point)
                        .unwrap()
                        .unwrap()
                        .revision()
                        .get(),
                );
                code_history_revision::wait_range(window, &panel, Some(complete_revision), 900, cx)
                    .await;
                for _ in 0..10 {
                    code_history_revision::draw(window, cx);
                    panel
                        .read_with(cx, |panel, _| {
                            assert_continuity(panel);
                            assert_eq!(panel.code_viewport_for_test().unwrap().0, grown_resource);
                        })
                        .unwrap();
                }
                assert!(
                    storage
                        .history_summary(&home, thread, point)
                        .unwrap()
                        .unwrap()
                        .complete()
                );
                home.scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                    .unwrap();
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let mut drained = false;
                for _ in 0..300 {
                    drained = window
                        .update(cx, |root, native, app| {
                            root.suspend_running_thread_reads(native, app);
                            if root.release_suspended_running_thread_sources() {
                                root.retire_running_thread_views(native, app).unwrap();
                                true
                            } else {
                                false
                            }
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
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                code_history_revision::draw(window, cx);
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}
