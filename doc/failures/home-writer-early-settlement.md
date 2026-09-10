# Home Writer Quiescence Before Early-Return Custody Destruction

## Scope And Invalidated Assumption

Adding a quiescence notification to the home writer's local guard does not by itself prove that
all operation-owned state has settled. In `execute_serialized`, function parameters outlived the
local writer guard on early returns. An unconsumed command closure and reconciliation reservation
could therefore survive cancellation or health refusal past the advertised quiescence cut.

## Evidence And Correction

Independent review identified the Rust destruction order and the reservation's retained slot and
byte charge. The real failed-home regression
`early_health_refusal_drops_command_custody_before_publishing_quiescence` observed the boundary from
the rejected contributor's destructor. It failed because the writer had already published idle.

Move command and reservation custody into locals inside both writer and writer-reentry guards
before any post-acquisition early return. Sample terminal cancellation before entering the reentry
guard. Destruction then precedes quiescence and notification on success and rejection alike.
The regression and the focused mutation-observation suite pass with this order, and independent
semantic review accepts the correction. Preserve this drop order when refactoring the writer.

The owning contract is [atomic commands](../../crates/beryl-home-store/doc/design-atomic-commands.md).
