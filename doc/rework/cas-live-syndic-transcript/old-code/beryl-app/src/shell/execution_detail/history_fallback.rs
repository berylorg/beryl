use beryl_backend::{ThreadItem, TurnInfo, TurnItemsView};

const OVERSIZED_TURN_FALLBACK_ITEM_TYPE: &str = "beryl.oversizedTurnFallback";

pub(super) fn is_oversized_turn_fallback_marker(turn: &TurnInfo) -> bool {
    turn.items_view == TurnItemsView::Summary
        && turn.items.iter().any(is_oversized_turn_fallback_item)
}

fn is_oversized_turn_fallback_item(item: &ThreadItem) -> bool {
    matches!(
        item,
        ThreadItem::Generic(generic)
            if generic.item_type == OVERSIZED_TURN_FALLBACK_ITEM_TYPE
    )
}
