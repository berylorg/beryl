# Scope

Exact terminal-turn historical repair against pinned CAS 0.146.0.

# Invalidated Assumption

A last-turn fence plus `thread/turns/list` with `limit=1`, descending order and `itemsView=full`
is enough to recover the complete ordered public item view with exact original identities.

# Decisive Evidence

Exact source commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b` reconstructs legacy history after
applying a persistence filter that drops operational lifecycle events and most completed public
items. The raw-response handler does not reconstruct those missing operations. Message identities
are synthesized, and inactive unfinished turns can be relabeled interrupted. `Full` labels the
resulting items without proving their completeness. See the
[pinned investigation](../memory/github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/terminal-turn-repair-history-surface.md#exact-processor-and-persistence-findings).

Beryl does not select or authenticate paginated history mode for repair targets. The pinned
paginated path has different persistence and internal hydration behavior; it does not justify
accepting legacy or unknown-mode responses. Matching ids and a terminal status cannot detect an
operational item omitted before response serialization.

# Course Correction

The existing [repair contract](../systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair)
already requires the adapter to remain unavailable when exact source cannot prove its required
semantics. Phase 464 records that negative result; phase 465 cannot proceed under current
prerequisites. No request, cursor traversal, item-history fallback, partial publication or
weakened identity check was implemented.

Independent source and artifact review accepted the negative outcome on 2026-09-16. Scoped
source checks, documentation link/diff checks and Markdown index reconciliation passed. This
evidence phase ran no live backend probe or Cargo tests and changed no production code.

# Remaining Decision

The recommended next step is to retain unavailable repair and specify the remaining recovery
components around the already required explicit-incomplete outcome. Enabling repair instead needs
separate Operator-approved authority for admitted backend/history modes and a new complete
persistence, identity, finalization and lineage investigation. Merely requesting paginated mode
is not established as sufficient. No change to target authority is made by this failure record.
