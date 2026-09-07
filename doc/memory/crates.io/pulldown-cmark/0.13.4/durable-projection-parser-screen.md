# Reason For Investigation

On 2026-09-07, audit SX-R01 screened pulldown-cmark 0.13.4 as a replacement for Syndic's 1,057-line
projection parser. Beryl requires exact source-byte partitions, bounded carry and output pages,
stable digests/identities, markers, and persisted checkpoints that resume incomplete blocks.

# Outcome

Retain the current parser. The upstream constructor runs a complete first pass over its input
before returning the event iterator. Its input, tree and allocation state are private, with no
public append/checkpoint/restore interface. Optional serde support covers event/value types rather
than parser state. Whole-item parsing therefore does not satisfy Beryl's bounded durable resume.
Event offsets also do not directly provide Beryl's exact nonoverlapping source-byte partition.

A closed-block adapter could bound input, but would retain block recognition, fence/table counters,
carry, digests, source cursors, fallback, stable output IDs and markers. No substantial net deletion
was demonstrated after those adapters. No dependency adoption or contract change is proposed.

Screened configuration: `default-features = false`, no optional features. MIT license, Rust 1.71.1,
edition 2021; mandatory dependencies are bitflags 2, unicase 2.6 and memchr 2.5 compatible ranges.
Default HTML/getopts and optional SIMD are unnecessary. The inspected upstream CI exercises MSRV,
stable/nightly, feature combinations, WASM and denial-of-service regression fixtures. Native Windows
behavior was not verified. Version 0.13.4 was released on 2026-05-20 and is not yanked; its release
notes include a TightParagraph panic fix.

Rust's Cargo declares the 0.13.3-compatible range with HTML support at the cited commit. This is
real ecosystem adoption evidence, not proof that Cargo resolves exactly 0.13.4. No package advisory
path appeared in the complete official RustSec tree at the cited revision; that bounded check is
neither a security audit nor vulnerability clearance. No software was installed or compiled.

Revisit only with measured adapter size and peak residency, byte/digest equivalence, Unicode and
CRLF/bare-CR cases, zero-width markers, threshold boundaries and interrupted fence/table resume.
Detailed registry checksum, tag commit and evidence are retained in the
[SX report](../../../../audits/code-simplification/reviews/syndic-projection-findings.json).

# Sources

- [Exact registry release](https://crates.io/api/v1/crates/pulldown-cmark/0.13.4).
- [Release notes](https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.4).
- [Versioned manifest](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/pulldown-cmark/Cargo.toml).
- [Parser construction and private state](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/pulldown-cmark/src/parse.rs).
- [First-pass implementation](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/pulldown-cmark/src/firstpass.rs).
- [Public values and serde scope](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/pulldown-cmark/src/lib.rs).
- [Platform and SIMD notes](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/README.md).
- [Versioned CI](https://raw.githubusercontent.com/pulldown-cmark/pulldown-cmark/v0.13.4/.github/workflows/rust.yml).
- [Cargo adoption at a fixed commit](https://raw.githubusercontent.com/rust-lang/cargo/77fb9722e3e5ec33ca7adc1e99c9e15e512bb21f/Cargo.toml).
- [RustSec tree at the screened revision](https://api.github.com/repos/rustsec/advisory-db/git/trees/5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5?recursive=1).
