#[allow(dead_code)]
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

#[allow(dead_code)]
#[path = "../src/shell/transcript_anchor.rs"]
mod transcript_anchor;

use beryl_app::{AppearanceRoleSettings, AppearanceSettings};
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

    assert_eq!(offset, px(109.0));
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
fn submit_anchor_does_not_add_a_synthetic_list_row() {
    assert_eq!(transcript_anchor::transcript_list_item_count(3), 3);
}

#[test]
fn passive_loaded_history_anchor_keeps_forced_viewport_disabled() {
    let anchor =
        transcript_anchor::TranscriptSubmitAnchor::passive(1, 0, "loaded prompt".to_string());

    let snapshot = anchor.snapshot();
    assert_eq!(snapshot.turn_index, 1);
    assert_eq!(snapshot.fragment_index, 0);
    assert_eq!(snapshot.user_input, "loaded prompt");
    assert!(!snapshot.force_viewport);
}

#[test]
fn manual_scroll_release_only_stops_forced_viewport_once() {
    let mut anchor = Some(transcript_anchor::TranscriptSubmitAnchor::new(
        2,
        0,
        "submitted prompt".to_string(),
    ));

    assert!(anchor.as_ref().unwrap().snapshot().force_viewport);
    assert!(transcript_anchor::release_forced_submit_anchor(&mut anchor));
    assert!(anchor.is_some());
    assert!(!anchor.as_ref().unwrap().snapshot().force_viewport);
    assert!(!transcript_anchor::release_forced_submit_anchor(
        &mut anchor
    ));
}
