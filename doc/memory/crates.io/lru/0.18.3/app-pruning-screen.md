# Reason For Investigation

On 2026-09-07, application synthesis checked whether an LRU primitive would substantially replace
AS-001's bounded cache implementation after Moka failed the exact-admission fit screen.

# Outcome

Do not add the dependency for this target. The two existing pruning scans are only 17 lines each.
Count-capacity LRU operations leave Beryl's independent byte accounting, admission, owners,
request tickets and pending-work behavior. No substantial net reduction was demonstrated.

Version 0.18.3 is a non-yanked 2026-08-28 release, MIT licensed, declaring Rust 1.85.0. Its default
hashbrown feature uses optional hashbrown 0.17. No feature configuration was selected or resolved.
RUSTSEC-2026-0253 is fixed in 0.18.2; the other cited advisories were fixed earlier. This is a scoped
screen, not dependency-graph security clearance. Platform and substantial adoption qualification
stopped after the replacement surface proved too small. Nothing was installed or executed.

# Sources

- [Exact registry release](https://crates.io/api/v1/crates/lru/0.18.3).
- [Versioned LRU API](https://docs.rs/lru/0.18.3/lru/struct.LruCache.html).
- [Versioned manifest](https://raw.githubusercontent.com/jeromefroe/lru-rs/0.18.3/Cargo.toml).
- [RUSTSEC-2026-0253](https://rustsec.org/advisories/RUSTSEC-2026-0253.html).
- [RUSTSEC-2026-0002](https://rustsec.org/advisories/RUSTSEC-2026-0002.html).
- [RUSTSEC-2021-0130](https://rustsec.org/advisories/RUSTSEC-2021-0130.html).
- [Application synthesis evidence](../../../../audits/code-simplification/reviews/subsystem-app.json).
