# Reason For Investigation

On 2026-09-07, the complete simplification audit revisited PRE-004's custom test-home helpers.
The existing adjacent-durable-writes note preserves a historical production architecture; it does
not establish current test cleanup semantics or authorize changing physical publication behavior.

# Outcome

Reuse the existing tempfile 3.27.0 dependency for the audited disposable test roots. Foundation
crates already use it; application and Syndic reports own their distinct incremental reductions.
Preserve scenario identity, reopen behavior and resource retirement. Do not replace production
sidecar or theme-file residue with automatic cleanup.

The exact package is MIT OR Apache-2.0, declares Rust 1.63 and uses the existing default getrandom
feature. Supporting dependencies include fastrand, once_cell and target-specific rustix or
windows-sys. The baseline lock checksum is
`32497e9a4c7b38532efcdebeef879707aa9f794296a4f0244f6f69e9bc8574bd`.
Its packaged source commit is `5c8fa12eb584931b4f1bccfde87eb72fbfa7dc61`.

Randomized creation retries collisions; the Windows generic backend uses exclusive `create_dir`.
Store handles, service owners and children must retire before directory removal. `TempDir::drop`
ignores deletion errors and does not retry. `close` reports removal failure but forgets ownership,
so preserving a fixture's retry policy requires deliberate retained-path handling. Neither operation
provides publication durability. Account path adapters, retry helpers and checked cleanup in the
net estimate; changing the directory type alone does not fix field destruction order.

The non-yanked release was published on 2026-03-11. The upstream repository was unarchived and had
recent activity when checked. Cargo at the cited fixed commit declares 3.27.0 and uses tempfile in
production; Beryl already uses it in tests. An exact-package OSV query on 2026-09-07 returned no
entries. This does not qualify the entire resolved dependency graph or future releases. No package
was installed, upgraded or executed for this screen; Windows behavior was inspected in source.

Detailed evidence and the OSV query outcome are retained in the
[foundation synthesis](../../../../audits/code-simplification/reviews/subsystem-foundations.json).

# Sources

- [Exact registry release](https://crates.io/api/v1/crates/tempfile/3.27.0).
- [Packaged manifest](https://docs.rs/crate/tempfile/3.27.0/source/Cargo.toml).
- [Packaged source identity](https://docs.rs/crate/tempfile/3.27.0/source/.cargo_vcs_info.json).
- [Creation retry implementation](https://raw.githubusercontent.com/Stebalien/tempfile/5c8fa12eb584931b4f1bccfde87eb72fbfa7dc61/src/util.rs).
- [Directory ownership and cleanup](https://raw.githubusercontent.com/Stebalien/tempfile/5c8fa12eb584931b4f1bccfde87eb72fbfa7dc61/src/dir/mod.rs).
- [Generic directory backend](https://raw.githubusercontent.com/Stebalien/tempfile/5c8fa12eb584931b4f1bccfde87eb72fbfa7dc61/src/dir/imp/any.rs).
- [Upstream repository metadata](https://api.github.com/repos/Stebalien/tempfile).
- [Cargo adoption at a fixed commit](https://raw.githubusercontent.com/rust-lang/cargo/77fb9722e3e5ec33ca7adc1e99c9e15e512bb21f/Cargo.toml).
- [OSV package query endpoint](https://api.osv.dev/v1/query), queried for Cargo package tempfile 3.27.0.
