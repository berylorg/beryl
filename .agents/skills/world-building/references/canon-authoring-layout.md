# Canon Authoring Layout

This reference is normative. Read it in full before creating or restructuring canonical world documentation, changing canonical paths or document contracts, reviewing, validating, or reconciling structural conformance, or authoring and reorganizing temporal history authorities.

## Contents

- Canonical filesystem
- Document contracts
- Admission evidence
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
      <design>/
        design.md
        evidence/
          evidence.md
          <supporting-evidence>.md
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

The two design shapes are alternatives. Use `<design>.md` for a flat owner and `<design>/design.md` for an evidence-bearing design package; never keep both forms for the same owner. An index links to whichever canonical owner form is live. Converting a flat owner to a package is an atomic path change with its indexes and consumers, not a compatibility migration.

Let `foundations/physics/index.md` own the default that established reality applies except where a linked admission says otherwise. Do not create a general restatement of known physics or a separate baseline document; create focused physics files only for explicit admissions or world-specific physical facts that require their own authority.

Create an optional layer document such as `tiers.md` only when that concept exists, but never invent an alternative path for it. Omit unused registry branches instead of creating empty directories; when a category gains its first authority, use the canonical path. Add domain, design-kind, and entity-kind directories as needed without renaming the fixed authority layers.

Apply these filesystem rules:

- Give every canonical routing directory an `index.md` that owns scope and routes to focused authorities. An evidence-bearing subject package and its `evidence/` directory use their contracts below instead of adding redundant indexes.
- Keep indexes compact. Repeat only discovery metadata such as titles, dates, and links; do not duplicate substantive explanations.
- Give each substantive claim one focused owner and link to it from consumers.
- Use lowercase kebab-case names. Do not create `misc`, `unsorted`, or other authority-ambiguous buckets.
- Keep canonical-owner files free of authoring candidates, unresolved proposals, research notes, and archived alternatives. Store rough ideas only under `doc/inbox/idea/` and design intents only under `doc/inbox/intent/`. Admission evidence may contain verification reasoning, research links, calculations, and rejected stronger interpretations under the exact evidence contract below, but never exploratory authoring candidates.
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

## Admission Evidence

Treat `evidence/` inside an evidence-bearing canonical-owner package as authoritative admission evidence, not as an additional canonical owner and not as a rough research or proposal area. The package's main canonical document owns what is true in-world. Evidence owns why those claims were admitted and must remain in alignment with the owner and the owner revision it evaluates.

For an integrated design package, use this fixed shape:

```text
doc/world/designs/<design-kind>/<design-name>/
  design.md
  evidence/
    evidence.md
    <supporting-evidence>.md
```

Use `design.md` as the canonical entry point and focused owner. Use `evidence/evidence.md` as the primary admission record. Add focused supporting evidence files such as calculations, source analysis, or generated-result interpretation only when their independent size or reuse justifies them; do not create empty placeholders.

The primary admission record identifies the canonical owner and evaluated revision, admitted scope, atomic claims checked, upstream authorities, approval basis, evidence and research used, material assumptions and calculations, conflicts and downstream effects examined, rejected stronger interpretations, unresolved limits, and final verification result. Link reusable external research from `doc/memory/` rather than copying it.

Every world rule, hard limit, capability, configuration, or historical fact needed to understand or consume the admission belongs in the canonical owner. Evidence may derive, test, and justify those statements but must never be their only source. Canon consumers link to the canonical owner rather than to its proof unless they specifically consume the verification record.

Bind admission evidence to the owner revision it evaluates. After a material owner change, preserve the earlier record as accurate for its former revision through version history, and refresh the live evidence before treating it as support for the changed claim. Reuse sufficient prior research and qualification; perform new verification only where coverage is missing, stale, contradictory, or addressed a different claim.

Do not improvise package filenames for physics, advancements, domain capabilities, or temporal authorities merely by analogy with integrated designs. Add their exact package contracts here before converting those owner types.

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
