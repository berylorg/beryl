use std::time::{Duration, Instant};

use beryl_model::semantic_graph::{SemanticNodeId, ThreadRefId};
use gpui::{Bounds, Pixels, Point};

use crate::member_thread_inventory::MemberThreadInventoryMemberKey;

pub(crate) const GRAPH_NODE_DELETE_HOLD_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Default)]
pub(crate) struct GraphNodeActionMenuState {
    open: Option<GraphNodeActionMenuOpen>,
    delete_hold: Option<GraphNodeDeleteHoldState>,
    delete_in_flight: Option<GraphNodeDeleteInFlightState>,
}

#[derive(Clone, Debug)]
pub(crate) struct GraphNodeActionMenuOpen {
    node_id: SemanticNodeId,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    view: GraphNodeActionMenuView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphNodeActionMenuView {
    Root,
    LinkThreads,
    MemberThreads(MemberThreadInventoryMemberKey),
    RebindThreads(ThreadRefId),
    RebindMemberThreads {
        thread_ref_id: ThreadRefId,
        member_key: MemberThreadInventoryMemberKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphNodeDeleteHoldSource {
    Pointer,
    Keyboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphNodeDeleteInFlightKind {
    Leaf,
    Subtree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphNodeDeleteHoldState {
    node_id: SemanticNodeId,
    source: GraphNodeDeleteHoldSource,
    started_at: Instant,
    duration: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphNodeDeleteInFlightState {
    node_id: SemanticNodeId,
    kind: GraphNodeDeleteInFlightKind,
}

pub(crate) type GraphThreadLinkMenuState = GraphNodeActionMenuState;
pub(crate) type GraphThreadLinkMenuOpen = GraphNodeActionMenuOpen;
pub(crate) type GraphThreadLinkMenuView = GraphNodeActionMenuView;

impl GraphNodeActionMenuState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn open_node(&mut self, node_id: SemanticNodeId, position: Point<Pixels>) {
        self.delete_hold = None;
        self.delete_in_flight = None;
        self.open = Some(GraphThreadLinkMenuOpen {
            node_id,
            position,
            bounds: None,
            view: GraphNodeActionMenuView::Root,
        });
    }

    pub(crate) fn open_thread_ref_rebind(
        &mut self,
        node_id: SemanticNodeId,
        thread_ref_id: ThreadRefId,
        position: Point<Pixels>,
    ) {
        self.delete_hold = None;
        self.delete_in_flight = None;
        self.open = Some(GraphThreadLinkMenuOpen {
            node_id,
            position,
            bounds: None,
            view: GraphNodeActionMenuView::RebindThreads(thread_ref_id),
        });
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
        self.delete_hold = None;
        self.delete_in_flight = None;
    }

    pub(crate) fn active(&self) -> Option<&GraphNodeActionMenuOpen> {
        self.open.as_ref()
    }

    pub(crate) fn set_member_threads_view(&mut self, member: MemberThreadInventoryMemberKey) {
        if self.delete_in_flight.is_some() {
            return;
        }

        if let Some(open) = self.open.as_mut() {
            self.delete_hold = None;
            open.view = GraphNodeActionMenuView::MemberThreads(member);
        }
    }

    pub(crate) fn set_rebind_member_threads_view(
        &mut self,
        thread_ref_id: ThreadRefId,
        member: MemberThreadInventoryMemberKey,
    ) {
        if self.delete_in_flight.is_some() {
            return;
        }

        if let Some(open) = self.open.as_mut() {
            self.delete_hold = None;
            open.view = GraphNodeActionMenuView::RebindMemberThreads {
                thread_ref_id,
                member_key: member,
            };
        }
    }

    pub(crate) fn set_link_threads_view(&mut self) {
        if self.delete_in_flight.is_some() {
            return;
        }

        if let Some(open) = self.open.as_mut() {
            self.delete_hold = None;
            open.view = GraphNodeActionMenuView::LinkThreads;
        }
    }

    pub(crate) fn set_rebind_threads_view(&mut self, thread_ref_id: ThreadRefId) {
        if self.delete_in_flight.is_some() {
            return;
        }

        if let Some(open) = self.open.as_mut() {
            self.delete_hold = None;
            open.view = GraphNodeActionMenuView::RebindThreads(thread_ref_id);
        }
    }

    pub(crate) fn set_root_view(&mut self) {
        if self.delete_in_flight.is_some() {
            return;
        }

        if let Some(open) = self.open.as_mut() {
            self.delete_hold = None;
            open.view = GraphNodeActionMenuView::Root;
        }
    }

    pub(crate) fn set_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(open) = self.open.as_mut() {
            open.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

    pub(crate) fn delete_hold_active(&self) -> bool {
        self.delete_hold.is_some()
    }

    pub(crate) fn leaf_delete_in_flight_for_target(&self, node_id: &SemanticNodeId) -> bool {
        self.delete_in_flight.as_ref().is_some_and(|in_flight| {
            in_flight.node_id == *node_id && in_flight.kind == GraphNodeDeleteInFlightKind::Leaf
        })
    }

    pub(crate) fn subtree_delete_in_flight_for_target(&self, node_id: &SemanticNodeId) -> bool {
        self.delete_in_flight.as_ref().is_some_and(|in_flight| {
            in_flight.node_id == *node_id && in_flight.kind == GraphNodeDeleteInFlightKind::Subtree
        })
    }

    pub(crate) fn delete_hold_progress_for_target(
        &self,
        node_id: &SemanticNodeId,
        now: Instant,
    ) -> Option<f32> {
        let hold = self.delete_hold.as_ref()?;
        (hold.node_id == *node_id).then(|| hold.progress(now))
    }

    pub(crate) fn begin_delete_hold(
        &mut self,
        node_id: SemanticNodeId,
        source: GraphNodeDeleteHoldSource,
        now: Instant,
    ) -> bool {
        let target_is_active_root = self.open.as_ref().is_some_and(|open| {
            open.node_id == node_id && open.view == GraphNodeActionMenuView::Root
        });
        if !target_is_active_root || self.delete_in_flight.is_some() {
            return false;
        }

        if self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.node_id == node_id && hold.source == source)
        {
            return false;
        }

        self.delete_hold = Some(GraphNodeDeleteHoldState {
            node_id,
            source,
            started_at: now,
            duration: GRAPH_NODE_DELETE_HOLD_DURATION,
        });
        true
    }

    pub(crate) fn cancel_delete_hold(&mut self) -> bool {
        self.delete_hold.take().is_some()
    }

    pub(crate) fn cancel_delete_hold_source(&mut self, source: GraphNodeDeleteHoldSource) -> bool {
        let matches_source = self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.source == source);
        if matches_source {
            self.delete_hold = None;
        }
        matches_source
    }

    pub(crate) fn cancel_delete_hold_for_stale_target(&mut self, target_exists: bool) -> bool {
        let should_cancel = self.delete_hold.is_some() && !target_exists;
        if should_cancel {
            self.delete_hold = None;
        }
        should_cancel
    }

    pub(crate) fn complete_delete_hold_if_ready(&mut self, now: Instant) -> Option<SemanticNodeId> {
        let ready = self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.is_complete(now));
        if ready {
            return self.delete_hold.take().map(|hold| hold.node_id);
        }
        None
    }

    pub(crate) fn mark_delete_in_flight(&mut self, node_id: SemanticNodeId) -> bool {
        self.mark_delete_in_flight_kind(node_id, GraphNodeDeleteInFlightKind::Subtree)
    }

    pub(crate) fn mark_leaf_delete_in_flight(&mut self, node_id: SemanticNodeId) -> bool {
        self.mark_delete_in_flight_kind(node_id, GraphNodeDeleteInFlightKind::Leaf)
    }

    fn mark_delete_in_flight_kind(
        &mut self,
        node_id: SemanticNodeId,
        kind: GraphNodeDeleteInFlightKind,
    ) -> bool {
        let target_is_active_root = self.open.as_ref().is_some_and(|open| {
            open.node_id == node_id && open.view == GraphNodeActionMenuView::Root
        });
        if !target_is_active_root {
            return false;
        }

        self.delete_hold = None;
        self.delete_in_flight = Some(GraphNodeDeleteInFlightState { node_id, kind });
        true
    }

    pub(crate) fn clear_delete_in_flight(&mut self) -> bool {
        self.delete_in_flight.take().is_some()
    }
}

impl GraphNodeActionMenuOpen {
    pub(crate) fn node_id(&self) -> &SemanticNodeId {
        &self.node_id
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }

    pub(crate) fn view(&self) -> &GraphNodeActionMenuView {
        &self.view
    }
}

impl GraphNodeDeleteHoldState {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn is_complete(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }
}
