use beryl_model::semantic_graph::ThreadRefId;
use beryl_model::{semantic_graph::SemanticNodeId, workspace::WorkspaceMemberId};
use gpui::{Bounds, point, px, size};
use graph_link_menu_state::{
    GRAPH_NODE_DELETE_HOLD_DURATION, GraphNodeDeleteHoldSource, GraphThreadLinkMenuState,
    GraphThreadLinkMenuView,
};
use member_thread_inventory::MemberThreadInventoryMemberKey;
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[path = "../src/shell/graph_link_menu_state.rs"]
mod graph_link_menu_state;

#[test]
fn graph_thread_link_menu_tracks_open_node_view_and_dismissal_bounds() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let position = point(px(120.0), px(80.0));

    assert!(!menu.is_open());

    menu.open_node(node_id.clone(), position);
    let open = menu.active().unwrap();
    assert_eq!(open.node_id(), &node_id);
    assert_eq!(open.position(), position);
    assert_eq!(open.view(), &GraphThreadLinkMenuView::Root);
    assert!(menu.should_dismiss_for_mouse_down(point(px(121.0), px(81.0))));

    menu.set_bounds(Some(Bounds::new(
        point(px(100.0), px(70.0)),
        size(px(240.0), px(180.0)),
    )));
    assert!(!menu.should_dismiss_for_mouse_down(point(px(120.0), px(90.0))));
    assert!(menu.should_dismiss_for_mouse_down(point(px(40.0), px(90.0))));
}

#[test]
fn graph_thread_link_menu_switches_between_member_threads_and_root_views() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let member_key =
        MemberThreadInventoryMemberKey::Explicit(WorkspaceMemberId::new("member_2").unwrap());

    menu.open_node(node_id, point(px(120.0), px(80.0)));
    menu.set_link_threads_view();
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::LinkThreads
    );

    menu.set_member_threads_view(member_key.clone());
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::MemberThreads(member_key)
    );

    menu.set_root_view();
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::Root
    );

    menu.close();
    assert!(!menu.is_open());
    assert!(!menu.should_dismiss_for_mouse_down(point(px(40.0), px(90.0))));
}

#[test]
fn graph_thread_link_menu_tracks_explicit_rebind_views() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let thread_ref_id = ThreadRefId::new("release_thread_ref").unwrap();
    let member_key =
        MemberThreadInventoryMemberKey::Explicit(WorkspaceMemberId::new("member_2").unwrap());

    menu.open_thread_ref_rebind(
        node_id.clone(),
        thread_ref_id.clone(),
        point(px(120.0), px(80.0)),
    );
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::RebindThreads(thread_ref_id.clone())
    );

    menu.set_rebind_member_threads_view(thread_ref_id.clone(), member_key.clone());
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::RebindMemberThreads {
            thread_ref_id: thread_ref_id.clone(),
            member_key
        }
    );

    menu.set_rebind_threads_view(thread_ref_id.clone());
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::RebindThreads(thread_ref_id)
    );
    assert_eq!(menu.active().unwrap().node_id(), &node_id);
}

#[test]
fn graph_node_delete_hold_tracks_progress_and_cancels_pointer_release() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let now = Instant::now();

    menu.open_node(node_id.clone(), point(px(120.0), px(80.0)));
    assert!(menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));
    assert!(
        menu.delete_hold_progress_for_target(&node_id, now)
            .is_some()
    );

    let progress = menu
        .delete_hold_progress_for_target(&node_id, now + Duration::from_millis(1500))
        .unwrap();
    assert!((0.45..0.55).contains(&progress));

    assert!(menu.cancel_delete_hold_source(GraphNodeDeleteHoldSource::Pointer));
    assert!(!menu.delete_hold_active());
}

#[test]
fn graph_node_delete_hold_completes_once_after_required_duration() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let now = Instant::now();

    menu.open_node(node_id.clone(), point(px(120.0), px(80.0)));
    assert!(menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Keyboard, now,));
    assert_eq!(
        menu.complete_delete_hold_if_ready(now + GRAPH_NODE_DELETE_HOLD_DURATION),
        Some(node_id)
    );
    assert_eq!(
        menu.complete_delete_hold_if_ready(now + GRAPH_NODE_DELETE_HOLD_DURATION),
        None
    );
}

#[test]
fn graph_node_delete_in_flight_state_blocks_duplicate_holds_and_view_changes() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let member_key =
        MemberThreadInventoryMemberKey::Explicit(WorkspaceMemberId::new("member_2").unwrap());
    let now = Instant::now();

    menu.open_node(node_id.clone(), point(px(120.0), px(80.0)));
    assert!(menu.mark_delete_in_flight(node_id.clone()));
    assert!(menu.subtree_delete_in_flight_for_target(&node_id));
    assert!(!menu.leaf_delete_in_flight_for_target(&node_id));

    menu.set_link_threads_view();
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::Root
    );
    menu.set_member_threads_view(member_key);
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::Root
    );
    assert!(!menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));

    assert!(menu.clear_delete_in_flight());
    assert!(!menu.subtree_delete_in_flight_for_target(&node_id));
    assert!(menu.begin_delete_hold(node_id, GraphNodeDeleteHoldSource::Pointer, now));
}

#[test]
fn graph_node_leaf_delete_in_flight_state_blocks_holds_and_view_changes() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let member_key =
        MemberThreadInventoryMemberKey::Explicit(WorkspaceMemberId::new("member_2").unwrap());
    let now = Instant::now();

    menu.open_node(node_id.clone(), point(px(120.0), px(80.0)));
    assert!(menu.mark_leaf_delete_in_flight(node_id.clone()));
    assert!(menu.leaf_delete_in_flight_for_target(&node_id));
    assert!(!menu.subtree_delete_in_flight_for_target(&node_id));

    menu.set_link_threads_view();
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::Root
    );
    menu.set_member_threads_view(member_key);
    assert_eq!(
        menu.active().unwrap().view(),
        &GraphThreadLinkMenuView::Root
    );
    assert!(!menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));

    assert!(menu.clear_delete_in_flight());
    assert!(!menu.leaf_delete_in_flight_for_target(&node_id));
    assert!(menu.begin_delete_hold(node_id, GraphNodeDeleteHoldSource::Pointer, now));
}

#[test]
fn graph_node_delete_hold_cancels_on_menu_close_view_change_and_stale_target() {
    let mut menu = GraphThreadLinkMenuState::default();
    let node_id = SemanticNodeId::new("release_node").unwrap();
    let now = Instant::now();

    menu.open_node(node_id.clone(), point(px(120.0), px(80.0)));
    assert!(menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));
    menu.set_link_threads_view();
    assert!(!menu.delete_hold_active());
    assert!(!menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now,));

    menu.set_root_view();
    assert!(menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));
    assert!(menu.cancel_delete_hold());
    assert!(!menu.delete_hold_active());

    assert!(menu.begin_delete_hold(node_id.clone(), GraphNodeDeleteHoldSource::Pointer, now));
    assert!(menu.cancel_delete_hold_for_stale_target(false));
    assert!(!menu.delete_hold_active());

    assert!(menu.begin_delete_hold(node_id, GraphNodeDeleteHoldSource::Pointer, now));
    menu.close();
    assert!(!menu.delete_hold_active());
}
