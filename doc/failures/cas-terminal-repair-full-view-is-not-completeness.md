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
semantics. Phase464 recorded that negative result; Phase465 was blocked by those prerequisites.
No request, cursor traversal, item-history fallback, partial publication or
weakened identity check was implemented.

Independent source and artifact review accepted the negative outcome on 2026-09-16. Scoped
source checks, documentation link/diff checks and Markdown index reconciliation passed. This
evidence phase ran no live backend probe or Cargo tests and changed no production code.

# Selected Outcome

The Operator approved retaining unavailable repair and proceeding with explicit-incomplete recovery
on 2026-09-16. On 2026-10-18 the Operator reaffirmed that insufficient information should resolve
the turn as incomplete: the human can issue a new instruction asking the AI to inspect and recover
among potentially inconsistent data. The [feature contract](../features/backend-runtime-recovery/design.md#live-capture-gap-recovery)
and [system contract](../systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair)
own that policy. The blocked Phase465 adapter and dependent repair-only components are retired
from the current work queue; absent historical repair is no longer a pending implementation gate.

Existing ordinary and candidate startup recovery already implement exact incomplete convergence,
retaining target/provenance and request disposition, finishing canonical/transcript publication
and releasing the repair gate. Human-requested recovery is new work; it does not erase the
incomplete marker, establish missing canonical evidence or automatically replay prior effects.

Any future historical repair needs separate Operator-approved authority for admitted backend/history
modes and a new complete persistence, identity, finalization and lineage investigation. Merely
requesting paginated mode is not established as sufficient.

# Verification Of The Selected Outcome

The 2026-10-18 policy clarification changes documentation and work scope only. Existing
`StartupAccess::converge_repair` commits exact incomplete resolution and completes history
publication for ordinary and candidate recovery. Its owning `normal_terminal::repair_startup`
cases passed in normal run479: ordinary available/consumed disposition and repeated recovery
(2.814 seconds), consumed candidate recovery before publication (1.601 seconds), and candidate
reconciliation/reopen/retry (6.250 seconds). Their shared assertions require an idle gate,
`AuthorityLost`, retained original target and terminal witness, current transcript publication,
an incomplete summary and completed item finalization. The accepted source and configuration are
unchanged, so those scoped results are reused; no new test execution is claimed for this change.

Independent semantic review accepted the selected policy on2026-10-18, including ordinary live
source-loss convergence, exact mutation admission and finalization-before-gate-release ordering.
No production change is required. Phase784 closes the policy reconciliation, and the retired
Phase465 adapter no longer leaves a blocked phase in the active plan.

The accepted continuation separates bounded repair-gate values, canonical gate enforcement,
durable incomplete finalization and app startup integration. Outage capture and fresh-service
recovery remain prerequisites of branch services and complete graph publication. Independent
review of phase 466 accepted that ordering after the tracker distinguished proven source
unavailability from absent implementation and made repair-specific media gates conditional.
Documentation link/diff checks and Markdown reconciliation passed; no runtime implementation is
claimed by this planning correction.
