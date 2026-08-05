# Canon Authoring Layout

This reference is normative. Read it in full before creating or restructuring canonical world documentation, changing canonical paths or document contracts, reviewing, validating, or reconciling structural conformance, or authoring and reorganizing temporal history authorities.

## Contents

- Canonical filesystem
- Document contracts
- Temporal history authoring

## Canonical Filesystem

Keep durable canonical world truth under `doc/world/` and organize it by authority layer before subject:

```text
doc/world/
  index.md
  pillars.md
  foundations/
    index.md
    physics/
      index.md
      <physical-fact-or-admission>.md
    advancements/
      index.md
      tiers.md
      catalog.md
      <advancement>.md
  domains/
    index.md
    <domain>/
      index.md
      <capability-or-rule>.md
  designs/
    index.md
    <design-kind>/
      index.md
      <design>.md
  history/
    index.md
    periods/
      index.md
      <period>/
        index.md
        <period-subject>.md
    entities/
      index.md
      <entity-kind>/
        index.md
        <entity>.md
    events/
      index.md
      <event>.md
```

Use `pillars.md` for construction requirements and capability targets. Use `foundations/physics/` for the physical baseline and explicit fictional admissions. Use `foundations/advancements/` for tier meanings, the approved advancement catalog, and focused advancement definitions. Use `domains/` for reusable derived rules and capabilities. Use `designs/` for complete configurations. Use `history/` for every named or deployed realization and its temporal state.

Let `foundations/physics/index.md` own the default that established reality applies except where a linked admission says otherwise. Do not create a general restatement of known physics or a separate baseline document; create focused physics files only for explicit admissions or world-specific physical facts that require their own authority.

Create an optional layer document such as `tiers.md` only when that concept exists, but never invent an alternative path for it. Omit unused registry branches instead of creating empty directories; when a category gains its first authority, use the canonical path. Add domain, design-kind, and entity-kind directories as needed without renaming the fixed authority layers.

Apply these filesystem rules:

- Give every canonical directory an `index.md` that owns scope and routes to focused authorities.
- Keep indexes compact. Repeat only discovery metadata such as titles, dates, and links; do not duplicate substantive explanations.
- Give each substantive claim one focused owner and link to it from consumers.
- Use lowercase kebab-case names. Do not create `misc`, `unsorted`, or other authority-ambiguous buckets.
- Keep canonical files free of authoring candidates, unresolved proposals, research notes, and archived alternatives. Store those outside `doc/world/` in project-declared noncanonical locations.
- Use one document per substantial authority, not one document per noun. Promote a subject to its own file when it recurs, owns several independent facts, has meaningful dependencies or a lifecycle, or will be changed independently.
- Keep a simple subject as a section in its owning document. Turn a complex subject into a directory only when focused child documents need distinct ownership; use its `index.md` to route those owners.

## Document Contracts

Give `doc/world/index.md` the sections `## Canon Boundary`, `## Authority Order`, `## Protected Authorities`, and `## Authorities`. Use it to establish the canonical root, dependency order, explicitly locked documents, and links to each layer without repeating world facts.

Give every other index the sections `## Scope` and `## Authorities`. Add temporal routing sections to history indexes as specified below. Move a domain-wide substantive rule into a focused document and link it rather than hiding it in an index.

Give `pillars.md` the sections `## Construction Requirements`, `## Capability Targets`, and `## Boundaries`. Keep desired outcomes distinct from physical mechanisms and already-achieved capabilities.

Begin every substantive document with one top-level title followed by an `## Authority` section that states:

- **Owns:** one precise sentence naming the document's exclusive authority.
- **Depends on:** direct links to every upstream authority needed to interpret or validate its claims, or `None` for a physical root.

Then use sections appropriate to the authority type:

- For physics or fictional admissions, cover the rule, scope, observable consequences, conservation behavior, and exact boundaries of the admission.
- For advancements and domain capabilities, cover the granted capability, operating envelope, inputs and costs, limits and non-capabilities, and failure conditions.
- For integrated designs, cover purpose, dependencies, configuration, relevant budgets and constraints, operating envelope, and failure or degraded states.
- For historical periods, entities, and events, include `## Temporal Scope` and the applicable identity, lifecycle, state, relationships, preconditions, event, and consequences sections.

State unknowns as unknown without inventing values. Distinguish an in-world uncertainty from an unresolved authoring proposal; only the former belongs in canonical world truth.

## Temporal History Authoring

Use `doc/world/history/index.md` as the mandatory temporal entry point. Keep it useful for discovery by recording:

- the total documented temporal range;
- periods in order with their boundaries and links;
- major transitions when needed for orientation;
- links to period, entity, and event registries.

Give the history index the sections `## Scope`, `## Temporal Coverage`, `## Periods`, optional `## Major Transitions`, and `## Authorities`. Give each period index `## Scope`, `## Temporal Scope`, `## Active Entities`, `## Local Timeline`, and `## Authorities`.

Do not make a second exhaustive chronology by default. Let each period `index.md` provide its local timeline and links to active entities, deployed designs, available capabilities, and relevant events. Split the root history index only when necessary, while preserving it as the stable compact entry point.

Apply temporal scope according to the subject:

- A period document states its exact interval or explicit relative bounds and the spatial or civilizational scope to which its claims apply.
- An event document states its date or bounded interval and links to the containing period. Keep a minor event inside a period document; create a focused event document only when it has substantial causes, consequences, or reuse.
- An entity document states its lifecycle rather than pretending to cover one period. Link its formation, transformations, phases, and dissolution to the owning periods or events. Keep a minor entity inside its period authority until it merits independent ownership.
- A period index lists active entities and links to focused relationships for discovery. If a period has only a few cross-sectional facts, its index may own them under an `## Authority` section; otherwise focused period-subject documents own that state. Entity documents continue to own identity and continuity, and event documents own dated changes.
- A capability or design document defines what is possible independent of a particular deployment date. History owns when, where, and by whom that capability or design was discovered, available, built, deployed, altered, or lost.

If a claim changes during a period, qualify it with a narrower interval or divide the period. Do not imply that a condition held throughout an interval merely because it appears in that period's document.
