const BLOCK_MARKDOWN_SOURCE: &str =
    include_str!("../src/shell/render/transcript/block_markdown.rs");
const ANCHOR_MARKDOWN_LAYOUT_SOURCE: &str =
    include_str!("../src/shell/transcript_anchor/markdown_layout.rs");
const LIST_LAYOUT_SOURCE: &str = include_str!("../src/shell/transcript_markdown/list_layout.rs");

#[test]
fn transcript_markdown_lists_use_leading_margin_and_tight_marker_layout() {
    assert!(LIST_LAYOUT_SOURCE.contains("MARKDOWN_LIST_LEADING_MARGIN_M: f32 = 1.5"));
    assert!(LIST_LAYOUT_SOURCE.contains("MARKDOWN_LIST_MARKER_BODY_GAP_M: f32 = 0.5"));
    assert!(LIST_LAYOUT_SOURCE.contains("MARKDOWN_LIST_UNORDERED_MARKER_WIDTH_M: f32 = 0.5"));
    assert!(
        LIST_LAYOUT_SOURCE.contains("MARKDOWN_LIST_ORDERED_MARKER_CHARACTER_WIDTH_M: f32 = 0.75")
    );
    assert!(
        BLOCK_MARKDOWN_SOURCE
            .contains(".pl(conversation_m_advance * MARKDOWN_LIST_LEADING_MARGIN_M)")
    );
    assert!(BLOCK_MARKDOWN_SOURCE.contains(".w(marker_width)"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains("marker_align_end"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains("marker = marker.justify_end()"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains("markdown_list_marker_width_m"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains("list_marker_char_counts_vary"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains(".text_size(px(theme.list_marker.font_size))"));
    assert!(BLOCK_MARKDOWN_SOURCE.contains(".font_family(theme.list_marker.font_family.clone())"));
    assert!(!BLOCK_MARKDOWN_SOURCE.contains("list_depth"));
    assert!(!BLOCK_MARKDOWN_SOURCE.contains("const LIST_MARKER_WIDTH: f32 = 32.0;"));
    assert!(!BLOCK_MARKDOWN_SOURCE.contains("LIST_LEADING_MARGIN: f32 = 16.0"));
}

#[test]
fn transcript_anchor_markdown_list_measurement_matches_renderer_offsets() {
    assert!(ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("MARKDOWN_LIST_LEADING_MARGIN_M"));
    assert!(ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("MARKDOWN_LIST_MARKER_BODY_GAP_M"));
    assert!(
        ANCHOR_MARKDOWN_LAYOUT_SOURCE
            .contains("list_item_body_offset(list, conversation_m_advance)")
    );
    assert!(
        ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("list_marker_width(list, conversation_m_advance)")
    );
    assert!(ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("markdown_list_marker_width_m"));
    assert!(!ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("list_depth"));
    assert!(!ANCHOR_MARKDOWN_LAYOUT_SOURCE.contains("MARKDOWN_LIST_MARKER_WIDTH"));
}
