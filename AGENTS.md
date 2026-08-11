# Repository Instructions

## Authority And Implementation Ownership

- Authoritative documentation is Operator-owned and controls implementation.
- Source code and tests are AI-maintained working material. Agents may freely rewrite, replace, or delete them as required by authoritative documentation and the active implementation plan.
- A dirty worktree does not imply human ownership of source code or tests and is not, by itself, a reason to avoid changing them.
- Do not blindly discard unrelated or concurrent work. Reconcile existing changes against current authority and the active phase, and preserve work that remains consistent with them.

## Owned Dependency Forks

- When an architectural choice arises within a Beryl-owned dependency fork, prefer the option that best satisfies Beryl's authoritative requirements and lifecycle, even when another option would make the dependency more generally reusable or convenient in isolation.

## Rust code navigation

Optimize code exploration for context/token usage.

- Prefer Serena semantic navigation over reading entire Rust files.
- Use `get_symbols_overview` with `depth=0` to map an unfamiliar large file when its structure is
  unknown; use `find_symbol` directly when the target name is known.
- Use `find_symbol` with a `relative_path` whenever the search can be narrowed to a file or
  directory. Keep `include_body=false` until the exact symbol body is needed.
- Use `find_referencing_symbols` for symbol usages and call relationships.
- Use `find_implementations` from a trait or type symbol when investigating implementations, and
  use `find_declaration` when a concrete source occurrence must be resolved to its declaration.
- For potentially large queries, start with `max_matches` between 5 and 20 and
  `max_answer_chars=10000` when supported. Refine the name path or relative path before raising
  those bounds.
- Read source only around the specific symbols/functions needed.
- Continue using rg for strings, comments, error messages, configuration,
  macro text, and conceptual searches where no symbol is known.
- Do not read an entire large Rust file merely to find a symbol.
- Cargo workspace autoreload is intentionally disabled. After changing any Cargo.toml or workspace
  membership, preserve the current analyzer model until the validation command and the applicable
  focused Cargo check both succeed: `cargo metadata --format-version 1 --no-deps --locked`. If
  validation fails, repair the manifest or lockfile without restarting rust-analyzer.
- After that validation succeeds, call `restart_language_server` before relying on semantic results
  from the new Cargo model. If restart fails, stop and report the failure rather than continuing
  with stale semantic results.
