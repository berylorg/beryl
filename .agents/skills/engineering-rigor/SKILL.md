---
name: engineering-rigor
description: "Define and apply proportionate defensive-programming, verification, failure-handling, and adversarial-review requirements through mandatory # Engineering Rigor sections in authoritative feature, system, and workspace-project design.md files. Use when creating or revising design authority, selecting rigor profiles or modifiers, deciding whether robustness work or review findings are required, deriving implementation acceptance criteria, or calibrating review depth to trust, exposure, operating envelope, blast radius, and consequence."
---

# Engineering Rigor

## Core Rule

Require engineering rigor proportionate to the durable contract of the affected scope. Do not treat
maximum robustness as a universal quality target. Require defensive behavior, verification, and
adversarial review only when an applicable rigor profile, modifier, scope-specific statement, or
concrete consequence requires them.

Keep rigor authority in design docs, not implementation plans. Use `# Engineering Rigor` as the
third required top-level section after `# Decisions` in every authoritative feature, system, and
workspace-project `design.md`.

## Rigor Declaration

Before creating, revising, interpreting, planning from, implementing from, or reviewing a
`# Engineering Rigor` section, read [Rigor Profile Catalog](references/rigor-profiles.md) in full as
normative.

Declare exactly one versioned profile and zero or more versioned modifiers:

```markdown
# Engineering Rigor

Profile: `trusted-internal-tool/v1`

Modifiers:

- `persistent-state-integrity/v1`
```

Use `Modifiers: none` when none apply. Add prose only for scope-specific trust boundaries,
operating envelopes, consequences, or guarantees that catalog identifiers cannot express. Do not
copy catalog definitions into design docs.

Treat runtime timing, frequency, and whole-scope coverage as separate explicit guarantees. Do not
infer them from words such as atomic, complete, coherent, or integrity. Before requiring a
nontrivial runtime check, trace the concrete fault, affected state or effect slice, supported-envelope
consequence, enforcement mechanism, and why dependency guarantees plus development evidence do not
cover it. Scope the check to that boundary.

Treat a missing section, missing profile, unknown identifier, or incompatible declaration as an
incomplete design. Resolve it before implementation planning or review relies on that design.

## Authority Composition

Apply each declaration only to the feature, system, or workspace-project scope that owns it. Combine
all applicable higher- and lower-scope guarantees for a change:

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

Classify a review finding as blocking only when it identifies one of:

- An unmet applicable profile criterion, modifier, or scope-specific guarantee.
- A concrete consequence inside the supported operating envelope.
- An undeclared trust, exposure, blast-radius, or irreversibility condition that requires the
  owning design authority to be corrected.

Treat speculative hardening outside the effective contract as non-blocking. Stop review when the
contract's evidence threshold is met. Do not broaden supported inputs or promise graceful behavior
outside the declared operating envelope merely because stronger behavior is technically possible.

Escalate the owning design authority before implementation when actual inputs, users, shared
resources, sensitive data, privileges, external side effects, recovery constraints, or failure
consequences exceed its declaration.
