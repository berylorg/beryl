use super::super::super::super::{
    IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    provider::resource_index as index,
};
use super::super::super::code_viewport::{CodeInlineAnchor, CodeViewport};
use super::*;
use beryl_model::*;
use syndic_storage::{
    ProjectionSourceRange, ProjectionTextSource, ProviderNarrativeGeneration,
    ProviderNarrativeReference, ResourceBacking, ResourceMetadataRecord, ResourceOrdinal,
    ResourceStructure,
};

#[path = "code_lifecycle.rs"]
mod code_lifecycle;

pub(super) fn binding(revision: u64) -> index::Binding {
    binding_with_source(revision, 1)
}

fn binding_with_source(revision: u64, source_revision: u64) -> index::Binding {
    binding_with_resource(revision, source_revision, [11; 16])
}

fn binding_with_resource(revision: u64, source_revision: u64, id: [u8; 16]) -> index::Binding {
    narrative_binding(revision, source_revision, id, 10000)
}

fn narrative_binding(revision: u64, generation: u64, id: [u8; 16], bytes: u64) -> index::Binding {
    let source = ProjectionTextSource::provider_narrative(
        ProviderNarrativeReference::new(
            SyndicContentId::from_bytes([33; 16]),
            ProviderNarrativeGeneration::new(generation).unwrap(),
            bytes.div_ceil(10000),
            bytes,
            [44; 32],
        )
        .unwrap(),
    );
    let resource = index::ResourceKey {
        id: SyndicResourceId::from_bytes(id),
        revision: ProjectionRevision::new(revision).unwrap(),
    };
    index::Binding {
        resource,
        metadata: ResourceMetadataRecord::new(
            resource.id,
            resource.revision,
            SyndicProjectionId::from_bytes([55; 16]),
            SyndicItemId::from_bytes([66; 16]),
            ResourceOrdinal::FIRST,
            syndic_storage::ResourceKind::Code,
            "text/plain",
            ResourceBacking::TextRange {
                source,
                range: ProjectionSourceRange::new(0, bytes).unwrap(),
            },
            [77; 32],
            None,
            ResourceStructure::code(Some("rust"), 2000).unwrap(),
        )
        .unwrap(),
        source,
        range: index::ByteRange {
            start: 0,
            end: bytes,
        },
    }
}

fn root(binding: &index::Binding, lines: u64) -> index::Root {
    let build = index::BuildKey {
        resource: binding.resource,
        operation: [22; 16],
    };
    index::Root {
        key: build,
        binding_digest: [88; 32],
        pages: 1,
        chain: [99; 32],
        counts: [lines, 0, 0, 0],
        code_lines: lines,
        body_rows: 0,
        columns: 0,
        header: None,
        language: Some("rust".into()),
        progress: index::SequenceKey { build, ordinal: 1 },
        digest: [55; 32],
    }
}

fn descriptor(ordinal: u64) -> index::IndexedDescriptor {
    let content = index::ByteRange {
        start: ordinal * 5,
        end: ordinal * 5 + 4,
    };
    index::IndexedDescriptor {
        descriptor: index::Descriptor::CodeLine {
            ordinal,
            raw: index::ByteRange {
                start: content.start,
                end: content.end + 1,
            },
            content,
            terminator: index::ByteRange {
                start: content.end,
                end: content.end + 1,
            },
        },
        absolute_raw: Some(index::ByteRange {
            start: content.start,
            end: content.end + 1,
        }),
    }
}

fn captured_viewport(cx: &mut TestAppContext) -> CodeViewport {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = CodeViewport::new(owner, ProjectionRevision::new(1).unwrap());
    viewport.scroll.borrow_mut().set_lines(2000);
    viewport.scroll.borrow_mut().set_offset(1007.5);
    viewport.scroll.borrow_mut().horizontal = 45.0;
    viewport.adopt(&range_response(binding(1), 50));
    assert_eq!(viewport.anchor.unwrap().visual_offset, -8);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(2).unwrap());
    assert!(viewport.remapping);
    viewport
}

fn retain_inline_from_vertical(viewport: &mut CodeViewport) {
    let logical_line = viewport.scroll.borrow().top_line;
    viewport.retain_inline_anchor(CodeInlineAnchor {
        source: viewport.anchor.unwrap(),
        logical_line,
        visual_offset: -0.375,
    });
    viewport.begin_inline_remap();
}

#[gpui::test]
fn shrinking_code_row_metrics_preserves_the_source_anchor_within_overscan(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = CodeViewport::new(owner, ProjectionRevision::new(1).unwrap());
    viewport.scroll.borrow_mut().set_lines(2000);
    viewport.set_line_height(40.0);
    viewport.scroll.borrow_mut().set_offset(190.5);
    viewport.scroll.borrow_mut().horizontal = 5000.375;
    viewport.adopt(&range_response(binding(1), 4));
    let retained = CodeInlineAnchor {
        source: index::SourceAnchor {
            position: 22,
            ..viewport.anchor.unwrap()
        },
        logical_line: 4,
        visual_offset: -0.375,
    };
    viewport.retain_inline_anchor(retained);
    assert_eq!(viewport.anchor_visual_offset(), Some(-30.5));
    viewport.begin_inline_remap();
    viewport.set_line_height(20.0);
    assert_eq!(viewport.scroll.borrow().top_line, 5);
    assert_eq!(viewport.scroll.borrow().remainder, 10.5);
    viewport.adopt(&range_response(binding(1), 5));
    assert_eq!(viewport.pending_inline_anchor(), Some(retained));
    assert_eq!(viewport.anchor_visual_offset(), Some(-30.5));
    assert!(viewport.resolve_inline_anchor(retained, 10_000.0));
    assert_eq!(viewport.scroll.borrow().horizontal, 10_000.375);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(2).unwrap());
    assert!(
        matches!(viewport.query().query, IndexedResourceQuery::Anchor { anchor, .. }
        if anchor.position == 22 && anchor.visual_offset == -31)
    );
    assert_eq!(viewport.pending_inline_anchor(), Some(retained));
}

pub(super) fn range_response(binding: index::Binding, ordinal: u64) -> IndexedResourceResponse {
    let root = root(&binding, 2000);
    let request = index::Request::Code {
        lines: index::OrdinalRange {
            start: ordinal.saturating_sub(2),
            end: ordinal + 14,
        },
    };
    IndexedResourceResponse {
        value: IndexedResourceValue::Range(index::Response {
            binding,
            cursor: index::Cursor {
                build: root.key,
                root_digest: root.digest,
                request,
                next: index::Position::Code(ordinal + 14),
                fragment_offset: 0,
            },
            items: vec![index::Item::Descriptor(descriptor(ordinal))],
            request_complete: true,
            resource_complete: false,
            encoded_bytes: 0,
            presentations: None,
        }),
        root: Some(root),
        encoded_bytes: 0,
    }
}

fn successor(viewport: &CodeViewport, lines: u64, ordinal: Option<u64>) -> IndexedResourceResponse {
    let binding = binding(viewport.revision.get());
    let resolved = ordinal.map(|ordinal| index::ResolvedAnchor {
        descriptor: descriptor(ordinal),
        anchor: index::SourceAnchor {
            source: binding.source,
            position: ordinal * 5,
            ..viewport.anchor.unwrap()
        },
    });
    IndexedResourceResponse {
        root: Some(root(&binding, lines)),
        value: IndexedResourceValue::Anchor(index::AnchorResponse {
            binding,
            resolved,
            encoded_bytes: 0,
        }),
        encoded_bytes: 0,
    }
}

#[gpui::test]
fn code_rebind_captures_a_new_top_line_from_the_resident_range(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    viewport.adopt(&successor(&viewport, 2000, Some(75)));
    let mut response = range_response(binding(2), 75);
    let IndexedResourceValue::Range(range) = &mut response.value else {
        unreachable!();
    };
    range.items = (73..89)
        .map(|ordinal| index::Item::Descriptor(descriptor(ordinal)))
        .collect();
    viewport.adopt(&response);
    viewport.scroll.borrow_mut().scroll_by(24.75);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(3).unwrap());
    assert_eq!(viewport.anchor.unwrap().position, 380);
    assert_eq!(viewport.anchor_visual_offset(), Some(-12.25));
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Anchor { anchor, .. }
            if anchor.position == 380 && anchor.visual_offset == -12
    ));
    viewport.adopt(&successor(&viewport, 2000, Some(90)));
    assert_eq!(viewport.scroll.borrow().top_line, 90);
    assert_eq!(viewport.scroll.borrow().remainder, 12.25);
    assert_eq!(viewport.scroll.borrow().horizontal, 45.0);
}

#[gpui::test]
fn code_rebind_discards_the_previous_revisions_resident_positions(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    viewport.adopt(&successor(&viewport, 2000, Some(75)));
    let mut response = range_response(binding(2), 75);
    let IndexedResourceValue::Range(range) = &mut response.value else {
        unreachable!();
    };
    range.items = (73..89)
        .map(|ordinal| index::Item::Descriptor(descriptor(ordinal)))
        .collect();
    viewport.adopt(&response);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(3).unwrap());
    viewport.adopt(&successor(&viewport, 2000, Some(80)));
    viewport.scroll.borrow_mut().scroll_by(24.75);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(4).unwrap());
    assert_eq!(viewport.scroll.borrow().top_line, 81);
    assert!(viewport.anchor.is_none());
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Range {
            request: index::Request::Code { lines }, ..
        } if lines.start == 79
    ));
}

#[gpui::test]
fn code_rebind_captures_fractional_scrolling_before_another_range_arrives(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    viewport.adopt(&successor(&viewport, 2000, Some(75)));
    viewport.scroll.borrow_mut().scroll_by(4.75);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(3).unwrap());
    assert_eq!(viewport.anchor_visual_offset(), Some(-12.25));
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Anchor { anchor, .. } if anchor.visual_offset == -12
    ));
    let response = successor(&viewport, 2000, Some(90));
    viewport.adopt(&response);
    assert_eq!(viewport.scroll.borrow().top_line, 90);
    assert_eq!(viewport.scroll.borrow().remainder, 12.25);
    assert_eq!(viewport.scroll.borrow().horizontal, 45.0);
}

#[gpui::test]
fn code_rebind_fetches_the_current_line_when_revision_precedes_range_admission(
    cx: &mut TestAppContext,
) {
    let mut viewport = captured_viewport(cx);
    viewport.adopt(&successor(&viewport, 2000, Some(75)));
    viewport.scroll.borrow_mut().scroll_by(24.75);
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(3).unwrap());
    assert_eq!(viewport.scroll.borrow().top_line, 76);
    assert!(viewport.anchor.is_none());
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Range {
            request: index::Request::Code { lines }, ..
        } if lines.start == 74
    ));
    viewport.adopt(&range_response(binding(3), 76));
    assert_eq!(viewport.scroll.borrow().top_line, 76);
    assert_eq!(viewport.scroll.borrow().remainder, 12.25);
    assert_eq!(viewport.scroll.borrow().horizontal, 45.0);
    assert_eq!(viewport.anchor.unwrap().position, 380);
    assert_eq!(viewport.anchor_visual_offset(), Some(-12.25));
    assert_eq!(viewport.source_continuity, "same");
}

#[gpui::test]
fn code_successor_anchor_adoption_preserves_fractional_offset_and_returns_to_range_queries(
    cx: &mut TestAppContext,
) {
    let mut viewport = captured_viewport(cx);
    let response = successor(&viewport, 2000, Some(75));
    viewport.adopt(&response);
    let scroll = viewport.scroll.borrow();
    assert_eq!(scroll.top_line, 75);
    assert_eq!(viewport.source_continuity, "same");
    assert_eq!(viewport.anchor_remap_result, "resolved");
    assert_eq!(viewport.anchor_visual_offset(), Some(-7.5));
    assert_eq!(scroll.remainder, 7.5);
    assert_eq!(scroll.horizontal, 45.0);
    assert_eq!(viewport.anchor.unwrap().source, binding(2).source);
    assert_eq!(viewport.anchor.unwrap().position, 375);
    assert!(!viewport.remapping);
    assert_eq!(viewport.query().root, Some([55; 32]));
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Range {
            request: index::Request::Code {
                lines: index::OrdinalRange { start: 73, end: 89 }
            },
            ..
        }
    ));
}

#[gpui::test]
fn code_successor_anchor_adoption_clamps_at_the_new_end(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    let response = successor(&viewport, 25, Some(24));
    viewport.adopt(&response);
    let scroll = viewport.scroll.borrow();
    assert_eq!(scroll.top_line, 13);
    assert_eq!(viewport.anchor_remap_result, "clamped");
    assert_eq!(scroll.remainder, 0.0);
    assert_eq!(scroll.horizontal, 45.0);
    assert_eq!(scroll.range().end, 25);
    assert!(!viewport.remapping);
}

#[gpui::test]
fn code_unresolved_successor_anchor_resets_both_axes(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    let response = successor(&viewport, 2000, None);
    viewport.adopt(&response);
    assert_eq!(viewport.scroll.borrow().offset(), 0.0);
    assert_eq!(viewport.scroll.borrow().horizontal, 0.0);
    assert_eq!(viewport.anchor, None);
    assert!(!viewport.remapping);
    assert_eq!(viewport.anchor_remap_result, "unresolved");
    assert_eq!(viewport.anchor_visual_offset(), None);
}

#[gpui::test]
fn code_empty_successor_root_clears_the_anchor_even_with_a_resolved_response(
    cx: &mut TestAppContext,
) {
    let mut viewport = captured_viewport(cx);
    retain_inline_from_vertical(&mut viewport);
    let response = successor(&viewport, 0, Some(0));
    viewport.adopt(&response);
    assert_eq!(viewport.scroll.borrow().offset(), 0.0);
    assert_eq!(viewport.scroll.borrow().horizontal, 0.0);
    assert_eq!(viewport.anchor, None);
    assert!(!viewport.remapping);
    assert_eq!(viewport.anchor_remap_result, "empty");
    assert!(viewport.inline_anchor.is_none());
    assert!(!viewport.inline_remapping);
}

#[gpui::test]
fn code_changed_source_without_an_anchor_starts_at_the_origin(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    retain_inline_from_vertical(&mut viewport);
    viewport.anchor = None;
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(2).unwrap());
    viewport.adopt(&range_response(binding_with_source(2, 2), 50));
    assert_eq!(viewport.source_continuity, "changed");
    assert_eq!(viewport.anchor_remap_result, "changed_source");
    assert_eq!(viewport.scroll.borrow().offset(), 0.0);
    assert_eq!(viewport.scroll.borrow().horizontal, 0.0);
    assert_eq!(viewport.anchor, None);
    assert!(!viewport.remapping);
    assert!(viewport.inline_anchor.is_none());
    assert!(!viewport.inline_remapping);
    assert!(matches!(
        viewport.query().query,
        IndexedResourceQuery::Range {
            request: index::Request::Code {
                lines: index::OrdinalRange { start: 0, end: 16 }
            },
            ..
        }
    ));
    viewport.adopt(&range_response(binding_with_source(2, 2), 0));
    assert_eq!(
        viewport.anchor.unwrap().source,
        binding_with_source(2, 2).source
    );
    viewport.scroll.borrow_mut().set_offset(27.5);
    viewport.adopt(&range_response(binding_with_source(2, 2), 1));
    assert_eq!(viewport.scroll.borrow().offset(), 27.5);
}

#[gpui::test]
fn code_changed_block_start_resets_retained_scroll(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    let mut next = binding(2);
    next.range.start = 5;
    viewport.adopt(&range_response(next, 50));
    assert_eq!(viewport.scroll.borrow().offset(), 0.0);
    assert_eq!(viewport.scroll.borrow().horizontal, 0.0);
    assert_eq!(viewport.anchor, None);
    assert!(!viewport.remapping);
}

#[gpui::test]
fn code_same_source_without_an_anchor_preserves_retained_scroll(cx: &mut TestAppContext) {
    let mut viewport = captured_viewport(cx);
    viewport.anchor = None;
    viewport.rebind(viewport.owner.clone(), ProjectionRevision::new(2).unwrap());
    viewport.adopt(&range_response(binding(2), 50));
    assert_eq!(viewport.source_continuity, "same");
    assert_eq!(viewport.scroll.borrow().top_line, 50);
    assert_eq!(viewport.scroll.borrow().remainder, 7.5);
    assert_eq!(viewport.scroll.borrow().horizontal, 45.0);
    assert_eq!(viewport.anchor.unwrap().position, 250);
    assert!(!viewport.remapping);
}
