#[allow(dead_code)]
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

#[allow(dead_code)]
#[path = "../src/shell/transcript_anchor.rs"]
mod transcript_anchor;

use gpui::px;

#[test]
fn prompt_anchor_offset_targets_last_visual_line() {
    let offset = transcript_anchor::test_support::prompt_last_line_top_offset_from_counts(
        3,
        &[1, 3],
        px(20.0),
    );

    assert_eq!(offset, px(73.0));
}

#[test]
fn first_turn_anchor_includes_initial_row_padding() {
    let offset =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_counts(0, &[1], px(20.0));

    assert_eq!(offset, px(29.0));
}

#[test]
fn prompt_start_geometry_targets_fragment_block_top() {
    let geometry = transcript_anchor::test_support::prompt_geometry_from_markdown_no_wrap(
        0,
        "alpha",
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(18.0),
        px(12.0),
    );

    assert_eq!(geometry.fragment_start_offset, px(16.0));
    assert_eq!(geometry.fragment_content_start_offset, px(29.0));
    assert_eq!(geometry.last_visual_line_top_offset, px(29.0));
    assert_eq!(geometry.fragment_tail_offset, px(62.0));
}

#[test]
fn prompt_anchor_lines_match_transcript_newline_rendering() {
    assert_eq!(
        transcript_anchor::test_support::prompt_lines("alpha\nbeta\n\n gamma "),
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            String::new(),
            " gamma ".to_string()
        ]
    );
    assert_eq!(
        transcript_anchor::test_support::prompt_lines("alpha\n"),
        vec!["alpha".to_string(), String::new()]
    );
}

#[test]
fn markdown_prompt_anchor_accounts_for_headings_lists_and_block_gaps() {
    let offset = transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_no_wrap(
        3,
        "# Title\n\nParagraph\n\n- first\n- second",
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(18.0),
        px(12.0),
    );

    assert_eq!(offset, px(107.0));
}

#[test]
fn markdown_prompt_anchor_accounts_for_quotes_and_fenced_code_blocks() {
    let offset = transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_no_wrap(
        2,
        "> quoted\n\n```rust\nfn main() {}\nlet x = 1;\n```",
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(18.0),
        px(12.0),
    );

    assert_eq!(offset, px(129.0));
}

#[test]
fn markdown_prompt_anchor_caps_fenced_code_to_rendered_panel_window() {
    let visible_code = (0..12)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_code = (0..40)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let visible_source = format!("```rust\n{visible_code}\n```");
    let long_source = format!("```rust\n{long_code}\n```");
    let visible_geometry = transcript_anchor::test_support::prompt_geometry_from_markdown_no_wrap(
        1,
        visible_source.as_str(),
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(20.0),
        px(18.0),
    );
    let long_geometry = transcript_anchor::test_support::prompt_geometry_from_markdown_no_wrap(
        1,
        long_source.as_str(),
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(20.0),
        px(18.0),
    );

    assert_eq!(
        long_geometry.last_visual_line_top_offset,
        visible_geometry.last_visual_line_top_offset
    );
    assert_eq!(
        long_geometry.fragment_tail_offset,
        visible_geometry.fragment_tail_offset
    );
}

#[test]
fn markdown_prompt_anchor_treats_long_fenced_code_line_as_no_wrap_panel_line() {
    let short_source = "```rust\nlet value = 1;\n```";
    let long_line = format!("let value = \"{}\";", "x".repeat(400));
    let long_source = format!("```rust\n{long_line}\n```");
    let short_geometry = transcript_anchor::test_support::prompt_geometry_from_markdown_no_wrap(
        1,
        short_source,
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(20.0),
        px(18.0),
    );
    let long_geometry = transcript_anchor::test_support::prompt_geometry_from_markdown_no_wrap(
        1,
        long_source.as_str(),
        px(480.0),
        80,
        px(20.0),
        px(30.0),
        px(20.0),
        px(18.0),
    );

    assert_eq!(
        long_geometry.last_visual_line_top_offset,
        short_geometry.last_visual_line_top_offset
    );
    assert_eq!(
        long_geometry.fragment_tail_offset,
        short_geometry.fragment_tail_offset
    );
}

#[test]
fn markdown_prompt_anchor_accounts_for_wrapping_and_fallback_nodes() {
    let offset = transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_columns(
        1,
        "![diagram](artifact://diagram.png)\n\n<raw>",
        px(480.0),
        80,
        10,
        80,
        px(20.0),
        px(30.0),
        px(18.0),
        px(12.0),
    );

    assert_eq!(offset, px(101.0));
}

#[test]
fn markdown_prompt_anchor_wraps_root_list_body_after_marker_offset() {
    let source = "- 1234567890";
    let line_height = px(20.0);
    let unwrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(100.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );
    let wrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(99.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );

    assert_eq!(wrapped - unwrapped, line_height);
}

#[test]
fn markdown_prompt_anchor_wraps_nested_list_body_after_recursive_offsets() {
    let source = "- parent\n  - 1234567890";
    let line_height = px(20.0);
    let unwrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(120.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );
    let wrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(119.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );

    assert_eq!(wrapped - unwrapped, line_height);
}

#[test]
fn tall_prompt_placement_uses_latest_measured_visual_lines_with_runway() {
    let placement =
        transcript_anchor::test_support::prompt_viewport_placement_from_markdown_no_wrap(
            1,
            "one\ntwo\nthree\nfour\nfive\nsix",
            px(480.0),
            80,
            px(20.0),
            px(30.0),
            px(18.0),
            px(12.0),
            px(100.0),
            None,
        );

    assert_eq!(
        placement.anchor_kind,
        transcript_anchor::TranscriptPromptAnchorKind::FragmentTail
    );
    assert_eq!(placement.scroll_offset, px(93.0));
    assert_eq!(placement.virtual_runway, px(47.0));
}

#[test]
fn prompt_that_fits_only_without_runway_uses_tail_placement() {
    let placement =
        transcript_anchor::test_support::prompt_viewport_placement_from_markdown_no_wrap(
            1,
            "one\ntwo\nthree",
            px(480.0),
            80,
            px(20.0),
            px(30.0),
            px(18.0),
            px(12.0),
            px(115.0),
            None,
        );

    assert_eq!(
        placement.anchor_kind,
        transcript_anchor::TranscriptPromptAnchorKind::FragmentTail
    );
    assert!(placement.scroll_offset > placement.prompt.fragment_start_offset);
    assert!(placement.virtual_runway > px(0.0));
}

#[test]
fn small_viewport_prompt_runway_clamps_to_prompt_orientation() {
    let placement =
        transcript_anchor::test_support::prompt_viewport_placement_from_markdown_no_wrap(
            1,
            "one\ntwo\nthree\nfour\nfive\nsix",
            px(480.0),
            80,
            px(20.0),
            px(30.0),
            px(18.0),
            px(12.0),
            px(30.0),
            None,
        );

    assert_eq!(
        placement.anchor_kind,
        transcript_anchor::TranscriptPromptAnchorKind::FragmentTail
    );
    assert_eq!(placement.scroll_offset, px(116.0));
    assert_eq!(placement.virtual_runway, px(0.0));
}

#[test]
fn markdown_prompt_anchor_uses_widest_ordered_marker_for_body_width() {
    let source = "9. short\n10. 1234567890";
    let line_height = px(20.0);
    let unwrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(114.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );
    let wrapped =
        transcript_anchor::test_support::prompt_last_line_top_offset_from_markdown_char_width(
            1,
            source,
            px(113.0),
            80,
            px(8.0),
            line_height,
            px(30.0),
            px(18.0),
            px(12.0),
        );

    assert_eq!(wrapped - unwrapped, line_height);
}

#[test]
fn trailing_slack_stays_below_visible_transcript_height() {
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(240.0), None),
        px(239.0)
    );
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(0.5), None),
        px(0.0)
    );
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(-12.0), None),
        px(0.0)
    );
}

#[test]
fn trailing_slack_shrinks_as_content_below_anchor_grows() {
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(240.0), Some(px(80.0))),
        px(160.0)
    );
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(240.0), Some(px(239.5))),
        px(0.5)
    );
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(240.0), Some(px(240.0))),
        px(0.0)
    );
    assert_eq!(
        transcript_anchor::trailing_scroll_slack(px(240.0), Some(px(360.0))),
        px(0.0)
    );
}

#[test]
fn final_answer_start_geometry_targets_item_top() {
    let items = vec![
        transcript_anchor::TranscriptNarrativeItemGeometry::new(
            Some("commentary".to_string()),
            px(64.0),
            px(48.0),
        ),
        transcript_anchor::TranscriptNarrativeItemGeometry::new(
            Some("final".to_string()),
            px(128.0),
            px(160.0),
        ),
    ];

    let geometry = transcript_anchor::final_answer_start_geometry("final", &items).unwrap();

    assert_eq!(geometry.item_id, "final");
    assert_eq!(geometry.scroll_offset, px(128.0));
}

#[test]
fn final_answer_start_placement_adds_runway_while_final_content_is_short() {
    let items = vec![
        transcript_anchor::TranscriptNarrativeItemGeometry::new(
            Some("commentary".to_string()),
            px(64.0),
            px(48.0),
        ),
        transcript_anchor::TranscriptNarrativeItemGeometry::new(
            Some("final".to_string()),
            px(128.0),
            px(40.0),
        ),
    ];

    let placement =
        transcript_anchor::final_answer_start_placement("final", &items, px(240.0), None).unwrap();

    assert_eq!(placement.item_id, "final");
    assert_eq!(placement.scroll_offset, px(128.0));
    assert_eq!(placement.virtual_runway, px(200.0));
}

#[test]
fn final_answer_start_placement_drops_runway_after_content_fills_viewport() {
    let items = vec![transcript_anchor::TranscriptNarrativeItemGeometry::new(
        Some("final".to_string()),
        px(128.0),
        px(40.0),
    )];

    let placement = transcript_anchor::final_answer_start_placement(
        "final",
        &items,
        px(240.0),
        Some(px(420.0)),
    )
    .unwrap();

    assert_eq!(placement.scroll_offset, px(128.0));
    assert_eq!(placement.virtual_runway, px(0.0));
}

#[test]
fn commentary_follow_geometry_targets_item_bottom() {
    let items = vec![transcript_anchor::TranscriptNarrativeItemGeometry::new(
        Some("commentary".to_string()),
        px(64.0),
        px(180.0),
    )];

    let geometry =
        transcript_anchor::commentary_follow_geometry(Some("commentary"), &items, px(120.0))
            .unwrap();

    assert_eq!(geometry.item_id.as_deref(), Some("commentary"));
    assert_eq!(geometry.item_bottom_offset, px(244.0));
    assert_eq!(geometry.scroll_offset, px(124.0));
}

#[test]
fn commentary_follow_without_item_targets_latest_rendered_block() {
    let items = vec![
        transcript_anchor::TranscriptNarrativeItemGeometry::new(None, px(0.0), px(48.0)),
        transcript_anchor::TranscriptNarrativeItemGeometry::new(
            Some("commentary".to_string()),
            px(64.0),
            px(24.0),
        ),
        transcript_anchor::TranscriptNarrativeItemGeometry::new(None, px(100.0), px(52.0)),
    ];

    let geometry = transcript_anchor::commentary_follow_geometry(None, &items, px(80.0)).unwrap();

    assert_eq!(geometry.item_id, None);
    assert_eq!(geometry.item_bottom_offset, px(152.0));
    assert_eq!(geometry.scroll_offset, px(72.0));
}

#[test]
fn submit_anchor_does_not_add_a_synthetic_list_row() {
    assert_eq!(transcript_anchor::transcript_list_item_count(3), 3);
}

#[test]
fn submit_anchor_snapshot_uses_explicit_prompt_reread_action() {
    let anchor = transcript_anchor::TranscriptSubmitAnchor::new(
        2,
        Some("thread:main:turn:abc".to_string()),
        0,
        "submitted prompt".to_string(),
    );
    let snapshot = anchor.snapshot(transcript_anchor::TranscriptSubmitViewportAction::PromptReread);

    assert_eq!(snapshot.turn_index, 2);
    assert_eq!(
        snapshot.row_identity.as_deref(),
        Some("thread:main:turn:abc")
    );
    assert_eq!(snapshot.fragment_index, 0);
    assert_eq!(snapshot.user_input, "submitted prompt");
    assert_eq!(
        snapshot.viewport_action,
        transcript_anchor::TranscriptSubmitViewportAction::PromptReread
    );
}
