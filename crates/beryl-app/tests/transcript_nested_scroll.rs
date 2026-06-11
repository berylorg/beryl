#[path = "../src/shell/transcript_markdown.rs"]
pub(crate) mod transcript_markdown;

mod shell {
    pub(crate) use crate::transcript_markdown;
}

#[path = "../src/shell/render/transcript/nested_scroll.rs"]
mod nested_scroll;

use nested_scroll::TranscriptNestedScrollOwnership;
use transcript_markdown::TranscriptCodePanelIdentity;

fn panel(code_path: &str) -> TranscriptCodePanelIdentity {
    TranscriptCodePanelIdentity::new("row-a", "item:answer", code_path)
}

fn row_panel(row_identity: &str, code_path: &str) -> TranscriptCodePanelIdentity {
    TranscriptCodePanelIdentity::new(row_identity, "item:answer", code_path)
}

#[test]
fn transcript_owns_vertical_wheel_by_default() {
    let ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");

    assert_eq!(ownership.selected_panel_id(), None);
    assert!(!ownership.panel_owns_vertical_wheel(&panel_a));
}

#[test]
fn selecting_panel_grants_nested_vertical_wheel_ownership() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");
    let panel_b = panel("b1");

    assert!(ownership.select_panel(panel_a.clone()));

    assert_eq!(ownership.selected_panel_id(), Some(panel_a.as_str()));
    assert!(ownership.panel_owns_vertical_wheel(&panel_a));
    assert!(!ownership.panel_owns_vertical_wheel(&panel_b));
}

#[test]
fn selecting_another_panel_replaces_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");
    let panel_b = panel("b1");

    ownership.select_panel(panel_a.clone());
    assert!(ownership.select_panel(panel_b.clone()));

    assert_eq!(ownership.selected_panel_id(), Some(panel_b.as_str()));
    assert!(!ownership.panel_owns_vertical_wheel(&panel_a));
    assert!(ownership.panel_owns_vertical_wheel(&panel_b));
}

#[test]
fn same_local_panel_in_different_rows_is_distinct() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let row_a_panel = row_panel("row-a", "b0");
    let row_b_panel = row_panel("row-b", "b0");

    assert_ne!(row_a_panel, row_b_panel);
    ownership.select_panel(row_a_panel.clone());

    assert!(ownership.panel_owns_vertical_wheel(&row_a_panel));
    assert!(!ownership.panel_owns_vertical_wheel(&row_b_panel));
}

#[test]
fn selecting_current_panel_is_not_a_state_change() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");

    assert!(ownership.select_panel(panel_a.clone()));
    assert!(!ownership.select_panel(panel_a.clone()));

    assert_eq!(ownership.selected_panel_id(), Some(panel_a.as_str()));
}

#[test]
fn clicking_transcript_clears_nested_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");

    ownership.select_panel(panel_a.clone());
    assert!(ownership.clear_to_transcript());

    assert_eq!(ownership.selected_panel_id(), None);
    assert!(!ownership.panel_owns_vertical_wheel(&panel_a));
}

#[test]
fn clearing_when_transcript_already_owns_wheel_is_not_a_state_change() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    assert!(!ownership.clear_to_transcript());

    assert_eq!(ownership.selected_panel_id(), None);
}

#[test]
fn scrollbar_activity_does_not_change_selected_panel() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");
    let panel_b = panel("b1");

    ownership.select_panel(panel_a.clone());
    assert!(!ownership.record_scrollbar_activity(&panel_b));

    assert_eq!(ownership.selected_panel_id(), Some(panel_a.as_str()));
}

#[test]
fn escape_does_not_clear_nested_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");

    ownership.select_panel(panel_a.clone());
    assert!(!ownership.handle_escape());

    assert_eq!(ownership.selected_panel_id(), Some(panel_a.as_str()));
}

#[test]
fn visible_selected_panel_is_retained() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");
    let panel_b = panel("b1");
    let panel_c = panel("b2");

    ownership.select_panel(panel_b.clone());
    assert!(!ownership.retain_visible_panel_identities([&panel_a, &panel_b, &panel_c,]));

    assert_eq!(ownership.selected_panel_id(), Some(panel_b.as_str()));
}

#[test]
fn removed_selected_panel_is_cleared() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let panel_a = panel("b0");
    let panel_b = panel("b1");
    let panel_c = panel("b2");

    ownership.select_panel(panel_b);
    assert!(ownership.retain_visible_panel_identities([&panel_a, &panel_c]));

    assert_eq!(ownership.selected_panel_id(), None);
}

#[test]
fn retaining_visible_panel_identities_clears_released_row_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();
    let released_row_panel = row_panel("row-a", "b0");
    let visible_row_panel = row_panel("row-b", "b0");

    ownership.select_panel(released_row_panel);
    assert!(ownership.retain_visible_panel_identities([&visible_row_panel]));

    assert_eq!(ownership.selected_panel_id(), None);
}
