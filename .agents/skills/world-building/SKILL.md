---
name: world-building
description: Maintain authority, physical plausibility, causality, temporal state, and dependency integrity in physically grounded speculative-fiction and science-fiction world documentation. Use when creating, reviewing, reconciling, reorganizing, or changing world pillars, physics and fictional admissions, advancement tiers, biological or technological capabilities, environments, societies, economies, infrastructure, integrated designs, canonical world-document layouts, historical periods, entities, factions, places, populations, resources, chronology, or events.
---

# World Building

## Core Rule

Build every world claim from its upstream authorities. Treat the world as a directed dependency graph: physics constrains advancements, advancements constrain derived capabilities, capabilities constrain integrated designs, and designs constrain what can exist in history.

Do not preserve a downstream idea by silently weakening, bypassing, or inventing an exception to an upstream rule. Surface the conflict and resolve the controlling authority first.

Use established reality as the physical baseline. Replace or extend it only within the exact scope of an explicitly approved fictional admission. Treat an invented advancement as a hypothetical engineering achievement, not an additional physics admission; require it to remain compatible with the controlling physical model.

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

Before creating, editing, or restructuring canonical world documentation, changing canonical paths or document contracts, reviewing, validating, or reconciling structural conformance, or authoring and reorganizing temporal history authorities, read [Canon Authoring Layout](references/canon-authoring-layout.md) in full and follow it as normative. It owns the exact filesystem tree, placement rules, document contracts, and temporal authoring schemas. Do not improvise alternate structures.

## Temporal History

Treat history as the complete time-indexed realization of the world, including the latest documented period. Do not create a separate present-state authority and do not use unqualified terms such as `current` or `present` when the intended date or period can be named.

Use `doc/world/history/index.md` as the mandatory temporal entry point. A capability or design authority defines what is possible; history owns when, where, and by whom it was discovered, available, built, deployed, altered, or lost. Qualify changing claims with a narrower interval or divide the period rather than implying that a condition held throughout it.

## Authority and Canon

- Follow explicit operator decisions and project-declared locked authorities.
- Locate the focused owner of each claim. Within its scope, the focused owner controls over broader or dependent documents.
- Keep each substantive claim in one owner and link to it from consumers instead of duplicating it.
- Treat approved canonical content separately from candidates, authoring questions, research, and archives.
- Use research as evidence. It may justify changing authority but never changes world truth by itself.
- Treat an absent mechanism, missing number, or unresolved outcome as unresolved rather than approved or impossible.

## Dependency-First Workflow

Before answering, reviewing, or editing:

1. Classify every material claim by domain and hierarchy layer.
2. Read the project's instruction files and authority entry points.
3. Identify the focused owners for the task's claims.
4. Traverse all relevant upstream dependencies before judging a derived claim.
5. Read adjacent authorities when the claim crosses domains.
6. For a time-dependent claim, enter through `history/index.md`, select the applicable period, and then read the focused entities and events.
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

Do not let a shared word, evocative name, tier label, or nearby technology transfer capabilities between those levels.

## Change and Review Rules

- Change the highest authority that is actually wrong before repairing dependent documents.
- Require explicit approval before changing a locked foundation, fictional admission, or protected advancement catalog.
- Reconcile contradictions in authority before relying on either side.
- Report downstream consequences of every upstream change, including configurations and affected historical periods, entities, and events.
- Preserve uncertainty where the evidence supports possibility but not a specific mechanism or performance figure.
- State whether a concern is an actual canonical contradiction, an unresolved gap, or only a prohibited interpretation.

For completion, verify that no applicable upstream owner was skipped, every hard numeric limit was carried forward, noncanonical material remained noncanonical, and all affected downstream claims were identified.

## Routing Examples

- For a human-performance target, consult the relevant pillar, physics, applicable advancements, focused biological and medical capabilities, and only then an integrated vehicle or habitat design.
- For a reactor change, consult physics, the advancement envelope, the reactor authority, fuel logistics, conversion, thermal rejection, storage, structural limits, and every named installation that uses the reactor.
- For a faction at a particular date, enter through the history index, read the containing period, the faction lifecycle, and relevant events, then consult the upstream institutions, technologies, designs, and resources that period makes available.
