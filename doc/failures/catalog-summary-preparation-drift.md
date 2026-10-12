# Catalog Summary Preparation Drift

Actual mounted submission in the recovered first-conversation fixture consumed its original
draft, adopted the successor editor, admitted CAS capture history, loaded indexed Code, scrolled
to line 900, preserved ten frames and disposed the editor. Process shutdown then reported
`Consumed(Catalog(Source(Projection(SyndicRead(ConcurrentChange ...)))))`, naming
`thread-catalog summary preparation`. Runs `6050503f-8a16-419d-8d32-6a4780eca450` and
`9efac86c-2fb1-4259-836d-464b5bb85a24` reproduced the same failure after passing those checks.

The canonical summary owner checks its Syndic revision around ordinary preparation. Concurrent
history writes correctly refuse an unstable summary. The app projection adapter previously
propagated that refusal as a fatal source error even though the catalog coordinator already
handles its own `ConcurrentPreparation` outcome by remaining unready for a coalesced refresh.
The [published Catalog contract](../../crates/beryl-app/doc/design-catalog-and-composer.md#published-frozen-catalog-reader)
requires drift/conflict to remain unready with bounded refresh custody.

Normalize the canonical concurrent-change refusal only when the sampled Home revision has
advanced. Preserve the original error otherwise, and preserve all structural/read failures.
Reuse the existing coordinator wake and refresh ownership; do not retry inside the adapter,
publish the rejected summary or invent another refresh queue.

The first-conversation fixture separately needs the actual mounted submission and successor
adoption before capture admission. The [recovery source lesson](indexed-code-recovery-source.md)
owns the rejected external-draft submission and early-flush arrangements.


The correction passed the actual first-conversation Code case through process shutdown in
`a85beef8-6062-4d5d-881e-ec7fe1845dc2` (68 selected service/catalog/panel cases passed,
67.481 seconds), plus both catalog projection tests in
`40104c29-4e07-4f3e-aa65-6d14ec8f5ce0` (2.315 seconds). Native first-conversation and real
CAS execution are separate remaining qualifications.
