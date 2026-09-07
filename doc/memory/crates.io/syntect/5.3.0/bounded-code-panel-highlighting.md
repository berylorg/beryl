# Reason For Investigation

The complete simplification audit evaluated Syntect as a replacement for Beryl's code-panel
language adapters. These adapters highlight literal source; they are distinct from transcript
Markdown parsing. Local JSON and TOML adapters already use jsonc-parser 0.32.4 and taplo 0.14.0.

# Outcome

Retain the current adapters under their current contracts. Syntect 5.3.0 is a substantial candidate
only if Beryl explicitly chooses grammar-based highlighting in place of strict JSON/error/key
segmentation and the narrow Windows INI dialect. Roughly 1,144 local adapter lines are in scope,
but mapping scopes, preserving dialect exceptions, loading grammar assets, translating byte ranges,
bounding parser state and testing compatibility may erase the saving. No defensible net reduction
has been established. Do not add it solely to replace the small INI or Markdown scanner.

ParseState::parse_line takes a contiguous logical line and emits scope operations. Persistent parse
state and a separate scope stack need admitted ownership. The crate provides no Beryl-specific
byte, CPU or concurrency limit. A prototype would need explicit overlong-line unavailable behavior,
bounded checkpoints and scope-to-role mapping. JSON/TOML validators might still be required,
eliminating any package reduction. Cache simplification AS-001 is a separate local opportunity.

The inspected pure Rust configuration is default-features=false with parsing, default-syntaxes and
regex-fancy. This avoids default Oniguruma native linkage and unnecessary HTML, plist, YAML and
default-theme paths. Bundled grammar coverage is not assumed. Upstream cautions about fancy-regex
debug performance; production and debug behavior both matter to Beryl. The tagged CI uses Ubuntu,
so this investigation does not qualify Windows behavior by execution.

Version 5.3.0 was published 2025-09-27 and was the latest stable, non-yanked release checked on
2026-09-06. It is MIT-licensed. Neither release manifest nor registry declares a fixed MSRV;
the rolling last-three-stable CI policy is not proof of compatibility with Beryl's Rust 1.92.
Upstream maintains the mature implementation and runs bat syntax regression and fancy-engine
tests. Typst directly consumes 5.3 with regex-fancy. An official RustSec advisory-tree search found
no direct syntect or fancy-regex advisory path on the audit date, but no resolved dependency-closure
audit or security clearance is claimed. No install, dependency change, build or prototype was run.

# Sources

- [Exact registry metadata](https://crates.io/api/v1/crates/syntect/5.3.0).
- [Release manifest](https://docs.rs/crate/syntect/5.3.0/source/Cargo.toml.orig) and
  [ParseState API](https://docs.rs/syntect/5.3.0/syntect/parsing/struct.ParseState.html).
- [Tagged README](https://raw.githubusercontent.com/trishume/syntect/v5.3.0/Readme.md) and
  [CI policy and regression commands](https://raw.githubusercontent.com/trishume/syntect/v5.3.0/.github/workflows/CI.yml).
- [Typst adoption](https://github.com/typst/typst/blob/main/Cargo.toml).
- [Official RustSec tree query](https://api.github.com/repos/RustSec/advisory-db/git/trees/main?recursive=1).
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: all shell/syntax_highlighting source
  bodies and related language tests, inspected by the app-shell auditor. See
  [AS-004 evidence](../../../../audits/code-simplification/reviews/app-shell-findings.json).
  The complete 117-file shell cohort was subsequently accepted at full-body review depth.
