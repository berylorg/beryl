# Plan Authoring Template and Edge-Case Prompts

This reference is normative for plan creation, scope or phase-content authoring or revision, and authoring-completeness review.

## Plan Template

Use this skeleton, expanding only the active and near-term phases enough to execute and verify them:

```markdown
# Scope

<Scope derived from design authority and constraints from other active authorities.>

# Phase 1: <one acceptance boundary> (wip)

<Tasks needed for this boundary, relevant edge cases, verification, and the latest resumable milestone or blocker.>

# Phase 2: <one acceptance boundary> (pending)

<Concise acceptance-boundary summary until this phase approaches activation.>
```

The second phase illustrates a known future boundary; omit it when none exists. A non-empty plan has one active `wip` phase when implementation is underway; keep every known future acceptance boundary as `pending`. Use `finished` only after the required completion review, then compact the phase as directed by the main skill.

## Expanded Edge-Case Prompts

During planning, derive an explicit edge-case checklist from relevant design docs and contracts. Pay special attention when work:

- Creates new state from existing state: copy, fork, clone, import, restore, resume, retry, migration, or template flows.
- Combines ownership boundaries: local, remote, persisted, generated, cached, or user-authored state.
- Has precedence, fallback, inheritance, defaulting, or override rules.
- Runs asynchronously, in the background, or across sessions or processes.
- Depends on optional, stale, partial, missing, or externally supplied metadata.
- Must preserve identity, ordering, provenance, permissions, or user intent.
- Has cleanup, cancellation, rollback, or partial-failure behavior.

For each identified interaction, include a verification case or state why no additional verification is needed.
