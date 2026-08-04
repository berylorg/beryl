# Beryl Thread Metadata Split Authority

## Scope

Checkpoint 3 catalog-summary and same-home recovery planning across `beryl-state`,
`syndic-storage`, and `beryl-app`.

## Invalidated Approach

The target placed execution binding, generated title, automatic branch-discussion archive state,
activity, and token usage in one Beryl-owned thread-metadata record while Syndic owned the thread,
lineage, draft, history, and CAS projection lifecycle.

## Evidence And Failure

The split left no Syndic title-bearing bounded summary even though the catalog required a Syndic-
derived fallback. It also made the admitted-work scheduler recover execution from a Beryl copy that
Syndic could not validate against its own thread or CAS binding records. Activity duplicated the
existing Syndic history summary, and durable usage and archive updates could diverge from the exact
thread and binding lifecycle they described.

The bucket was therefore not application metadata. It duplicated intrinsic properties of a thread
owned by Syndic and made a coherent catalog and recovery fence impossible without dual authority.

## Course Correction

Syndic owns immutable thread execution, revisioned title/archive attributes, exact token-usage
observations, history activity, and one rebuildable resolved catalog summary. Beryl owns only
runtime/root availability, window/session relationships, durable host jobs, and rebuildable catalog
copies. The old `beryl-thread-metadata` domain is removed directly; no alias, replacement-shaped
relationship record, dual read, decoder, or migration adapter is retained.

## Affected Authority

- `doc/design.md` and the conversation-thread and status-line feature contracts.
- Beryl-home, Syndic-history, and CAS-live system contracts.
- `beryl-state`, `syndic-storage`, `beryl-model`, `beryl-backend`, and `beryl-app` package contracts.
- `doc/plan.md` and `doc/rework/beryl-home/REWORK.md`.
