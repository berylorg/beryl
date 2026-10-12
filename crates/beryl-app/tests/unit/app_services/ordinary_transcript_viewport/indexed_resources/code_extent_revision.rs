use super::*;
use code_history_revision::{draw, wait_range};
use sha2::{Digest, Sha256};
use syndic_storage::{
    ProjectionSourceRange, ResourceBacking, ResourceMetadataRecord, ResourceStructure,
};

#[test]
fn native_code_extent_revision_releases_wheel_owner_without_reacquiring_on_growth() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```rust\n{}```",
                    "let payload = 1;\n".repeat(2000)
                )),
            )
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_range(window, &panel, None, 0, cx).await;
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                        code_interaction::press(native, app, bounds.center());
                        code_interaction::release(native, app, bounds.center());
                        code_interaction::wheel(native, app, bounds.center());
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                wait_range(window, &panel, None, 1, cx).await;
                let (resource, identity, key, revision, activation) = panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.code_viewport_for_test().unwrap().3);
                        (
                            panel.code_viewport_for_test().unwrap().0,
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel.code_provider_revision_for_test().unwrap(),
                            panel.snapshot().activation_revision,
                        )
                    })
                    .unwrap();
                let selected = snapshot(&owner).windows()[0].selected_thread();
                let original = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    graph
                        .syndic()
                        .resource(
                            graph.home(),
                            resource.0.parse().unwrap(),
                            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                        )
                        .unwrap()
                        .unwrap()
                };
                let ResourceBacking::TextRange { source, range } = original.backing() else {
                    panic!("Code text backing")
                };
                let short_text = "let payload = 1;\n".repeat(10);
                let shortened = metadata(
                    &original,
                    original.revision().get() + 1,
                    ResourceBacking::TextRange {
                        source,
                        range: ProjectionSourceRange::new(
                            range.start(),
                            range.start() + short_text.len() as u64,
                        )
                        .unwrap(),
                    },
                    Sha256::digest(short_text.as_bytes()).into(),
                    ResourceStructure::code(Some("rust"), 10).unwrap(),
                );
                let short_revision =
                    history_storage::advance_history_revision(&owner, Some(shortened.clone()));
                assert!(short_revision > revision);
                wait_range(window, &panel, Some(short_revision), 0, cx).await;
                for _ in 0..10 {
                    draw(window, cx);
                    panel.read_with(cx, |panel, _| {
                    assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                    assert_eq!(panel.code_presentation_for_test().unwrap().2, 0.0);
                    assert_eq!(panel.code_viewport_for_test().unwrap().0, resource);
                    assert!(!panel.code_viewport_for_test().unwrap().3);
                    assert!(panel.snapshot().activation_revision > activation);
                    let diagnostics = panel.diagnostic_snapshot();
                    let code = &diagnostics.code_panels[0];
                    assert_eq!(code.key, key);
                    assert_eq!(code.logical_lines, 10);
                    assert!(code.realized_visual_lines <= 10);
                    assert!(!code.selected_for_wheel);
                    let response = panel.code_foreground_response_for_test(&resource).unwrap();
                    assert!(matches!(&response.value, IndexedResourceValue::Range(range)
                        if range.binding.metadata == shortened && range.request_complete && range.items.len() == 10));
                }).unwrap();
                }
                let restored = metadata(
                    &original,
                    shortened.revision().get() + 1,
                    original.backing(),
                    *original.digest().unwrap(),
                    original.structure().clone(),
                );
                let restored_revision =
                    history_storage::advance_history_revision(&owner, Some(restored));
                wait_range(window, &panel, Some(restored_revision), 0, cx).await;
                for _ in 0..10 {
                    draw(window, cx);
                    panel
                        .read_with(cx, |panel, _| {
                            assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                            assert!(!panel.code_viewport_for_test().unwrap().3);
                            let diagnostics = panel.diagnostic_snapshot();
                            assert_eq!(diagnostics.code_panels[0].key, key);
                            assert_eq!(diagnostics.code_panels[0].logical_lines, 2000);
                            assert!(diagnostics.code_panels[0].realized_visual_lines <= 16);
                        })
                        .unwrap();
                }
                assert_eq!(snapshot(&owner).windows()[0].selected_thread(), selected);
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_transcript_panel().entity_id())
                        .unwrap(),
                    panel.entity_id()
                );
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                        code_interaction::press(native, app, bounds.center());
                        code_interaction::release(native, app, bounds.center());
                        assert!(panel.read(app).code_viewport_for_test().unwrap().3);
                        let outer = panel
                            .read(app)
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px;
                        code_interaction::wheel(native, app, bounds.center());
                        assert_eq!(panel.read(app).code_viewport_for_test().unwrap().2, 1);
                        assert_eq!(
                            panel
                                .read(app)
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                wait_range(window, &panel, Some(restored_revision), 1, cx).await;
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
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                draw(window, cx);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .code_panels
                            .is_empty())
                        .unwrap()
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

fn metadata(
    original: &ResourceMetadataRecord,
    revision: u64,
    backing: ResourceBacking,
    digest: [u8; 32],
    structure: ResourceStructure,
) -> ResourceMetadataRecord {
    ResourceMetadataRecord::new(
        original.id(),
        beryl_model::ProjectionRevision::new(revision).unwrap(),
        original.projection_id().unwrap(),
        original.item_id(),
        original.ordinal().unwrap(),
        original.kind(),
        original.media_type().unwrap(),
        backing,
        digest,
        None,
        structure,
    )
    .unwrap()
}
