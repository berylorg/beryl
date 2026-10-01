# Upstream Baseline

Installed AIPM skills were reconciled with `https://github.com/berylorg/aipm` at commit
`1053a32` on 2026-09-23. The local adaptations below are intentional; preserve them during updates.
`world-building` is not installed in this repository.

# Remote

From `https://github.com/berylorg/aipm`:

- agent-environment-health
- cargo-projects
- exploration-memory
- failure-logging
- feature-design-docs
- multi-agent-vcs
- system-design-docs
- workspace-package-policy

# Remote With Local Adaptations

Based on `https://github.com/berylorg/aipm` with Beryl-specific changes:

- `implementation-planning`: Beryl groups bounded supporting work under behavioral acceptance,
  tracks the intended production consumer and completion condition, and keeps continuation context compact.
- `architectural-rework`: Beryl uses selective authority reads and outcome-based tracker slices,
  consolidating completed history instead of feeding helper-sized phases into the plan.
- `engineering-rigor`: Beryl applies review to the complete affected acceptance boundary and
  reuses unchanged evidence without weakening explicit verification or independent-review requirements.
- `project-doc-authority`: Beryl reuses unchanged authority readings while refreshing relevant
  requirements and exact source needed for consequential decisions.
- `subagent-orchestration`: Beryl defaults implementation workers to GPT-6.1 Sol, keeps task-based
  Luna/Astra exceptions and total-cost controls, reuses unchanged workflow readings and scopes
  reviews and handoffs to meaningful acceptance boundaries.
- `gui`: Beryl retains Beryl-specific naming, Windows-first text bindings, and bounded GPUI static
  or virtualized context-menu contracts around the canonical skill.
- `rust-first-automation`: Beryl and its owned dependency forks are the default Rust-first scope;
  unrelated projects retain the canonical toolchain and project-authority guard.
- `rag-rat-project-docs`: Beryl requires its Operator-provided patched rag-rat build instead of the
  canonical public release, batches documentation reconciliation before semantic use or handoff,
  and permits explicit direct-read continuation when the derived index is unavailable.

# Local

- gpui-scroll-surfaces
