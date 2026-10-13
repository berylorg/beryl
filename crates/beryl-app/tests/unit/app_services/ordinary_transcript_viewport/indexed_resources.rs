use super::*;
use crate::syndic_transcript::{IndexedResourceDemand, IndexedResourceQuery, IndexedResourceValue};
use syndic_storage::indexed_resource as index;

#[path = "indexed_resources/code_interaction.rs"]
mod code_interaction;

#[path = "indexed_resources/code_ownership.rs"]
mod code_ownership;

#[path = "indexed_resources/code_anchor.rs"]
mod code_anchor;

#[path = "indexed_resources/code_recovery.rs"]
mod code_recovery;

#[path = "indexed_resources/code_horizontal.rs"]
mod code_horizontal;

#[path = "indexed_resources/code_long_line.rs"]
mod code_long_line;

#[path = "indexed_resources/code_native_mount.rs"]
mod code_native_mount;
#[path = "indexed_resources/table_native_mount.rs"]
mod table_native_mount;

#[path = "indexed_resources/narrative_native.rs"]
mod narrative_native;
#[path = "indexed_resources/quote_insertion.rs"]
mod quote_insertion;
#[path = "indexed_resources/turn_menus.rs"]
mod turn_menus;

#[path = "indexed_resources/narrative_source_revision.rs"]
mod narrative_source_revision;

#[path = "indexed_resources/code_history_revision.rs"]
mod code_history_revision;

#[path = "indexed_resources/history_storage.rs"]
mod history_storage;

#[path = "indexed_resources/code_extent_revision.rs"]
mod code_extent_revision;

#[path = "indexed_resources/code_streaming.rs"]
mod code_streaming;

mod native_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_shell_appearance.rs"
    ));
}

#[test]
fn native_measured_indexed_resource_build_retries_pending_and_publishes_ready() {
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
                let mut selected = None;
                let mut observed = None;
                for _ in 0..150 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    selected = window
                        .update(cx, |root, _, app| {
                            let panel = root.test_transcript_panel();
                            let snapshot = panel.read(app).snapshot();
                            observed = Some((
                                root.coherent_selected_thread_title(app).is_some(),
                                snapshot.records.len(),
                                snapshot.resources.metadata.len(),
                                snapshot.resources.slices.len(),
                                panel.read(app).measured_frame_for_test().is_some(),
                            ));
                            let demand = panel.read(app).resource_demand()?;
                            let (metadata, baseline_frozen) = {
                                let owner = owner.borrow();
                                let graph = owner.test_services().graph().unwrap();
                                let metadata = graph
                                    .syndic()
                                    .resource(
                                        graph.home(),
                                        demand.resource_id.0.parse().unwrap(),
                                        syndic_storage::SyndicPointReadLimit::new(4096).unwrap(),
                                    )
                                    .unwrap()
                                    .unwrap();
                                (metadata, graph.home().retained_frozen_read_count().unwrap())
                            };
                            assert_eq!(metadata.kind(), syndic_storage::ResourceKind::Code);
                            let before = panel.read(app).snapshot().activation_revision;
                            assert!(panel.update(app, |panel, cx| {
                                let accepted = panel.set_indexed_resource_demand(
                                    &demand,
                                    Some(IndexedResourceDemand {
                                        revision: metadata.revision(),
                                        root: None,
                                        query: IndexedResourceQuery::Display {
                                            request: index::DisplayRequest {
                                                target: index::Target::Code(1999),
                                                offset: 0,
                                            },
                                            cursor: None,
                                            bytes: 8192,
                                        },
                                    }),
                                );
                                cx.notify();
                                accepted
                            }));
                            Some((panel, demand.resource_id, metadata, before, baseline_frozen))
                        })
                        .unwrap();
                    if selected.is_some() {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                eprintln!("native indexed selection facts: {observed:?}");
                let (panel, resource, metadata, activation, baseline_frozen) =
                    selected.expect("selected Code demand");
                window
                    .update(cx, |_, native, _| native.minimize_window())
                    .unwrap();
                let mut ready = None;
                for _ in 0..300 {
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap()
                    })
                    .unwrap();
                    ready = window
                        .update(cx, |root, _, app| {
                            assert!(root.coherent_selected_thread_title(app).is_some());
                            assert_eq!(panel.read(app).snapshot().activation_revision, activation);
                            panel.read(app).indexed_resource_response(&resource).filter(
                                |response| {
                                    matches!(response.value, IndexedResourceValue::Display(_))
                                },
                            )
                        })
                        .unwrap();
                    if ready.is_some() {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                let response = ready
                    .expect("native construction and measured Pending refresh must reach Ready");
                let IndexedResourceValue::Display(value) = &response.value else {
                    panic!("display")
                };
                assert_eq!(value.binding.metadata, metadata);
                assert_eq!(value.fragment.as_ref().unwrap().text, "let payload = 1;");
                assert_eq!(
                    response.root.as_ref().unwrap().language.as_deref(),
                    Some("rust")
                );
                assert!(response.encoded_bytes <= 8192);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.resource_demand().is_none())
                        .unwrap()
                );
                drop(response);
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app)
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
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .indexed_resource_response(&resource)
                            .is_none())
                        .unwrap()
                );
                assert!(
                    owner
                        .borrow()
                        .test_services()
                        .graph()
                        .unwrap()
                        .home()
                        .retained_frozen_read_count()
                        .unwrap()
                        <= baseline_frozen
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}
