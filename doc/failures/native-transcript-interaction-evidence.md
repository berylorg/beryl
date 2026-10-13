# Native Transcript Interaction Evidence

Historical replacement qualification on 2026-10-13 exposed a recurring fixture assumption:
retained transcript state was treated as proof that a native interaction was ready. That assumption
produced false failures for geometry, subscriber delivery and menu activation, and initially hid
the real resource-provenance defect. It did not require changing product behavior.

Virtualized rows may retain measured geometry outside the viewport. Native fixtures must scroll
the actual outer surface, acquire current geometry again, and verify that the content hit point
is inside the viewport. Bypassing clipping or reordering production hit testing to admit an
offscreen point is invalid. Likewise, recovery assertions must inspect the canonical authored
child and narrative source rather than require a legacy text representation.

GPUI queues `Context::emit` effects and flushes them when the outer `App::update` finishes.
An action and its subscriber-delivery assertion inside that same update cannot prove delivery.
Separate the native action from the subsequent observation, including preview dismissal and
return-focus checks. Anchored menus also retain their actual focused row: disabled Copy remains
focused initially, and Enter correctly does nothing. Navigate to the enabled Edit row before
asserting its activation instead of changing menu semantics to satisfy the fixture.

Current geometry and a callback still do not prove command authority. Code menu admission
correctly rejected a provider record whose authenticated resource payload had not been copied
into its source provenance. The correction carries that same resource identity into both
projection and view records while retaining every existing ownership and currentness check.

Native Code recovery/range routing passed in `run-046`; the complete grouped image workflow
passed in `run-050`, including Code/Table/marker origins, cross-marker selection, primary pointer
and keyboard actions, preview dismissal, focus return, actual Edit/import, dimming and Escape
cancellation. These lessons inform the remaining transcript workflows in [the plan](../plan.md)
and the [transcript contract](../features/conversation-threads/design.md); they are verification
guidance, not a replacement for their authority.
