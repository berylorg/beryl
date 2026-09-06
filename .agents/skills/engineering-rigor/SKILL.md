---
name: engineering-rigor
description: "Define and apply proportionate defensive-programming, verification, failure-handling, and adversarial-review requirements through mandatory # Engineering Rigor sections in authoritative feature, system, and workspace-project design.md files. Use when creating or revising design authority, selecting rigor profiles or modifiers, deciding whether robustness work or review findings are required, deriving implementation acceptance criteria, or calibrating review depth to trust, exposure, operating envelope, blast radius, and consequence."
---

# Engineering Rigor

## Core Rule

Require engineering rigor proportionate to the durable contract of the affected scope. Do not treat
maximum robustness as a universal quality target. Accept the simplest implementation that satisfies
the contract, including its permitted failed invocation, manual retry, or rebuild. Require defensive
behavior, verification, and adversarial review only when an applicable rigor profile, modifier,
scope-specific statement, or material consequence beyond permitted failures requires them.

Keep rigor authority in design docs, not implementation plans. Use `# Engineering Rigor` as the
third required top-level section after `# Decisions` in every authoritative feature, system, and
workspace-project `design.md`.

## Rigor Declaration

Before creating, revising, interpreting, planning from, implementing from, or reviewing a
`# Engineering Rigor` section, read the versioning rules, verification semantics, selection rules,
and selected profile and modifier entries in the normative
[Rigor Profile Catalog](references/rigor-profiles.md). Compare other entries and consult worked
examples when needed to resolve selection; do not reread unrelated versions for a settled contract.

Declare exactly one versioned profile and zero or more versioned modifiers:

```markdown
# Engineering Rigor

Profile: `trusted-internal-tool/v2`

Modifiers:

- `persistent-state-integrity/v2`
```

Use `Modifiers: none` when none apply. Add prose only for scope-specific trust boundaries,
operating envelopes, consequences, or guarantees that catalog identifiers cannot express. Do not
copy catalog definitions into design docs.

For explicit inheritance, declare the exact inherited profile and applicable modifier identifiers
and cite the authoritative source and its applicable scope. This uses the same declaration format
and must resolve to one unambiguous existing contract; an unrelated parent is not a default.

Treat runtime timing, frequency, and whole-scope coverage as separate explicit guarantees. Do not
infer them from words such as atomic, complete, coherent, or integrity. Before requiring a
nontrivial runtime check, trace the concrete fault, affected state or effect slice, supported-envelope
consequence, enforcement mechanism, and why dependency guarantees plus development evidence do not
cover it. Scope the check to that boundary.

Resolve a missing section or profile through an explicit declaration or explicit inheritance of an
unambiguous existing applicable contract. Do not silently invent a profile. Resolve unknown
identifiers and incompatible declarations in their owning authority. Stop for Operator resolution
only when an unresolved choice would materially change implementation or acceptance; an explicit,
unambiguous inheritance needs no new policy choice.

Versioned criteria remain binding. Adopting a relaxed `/v2` contract requires an explicit design
change; generic proportionality guidance never weakens an existing `/v1` declaration or its review
guarantees. Unchanged `/v1` contracts remain valid choices. Do not mass-upgrade declarations.

## Authority Composition

Apply each declaration only to the feature, system, or workspace-project scope that owns it. Combine
all applicable higher- and lower-scope guarantees for a change, tracing each to the protected outcome
the change can affect. A parent product's profile does not automatically classify its development
tools as production applications; relevant parent guarantees still apply to their effects.

- Let a narrower design strengthen or specialize an applicable contract.
- Do not let a narrower design weaken a broader applicable guarantee unless the broader authority
  explicitly permits that exception.
- Treat different profiles as contracts across several dimensions, not as a single ordinal scale.
- Do not duplicate enforcement or verification mechanisms merely because profiles, modifiers, and
  scope-specific guarantees overlap; one mechanism may satisfy several requirements when its
  evidence covers each one.
- Stop and reconcile incompatible trust, exposure, operating-envelope, or consequence statements
  in their owning design docs.
- Keep shared user-visible rigor requirements in feature docs, shared technical requirements in
  system docs, and one workspace project's boundary requirements in its own design doc.

## Implementation And Review

Derive defensive coding, failure handling, verification breadth, and review method from the
effective rigor contract. Do not restate that contract in `doc/plan.md`; translate it into concrete
tasks and acceptance evidence.

Distinguish evidence strength from runtime machinery: strong focused tests can justify a small
implementation. For low-consequence work under contracts that permit it, ordinary errors and
manual retry or rebuild are defaults. Automatic retries, journals, redundant validation, generalized
recovery frameworks, and exact resource accounting need a concrete contract requirement. Permitted
failure alone is not a defect. Never require machinery merely to prevent that permitted outcome.

Choose review depth separately from implementation defenses. Where the selected contract permits
it, low-consequence work can use objective evidence plus author self-review, and weak verification
alone does not require an independent reviewer. Mechanical production changes can use the same
path under `production-application/v2` when objective evidence establishes the affected guarantees.
These options do not override explicit independent-review requirements in a selected version,
modifier, or other authority.

Classify a review finding as blocking only when it identifies one of:

- An unmet applicable profile criterion, modifier, or scope-specific guarantee.
- A concrete consequence inside the supported operating envelope that exceeds its tolerated
  failure or violates a protected outcome.
- An undeclared trust, exposure, blast-radius, or irreversibility condition that creates material
  consequences beyond permitted failures and requires the owning design authority to be corrected.

Demonstrate material harm with a concrete failure mechanism and supporting evidence within the
supported envelope; an actual incident, numeric probability, or risk register is not required.

Treat speculative hardening outside the effective contract as non-blocking. Stop review when the
contract's evidence threshold is met. Do not broaden supported inputs or promise graceful behavior
outside the declared operating envelope merely because stronger behavior is technically possible.

Escalate the owning design authority before implementation when actual inputs, users, shared
resources, sensitive data, privileges, external side effects, recovery constraints, or failure
consequences exceed its declaration.
