use beryl_model::semantic_graph::SemanticNodeId;

#[path = "../src/shell/checklist_sidebar_visibility.rs"]
mod checklist_sidebar_visibility;

use checklist_sidebar_visibility::ChecklistSidebarVisibilityState;

#[test]
fn checklist_sidebar_visibility_starts_hidden_and_auto_shows_for_checklist_selection() {
    let checklist = node_id("release_checklist");
    let mut visibility = ChecklistSidebarVisibilityState::default();

    assert!(!visibility.visible());
    assert!(visibility.reconcile_selection(Some(&checklist)));
    assert!(visibility.visible());
}

#[test]
fn checklist_sidebar_visibility_can_hide_current_auto_selected_checklist() {
    let checklist = node_id("release_checklist");
    let mut visibility = ChecklistSidebarVisibilityState::default();

    visibility.reconcile_selection(Some(&checklist));

    assert!(visibility.toggle(Some(&checklist)));
    assert!(!visibility.visible());
    assert!(!visibility.reconcile_selection(Some(&checklist)));
    assert!(!visibility.visible());
}

#[test]
fn checklist_sidebar_visibility_auto_shows_after_selecting_a_different_checklist() {
    let hidden_checklist = node_id("hidden_checklist");
    let next_checklist = node_id("next_checklist");
    let mut visibility = ChecklistSidebarVisibilityState::default();

    visibility.reconcile_selection(Some(&hidden_checklist));
    visibility.toggle(Some(&hidden_checklist));

    assert!(visibility.reconcile_selection(Some(&next_checklist)));
    assert!(visibility.visible());
}

#[test]
fn checklist_sidebar_visibility_hides_auto_panel_when_selection_is_not_checklist() {
    let checklist = node_id("release_checklist");
    let mut visibility = ChecklistSidebarVisibilityState::default();

    visibility.reconcile_selection(Some(&checklist));

    assert!(visibility.reconcile_selection(None));
    assert!(!visibility.visible());
}

#[test]
fn checklist_sidebar_visibility_hides_auto_panel_when_selected_checklist_is_deleted() {
    let checklist = node_id("release_checklist");
    let mut visibility = ChecklistSidebarVisibilityState::default();

    visibility.reconcile_selection(Some(&checklist));

    assert!(visibility.reconcile_selection(None));
    assert!(!visibility.visible());
}

#[test]
fn checklist_sidebar_visibility_keeps_manual_panel_open_without_checklist_selection() {
    let mut visibility = ChecklistSidebarVisibilityState::default();

    assert!(visibility.toggle(None));
    assert!(visibility.visible());
    assert!(!visibility.reconcile_selection(None));
    assert!(visibility.visible());
}

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}
