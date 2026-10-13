use super::code_long_line::{retire_code_fixture, wait_until_with_visibility, with_native};
use super::narrative_native::{
    copy_both, drag, key, point_at, ready, record, require_fixture_ledger, writer,
};
use super::*;
use crate::{support::exact_cas, syndic_transcript::ResidentPresentationRecordKind};
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId};
use syndic_storage::{
    ProviderAgentMessageV1, ProviderFrameOrdinalV1, ProviderItemDeltaV1, ProviderItemFrameV1,
    ProviderItemObservationV1, ProviderItemV1, ProviderLifecycleTimestampMsV1, ProviderTextV1,
    SourceEventPayload, SyndicPointReadLimit, SyndicTimestamp, TurnEndStatus,
};

fn agent(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: None,
        memory_citation: None,
    })
}

#[test]
fn native_narrative_font_change_invalidates_selection_and_preserves_source_mapping() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("font **café 🌸** sample\r\n")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["font **café"]).await;
                let (captured, _) = writer(&panel, cx);
                let (id, start, before_width) = with_native(window, cx, |native, app| {
                    let (id, _, display) = record(panel.read(app), "font **café");
                    let start = display.find("café 🌸").unwrap();
                    let first = point_at(panel.read(app), &id, start);
                    let last = point_at(panel.read(app), &id, start + "café 🌸".len());
                    drag(native, app, first, last);
                    copy_both(native, app, &captured, "**café 🌸**", "café 🌸");
                    (id, start, last.x - first.x)
                })
                .unwrap();
                let original = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    native_appearance::system_font_appearance_with_narrative_size(
                        owner.test_services().graph().unwrap().state(),
                        28,
                    )
                };
                let enlarged =
                    std::sync::Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                        original.number().checked_next().unwrap(),
                        prepared,
                        crate::theme_runtime::AppearancePublication::Durable,
                    ));
                for (appearance, large) in [(enlarged, true), (original, false)] {
                    with_native(window, cx, |native, app| {
                        key(native, app, "shift-f10");
                        assert!(panel.read(app).quote_menu_open_for_test());
                    })
                    .unwrap();
                    let before_copy = captured.borrow().len();
                    cx.update(|app| {
                        MainWindowShellRoot::test_apply_appearance(window, appearance, app)
                    })
                    .unwrap();
                    with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
                    assert!(
                        !panel
                            .read_with(cx, |panel, _| panel.quote_menu_open_for_test())
                            .unwrap()
                    );
                    assert_eq!(
                        captured.borrow().len(),
                        before_copy,
                        "prior appearance geometry cannot authorize Copy"
                    );
                    wait_until_with_visibility(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    let Some(first) = panel.narrative_position_for_test(&id, start)
                                    else {
                                        return false;
                                    };
                                    let Some(last) = panel
                                        .narrative_position_for_test(&id, start + "café 🌸".len())
                                    else {
                                        return false;
                                    };
                                    if large {
                                        last.x - first.x > before_width
                                    } else {
                                        (last.x - first.x - before_width).abs() < gpui::px(0.5)
                                    }
                                })
                                .unwrap()
                        },
                        "body font change must settle current native narrative positions",
                        true,
                    )
                    .await;
                    with_native(window, cx, |native, app| {
                        let first = point_at(panel.read(app), &id, start);
                        let last = point_at(panel.read(app), &id, start + "café 🌸".len());
                        drag(native, app, first, last);
                        copy_both(native, app, &captured, "**café 🌸**", "café 🌸");
                    })
                    .unwrap();
                }
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_narrative_saved_source_growth_resolves_syntax_and_terminal_reuses_model() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        4,
        Duration::from_secs(60),
        |_| {},
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let (home, storage, thread) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
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
                let user = SyndicItemId::from_bytes([231; 16]);
                let assistant = SyndicItemId::from_bytes([232; 16]);
                let turn = exact_cas::submit_current_draft(
                    &home,
                    storage.clone(),
                    thread,
                    SyndicDraftId::from_bytes([233; 16]),
                    user,
                    "show growing narrative",
                    time(3001),
                );
                let source =
                    exact_cas::establish_turn(&home, storage.clone(), thread, turn, time(3002));
                exact_cas::admit_event(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    &source,
                    SourceEventPayload::TurnActivated,
                    time(3003),
                );
                exact_cas::correlate_user_item(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    user,
                    &source,
                    time(3004),
                );
                let cas_item = CasItemId::new("native-growing-narrative").unwrap();
                let initial = "growing **café";
                exact_cas::admit_item_frame(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    assistant,
                    &source,
                    ProviderItemFrameV1::new(
                        ProviderFrameOrdinalV1::FIRST,
                        cas_item.clone(),
                        ProviderItemObservationV1::Started {
                            observed_at: ProviderLifecycleTimestampMsV1::new(3005),
                            item: agent(initial),
                        },
                    ),
                    time(3005),
                );
                exact_cas::project_item_if_needed(&home, storage.clone(), user);
                exact_cas::project_item_if_needed(&home, storage.clone(), assistant);
                exact_cas::converge_transcript(&home, storage.clone(), thread);
                ready(window, &panel, cx, &[initial]).await;
                let (captured, _) = writer(&panel, cx);
                let original_id = with_native(window, cx, |native, app| {
                    let (id, original, display) = record(panel.read(app), initial);
                    assert_eq!(original, initial);
                    assert_eq!(
                        display, initial,
                        "ambiguous open syntax must retain literal spelling"
                    );
                    let first = point_at(panel.read(app), &id, 0);
                    let last = point_at(panel.read(app), &id, initial.len());
                    drag(native, app, first, last);
                    copy_both(native, app, &captured, initial, initial);
                    key(native, app, "shift-f10");
                    assert!(panel.read(app).quote_menu_open_for_test());
                    id
                })
                .unwrap();
                let delta = "** tail";
                let grown = format!("{initial}{delta}");
                exact_cas::admit_item_frame(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    assistant,
                    &source,
                    ProviderItemFrameV1::new(
                        ProviderFrameOrdinalV1::new(2).unwrap(),
                        cas_item.clone(),
                        ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                            delta: ProviderTextV1::inline(delta),
                        }),
                    ),
                    time(3006),
                );
                exact_cas::project_item_if_needed(&home, storage.clone(), assistant);
                exact_cas::converge_transcript(&home, storage.clone(), thread);
                panel.read_with(cx, |panel, _| {
                assert!(panel.narrative_text_for_test().iter().any(|(id, text, display)| id == &original_id && text == initial && display == initial),
                    "source growth keeps prior coherent presentation until the provider replacement runs");
            }).unwrap();
                ready(window, &panel, cx, &[&grown]).await;
                assert!(
                    !panel
                        .read_with(cx, |panel, _| panel.quote_menu_open_for_test())
                        .unwrap()
                );
                let (grown_id, identity) = with_native(window, cx, |native, app| {
                    let (id, source, display) = record(panel.read(app), &grown);
                    assert_eq!(id, original_id);
                    assert_eq!(source, grown);
                    assert_eq!(display, "growing café tail");
                    let first = point_at(panel.read(app), &id, 8);
                    let last = point_at(panel.read(app), &id, 8 + "café".len());
                    drag(native, app, first, last);
                    copy_both(native, app, &captured, "**café**", "café");
                    let identity = panel
                        .read(app)
                        .narrative_model_identity_for_test(&id)
                        .unwrap();
                    (id, identity)
                })
                .unwrap();
                exact_cas::admit_item_frame(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    assistant,
                    &source,
                    ProviderItemFrameV1::new(
                        ProviderFrameOrdinalV1::new(3).unwrap(),
                        cas_item,
                        ProviderItemObservationV1::Completed {
                            observed_at: ProviderLifecycleTimestampMsV1::new(3007),
                            item: agent(&grown),
                        },
                    ),
                    time(3007),
                );
                exact_cas::admit_event(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                    &source,
                    SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
                    time(3008),
                );
                exact_cas::converge_and_release_terminal_history(
                    &home,
                    storage.clone(),
                    thread,
                    turn,
                );
                let revision = storage
                    .transcript_view_head(&home, thread, point)
                    .unwrap()
                    .unwrap()
                    .revision()
                    .get();
                wait_until_with_visibility(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.narrative_provider_revision_for_test(&grown_id)
                                    == Some(crate::syndic_transcript::ProviderRevision(revision))
                                    && panel.narrative_model_identity_for_test(&grown_id).is_some()
                            })
                            .unwrap()
                    },
                    "unchanged terminal projection must reach the measured provider",
                    true,
                )
                .await;
                panel
                    .read_with(cx, |panel, _| {
                        assert_eq!(
                            panel.narrative_model_identity_for_test(&grown_id),
                            Some(identity)
                        );
                        let (_, source, display) = record(panel, &grown);
                        assert_eq!(source, grown);
                        assert_eq!(display, "growing café tail");
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let first = point_at(panel.read(app), &grown_id, 8);
                    let last = point_at(panel.read(app), &grown_id, 8 + "café".len());
                    drag(native, app, first, last);
                    copy_both(native, app, &captured, "**café**", "café");
                })
                .unwrap();
                assert!(
                    storage
                        .history_summary(&home, thread, point)
                        .unwrap()
                        .unwrap()
                        .complete()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_narrative_literal_continuation_selects_realized_boundary_and_refuses_unrealized_edge() {
    require_fixture_ledger();
    let source = "x".repeat(syndic_storage::MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES - 4)
        + "LEFTRIGHT **remain literal** tail";
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| seed_history_text(path, 1, Some(&source)),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["LEFT", "RIGHT"]).await;
                let (captured, _) = writer(&panel, cx);
                with_native(window, cx, |native, app| {
                let (left, left_source, left_display) = record(panel.read(app), "LEFT");
                let (right, right_source, right_display) = record(panel.read(app), "RIGHT");
                assert_eq!(left_source, left_display);
                assert_eq!(right_source, right_display);
                assert!(right_display.contains("**remain literal**"));
                let start = left_display.find("LEFT").unwrap();
                let first = point_at(panel.read(app), &left, start);
                let last = point_at(panel.read(app), &right, 5);
                drag(native, app, first, last);
                copy_both(native, app, &captured, "LEFTRIGHT", "LEFTRIGHT");
                let start = right_display.find("**remain literal**").unwrap();
                let first = point_at(panel.read(app), &right, start);
                let last = point_at(panel.read(app), &right, start + "**remain literal**".len());
                drag(native, app, first, last);
                copy_both(native, app, &captured, "**remain literal**", "**remain literal**");
                let first_id = panel.read(app).snapshot().records.iter().find_map(|record| {
                    match &record.kind {
                        ResidentPresentationRecordKind::NarrativeReady { prepared } if prepared.span_ordinal() == 1 && prepared.source().starts_with("xxxxx") => Some(record.id.clone()),
                        _ => None,
                    }
                }).expect("saved first span retains its source owner");
                let unrealized = panel.read(app).narrative_position_for_test(&first_id, 0);
                let bounds = panel.read(app).viewport_bounds_for_test().unwrap();
                assert!(unrealized.is_none_or(|point| !bounds.contains(&point)), "first span opening must remain outside the realized viewport");
                let before = captured.borrow().len();
                let outside = gpui::point(bounds.center().x, bounds.top() - gpui::px(24.0));
                drag(native, app, first, outside);
                key(native, app, "ctrl-c");
                assert!(captured.borrow()[before..].iter().all(|text| text == "**remain literal**"),
                    "drag across an unrealized edge cannot fabricate the unloaded source interval");
            }).unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
