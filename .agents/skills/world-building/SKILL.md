---
name: world-building
description: Maintain authority, physical plausibility, causality, temporal state, and dependency integrity in physically grounded speculative-fiction and science-fiction world documentation. Use when creating, reviewing, reconciling, reorganizing, or changing world pillars, physics and fictional admissions, approved advancements, biological or technological capabilities, environments, societies, economies, infrastructure, integrated designs, canonical world-document layouts, historical periods, entities, factions, places, populations, resources, chronology, or events.
---

# World Building

## Core Rule

Build every world claim from its upstream authorities. Treat the world as a directed dependency graph: physics constrains advancements, advancements constrain derived capabilities, capabilities constrain integrated designs, and designs constrain what can exist in history.

Do not preserve a downstream idea by silently weakening, bypassing, or inventing an exception to an upstream rule. Surface the conflict and resolve the controlling authority first.

Use established reality as the physical baseline. Replace or extend it only within the exact scope of an explicitly approved fictional admission. Treat an invented advancement as a hypothetical engineering achievement, not an additional physics admission; require it to remain compatible with the controlling physical model.

## Document Brevity

Make every document governed by this skill as short as possible while preserving enough context, precision, dependencies, boundaries, and unresolved state for a capable reasoning model to recover the intended meaning with high confidence. Remove repetition, exhaustive restatement, and prose that does not materially improve that recovery; never achieve brevity by dropping a controlling constraint or creating ambiguity.

## World Construction Constraints

Treat approved world pillars and capability targets as construction requirements rather than diegetic mechanisms or achieved facts. Use them to judge whether the world being built serves its intended shape, but never let them silently override established world truth.

When a pillar conflicts with physics or another foundational rule, identify the conflict and require an explicit decision: revise the design, revise the pillar, or approve a bounded fictional admission.

## Placement In Project Authority

Treat canonical world docs as a project-declared bounded authority for in-world truth. Follow the
project's instructions and declared documentation hierarchy for process, protected changes, and
conflict resolution. Canonical world docs own what is true in-world; consumer docs may describe how
that truth is presented or implemented but must not redefine it.

When a product or implementation requirement conflicts with world canon, resolve the controlling
world authority first or obtain an explicit decision to change it. Route non-canon behavior to its
normal project authority and keep world docs limited to what is true in-world.

## World-Truth Hierarchy

Apply these layers from upstream to downstream:

1. **Physics and fictional admissions:** Establish the physical model by using known reality by default and documenting every approved departure precisely.
2. **Approved advancements:** Establish extraordinary discoveries or engineering achievements and their exact granted scope. Do not infer unstated outcomes from an enabling advancement.
3. **Domain capabilities:** Define bounded outcomes within focused authorities such as biology, technology, ecology, society, economy, infrastructure, or institutions.
4. **Integrated designs:** Compose capabilities into vehicles, habitats, installations, networks, organisms, or other complete systems and close their interacting budgets.
5. **History:** Instantiate named entities, places, populations, factions, resources, deployed systems, periods, and events as time-indexed world reality without enlarging the capabilities they use.

Treat this hierarchy as a graph, not a single inheritance chain. A derived claim may depend on several upstream domains at once.

Keep each hard rule, numeric limit, or bounded capability with the focused authority that establishes it. Do not create a separate invariant layer or elevate a construction target into a natural law. Distinguish semantic authority from change control: locking a document protects it from unauthorized edits but does not turn its contents into physics.

## Canon Authoring Layout

Before creating, editing, or restructuring canonical world documentation or structured dataset packages, changing canonical paths or document contracts, reviewing, validating, or reconciling structural conformance, or authoring and reorganizing temporal history authorities, read [Canon Authoring Layout](references/canon-authoring-layout.md) in full and follow it as normative. It owns the exact filesystem tree, placement rules, document contracts, structured dataset packages, and temporal authoring schemas. Do not improvise alternate structures.

Treat that layout as a closed allowlist. Under `doc/world/`, create, move, or rename only a directory or file matching an explicit path form permitted by the layout. Absence from the layout is prohibition; never infer helper, navigation, registry, summary, convenience, or alternate package paths. If a task requires an unlisted path form, stop and request explicit Operator approval to amend the layout before changing the filesystem. Treat an existing unlisted path as a violation to report, not as precedent or implicit permission; do not remove it unless the task authorizes removal.

## Temporal History

Treat history as the complete time-indexed realization of the world, including the latest documented period. Do not create a separate present-state authority and do not use unqualified terms such as `current` or `present` when the intended date or period can be named. Locate applicable period authorities under `doc/world/history/periods/` by their declared temporal scopes.

A capability or design authority defines what is possible; history owns when, where, and by whom it was discovered, available, built, deployed, altered, or lost. Qualify changing claims with a narrower interval or divide the period rather than implying that a condition held throughout it.

## Authority and Canon

- Follow explicit operator decisions and project-declared locked authorities.
- Locate the focused owner of each claim. Within its scope, the focused owner controls over broader or dependent documents.
- Keep each substantive claim in one owner and link to it from consumers instead of duplicating it.
- Treat canonical authority separately from decision or commitment state. A noncanonical, unapproved, or archived label alone does not establish that material is speculative.
- Use research as evidence. It may justify changing authority but never changes world truth by itself.
- Treat an absent mechanism, missing number, or unresolved outcome as unresolved rather than approved or impossible.

## Admission Workflow

Treat world development as a sequence of distinct commitment and authority states. Never bypass a state by treating provenance, prior analysis, technical plausibility, or an archived canonical location as Operator acceptance.

Use these fixed inbox paths:

```text
doc/inbox/idea/<idea-name>.md
doc/inbox/intent/<intent-name>.md
doc/inbox/idea/datasets/<dataset-name>/dataset.md
doc/inbox/intent/datasets/<dataset-name>/dataset.md
```

Use the dataset package forms only for structured data that cannot reasonably be represented as ordinary prose. Read the structured-dataset contract in [Canon Authoring Layout](references/canon-authoring-layout.md) before creating or promoting one. Do not create loose JSON or CSV files or alternate rough-idea or design-intent silos. Move documents or complete dataset packages out only through explicit promotion, admission, or rejection.

### Dependency Layers

Enforce one-way dependencies among commitment layers:

- Canon may depend only on canon.
- An intent may depend only on canon and other intents.
- An idea may depend on canon, intents, and other ideas.

When material is changed, promoted, rejected, or resolved, search both linked and semantic downstream dependencies: an idea can affect ideas, an intent can affect intents and ideas, and canon can affect canon, intents, and ideas. Update temporary selection guidance when it records an affected dependency. Revise affected consumers or leave them explicitly conditional; never silently substitute a surviving alternative.

Before promoting an idea outcome to intent, verify that every retained dependency is canon or intent. If any retained dependency is still an idea, stop for explicit manual Operator resolution; never promote that dependency or treat it as verified merely to unblock the requested promotion. After promotion, update dependent ideas to reference the new intent where applicable.

### Rough Ideas

Store exploratory, noncanonical, undecided world proposals under `doc/inbox/idea/`. Keep each ordinary idea concise and human-readable: preserve the proposed outcome, meaningful specifics, important interdependencies, and a compact provenance pointer when prior material exists. Use `doc/inbox/idea/datasets/<dataset-name>/` when the proposal is inherently structured data; its `dataset.md` remains the human-readable owner and its `data/` payloads carry the structured records. Do not inflate an idea into atomic verification declarations, exhaustive boundary warnings, coverage proofs, research reports, or canon-compliance audits.

Choose idea boundaries and filenames by coherent concept and Operator direction rather than imposing a fixed subject taxonomy. An idea may cross technical, political, cartographic, historical, or story concerns when those concerns are materially interdependent.

Only explicit Operator action may promote or reject an idea. An unselected idea remains an idea. Promotion removes the rough-idea document or complete structured-dataset package and creates or rewrites the corresponding design-intent form; do not retain parallel idea and intent copies. Rejection removes an idea only when the Operator explicitly chooses that outcome.

### Design Intent

Store Operator-accepted outcomes or directions under `doc/inbox/intent/`. Intent remains noncanonical pending technical validation, reconciliation, authority routing, and canonical integration. It protects the accepted outcome or direction, not every proposed mechanism, number, interpretation, or supporting claim. Use `doc/inbox/intent/datasets/<dataset-name>/` for accepted structured data and keep its Markdown owner and payloads together as one package.

Keep intent concise. Preserve dependencies, accepted boundaries, unresolved choices, and compact pointers to reusable prior work, but defer atomic claim decomposition and full proof until admission work begins. If an intent conflicts with upstream authority or another claim, surface the conflict and seek Operator resolution or a canon-compatible route without silently weakening the accepted outcome.

### Admission Evidence And Canon

When admitting intent, identify the focused canonical owner or owners, decompose the material claims inside admission evidence, traverse their upstream dependencies, reuse sufficient prior work, and verify only uncovered, stale, or materially different claims. Create or update canon and its admission evidence together, update affected authority links and consumers, and remove the intent only after its accepted outcome is fully resolved. Admit an inherently structured dataset as the complete `doc/world/datasets/<dataset-name>/` package defined by the canonical layout; never transcribe its records into Markdown merely to make them canonical.

Use only the exact owner-and-evidence package contracts defined by the canon-authoring layout. For an explicit fictional physics admission, the Operator's explicit decision is sufficient approval evidence; record the exact approved departure and its boundaries without inventing empirical support. For an advancement, identify its explicit Operator approval and exact grant, list every canonical physical authority that permits that grant, and explain their compatibility. Physical possibility never enlarges approval or establishes an unstated capability; an absent grant is not permission.

Treat admission evidence as authoritative about why a canonical claim was admitted: its source basis, assumptions, calculations, dependency checks, conflicts considered, rejected stronger interpretations, approval basis, and verification result. Evidence does not own in-world truth. Every rule, limit, capability, configuration, or historical fact needed to understand the world must appear in its focused canonical owner; evidence must never become a hidden source of world rules.

Bind evidence to the canonical owner and revision it evaluates. If that owner changes materially, treat the prior evidence as an accurate record of the earlier admission but recheck its coverage before relying on it for the changed claim. Link reusable external research from `doc/memory/` instead of duplicating it.

For existing canon without a colocated admission record, assemble evidence dependency-first from explicit Operator admissions, reusable research, completed qualification work, current authority, and Git provenance. Do not repeat prior verification merely because its result is being moved into the admission-evidence layout; perform fresh work only when coverage is missing, stale, contradictory, or addressed a different claim.

## Dependency-First Workflow

Before answering, reviewing, or editing:

1. Classify every material claim by domain and hierarchy layer.
2. Read the project's instruction files and authority entry points.
3. Identify the focused owners for the task's claims.
4. Traverse all relevant upstream dependencies before judging a derived claim.
5. Read adjacent authorities when the claim crosses domains.
6. For a time-dependent claim, select the period authority under `doc/world/history/periods/` whose declared temporal scope applies, and then read the focused entities and events.
7. If changing authority, search downstream consumers before editing.
8. Bound the authority slice to material dependencies; do not survey unrelated sibling documents.

For a read-only question, stop once the focused owner, every material upstream constraint, and the downstream context needed for the answer are established. Perform a broad downstream scan only for an upstream change or an explicit impact audit.

Create a concise constraint ledger containing:

- locked decisions and approved fictional admissions;
- applicable physical rules, owner-local numeric limits, and construction targets;
- required advancements and their exact granted scope;
- unresolved or noncanonical material that must not be assumed;
- the applicable historical interval and availability state;
- relevant mass, energy, momentum, heat, material, biological, time, information, economic, and logistical constraints.

Then test the claim against the complete ledger. Do not evaluate an advancement, capability, or configuration in isolation when another focused authority owns a relevant limit.

## Derived-System Validation

Require an integrated design to identify its upstream capabilities and close every material budget relevant to the claim. Check conservation, causality, scale, operating duration, waste products, failure modes, maintenance, supply, environment, and interactions among subsystems.

Distinguish clearly among:

- an enabling discovery existing;
- a bounded capability being achievable;
- a complete design implementing that capability;
- a historical entity possessing or deploying that design during a stated interval.

Do not let a shared word, evocative name, or nearby technology transfer capabilities between those levels.

## Change and Review Rules

- Change the highest authority that is actually wrong before repairing dependent documents.
- Require explicit Operator approval before changing a locked foundation or fictional admission, or adding, removing, renaming, or materially changing the exact grant of a protected advancement package.
- Reconcile contradictions in authority before relying on either side.
- Report downstream consequences of every upstream change, including configurations and affected historical periods, entities, and events.
- Preserve uncertainty where the evidence supports possibility but not a specific mechanism or performance figure.
- State whether a concern is an actual canonical contradiction, an unresolved gap, or only a prohibited interpretation.

For completion, verify that no applicable upstream owner was skipped, every hard numeric limit was carried forward, noncanonical material remained noncanonical, and all affected downstream claims were identified.

## Routing Examples

- For a human-performance target, consult the relevant pillar, physics, applicable advancements, focused biological and medical capabilities, and only then an integrated vehicle or habitat design.
- For a reactor change, consult physics, the advancement envelope, the reactor authority, fuel logistics, conversion, thermal rejection, storage, structural limits, and every named installation that uses the reactor.
- For a faction at a particular date, read the applicable period authority, the faction lifecycle, and relevant events, then consult the upstream institutions, technologies, designs, and resources that period makes available.
