# Reason For Investigation

Audit HS-009 evaluated whether a mature native watcher could replace Beryl's theme polling worker,
bounded file observation and hashing, and neutral change hints.

# Outcome

notify 8.2.0 is mature, but is not a demonstrated drop-in simplification. Its Windows implementation
discards the spawned worker JoinHandle; Drop sends Stop without acknowledging worker termination.
Beryl currently joins its worker and retires the generation. The dependency's 16 KiB native buffer
and callbacks can feed a bounded adapter, but that does not close the shutdown gap. Native network
filesystem and WSL support also differs from polling. Retain current code pending an explicit
lifecycle/platform decision or a suitable acknowledged shutdown API.

The stable release was published 2025-08-03; 9.0 prereleases continued through 2026-08-30. Typst
directly consumes notify 8. The notify license is CC0-1.0; notify-types is MIT OR Apache-2.0; MSRV is
1.77. For Windows, default-features=false avoids the default macos_fsevent feature; optional
channel, serde and debouncer features are unnecessary. Dependencies include notify-types, libc,
log, walkdir and target-specific windows-sys 0.60.1. Other platforms bring their native backends.

Gross replacement of roughly 150-190 scanner/worker lines needs perhaps 80-150 adapter/lifecycle
lines before unresolved fallback policy. No safe net reduction is claimed. No installation,
dependency change, build or test occurred. The upstream advisory page and targeted RustSec search
showed no notify-specific advisory on 2026-09-06; this is not a dependency-closure security audit.

# Sources

- [Exact manifest](https://raw.githubusercontent.com/notify-rs/notify/notify-8.2.0/notify/Cargo.toml)
  and [workspace manifest](https://raw.githubusercontent.com/notify-rs/notify/notify-8.2.0/Cargo.toml).
- [Windows source](https://raw.githubusercontent.com/notify-rs/notify/notify-8.2.0/notify/src/windows.rs):
  worker creation, fixed buffer, control channels and Drop inspected on 2026-09-06.
- [Release metadata](https://docs.rs/crate/notify/latest) and
  [documented filesystem limitations](https://docs.rs/notify/latest/notify/).
- [Typst adoption](https://github.com/typst/typst/blob/main/Cargo.toml).
- [Upstream advisories](https://github.com/notify-rs/notify/security/advisories) and
  [RustSec](https://rustsec.org/advisories/).
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07:
  crates/beryl-home-store/src/theme/watcher.rs and tests/theme_watcher.rs; full evidence in
  [HS-009](../../../../audits/code-simplification/reviews/home-store-findings.json).
