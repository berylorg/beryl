pub(crate) const GRAPH_MUTATION_FAILURE_NOTICE_TITLE: &str = "Graph update failed";
pub(crate) const GRAPH_NODE_ACTION_BUSY_REASON: &str =
    "Wait for the current graph operation to finish.";
pub(crate) const GRAPH_NODE_ACTION_STALE_REASON: &str =
    "The selected semantic node is no longer available.";
pub(crate) const GRAPH_NODE_LEAF_DELETE_BUSY_REASON: &str = GRAPH_NODE_ACTION_BUSY_REASON;
pub(crate) const GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON: &str = "This semantic node has hard children. Use Delete Recursively to remove it and its descendants.";
pub(crate) const GRAPH_NODE_LEAF_DELETE_STALE_REASON: &str = GRAPH_NODE_ACTION_STALE_REASON;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphNodeLeafDeleteAvailability {
    Enabled,
    Disabled(&'static str),
}

pub(crate) fn graph_mutation_failure_notice(error: impl Into<String>) -> (&'static str, String) {
    (GRAPH_MUTATION_FAILURE_NOTICE_TITLE, error.into())
}

pub(crate) fn graph_node_delete_blocked_by_graph_work(
    graph_mutation_in_flight: bool,
    graph_thread_start_in_flight: bool,
) -> bool {
    graph_mutation_in_flight || graph_thread_start_in_flight
}

pub(crate) fn graph_node_action_keyboard_activation_key(key: &str) -> bool {
    matches!(key, "enter" | "space" | " ")
}

pub(crate) fn graph_node_leaf_delete_availability(
    target_exists: bool,
    has_hard_children: bool,
    graph_mutation_in_flight: bool,
    graph_thread_start_in_flight: bool,
) -> GraphNodeLeafDeleteAvailability {
    if graph_node_delete_blocked_by_graph_work(
        graph_mutation_in_flight,
        graph_thread_start_in_flight,
    ) {
        return GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_BUSY_REASON);
    }

    if !target_exists {
        return GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_STALE_REASON);
    }

    if has_hard_children {
        return GraphNodeLeafDeleteAvailability::Disabled(GRAPH_NODE_LEAF_DELETE_NON_LEAF_REASON);
    }

    GraphNodeLeafDeleteAvailability::Enabled
}

pub(crate) fn graph_node_recursive_delete_disabled_reason(
    target_exists: bool,
    graph_mutation_in_flight: bool,
    graph_thread_start_in_flight: bool,
    subtree_delete_in_flight: bool,
) -> Option<&'static str> {
    if subtree_delete_in_flight {
        return None;
    }

    if graph_node_delete_blocked_by_graph_work(
        graph_mutation_in_flight,
        graph_thread_start_in_flight,
    ) {
        return Some(GRAPH_NODE_ACTION_BUSY_REASON);
    }

    if !target_exists {
        return Some(GRAPH_NODE_ACTION_STALE_REASON);
    }

    None
}

pub(crate) fn semantic_node_summary_tooltip_allowed(graph_node_action_menu_open: bool) -> bool {
    !graph_node_action_menu_open
}
