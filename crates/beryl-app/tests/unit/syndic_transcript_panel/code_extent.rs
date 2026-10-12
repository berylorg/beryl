use super::*;
use crate::syndic_transcript::IndexedSourceResponse;

#[gpui::test]
fn malformed_whole_code_source_refuses_extent_without_replacing_foreground_geometry(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |mount, viewport, window| {
        let appearance = CodeAppearance::default();
        let source = "x\n".repeat(20) + &"x".repeat(9000) + "\n" + &"x\n".repeat(19);
        let response = extent_response(&source, 1, viewport.scroll.borrow().range());
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
        for _ in 0..100 {
            mount.advance(viewport, &appearance, window).unwrap();
            mount.publish_candidate(viewport);
            if mount.query.is_some() {
                break;
            }
        }
        let Some(IndexedResourceQuery::Source { range, .. }) =
            mount.query.as_ref().map(|query| query.query.clone())
        else {
            panic!("bounded extent source demand")
        };
        let prior = mount
            .prepared()
            .map(|prepared| prepared.target.clone())
            .collect::<Vec<_>>();
        let IndexedResourceValue::Range(value) = &response.value else {
            unreachable!();
        };
        let mut bytes = source.as_bytes()[range.start as usize..range.end as usize].to_vec();
        bytes[0] = 0xff;
        assert_eq!(
            mount.adopt(
                viewport,
                vec![Arc::new(IndexedResourceResponse {
                    root: response.root.clone(),
                    encoded_bytes: bytes.len() + 1024,
                    value: IndexedResourceValue::Source(IndexedSourceResponse {
                        binding: value.binding.clone(),
                        range,
                        bytes
                    }),
                })],
                &appearance,
                window,
                window.text_system().font_environment_key().unwrap()
            ),
            Err(super::super::super::super::code_layout::CodeLayoutError::InvalidPage)
        );
        assert_eq!(mount.complete_width(40), None);
        assert_eq!(
            mount
                .prepared()
                .map(|prepared| prepared.target.clone())
                .collect::<Vec<_>>(),
            prior
        );
    });
}

#[gpui::test]
fn complete_code_extent_includes_an_unseen_line_and_survives_foreground_preemption(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |mount, viewport, window| {
        let appearance = CodeAppearance::default();
        let source = "x\n".repeat(20) + &"x".repeat(9000) + "\n" + &"x\n".repeat(19);
        let response = extent_response(&source, 1, viewport.scroll.borrow().range());
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
        for _ in 0..100 {
            mount.advance(viewport, &appearance, window).unwrap();
            mount.publish_candidate(viewport);
            if mount.state(viewport) == "ready" {
                break;
            }
        }
        assert_eq!(mount.complete_width(40), None);
        mount.advance(viewport, &appearance, window).unwrap();
        let source_query = mount.query.clone().unwrap();
        assert!(matches!(
            source_query.query,
            IndexedResourceQuery::Source { .. }
        ));
        let old_anchor = viewport.inline_anchor;
        supply_extent(mount, viewport, &response, &source, &appearance, window);
        for _ in 0..4 {
            mount.advance(viewport, &appearance, window).unwrap();
        }
        assert_eq!(viewport.inline_anchor, old_anchor);
        viewport.layout_query = Some(source_query);
        viewport.scroll.borrow_mut().horizontal = 1000.375;
        mount.reconcile_range(viewport);
        assert!(viewport.layout_query.is_none());
        for _ in 0..100 {
            mount.advance(viewport, &appearance, window).unwrap();
            mount.publish_candidate(viewport);
            if mount.state(viewport) == "ready" {
                break;
            }
        }
        assert_eq!(mount.state(viewport), "ready");
        assert_eq!(viewport.scroll.borrow().horizontal, 1000.375);
        assert_eq!(mount.complete_width(40), None);
        finish_extent(mount, viewport, &response, &source, &appearance, window);
        let exact = mount.complete_width(40).unwrap();
        let mut reference_mount = CodeLayoutMount::default();
        let mut reference_viewport = CodeViewport::new(viewport.owner.clone(), viewport.revision);
        reference_viewport.scroll.borrow_mut().bounds.size.width = gpui::px(400.0);
        drive_code_layout(
            &mut reference_mount,
            &mut reference_viewport,
            &code_line_response(1, 9000),
            &appearance,
            window,
        );
        assert_eq!(exact, reference_mount.complete_width(1).unwrap());
        assert!(exact > 1000.375 + 380.0);
        assert_eq!(viewport.scroll.borrow().horizontal, 1000.375);
        assert!(mount.foreground_response().is_some());
        mount.reset();
        assert_eq!(mount.charge(), 0);
        assert_eq!(mount.complete_width(40), None);
        assert_eq!(mount.checkpoints(), 0);
    });
}

#[gpui::test]
fn complete_code_extent_decodes_utf8_page_edges_and_crlf_without_counting_delimiters(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |mount, viewport, window| {
        let appearance = CodeAppearance::default();
        let source =
            "x\r\n".repeat(20) + "a" + &"🍒".repeat(2048) + "xxx\r\n" + &"x\r\n".repeat(19);
        let response = extent_response(&source, 1, viewport.scroll.borrow().range());
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
        finish_extent(mount, viewport, &response, &source, &appearance, window);
        let width = mount.complete_width(40).unwrap();
        assert!(width.is_finite() && width > 1000.0);
        assert_eq!(viewport.scroll.borrow().horizontal, 0.0);
        let mut changed = appearance.clone();
        changed.body_font.size *= 2.0;
        mount
            .reconcile_environment(
                viewport,
                &changed,
                window,
                20.0,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        assert_eq!(mount.complete_width(40), None);
        mount.reconcile_range(viewport);
        mount
            .adopt(
                viewport,
                vec![response.clone()],
                &changed,
                window,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        finish_extent(mount, viewport, &response, &source, &changed, window);
        assert!(mount.complete_width(40).unwrap() > width);
    });
}

fn extent_response(
    source: &str,
    revision: u64,
    lines: index::OrdinalRange,
) -> Arc<IndexedResourceResponse> {
    let base = code_line_response(revision, source.len() as u64);
    let IndexedResourceValue::Range(base_range) = &base.value else {
        unreachable!();
    };
    let mut root = base.root.clone().unwrap();
    let count = source.split_inclusive('\n').count() as u64;
    root.code_lines = count;
    root.counts = [count, 0, 0, 0];
    let mut binding = base_range.binding.clone();
    let metadata = &binding.metadata;
    binding.metadata = syndic_storage::ResourceMetadataRecord::new(
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
        syndic_storage::ResourceStructure::code(Some("rust"), count).unwrap(),
    )
    .unwrap();
    let mut offset = 0u64;
    let mut items = Vec::new();
    let mut presentations = Vec::new();
    for (ordinal, text) in source.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += text.len() as u64;
        let content = text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix('\n'))
            .unwrap_or(text);
        if ordinal as u64 >= lines.start && (ordinal as u64) < lines.end {
            let content_end = start + content.len() as u64;
            let raw = index::ByteRange { start, end: offset };
            items.push(index::Item::Descriptor(index::IndexedDescriptor {
                descriptor: index::Descriptor::CodeLine {
                    ordinal: ordinal as u64,
                    raw,
                    content: index::ByteRange {
                        start,
                        end: content_end,
                    },
                    terminator: index::ByteRange {
                        start: content_end,
                        end: offset,
                    },
                },
                absolute_raw: Some(raw),
            }));
            let mut end = content.len().min(8192);
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            presentations.push(index::Presentation {
                fragment: Some(index::Fragment {
                    text: content[..end].into(),
                    source: index::ByteRange {
                        start,
                        end: start + end as u64,
                    },
                    remaining: index::ByteRange {
                        start: start + end as u64,
                        end: content_end,
                    },
                    next_offset: end as u64,
                }),
                alignment: None,
                literal_fallback: false,
            });
        }
    }
    Arc::new(IndexedResourceResponse {
        root: Some(root.clone()),
        value: IndexedResourceValue::Range(index::Response {
            binding,
            cursor: index::Cursor {
                build: root.key,
                root_digest: root.digest,
                request: index::Request::Code { lines },
                next: index::Position::Code(lines.end.min(count)),
                fragment_offset: 0,
            },
            items,
            request_complete: true,
            resource_complete: lines.end >= count,
            encoded_bytes: 16384,
            presentations: Some(presentations),
        }),
        encoded_bytes: 16384,
    })
}

fn supply_extent(
    mount: &mut CodeLayoutMount,
    viewport: &CodeViewport,
    response: &Arc<IndexedResourceResponse>,
    source: &str,
    appearance: &CodeAppearance,
    window: &gpui::Window,
) {
    let Some(IndexedResourceQuery::Source {
        range,
        bytes: allowance,
    }) = mount.query.as_ref().map(|query| query.query.clone())
    else {
        return;
    };
    assert!(range.end - range.start <= 8192);
    let IndexedResourceValue::Range(value) = &response.value else {
        unreachable!();
    };
    let bytes = source.as_bytes()[range.start as usize..range.end as usize].to_vec();
    assert!(bytes.len() + 1024 <= allowance);
    mount
        .adopt(
            viewport,
            vec![Arc::new(IndexedResourceResponse {
                root: response.root.clone(),
                encoded_bytes: bytes.len() + 1024,
                value: IndexedResourceValue::Source(IndexedSourceResponse {
                    binding: value.binding.clone(),
                    range,
                    bytes,
                }),
            })],
            appearance,
            window,
            window.text_system().font_environment_key().unwrap(),
        )
        .unwrap();
}

fn finish_extent(
    mount: &mut CodeLayoutMount,
    viewport: &mut CodeViewport,
    response: &Arc<IndexedResourceResponse>,
    source: &str,
    appearance: &CodeAppearance,
    window: &gpui::Window,
) {
    let lines = response.root.as_ref().unwrap().code_lines;
    for _ in 0..2000 {
        mount.advance(viewport, appearance, window).unwrap();
        mount.resolve_inline_anchor(viewport);
        mount.publish_candidate(viewport);
        supply_extent(mount, viewport, response, source, appearance, window);
        assert!(mount.charge() <= 16 * 1024 * 1024);
        assert!(mount.checkpoints() <= 16 * 32 + 32);
        assert!(mount.prepared().count() <= 16);
        if mount.complete_width(lines).is_some() {
            return;
        }
    }
    panic!("complete source extent must finish within bounded quanta");
}
