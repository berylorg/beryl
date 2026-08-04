# Provider Structured Depth Is Not The Complete Validator Stack

## Context

The backend admits provider structured values through depth 128. Syndic replays the same typed
controls with a fixed-capacity frame stack so arbitrary provider size never creates proportional
resident state.

## Invalidated Shape

The first destination validator sized its complete frame stack as `128 * 2 + 1`, assuming every
structured level required at most one container and one entry frame plus one root.

That bound ignored fixed schema frames enclosing a structured value. An MCP result-content value,
for example, is nested below the provider result object, its content list, and the current list
element before the first caller-controlled structured frame is pushed. A backend-valid structured
value at exact depth 128 could therefore exhaust the destination stack and fail only because of its
field location.

Lowering the shared structured-depth contract to 127, special-casing MCP, or allowing the vector to
grow without a static bound would all violate authority.

## Replacement

Keep the semantic structured-depth limit at 128. Size the fixed validator frame storage from two
separate terms:

- the maximum schema-owned enclosing-frame prefix across every admitted field location;
- the exact worst-case frames per caller-controlled structured level, including the current entry
  or element.

The destination must prove exact depth 128 succeeds at the worst enclosing location, depth 129
fails as semantic structured-depth excess, and restart/replay preserves the same frontier. The
complete frame capacity remains a compile-time constant and must not become an input-sized
allocation.

## Related Identity Correction

Destination exactness also includes bounded semantic identity strings. Item IDs and the closed
collaboration/subagent thread-id fields are not generic text merely because their bytes arrive as
text fragments. Syndic must incrementally enforce the same `CasItemId` or `CasThreadId` contract as
the backend and persist that fixed validation state across restart.
