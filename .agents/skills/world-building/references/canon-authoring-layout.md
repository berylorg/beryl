# Canon Authoring Layout

This reference is normative. Read it in full before creating or restructuring canonical world documentation, changing canonical paths or document contracts, reviewing, validating, or reconciling structural conformance, or authoring and reorganizing temporal history authorities.

## Contents

- Canonical filesystem
- Document contracts
- Admission evidence
- Temporal history authoring

## Canonical Filesystem

Keep durable canonical world truth under `doc/world/` and organize it by authority layer before subject. The tree below is a closed allowlist for every directory and file under `doc/world/`. Literal names are fixed. Each angle-bracket placeholder permits one lowercase kebab-case name of the described type at exactly that depth; it does not permit extra nesting or additional sibling forms.

```text
doc/world/
  pillars.md
  datasets/
    <dataset>/
      dataset.md
      data/
        <data-file>.json
        <data-file>.csv
      evidence/
        evidence.md
        <supporting-evidence>.md
  foundations/
    physics/
      <physical-fact-or-admission>/
        physics.md
        evidence/
          evidence.md
          <supporting-evidence>.md
    advancements/
      <advancement>/
        advancement.md
        evidence/
          evidence.md
          <supporting-evidence>.md
  domains/
    <domain>/
      <capability-or-rule>.md
  designs/
    <design-kind>/
      <design>.md
      <design>/
        design.md
        evidence/
          evidence.md
          <supporting-evidence>.md
  history/
    periods/
      <period>.md
      <period>/
        period.md
        <period-subject>.md
    entities/
      <entity-kind>/
        <entity>.md
    events/
      <event>.md
```

Use `pillars.md` for construction requirements and capability targets. Use `datasets/` for canonical structured records whose scale or machine-oriented schema cannot reasonably be represented as prose. Use `foundations/physics/` for the physical baseline and explicit fictional admissions. Use `foundations/advancements/` only for focused advancement packages: its immediate packages are the complete official advancement set. Use `domains/` for reusable derived rules and capabilities. Use `designs/` for complete configurations. Use `history/` for every named or deployed realization and its temporal state.

Physics and focused advancement owners are always evidence-bearing packages. Use `physics.md` or `advancement.md` as the package's canonical entry point and never keep a flat owner for the same subject.

The two design shapes are alternatives. Use `<design>.md` for a flat owner and `<design>/design.md` for an evidence-bearing design package; never keep both forms for the same owner. Converting a flat owner to a package is an atomic path change with its consumers, not a compatibility migration.

Established reality applies except where a focused admission says otherwise. Do not create a general restatement of known physics or a separate baseline document; create focused physics packages only for explicit admissions or world-specific physical facts that require their own authority.

Omit unused branches instead of creating empty directories; when a category gains its first authority, use the canonical path. Add domain, design-kind, and entity-kind directories as needed without renaming the fixed authority layers.

Apply these filesystem rules:

- Create, move, or rename a directory or file only when its complete path matches an explicit form in the canonical tree or a narrower exact form stated elsewhere in this reference.
- Treat every unlisted path as prohibited regardless of filename or apparent usefulness. Do not infer helper, navigation, registry, summary, convenience, media, sidecar, or alternate package paths from convention, neighboring files, or task context.
- If a task requires an unlisted path form, stop and request explicit Operator approval to amend this reference before changing the filesystem.
- Treat an existing unlisted path as a layout violation to report, not as precedent or implicit permission. Do not remove it unless the task authorizes removal.
- Under `foundations/advancements/`, permit only an immediate `<advancement>/` package containing `advancement.md`, `evidence/evidence.md`, and justified supporting evidence. Do not create a catalog, tier registry, or another advancement authority form.
- The immediate advancement packages are the complete official advancement set. Adding, removing, or renaming a package, or materially changing its grant, requires explicit Operator approval.
- Give each substantive claim one focused owner and link to it from consumers.
- Use lowercase kebab-case names. Do not create `misc`, `unsorted`, or other authority-ambiguous buckets.
- Keep canonical-owner files free of authoring candidates, unresolved proposals, research notes, and archived alternatives. Store rough ideas only under `doc/inbox/idea/` and design intents only under `doc/inbox/intent/`. Admission evidence may contain verification reasoning, research links, calculations, and rejected stronger interpretations under the exact evidence contract below, but never exploratory authoring candidates.
- Use one document per substantial authority, not one document per noun. Promote a subject to its own file when it recurs, owns several independent facts, has meaningful dependencies or a lifecycle, or will be changed independently.
- Keep a simple subject as a section in its owning document. Use a directory-bearing subject package only where the canonical tree explicitly shows that form; otherwise do not introduce another directory level.

## Structured Dataset Packages

Use structured dataset packages when the authoritative material is inherently tabular, graph-shaped, spatial, or otherwise machine-oriented and representing its records in Markdown would be impractical. The matching authority-layer forms are:

```text
doc/inbox/idea/datasets/<dataset>/
  dataset.md
  data/
    <data-file>.json
    <data-file>.csv

doc/inbox/intent/datasets/<dataset>/
  dataset.md
  data/
    <data-file>.json
    <data-file>.csv

doc/world/datasets/<dataset>/
  dataset.md
  data/
    <data-file>.json
    <data-file>.csv
  evidence/
    evidence.md
    <supporting-evidence>.md
```

Each package contains at least one JSON or CSV payload. Permit only lowercase kebab-case dataset, payload, and supporting-evidence names, no extra nesting beneath `data/`, and no loose structured files outside a dataset package. JSON and CSV are the admitted structured authority formats; another format requires an explicit layout amendment.

`dataset.md` is the human-readable authority entry point for the package. Begin it with one top-level title and `## Authority` using the normal `Owns` and `Depends on` fields. It then identifies every payload path and role, schema and version, stable identity and relationship semantics, units and coordinate frames where applicable, coverage and completeness boundary, null and uncertainty meaning, and the rules needed to interpret the records without inventing prose equivalents. Payload files carry canonical records only at the canon layer and only within the meaning and limits declared by `dataset.md`; neither a filename nor a generated-data location grants authority.

Promote or reject one complete package. Canon may depend only on canon; intent datasets may depend on canon and intent; idea datasets may depend on canon, intent, and idea. Promotion removes the prior-layer package after the destination package and any required admission evidence are complete; do not retain parallel authority-layer copies.

## Document Contracts

Put every substantive scope or authority statement in the focused document that owns it, and link that owner directly from consumers. Do not create a document type or package contract absent from the canonical tree and this reference.

Give `pillars.md` the sections `## Construction Requirements`, `## Capability Targets`, and `## Boundaries`. Keep desired outcomes distinct from physical mechanisms and already-achieved capabilities.

Begin every canonical owner document with one top-level title followed by an `## Authority` section that states:

- **Owns:** one precise sentence naming the document's exclusive authority.
- **Depends on:** direct links to every upstream authority needed to interpret or validate its claims, or `None` for a physical root.

Then use sections appropriate to the authority type:

- For physics or fictional admissions, cover the rule, scope, observable consequences, conservation behavior, and exact boundaries of the admission.
- For an advancement, state its exact approved grant; absence of a stated grant is not permission. Also cover its operating envelope, inputs and costs, limits and non-capabilities, and failure conditions. For a domain capability, cover the granted capability, operating envelope, inputs and costs, limits and non-capabilities, and failure conditions.
- For integrated designs, cover purpose, dependencies, configuration, relevant budgets and constraints, operating envelope, and failure or degraded states.
- For historical periods, entities, and events, include `## Temporal Scope` and the applicable identity, lifecycle, state, relationships, preconditions, event, and consequences sections.

State unknowns as unknown without inventing values. Distinguish an in-world uncertainty from an unresolved authoring proposal; only the former belongs in canonical world truth.

## Admission Evidence

Treat `evidence/` inside an evidence-bearing canonical-owner package as authoritative admission evidence, not as an additional canonical owner and not as a rough research or proposal area. The package's main canonical document owns what is true in-world. Evidence owns why those claims were admitted and must remain in alignment with the owner and the owner revision it evaluates.

For a physics package, use this fixed shape:

```text
doc/world/foundations/physics/<physical-fact-or-admission>/
  physics.md
  evidence/
    evidence.md
    <supporting-evidence>.md
```

Use `physics.md` as the canonical entry point and `evidence/evidence.md` as the primary admission record. For an explicit fictional admission, the Operator's explicit decision is sufficient approval evidence. Record the exact approved departure, its scope, and its boundaries; do not invent empirical evidence for approved fictional physics. Add supporting evidence only when real-world physics, calculations, or prior analysis materially clarifies the boundary.

For a focused advancement package, use this fixed shape:

```text
doc/world/foundations/advancements/<advancement>/
  advancement.md
  evidence/
    evidence.md
    <supporting-evidence>.md
```

Use `advancement.md` as the canonical entry point and `evidence/evidence.md` as the primary admission record. The owner states its exact approved grant. Advancement evidence cites the explicit Operator approval, lists every canonical physical authority that permits the grant, and explains why they are compatible. Physical possibility never enlarges approval or establishes an unstated capability; absence of a grant is not permission.

For an integrated design package, use this fixed shape:

```text
doc/world/designs/<design-kind>/<design-name>/
  design.md
  evidence/
    evidence.md
    <supporting-evidence>.md
```

Use `design.md` as the canonical entry point and focused owner. Use `evidence/evidence.md` as the primary admission record. In every package type, add focused supporting evidence files such as calculations, source analysis, or generated-result interpretation only when their independent size or reuse justifies them; do not create empty placeholders.

For a canonical structured dataset, use the fixed `doc/world/datasets/<dataset>/` shape defined above. Use `dataset.md` as the canonical entry point, the JSON or CSV files beneath `data/` as its structured record payloads, and `evidence/evidence.md` as the primary admission record. Evidence establishes why the declared payloads and interpretation contract were admitted; it does not replace either the human-readable owner or its records.

The primary admission record identifies the canonical owner and evaluated revision, admitted scope, atomic claims checked, upstream authorities, approval basis, applicable evidence and research, material assumptions or calculations, conflicts and downstream effects examined, rejected stronger interpretations, unresolved limits, and final verification result. For a structured dataset, it also identifies every admitted payload and schema version and demonstrates that the evidence covers the declared identity, relationship, unit, coordinate, coverage, completeness, null, and uncertainty contracts. Keep it proportional: omit inapplicable fields rather than manufacturing analysis. Link reusable external research from `doc/memory/` rather than copying it.

Every world rule, hard limit, capability, configuration, or historical fact needed to understand or consume the admission belongs in the canonical owner. Evidence may derive, test, and justify those statements but must never be their only source. Canon consumers link to the canonical owner rather than to its proof unless they specifically consume the verification record.

Bind admission evidence to the owner revision it evaluates. After a material owner change, preserve the earlier record as accurate for its former revision through version history, and refresh the live evidence before treating it as support for the changed claim. Reuse sufficient prior research and qualification; perform new verification only where coverage is missing, stale, contradictory, or addressed a different claim.

Do not improvise package filenames for domain capabilities or temporal authorities merely by analogy with the package types defined above. Add their exact package contracts here before converting those owner types.

## Temporal History Authoring

Locate applicable period authorities under `doc/world/history/periods/` by their declared temporal scopes. Use `<period>.md` for a flat period owner or `<period>/period.md` when focused period-subject documents need distinct ownership; never keep both forms for the same period.

Give each period authority the sections `## Temporal Scope`, `## Active Entities`, `## Local Timeline`, and any additional sections needed for its owned facts. Keep the local timeline and direct links to active entities, deployed designs, available capabilities, and relevant events in that period authority. Do not make a second exhaustive chronology by default.

Apply temporal scope according to the subject:

- A period document states its exact interval or explicit relative bounds and the spatial or civilizational scope to which its claims apply.
- An event document states its date or bounded interval and links to the containing period. Keep a minor event inside a period document; create a focused event document only when it has substantial causes, consequences, or reuse.
- An entity document states its lifecycle rather than pretending to cover one period. Link its formation, transformations, phases, and dissolution to the owning periods or events. Keep a minor entity inside its period authority until it merits independent ownership.
- A period authority lists active entities and links to focused relationships for discovery. If a period has only a few cross-sectional facts, it may own them directly; otherwise focused period-subject documents own that state. Entity documents continue to own identity and continuity, and event documents own dated changes.
- A capability or design document defines what is possible independent of a particular deployment date. History owns when, where, and by whom that capability or design was discovered, available, built, deployed, altered, or lost.

If a claim changes during a period, qualify it with a narrower interval or divide the period. Do not imply that a condition held throughout an interval merely because it appears in that period's document.
