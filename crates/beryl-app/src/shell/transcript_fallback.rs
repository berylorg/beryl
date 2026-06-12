use std::collections::BTreeMap;

use beryl_backend::{GenericThreadItem, ThreadItem, TurnInfo, TurnItemsView};

const OVERSIZED_TURN_FALLBACK_ITEM_TYPE: &str = "beryl.oversizedTurnFallback";

pub(crate) fn oversized_turn_fallback_marker(mut turn: TurnInfo) -> TurnInfo {
    let marker_id = oversized_turn_fallback_item_id(turn.id.as_str());
    turn.items_view = TurnItemsView::Summary;
    turn.items = vec![ThreadItem::Generic(GenericThreadItem {
        id: marker_id,
        item_type: OVERSIZED_TURN_FALLBACK_ITEM_TYPE.to_string(),
        tool: None,
        server: None,
        namespace: None,
        mcp_app_resource_uri: None,
        status: None,
        model: None,
        reasoning_effort: None,
        receiver_thread_ids: Vec::new(),
        agents_states: BTreeMap::new(),
        agent_nickname: None,
    })];
    turn.error = None;
    turn
}

pub(crate) fn is_oversized_turn_fallback_marker(turn: &TurnInfo) -> bool {
    turn.items_view == TurnItemsView::Summary
        && turn.items.iter().any(is_oversized_turn_fallback_item)
}

pub(crate) fn is_oversized_turn_fallback_item(item: &ThreadItem) -> bool {
    matches!(
        item,
        ThreadItem::Generic(generic)
            if generic.item_type == OVERSIZED_TURN_FALLBACK_ITEM_TYPE
    )
}

fn oversized_turn_fallback_item_id(turn_id: &str) -> String {
    format!("beryl:oversized-turn-fallback:{turn_id}")
}
