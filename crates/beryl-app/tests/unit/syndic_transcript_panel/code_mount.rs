use super::super::super::{
    code_appearance::CodeAppearance, code_layout_mount::CodeLayoutMount,
    code_viewport::CodeViewport,
};
use super::*;
use crate::transcript_provider::{
    IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
};
use std::sync::Arc;
use syndic_storage::indexed_resource as index;

#[path = "code_extent.rs"]
mod code_extent;

#[path = "code_source_actions.rs"]
mod code_source_actions;
#[path = "code_save.rs"]
mod code_save;

#[gpui::test]
fn empty_admitted_code_layout_is_ready_without_fragments(cx: &mut TestAppContext) {
    use super::super::super::{code_layout_mount::CodeLayoutMount, code_viewport::CodeViewport};
    use crate::transcript_provider::IndexedResourceValue;
    use syndic_storage::{
        ProjectionSourceRange, ResourceBacking, ResourceMetadataRecord, ResourceOrdinal,
        ResourceStructure, indexed_resource as index,
    };

    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                let owner = panel.resource_demand().unwrap();
                let mut viewport =
                    CodeViewport::new(owner, beryl_model::ProjectionRevision::new(1).unwrap());
                let mut mount = CodeLayoutMount::default();
                mount.reconcile_range(&mut viewport);
                assert_eq!(
                    viewport.scroll.borrow().range(),
                    index::OrdinalRange { start: 0, end: 16 }
                );
                assert_eq!(mount.state(&viewport), "pending");
                assert!(mount.prepared().next().is_none());
                assert_eq!(mount.complete_width(0), None);

                let mut binding = super::code_anchors::binding(1);
                binding.range = index::ByteRange { start: 0, end: 0 };
                binding.metadata = ResourceMetadataRecord::new(
                    binding.resource.id,
                    binding.resource.revision,
                    beryl_model::SyndicProjectionId::from_bytes([55; 16]),
                    beryl_model::SyndicItemId::from_bytes([66; 16]),
                    ResourceOrdinal::FIRST,
                    syndic_storage::ResourceKind::Code,
                    "text/plain",
                    ResourceBacking::TextRange {
                        source: binding.source,
                        range: ProjectionSourceRange::new(0, 0).unwrap(),
                    },
                    [77; 32],
                    None,
                    ResourceStructure::code(Some("rust"), 0).unwrap(),
                )
                .unwrap();
                let mut response = super::code_anchors::range_response(binding, 2);
                let root = response.root.as_mut().unwrap();
                root.counts = [0; 4];
                root.code_lines = 0;
                let IndexedResourceValue::Range(range) = &mut response.value else {
                    unreachable!();
                };
                range.cursor.next = index::Position::Code(0);
                range.items.clear();
                range.resource_complete = true;
                mount
                    .adopt(
                        &viewport,
                        vec![std::sync::Arc::new(response)],
                        &panel.code_appearance,
                        window,
                        window.text_system().font_environment_key().unwrap(),
                    )
                    .unwrap();
                assert_eq!(mount.state(&viewport), "ready");
                assert_eq!(mount.source_bytes(), Some(0));
                assert!(mount.prepared().next().is_none());
                assert_eq!(mount.checkpoints(), 0);
                assert_eq!(mount.complete_width(0), Some(0.0));
                assert!(mount.query.is_none());
            });
        })
        .unwrap();
}

#[gpui::test]
fn empty_code_metadata_mounts_an_indexed_panel(cx: &mut TestAppContext) {
    for preview_range in [None, Some(0..0)] {
        let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
        draw_frames(cx, window, 5);
        let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
        let demand = panel.update(cx, |panel, _| {
            let demand = panel.resource_demand().unwrap();
            let TranscriptProviderResponseKind::ResourceMetadata(mut metadata) =
                metadata_response(&demand)
            else {
                unreachable!()
            };
            metadata.byte_len = 0;
            metadata.line_count = Some(0);
            metadata.preview_range = preview_range;
            assert!(panel.stage_resource_response(
                &demand,
                TranscriptProviderResponseKind::ResourceMetadata(metadata),
                None,
            ));
            demand
        });
        draw_frames(cx, window, 5);
        window
            .update(cx, |root, window, cx| {
                root.panel.update(cx, |panel, _| {
                    assert!(panel.measurement_current(window));
                    panel.publish_resource_candidate(&demand);
                });
            })
            .unwrap();
        draw_frames(cx, window, 5);
        panel.update(cx, |panel, _| {
            assert_eq!(panel.code_panels.len(), 1);
            let owner = panel
                .host
                .indexed_resource_owner(&demand.resource_id)
                .unwrap();
            assert!(owner.metadata.is_none());
            assert!(owner.range.is_none());
            assert!(matches!(
                owner.indexed.unwrap().query,
                super::super::super::super::IndexedResourceQuery::Range {
                    request: super::super::super::super::provider::resource_index::Request::Code {
                        lines: super::super::super::super::provider::resource_index::OrdinalRange {
                            start: 0,
                            end: 16,
                        },
                    },
                    cursor: None,
                    ..
                }
            ));
            assert_eq!(panel.code_wheel_owner.get(), None);
            assert_eq!(panel.code_panels[0].viewport.scroll.borrow().top_line, 0);
        });
    }
}

#[gpui::test]
fn code_measurement_epoch_restores_distant_source_and_fractional_inline_position(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |mount, viewport, window| {
        let response = code_line_response(1, 10_000);
        let mut appearance = CodeAppearance::default();
        drive_code_layout(mount, viewport, &response, &appearance, window);
        let original_width = mount.complete_width(1).unwrap();
        viewport.scroll.borrow_mut().horizontal = original_width * 0.9 + 0.375;
        drive_code_layout(mount, viewport, &response, &appearance, window);
        let anchor = viewport.inline_anchor.unwrap();
        assert!(anchor.source.position > 8192);
        assert!(anchor.visual_offset.fract() != 0.0);
        let old_target = viewport.scroll.borrow().horizontal;
        let old_epoch = mount.epoch;
        let old_checkpoints = mount.checkpoints();
        appearance.body_text = gpui::rgb(0x102030);
        mount
            .reconcile_environment(
                viewport,
                &appearance,
                window,
                20.0,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        assert_eq!(mount.epoch, old_epoch);
        assert_eq!(mount.checkpoints(), old_checkpoints);
        appearance.body_font.size *= 2.0;
        mount
            .reconcile_environment(
                viewport,
                &appearance,
                window,
                20.0,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        assert!(viewport.inline_remapping);
        assert_eq!(mount.complete_width(1), None);
        assert!(mount.prepared().next().is_none());
        drive_code_layout(mount, viewport, &response, &appearance, window);
        let restored = viewport.inline_anchor.unwrap();
        assert_eq!(restored.source.position, anchor.source.position);
        assert!((restored.visual_offset - anchor.visual_offset).abs() < 0.01);
        assert_ne!(mount.epoch, old_epoch);
        assert!(mount.complete_width(1).unwrap() > original_width);
        assert!(viewport.scroll.borrow().horizontal > old_target);
    });
}

#[gpui::test]
fn code_successor_admission_resolves_intraline_anchor_before_new_geometry(cx: &mut TestAppContext) {
    with_code_layout(cx, |mount, viewport, window| {
        let appearance = CodeAppearance::default();
        let original = code_line_response(1, 9500);
        drive_code_layout(mount, viewport, &original, &appearance, window);
        viewport.scroll.borrow_mut().horizontal = mount.complete_width(1).unwrap() * 0.95 + 0.375;
        drive_code_layout(mount, viewport, &original, &appearance, window);
        let anchor = viewport.inline_anchor.unwrap();
        let horizontal = viewport.scroll.borrow().horizontal;
        assert!(anchor.source.position > 8192);
        for (revision, bytes) in [(2, 10_000), (3, 8500)] {
            viewport.rebind(
                viewport.owner.clone(),
                beryl_model::ProjectionRevision::new(revision).unwrap(),
            );
            mount.reset();
            let IndexedResourceQuery::Anchor {
                anchor: requested, ..
            } = viewport.query().query
            else {
                panic!("successor must resolve source before layout")
            };
            assert_eq!(requested.position, anchor.source.position);
            let response = code_line_response(revision, bytes);
            let IndexedResourceValue::Range(range) = &response.value else {
                unreachable!();
            };
            let index::Item::Descriptor(descriptor) = &range.items[0] else {
                unreachable!();
            };
            let mut resolved = requested;
            resolved.source = range.binding.source;
            resolved.position = requested.position.min(bytes);
            viewport.adopt(&IndexedResourceResponse {
                root: response.root.clone(),
                value: IndexedResourceValue::Anchor(index::AnchorResponse {
                    binding: range.binding.clone(),
                    resolved: Some(index::ResolvedAnchor {
                        descriptor: descriptor.clone(),
                        anchor: resolved,
                    }),
                    encoded_bytes: 1,
                }),
                encoded_bytes: 1,
            });
            assert!(!viewport.remapping);
            assert!(viewport.inline_remapping);
            assert_eq!(
                viewport.inline_anchor.unwrap().source.position,
                requested.position.min(bytes)
            );
            assert_eq!(mount.complete_width(1), None);
            drive_code_layout(mount, viewport, &response, &appearance, window);
            if revision == 2 {
                assert!((viewport.scroll.borrow().horizontal - horizontal).abs() < 0.01);
                assert_eq!(
                    viewport.inline_anchor.unwrap().source.position,
                    anchor.source.position
                );
            } else {
                let scroll = viewport.scroll.borrow();
                assert_eq!(
                    scroll.horizontal,
                    (mount.complete_width(1).unwrap() - 380.0).max(0.0)
                );
                assert!(viewport.inline_anchor.unwrap().source.position <= bytes);
            }
        }
        mount.reset();
        assert_eq!(mount.charge(), 0);
        assert_eq!(mount.checkpoints(), 0);
        assert!(mount.prepared().next().is_none());
    });
}

#[gpui::test]
fn new_horizontal_target_supersedes_an_unresolved_source_remeasurement(cx: &mut TestAppContext) {
    with_code_layout(cx, |mount, viewport, window| {
        let response = code_line_response(1, 10_000);
        let mut appearance = CodeAppearance::default();
        drive_code_layout(mount, viewport, &response, &appearance, window);
        viewport.scroll.borrow_mut().horizontal = mount.complete_width(1).unwrap() * 0.9 + 0.375;
        drive_code_layout(mount, viewport, &response, &appearance, window);
        appearance.body_font.size *= 2.0;
        mount
            .reconcile_environment(
                viewport,
                &appearance,
                window,
                20.0,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        viewport.adopt(&response);
        mount.reconcile_range(viewport);
        mount
            .adopt(
                viewport,
                vec![response.clone()],
                &appearance,
                window,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        assert!(mount.advance(viewport, &appearance, window).unwrap());
        assert!(viewport.inline_remapping);
        viewport.scroll.borrow_mut().horizontal = 1000.375;
        drive_code_layout(mount, viewport, &response, &appearance, window);
        assert!(!viewport.inline_remapping);
        assert_eq!(viewport.scroll.borrow().horizontal, 1000.375);
        assert!(
            mount
                .prepared()
                .all(|prepared| f32::from(prepared.target.inline_start) == 1000.375)
        );
    });
}

#[gpui::test]
fn idle_code_publication_keeps_the_normalized_overscan_source_anchor(cx: &mut TestAppContext) {
    with_code_layout(cx, |mount, viewport, window| {
        let mut appearance = CodeAppearance::default();
        appearance.body_font.size *= 2.0;
        viewport.scroll.borrow_mut().set_lines(20);
        viewport.set_line_height(40.0);
        viewport.scroll.borrow_mut().set_offset(190.5);
        viewport.scroll.borrow_mut().horizontal = 5000.375;
        drive_multiline_code_layout(mount, viewport, &appearance, window);
        let anchor = viewport.inline_anchor.unwrap();
        assert_eq!(anchor.logical_line, 4);
        assert_eq!(viewport.anchor_visual_offset(), Some(-30.5));
        appearance.body_font.size /= 2.0;
        mount
            .reconcile_environment(
                viewport,
                &appearance,
                window,
                20.0,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        viewport.set_line_height(20.0);
        assert_eq!(viewport.scroll.borrow().top_line, 5);
        assert_eq!(viewport.scroll.borrow().remainder, 10.5);
        drive_multiline_code_layout(mount, viewport, &appearance, window);
        let restored = viewport.inline_anchor.unwrap();
        assert_eq!(restored, anchor);
        assert!(viewport.inline_anchor_is_current());
        for _ in 0..8 {
            mount.advance(viewport, &appearance, window).unwrap();
            mount.resolve_inline_anchor(viewport);
            mount.publish_candidate(viewport);
            assert_eq!(viewport.inline_anchor, Some(anchor));
            assert_eq!(viewport.anchor_visual_offset(), Some(-30.5));
            assert_eq!(viewport.scroll.borrow().top_line, 5);
            assert!(viewport.inline_anchor_is_current());
        }
    });
}

#[gpui::test]
fn qualified_code_environment_recovers_transient_refusal_without_retrying_capacity(
    cx: &mut TestAppContext,
) {
    use super::super::super::code_layout::CodeLayoutError;
    with_code_layout(cx, |mount, viewport, window| {
        let appearance = CodeAppearance::default();
        let response = code_line_response(1, 10_000);
        drive_code_layout(mount, viewport, &response, &appearance, window);
        let anchor = viewport.inline_anchor;
        let epoch = mount.epoch;
        let charge = mount.charge();
        for error in [
            CodeLayoutError::FontEnvironmentChanged,
            CodeLayoutError::FontEnvironmentUnavailable,
        ] {
            mount.refused = Some(error);
            assert_eq!(mount.state(viewport), "refused");
            mount
                .reconcile_environment(
                    viewport,
                    &appearance,
                    window,
                    20.0,
                    window.text_system().font_environment_key().unwrap(),
                )
                .unwrap();
            assert!(mount.refused.is_none());
            mount.advance(viewport, &appearance, window).unwrap();
            mount.publish_candidate(viewport);
            assert_eq!(mount.state(viewport), "ready");
            assert_eq!(viewport.inline_anchor, anchor);
            assert_eq!(mount.epoch, epoch);
        }
        mount.refused = Some(CodeLayoutError::Capacity);
        for _ in 0..3 {
            mount
                .reconcile_environment(
                    viewport,
                    &appearance,
                    window,
                    20.0,
                    window.text_system().font_environment_key().unwrap(),
                )
                .unwrap();
            assert_eq!(mount.refused, Some(CodeLayoutError::Capacity));
            assert!(!mount.advance(viewport, &appearance, window).unwrap());
            assert_eq!(mount.state(viewport), "refused");
            assert_eq!(mount.charge(), charge);
            assert_eq!(viewport.inline_anchor, anchor);
            assert_eq!(mount.epoch, epoch);
        }
    });
}

fn drive_multiline_code_layout(
    mount: &mut CodeLayoutMount,
    viewport: &mut CodeViewport,
    appearance: &CodeAppearance,
    window: &gpui::Window,
) {
    let mut response = super::code_anchors::range_response(super::code_anchors::binding(1), 2);
    response.root.as_mut().unwrap().code_lines = 20;
    response.root.as_mut().unwrap().counts = [20, 0, 0, 0];
    let IndexedResourceValue::Range(range) = &mut response.value else {
        unreachable!();
    };
    let metadata = &range.binding.metadata;
    range.binding.metadata = syndic_storage::ResourceMetadataRecord::new(
        metadata.id(),
        metadata.revision(),
        metadata.projection_id().unwrap(),
        metadata.item_id(),
        metadata.ordinal().unwrap(),
        metadata.kind(),
        metadata.media_type().unwrap(),
        metadata.backing(),
        *metadata.digest().unwrap(),
        None,
        syndic_storage::ResourceStructure::code(Some("rust"), 20).unwrap(),
    )
    .unwrap();
    let lines = viewport.scroll.borrow().range();
    range.cursor.request = index::Request::Code { lines };
    range.cursor.next = index::Position::Code(lines.end);
    range.items.clear();
    let mut presentations = Vec::new();
    for ordinal in lines.start..lines.end {
        let (start, end) = match ordinal {
            0..=3 => (ordinal, ordinal),
            4 => (4, 4994),
            5 => (4995, 9985),
            _ => (9986 + ordinal - 6, 9986 + ordinal - 6),
        };
        let content = index::ByteRange { start, end };
        let raw = index::ByteRange {
            start,
            end: end + 1,
        };
        range
            .items
            .push(index::Item::Descriptor(index::IndexedDescriptor {
                descriptor: index::Descriptor::CodeLine {
                    ordinal,
                    raw,
                    content,
                    terminator: index::ByteRange {
                        start: end,
                        end: end + 1,
                    },
                },
                absolute_raw: Some(raw),
            }));
        presentations.push(index::Presentation {
            fragment: Some(index::Fragment {
                text: "x".repeat((end - start) as usize),
                source: content,
                remaining: index::ByteRange { start: end, end },
                next_offset: end - start,
            }),
            alignment: None,
            literal_fallback: false,
        });
    }
    range.presentations = Some(presentations);
    range.encoded_bytes = 12_000;
    response.encoded_bytes = 12_000;
    viewport.adopt(&response);
    mount.reconcile_range(viewport);
    let response = Arc::new(response);
    let source = "\n".repeat(4) + &("x".repeat(4990) + "\n").repeat(2) + &"\n".repeat(14);
    mount
        .adopt(
            viewport,
            vec![response.clone()],
            appearance,
            window,
            window.text_system().font_environment_key().unwrap(),
        )
        .unwrap();
    for _ in 0..1000 {
        let more = mount.advance(viewport, appearance, window).unwrap();
        if let Some(IndexedResourceQuery::Source {
            range: requested, ..
        }) = mount.query.as_ref().map(|query| query.query.clone())
        {
            let IndexedResourceValue::Range(range) = &response.value else {
                unreachable!();
            };
            let bytes =
                source.as_bytes()[requested.start as usize..requested.end as usize].to_vec();
            mount
                .adopt(
                    viewport,
                    vec![Arc::new(IndexedResourceResponse {
                        root: response.root.clone(),
                        encoded_bytes: bytes.len() + 1024,
                        value: IndexedResourceValue::Source(
                            crate::syndic_transcript::IndexedSourceResponse {
                                binding: range.binding.clone(),
                                range: requested,
                                bytes,
                            },
                        ),
                    })],
                    appearance,
                    window,
                    window.text_system().font_environment_key().unwrap(),
                )
                .unwrap();
        }
        assert!(mount.charge() <= 16 * 1024 * 1024);
        assert!(mount.checkpoints() <= 16 * 32);
        mount.resolve_inline_anchor(viewport);
        mount.publish_candidate(viewport);
        if !more && mount.query.is_none() && mount.state(viewport) == "ready" {
            return;
        }
    }
    panic!("bounded multiline source must settle");
}

fn with_code_layout(
    cx: &mut TestAppContext,
    run: impl FnOnce(&mut CodeLayoutMount, &mut CodeViewport, &gpui::Window),
) {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            let owner = root.panel.read(cx).resource_demand().unwrap();
            let mut viewport =
                CodeViewport::new(owner, beryl_model::ProjectionRevision::new(1).unwrap());
            viewport.scroll.borrow_mut().bounds.size.width = gpui::px(400.0);
            run(&mut CodeLayoutMount::default(), &mut viewport, window);
        })
        .unwrap();
}

fn code_line_response(revision: u64, bytes: u64) -> Arc<IndexedResourceResponse> {
    let mut binding = super::code_anchors::binding(revision);
    binding.range = index::ByteRange {
        start: 0,
        end: bytes,
    };
    binding.metadata = syndic_storage::ResourceMetadataRecord::new(
        binding.resource.id,
        binding.resource.revision,
        beryl_model::SyndicProjectionId::from_bytes([55; 16]),
        beryl_model::SyndicItemId::from_bytes([66; 16]),
        syndic_storage::ResourceOrdinal::FIRST,
        syndic_storage::ResourceKind::Code,
        "text/plain",
        syndic_storage::ResourceBacking::TextRange {
            source: binding.source,
            range: syndic_storage::ProjectionSourceRange::new(0, bytes).unwrap(),
        },
        [77; 32],
        None,
        syndic_storage::ResourceStructure::code(Some("rust"), 1).unwrap(),
    )
    .unwrap();
    let mut response = super::code_anchors::range_response(binding, 2);
    let root = response.root.as_mut().unwrap();
    root.code_lines = 1;
    root.counts = [1, 0, 0, 0];
    let IndexedResourceValue::Range(range) = &mut response.value else {
        unreachable!();
    };
    range.cursor.next = index::Position::Code(1);
    range.resource_complete = true;
    let content = index::ByteRange {
        start: 0,
        end: bytes,
    };
    range.items = vec![index::Item::Descriptor(index::IndexedDescriptor {
        descriptor: index::Descriptor::CodeLine {
            ordinal: 0,
            raw: content,
            content,
            terminator: index::ByteRange {
                start: bytes,
                end: bytes,
            },
        },
        absolute_raw: Some(content),
    })];
    let end = bytes.min(8192);
    range.presentations = Some(vec![index::Presentation {
        fragment: Some(index::Fragment {
            text: "x".repeat(end as usize),
            source: index::ByteRange { start: 0, end },
            remaining: index::ByteRange {
                start: end,
                end: bytes,
            },
            next_offset: end,
        }),
        alignment: None,
        literal_fallback: false,
    }]);
    range.encoded_bytes = end as usize + 512;
    response.encoded_bytes = range.encoded_bytes;
    Arc::new(response)
}

fn drive_code_layout(
    mount: &mut CodeLayoutMount,
    viewport: &mut CodeViewport,
    response: &Arc<IndexedResourceResponse>,
    appearance: &CodeAppearance,
    window: &gpui::Window,
) {
    let IndexedResourceValue::Range(range) = &response.value else {
        unreachable!();
    };
    let source = "x".repeat(range.binding.range.end as usize);
    viewport.adopt(response);
    mount.reconcile_range(viewport);
    mount
        .adopt(
            viewport,
            vec![response.clone()],
            appearance,
            window,
            window.text_system().font_environment_key().unwrap(),
        )
        .unwrap();
    for _ in 0..1000 {
        mount.advance(viewport, appearance, window).unwrap();
        assert!(mount.charge() <= 8 * 1024 * 1024);
        assert!(mount.checkpoints() <= 32);
        mount.resolve_inline_anchor(viewport);
        mount.publish_candidate(viewport);
        if let Some(IndexedResourceQuery::Display { request, .. }) =
            mount.query.as_ref().map(|query| query.query.clone())
        {
            let start = request.offset as usize;
            let end = (start + 8192).min(source.len());
            let root = response.root.as_ref().unwrap();
            mount
                .adopt(
                    viewport,
                    vec![Arc::new(IndexedResourceResponse {
                        root: Some(root.clone()),
                        value: IndexedResourceValue::Display(index::DisplayResponse {
                            binding: range.binding.clone(),
                            cursor: index::DisplayCursor {
                                build: root.key,
                                root: root.digest,
                                request,
                                next_offset: end as u64,
                            },
                            descriptor: None,
                            fragment: Some(index::Fragment {
                                text: source[start..end].into(),
                                source: index::ByteRange {
                                    start: start as u64,
                                    end: end as u64,
                                },
                                remaining: index::ByteRange {
                                    start: end as u64,
                                    end: source.len() as u64,
                                },
                                next_offset: end as u64,
                            }),
                            alignment: None,
                            literal_fallback: false,
                            complete: end == source.len(),
                            encoded_bytes: end - start + 512,
                        }),
                        encoded_bytes: end - start + 512,
                    })],
                    appearance,
                    window,
                    window.text_system().font_environment_key().unwrap(),
                )
                .unwrap();
        }
        if let Some(width) = mount.complete_width(1) {
            let mut scroll = viewport.scroll.borrow_mut();
            scroll.horizontal = scroll.horizontal.min((width - 380.0).max(0.0));
        }
        if mount.state(viewport) == "ready" && mount.complete_width(1).is_some() {
            return;
        }
        mount.reconcile_range(viewport);
    }
    panic!("bounded Code coordinator must finish the target and extent");
}
