use beryl_model::semantic_graph::SemanticNodeId;
use gpui::{point, px};

#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[allow(dead_code)]
#[path = "../src/shell/graph_link_menu_state.rs"]
mod graph_link_menu_state;

#[path = "../src/shell/graph_node_action_policy.rs"]
mod graph_node_action_policy;

#[test]
fn graph_node_delete_policy_allows_delete_only_when_graph_work_is_idle() {
    assert!(!graph_node_action_policy::graph_node_delete_blocked_by_graph_work(false, false));
    assert!(graph_node_action_policy::graph_node_delete_blocked_by_graph_work(true, false));
    assert!(graph_node_action_policy::graph_node_delete_blocked_by_graph_work(false, true));
    assert!(graph_node_action_policy::graph_node_delete_blocked_by_graph_work(true, true));
}

#[test]
fn graph_mutation_failure_notice_preserves_error_detail() {
    let (title, detail) = graph_node_action_policy::graph_mutation_failure_notice("worker failed");

    assert_eq!(title, "Graph update failed");
    assert_eq!(detail, "worker failed");
}

#[test]
fn graph_node_leaf_delete_availability_requires_existing_leaf_and_idle_graph_work() {
    use graph_node_action_policy::{
        GRAPH_NODE_LEAF_DELETE_BUSY_REASON, GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON,
        GRAPH_NODE_LEAF_DELETE_STALE_REASON, GraphNodeLeafDeleteAvailability,
    };

    assert_eq!(
        graph_node_action_policy::graph_node_leaf_delete_availability(true, false, false, false),
        GraphNodeLeafDeleteAvailability::Enabled
    );
    assert_eq!(
        graph_node_action_policy::graph_node_leaf_delete_availability(true, true, false, false),
        GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_leaf_delete_availability(false, false, false, false),
        GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_STALE_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_leaf_delete_availability(true, false, true, false),
        GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_BUSY_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_leaf_delete_availability(true, false, false, true),
        GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_BUSY_REASON)
    );
}

#[test]
fn graph_node_recursive_delete_disabled_reason_tracks_stale_busy_and_in_flight_states() {
    use graph_node_action_policy::{GRAPH_NODE_ACTION_BUSY_REASON, GRAPH_NODE_ACTION_STALE_REASON};

    assert_eq!(
        graph_node_action_policy::graph_node_recursive_delete_disabled_reason(
            true, false, false, false
        ),
        None
    );
    assert_eq!(
        graph_node_action_policy::graph_node_recursive_delete_disabled_reason(
            false, false, false, false
        ),
        Some(GRAPH_NODE_ACTION_STALE_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_recursive_delete_disabled_reason(
            true, true, false, false
        ),
        Some(GRAPH_NODE_ACTION_BUSY_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_recursive_delete_disabled_reason(
            true, false, true, false
        ),
        Some(GRAPH_NODE_ACTION_BUSY_REASON)
    );
    assert_eq!(
        graph_node_action_policy::graph_node_recursive_delete_disabled_reason(
            true, true, false, true
        ),
        None
    );
}

#[test]
fn graph_node_action_keyboard_activation_accepts_enter_and_space() {
    assert!(graph_node_action_policy::graph_node_action_keyboard_activation_key("enter"));
    assert!(graph_node_action_policy::graph_node_action_keyboard_activation_key("space"));
    assert!(graph_node_action_policy::graph_node_action_keyboard_activation_key(" "));
    assert!(!graph_node_action_policy::graph_node_action_keyboard_activation_key("escape"));
}

#[test]
fn semantic_node_summary_tooltips_are_suppressed_while_graph_node_menu_is_open() {
    let mut menu = graph_link_menu_state::GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();

    assert!(graph_node_action_policy::semantic_node_summary_tooltip_allowed(menu.is_open()));

    menu.open_node(node_id, point(px(120.0), px(80.0)));
    assert!(!graph_node_action_policy::semantic_node_summary_tooltip_allowed(menu.is_open()));

    menu.close();
    assert!(graph_node_action_policy::semantic_node_summary_tooltip_allowed(menu.is_open()));
}
