# Scope

Independent subscription Responses research; no production implementation was changed.

# Invalidated Assumption

The research helper assumed `response.completed.response.output` contained all completed items
needed for continuation. Direct HTTP responses on 2026-09-17 instead had empty terminal arrays
after completed tool, reasoning and message items had already appeared in `output_item.done`.
The original helper therefore misclassified a returned call as absent and stopped before the
next request. A terminal response is not necessarily a repeated full output snapshot.

# Correction And Evidence

Collect actual completed stream items by explicit output index and correlate added/done item IDs
and created/completed response identity. Reject missing/conflicting closure rather than guessing
or manufacturing context. A nonempty terminal array is checked for agreement separately.

Five offline helper checks passed and independent review accepted this bounded research correction.
The resulting direct function round trip succeeded using the actual call and matching result.
See [direct stream continuation](../memory/topic/responses-agent-runtime/direct-stream-continuation.md)
for exact request conditions, received field order, sizes and hashes.

# Remaining Limits

This correction does not prove general stream completeness, production memory bounds or recovery
after a lost terminal event. It informs the ordering, durable execution and context phases in
the root plan. The entire production rework remains held pending the CAS decision.
