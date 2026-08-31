# Plan Authoring Template and Edge-Case Prompts

This reference is normative for plan creation, scope or phase-content authoring or revision, and authoring-completeness review.

## Plan Template

Use this skeleton, expanding only the active and near-term phases enough to execute and verify them:

```markdown
# Scope

<Scope derived from design authority and constraints from other active authorities. Where
engineering rigor applies, record only the resulting concrete work and evidence, not profile or
modifier declarations.>

# Phase 1: <one acceptance boundary> (wip)

<Tasks needed for this boundary, contract-required edge cases and defensive behavior, verification
evidence, and the latest resumable milestone or blocker.>

# Phase 2: <one acceptance boundary> (pending)

<Concise acceptance-boundary summary until this phase approaches activation.>
```

The second phase illustrates a known future boundary; omit it when none exists. A non-empty plan has one active `wip` phase when implementation is underway; keep every known future acceptance boundary as `pending`. Use `finished` only after the required completion review, then compact the phase as directed by the main skill.

## Rigor-Derived Planning

Derive only the concrete tasks, edge cases, failure handling, and verification evidence required by
the effective engineering-rigor contract. A plan may cite its authoritative design source, but must
not restate its durable profile, modifiers, or catalog definitions. Do not add adversarial
hardening, arbitrary-scale support, or recovery guarantees outside the declared operating envelope
unless another applicable authority or concrete consequence requires them.

## Expanded Edge-Case Prompts

During planning, derive an explicit checklist only for interactions relevant to the phase acceptance
boundary and required by applicable design contracts, including the effective engineering-rigor
contract. Pay special attention when work:

- Creates new state from existing state: copy, fork, clone, import, restore, resume, retry, migration, or template flows.
- Combines ownership boundaries: local, remote, persisted, generated, cached, or user-authored state.
- Has precedence, fallback, inheritance, defaulting, or override rules.
- Runs asynchronously, in the background, or across sessions or processes.
- Depends on optional, stale, partial, missing, or externally supplied metadata.
- Must preserve identity, ordering, provenance, permissions, or user intent.
- Has cleanup, cancellation, rollback, or partial-failure behavior.

For each applicable identified interaction, include a verification case or state why the effective
contract requires no additional verification. Do not promote a generic prompt into a requirement
outside the supported operating envelope.
