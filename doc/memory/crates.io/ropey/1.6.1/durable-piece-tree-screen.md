# Reason For Investigation

The complete simplification audit examined Ropey 1.6.1 as a possible replacement for Syndic's
durable draft piece trees. The cohort read all 18 assigned files and 19,626 lines, including
authenticated sequence, marker identity/order structures, codecs, bounded continuations and tests.
DT-007 retains the detailed investigation, checked on 2026-09-06; root confirmed the exact API,
manifest and release CI on 2026-09-07.

# Outcome

Rejected for this storage boundary. The API supports text editing, scalar/byte/line conversion and
cheap shared clones. Its reader/writer operations import and export text; they do not expose an
authenticated external-node store. Tagged source owns a private resident `Arc<Node>` tree. Our
inference from that API and Beryl's contracts is that a replacement would still require durable node
identities and hashes, marker structures, exact byte charges and per-command restart/replay custody.
Loading a complete draft would violate bounded active residency. No positive net reduction is
demonstrated. A bounded resident text window would be a separate investigation, not a storage cut.

The exact release is MIT-licensed and has no declared `rust-version`; a current master MSRV does
not establish the released version's minimum. Defaults enable Unicode/CR line recognition and SIMD;
normal dependencies are smallvec and str_indices, whose SIMD feature is forwarded. Beryl's exact
newline and empty-line semantics need separate treatment. Tagged CI covers Ubuntu stable/beta and
Miri; it does not establish Windows support. No build or platform verification was performed.

At the investigation date, registry evidence identified 1.6.1 as a non-yanked stable release from
2023-10-18, with a newer 2.0.0 beta. The upstream was active and unarchived; Helix's manifests used
1.6.1 with defaults disabled and SIMD enabled. That adoption supports maturity, not storage fit.
The inspected complete RustSec tree had no ropey or str_indices package entries and included
historical smallvec entries. Resolved dependency versions and patched ranges were not investigated
after architectural rejection, so no advisory clearance is claimed. No installation, manifest
change, benchmark or source change occurred.

# Sources

- [Exact release manifest](https://raw.githubusercontent.com/cessen/ropey/v1.6.1/Cargo.toml).
- [Exact Rope API](https://docs.rs/ropey/1.6.1/ropey/struct.Rope.html).
- [Tagged implementation](https://raw.githubusercontent.com/cessen/ropey/v1.6.1/src/rope.rs).
- [Tagged CI](https://raw.githubusercontent.com/cessen/ropey/v1.6.1/.github/workflows/ci.yml).
- [Registry metadata](https://crates.io/api/v1/crates/ropey).
- [Upstream metadata](https://api.github.com/repos/cessen/ropey).
- [Helix workspace manifest](https://raw.githubusercontent.com/helix-editor/helix/master/Cargo.toml)
  and [consumer manifest](https://raw.githubusercontent.com/helix-editor/helix/master/helix-core/Cargo.toml).
- [Inspected RustSec tree](https://api.github.com/repos/rustsec/advisory-db/git/trees/5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5?recursive=1).
- [DT-007 detailed findings](../../../../audits/code-simplification/reviews/syndic-draft-tree-findings.json).
- [Related im screen](../../im/15.1.0/durable-admission-index-screen.md).
