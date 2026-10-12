use super::super::code_layout::{
    CodeLayoutAnchor, CodeLayoutError, CodeLayoutIndex, CodeLayoutLimits, CodeLayoutProgress,
    CodeLayoutReadKind, CodeLayoutTarget, PreparedCodeLayout, tab_advance,
};
use gpui::{
    StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TestAppContext,
    TextRun, WindowTextSystem, font, px,
};

fn limits(cap: usize) -> CodeLayoutLimits {
    CodeLayoutLimits {
        segment_bytes: cap,
        page_bytes: 257,
        segments_per_quantum: 3,
        boundary_steps_per_quantum: 13,
        checkpoints: 8,
        fragments: 64,
        retained_bytes: 2 * 1024 * 1024,
        retained_items: 100_000,
    }
}

fn binding(start: u64, cap: usize) -> StreamingLayoutBinding {
    StreamingLayoutBinding {
        input_id: 7,
        segment_policy_id: 11,
        start_position: StreamingLayoutPosition::at(start),
        wrap_width: px(1.0),
        font_size: px(10.0),
        line_height: px(20.0),
        limits: StreamingLayoutLimits {
            segment_bytes: cap,
            runs: 4,
            decorations: 4,
            glyphs: cap * 4,
            wraps: 1,
            maps: cap * 4 + 2,
            fragments: 1,
            retained_items: 4096,
            retained_bytes: 128 * 1024,
        },
    }
}

fn run() -> TextRun {
    TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn drive(
    index: &mut CodeLayoutIndex,
    source: &str,
    origin: u64,
    page: usize,
    system: &WindowTextSystem,
) -> CodeLayoutProgress {
    for _ in 0..200_000 {
        if let Some(request) = index.next_read() {
            let mut start = (request.range.start - origin) as usize;
            let requested_end = (request.range.end - origin) as usize;
            let mut end = requested_end;
            if request.kind == CodeLayoutReadKind::Forward {
                end = (start + page).min(end);
                while end > start && !source.is_char_boundary(end) {
                    end -= 1;
                }
                if end == start {
                    end += source[start..].chars().next().unwrap().len_utf8();
                }
            } else {
                while !source.is_char_boundary(start) {
                    start += 1;
                }
            }
            index
                .supply_page(origin + start as u64, &source[start..end])
                .unwrap();
        }
        let progress = index.advance(system, &run()).unwrap();
        if matches!(
            progress,
            CodeLayoutProgress::Ready | CodeLayoutProgress::Complete
        ) {
            return progress;
        }
    }
    panic!("bounded layout did not converge")
}

fn full(
    source: &str,
    origin: u64,
    page: usize,
    cap: usize,
    system: &WindowTextSystem,
) -> PreparedCodeLayout {
    let mut index = CodeLayoutIndex::new(
        binding(origin, cap),
        origin..origin + source.len() as u64,
        19,
        limits(cap),
    )
    .unwrap();
    index
        .request_target(CodeLayoutTarget {
            request_id: 1,
            lines: 19..20,
            inline_start: px(0.0),
            inline_end: px(1_000_000.0),
        })
        .unwrap();
    assert_eq!(
        drive(&mut index, source, origin, page, system),
        CodeLayoutProgress::Complete
    );
    index.take_prepared().unwrap()
}

#[gpui::test]
fn code_layout_canonical_segments_and_geometry_ignore_transport_partitions(
    cx: &mut TestAppContext,
) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let text = "abé👩\u{200d}💻x🇨🇿z\t界e\u{301}W".repeat(4);
        let system = window.text_system();
        let reference = full(&text, 517, 257, 32, system);
        for page in [1, 4, 7, 17, 31, 256] {
            let partitioned = full(&text, 517, page, 32, system);
            assert_eq!(partitioned.terminal_width, reference.terminal_width);
            assert_eq!(partitioned.complete_max_width, reference.complete_max_width);
            assert_eq!(partitioned.fragments.len(), reference.fragments.len());
            for (left, right) in partitioned.fragments.iter().zip(&reference.fragments) {
                assert_eq!(left.source, right.source);
                assert_eq!(left.inline_start, right.inline_start);
                assert_eq!(left.inline_end, right.inline_end);
                assert_eq!(left.logical_line, 19);
                assert!(left.source.end - left.source.start <= 32);
                if let gpui::StreamingLayoutFragment::Text(text) = &left.fragments[0] {
                    assert!(text.line().wrap_boundaries().is_empty());
                }
            }
        }
    });
}

#[gpui::test]
fn code_layout_oversize_grapheme_preserves_exact_span_without_payload(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let text = format!("A{}B", "\u{301}".repeat(50_000));
        let prepared = full(&text, 10, 257, 32, window.text_system());
        assert_eq!(prepared.fragments.len(), 2);
        let atom = &prepared.fragments[0];
        assert!(atom.oversize);
        assert_eq!(atom.source, 10..10 + 100_001);
        assert!(matches!(
            atom.fragments[0],
            gpui::StreamingLayoutFragment::OversizeAtom(_)
        ));
        assert!(!prepared.fragments[1].oversize);
        assert_eq!(prepared.fragments[1].source, 100_011..100_012);
        assert!(prepared.charged_bytes < 128 * 1024);
    });
}

#[gpui::test]
fn code_layout_fixed_checkpoints_replay_distant_targets_and_exact_anchor(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let text = "x".repeat(100_000);
        let system = window.text_system();
        let mut index =
            CodeLayoutIndex::new(binding(70, 256), 70..100_070, 4, limits(256)).unwrap();
        assert_eq!(
            drive(&mut index, &text, 70, 257, system),
            CodeLayoutProgress::Complete
        );
        assert!(index.checkpoint_count() <= 8);
        let terminal = index.complete_max_width().unwrap();
        let anchor = CodeLayoutAnchor {
            request_id: 9,
            logical_line: 4,
            source: 75_070,
        };
        index.request_source_anchor(anchor).unwrap();
        assert_eq!(
            drive(&mut index, &text, 70, 7, system),
            CodeLayoutProgress::Ready
        );
        let resolved = index.resolved_anchor().unwrap();
        assert_eq!(resolved.anchor, anchor);
        assert!(resolved.inline > px(0.0) && resolved.inline < terminal);
        index
            .request_target(CodeLayoutTarget {
                request_id: 10,
                lines: 4..5,
                inline_start: resolved.inline,
                inline_end: resolved.inline + px(400.0),
            })
            .unwrap();
        assert_eq!(
            drive(&mut index, &text, 70, 17, system),
            CodeLayoutProgress::Ready
        );
        let prepared = index.take_prepared().unwrap();
        assert_eq!(prepared.target.request_id, 10);
        assert!(prepared.fragments.len() <= 4);
        assert!(prepared.fragments[0].source.start > 70);
        assert!(
            prepared
                .fragments
                .iter()
                .any(|fragment| fragment.source.contains(&anchor.source))
        );
        assert_eq!(prepared.complete_max_width, Some(terminal));
        assert!(index.take_prepared().is_none());
        assert!(index.checkpoint_count() <= 8);
        assert!(index.charged_bytes().unwrap() <= limits(256).retained_bytes);
    });
}

#[gpui::test]
fn code_layout_empty_and_crlf_terminal_geometry_remain_exact(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let empty = full("", 3, 4, 32, window.text_system());
        assert!(empty.fragments.is_empty());
        assert_eq!(empty.terminal_width, Some(px(0.0)));
        assert_eq!(empty.complete_max_width, Some(px(0.0)));
        let text = "xx\r\n";
        let mut index = CodeLayoutIndex::new(binding(20, 32), 20..24, 0, limits(32)).unwrap();
        index
            .request_target(CodeLayoutTarget {
                request_id: 1,
                lines: 0..1,
                inline_start: px(0.0),
                inline_end: px(10_000.0),
            })
            .unwrap();
        assert_eq!(
            drive(&mut index, text, 20, 1, window.text_system()),
            CodeLayoutProgress::Ready
        );
        let first = index.take_prepared().unwrap();
        assert_eq!(first.fragments[0].source, 20..22);
        assert!(first.terminal_width.unwrap() > px(0.0));
        assert!(first.complete_max_width.is_none());
    });
}

#[test]
fn code_layout_refuses_capacity_bad_pages_and_unrepresentable_tabs() {
    let mut tiny = limits(32);
    tiny.retained_bytes = 1;
    assert!(matches!(
        CodeLayoutIndex::new(binding(10, 32), 10..20, 0, tiny),
        Err(CodeLayoutError::Capacity)
    ));
    let mut index = CodeLayoutIndex::new(binding(10, 32), 10..20, 0, limits(32)).unwrap();
    assert_eq!(
        index.supply_page(11, "a"),
        Err(CodeLayoutError::InvalidPage)
    );
    assert_eq!(index.supply_page(10, ""), Err(CodeLayoutError::InvalidPage));
    assert_eq!(tab_advance(px(16.0), px(8.0)), Ok(px(8.0)));
    assert_eq!(tab_advance(px(17.5), px(8.0)), Ok(px(6.5)));
    assert_eq!(
        tab_advance(px(f32::MAX), px(8.0)),
        Err(CodeLayoutError::InvalidGeometry)
    );
    assert_eq!(
        tab_advance(px(0.0), px(0.0)),
        Err(CodeLayoutError::InvalidGeometry)
    );
}
