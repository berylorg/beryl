#[path = "../src/shell/checklist_sidebar_panel_state.rs"]
mod checklist_sidebar_panel_state;

use beryl_model::semantic_graph::SemanticNodeId;
use checklist_sidebar_panel_state::{ChecklistSidebarPanelState, ChecklistSidebarProjectionSync};

#[test]
fn checklist_sidebar_panel_resets_scroll_when_selected_checklist_changes() {
    let mut state = ChecklistSidebarPanelState::default();
    let first = node_id("release");
    let second = node_id("followup");

    assert_eq!(
        state.sync_projection(Some(&first), 3),
        ChecklistSidebarProjectionSync::ResetScroll
    );
    assert_eq!(
        state.sync_projection(Some(&first), 3),
        ChecklistSidebarProjectionSync::Unchanged
    );
    assert_eq!(
        state.sync_projection(Some(&second), 3),
        ChecklistSidebarProjectionSync::ResetScroll
    );
    assert_eq!(
        state.sync_projection(None, 0),
        ChecklistSidebarProjectionSync::ResetScroll
    );
}

#[test]
fn checklist_sidebar_panel_clamps_scroll_when_row_count_changes() {
    let mut state = ChecklistSidebarPanelState::default();
    let checklist = node_id("release");

    assert_eq!(
        state.sync_projection(Some(&checklist), 8),
        ChecklistSidebarProjectionSync::ResetScroll
    );
    assert_eq!(
        state.sync_projection(Some(&checklist), 3),
        ChecklistSidebarProjectionSync::ClampScroll
    );
    assert_eq!(
        state.sync_projection(Some(&checklist), 3),
        ChecklistSidebarProjectionSync::Unchanged
    );
    assert_eq!(
        state.sync_projection(Some(&checklist), 10),
        ChecklistSidebarProjectionSync::ClampScroll
    );
}

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}
