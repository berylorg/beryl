# Repository Instructions

## Authority And Implementation Ownership

- Authoritative documentation is Operator-owned and controls implementation.
- Source code and tests are AI-maintained working material. Agents may freely rewrite, replace, or
  delete them as required by authoritative documentation and the active implementation plan.
- A dirty worktree does not imply human ownership of source code or tests and is not, by itself, a
  reason to avoid changing them.
- Do not blindly discard unrelated or concurrent work. Reconcile existing changes against current
  authority and the active phase, and preserve work that remains consistent with them.

## Owned Dependency Forks

- When an architectural choice arises within a Beryl-owned dependency fork, prefer the option that
  best satisfies Beryl's authoritative requirements and lifecycle, even when another option would
  make the dependency more generally reusable or convenient in isolation.
