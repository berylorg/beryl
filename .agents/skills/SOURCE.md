# Remote

From `https://github.com/berylorg/aipm`:

- agent-environment-health
- architectural-rework
- cargo-projects
- engineering-rigor
- exploration-memory
- failure-logging
- feature-design-docs
- implementation-planning
- multi-agent-vcs
- project-doc-authority
- subagent-orchestration
- system-design-docs
- workspace-package-policy
- world-building

# Remote With Local Adaptations

Based on `https://github.com/berylorg/aipm` with Beryl-specific changes:

- `gui`: Beryl retains Beryl-specific naming, Windows-first text bindings, and bounded GPUI static
  or virtualized context-menu contracts around the canonical skill.
- `rust-first-automation`: Beryl and its owned dependency forks are the default Rust-first scope;
  unrelated projects retain the canonical toolchain and project-authority guard.
- `rag-rat-project-docs`: Beryl requires its Operator-provided patched rag-rat build instead of the
  canonical public release and retains matching local setup and recovery instructions.

# Local

- gpui-scroll-surfaces
