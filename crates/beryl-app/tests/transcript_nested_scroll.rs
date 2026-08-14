#[path = "../src/shell/render/transcript/nested_scroll.rs"]
mod nested_scroll;

use nested_scroll::TranscriptNestedScrollOwnership;

#[test]
fn transcript_owns_vertical_wheel_by_default() {
    let ownership = TranscriptNestedScrollOwnership::default();

    assert_eq!(ownership.selected_panel_id(), None);
    assert!(!ownership.panel_owns_vertical_wheel("panel-a"));
}

#[test]
fn selecting_panel_grants_nested_vertical_wheel_ownership() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    assert!(ownership.select_panel("panel-a"));

    assert_eq!(ownership.selected_panel_id(), Some("panel-a"));
    assert!(ownership.panel_owns_vertical_wheel("panel-a"));
    assert!(!ownership.panel_owns_vertical_wheel("panel-b"));
}

#[test]
fn selecting_another_panel_replaces_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    ownership.select_panel("panel-a");
    assert!(ownership.select_panel("panel-b"));

    assert_eq!(ownership.selected_panel_id(), Some("panel-b"));
    assert!(!ownership.panel_owns_vertical_wheel("panel-a"));
    assert!(ownership.panel_owns_vertical_wheel("panel-b"));
}

#[test]
fn selecting_current_panel_is_not_a_state_change() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    assert!(ownership.select_panel("panel-a"));
    assert!(!ownership.select_panel("panel-a"));

    assert_eq!(ownership.selected_panel_id(), Some("panel-a"));
}

#[test]
fn clicking_transcript_clears_nested_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    ownership.select_panel("panel-a");
    assert!(ownership.clear_to_transcript());

    assert_eq!(ownership.selected_panel_id(), None);
    assert!(!ownership.panel_owns_vertical_wheel("panel-a"));
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

    ownership.select_panel("panel-a");
    assert!(!ownership.record_scrollbar_activity("panel-b"));

    assert_eq!(ownership.selected_panel_id(), Some("panel-a"));
}

#[test]
fn escape_does_not_clear_nested_selection() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    ownership.select_panel("panel-a");
    assert!(!ownership.handle_escape());

    assert_eq!(ownership.selected_panel_id(), Some("panel-a"));
}

#[test]
fn visible_selected_panel_is_retained() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    ownership.select_panel("panel-b");
    assert!(!ownership.retain_visible_panel_ids(["panel-a", "panel-b", "panel-c"]));

    assert_eq!(ownership.selected_panel_id(), Some("panel-b"));
}

#[test]
fn removed_selected_panel_is_cleared() {
    let mut ownership = TranscriptNestedScrollOwnership::default();

    ownership.select_panel("panel-b");
    assert!(ownership.retain_visible_panel_ids(["panel-a", "panel-c"]));

    assert_eq!(ownership.selected_panel_id(), None);
}
