# Reason For Investigation

Phase 75 requires the `beryl-state` catalog domain to derive one schema-versioned Unicode
`NFKC_Casefold` search form at both publication and query boundaries while retaining original text
for display. This note determines the exact Unicode algorithm, evaluates established Rust options,
and records conformance and resource-bound implications. It is research memory, not design
authority.

The local baseline on 2026-08-01 was:

- The workspace targets Rust 1.92 and declares only `unicode-segmentation` among direct Unicode
  dependencies. `beryl-state` has no Unicode dependency.
- `Cargo.lock` contains no `unicode-normalization`, `unicode-casefold`, `caseless`, or
  `icu_casemap`. It contains transitive `icu_normalizer 2.2.0`, but `beryl-state` does not use it.
- `CatalogSearchFields::from_admitted_normalized` currently accepts caller-produced strings, and
  its documentation explicitly says the boundary does not normalize Unicode. The catalog codec
  stores four normalized strings without a normalization-profile or Unicode-data version.
- Authority bounds normalized title, environment, executable, and root fields at 2 KiB, 1 KiB,
  64 KiB, and 64 KiB, respectively, and a whole row at 256 KiB.

# Outcome

## Exact Unicode 17 algorithm

For the named transform, implement Unicode 17 Core Specification rule R5 exactly:

1. For every Unicode scalar `C` in the input, emit `NFKC_CF(C)` from the Unicode 17
   `DerivedNormalizationProps.txt` data. An omitted scalar maps to itself; an explicitly empty
   mapping emits nothing.
2. Normalize the complete emitted character stream to NFC. This whole-string step is required
   because adjacent per-character mappings can interact.

The `NFKC_CF` table is already the fixed point of repeatedly applying NFKC, default full case
folding, and removal of `Default_Ignorable_Code_Point` characters. Recreating it as one
`nfkc()`/case-fold pass is not equivalent. Default full folding uses the `C` and `F` entries from
`CaseFolding.txt` and excludes the Turkic `T` entries.

Do not substitute `str::to_lowercase()`, simple folding, `NFKC` alone, or an arbitrary order of
`NFKC` and full case folding. Useful Unicode 17 regression vectors include:

- `U+00DF` to `0073 0073`, and `U+0130` to `0069 0307` for full non-Turkic folding.
- `U+037A` to `0020 03B9`, demonstrating a fold introduced through compatibility mapping.
- `U+3392` to `006D 0068 007A` and `U+FB03` to `0066 0066 0069`.
- `U+00AD`, `U+200D`, and `U+FE0F` to empty, demonstrating default-ignorable removal.
- Reserved `U+2065` to empty, but omitted unassigned `U+0378` to itself. Rejecting or deleting all
  unassigned scalars is therefore not exact fixed-version behavior.
- `U+0041 U+0341` to NFC `U+00E1`: the two table mappings are `0061` and `0301`, and final NFC
  composes across their boundary.
- `U+0390` to itself: ordinary full folding produces `03B9 0308 0301`, but final NFC restores
  `0390`.

Unicode distinguishes R5 `toNFKC_Casefold(X)` from D144 default caseless equality and D147
identifier caseless equality. D147 compares `toNFKC_Casefold(NFD(X))`. Existing authority names
the R5 mapping for search keys over titles and paths, so a leading NFD pass should not be added
silently. If "Default Caseless Matching" was intended to require D147 rather than merely identify
the section defining R5, design authority must resolve that wording.

Original visible title, environment, executable, and root text must remain separate values. The
lossy result is only a derived search key and can be empty.

## Rust dependency findings and recommendation

No well-established Rust crate inspected exposes the complete R5 transform at a current fixed
Unicode version.

The smallest policy-conforming implementation option is:

- Use `unicode-normalization 0.1.25` for the required final NFC iterator. It is mature, licensed
  `MIT OR Apache-2.0`, defaults only to its `std` feature, depends on `tinyvec`, and exposes
  `UNICODE_VERSION == (17, 0, 0)`. `UnicodeNormalization::nfc()` accepts any
  `Iterator<Item = char>`.
- Generate and check in a catalog-owned compact mapping from the Unicode 17 `NFKC_CF` section,
  including explicit empty ranges and identity-by-omission. Feed that mapping iterator into
  `.nfc()` inside the single catalog API used by both publication and query.
- Bind the catalog normalization profile to both `NFKC_CF/R5` and Unicode `17.0.0`. Exact-pin the
  crate or enforce its public `UNICODE_VERSION` with a build/test assertion so a lock refresh
  cannot silently change data under the same schema. A future Unicode-data change requires a
  schema/profile change and catalog rebuild before mixed rows are queried.

`unicode-normalization` alone does **not** suffice: it implements NFC, NFD, NFKC, and NFKD, but
has no case-folding or `NFKC_CF` mapping and does not remove default-ignorables.

Other candidates were rejected for this phase:

- `unicode-casefold 0.2.0` is `MIT/Apache-2.0`, has no features, and offers streaming
  `.case_fold()` / `.case_fold_with(Variant::Full, Locale::NonTurkic)`. Its published table exposes
  Unicode `9.0.0`; it performs only folding and omits compatibility normalization,
  default-ignorable removal, and final NFC.
- `caseless 0.2.2` is MIT, has no features, depends on `unicode-normalization`, exposes Unicode
  `16.0.0`, and provides default, canonical, and compatibility caseless comparisons. It does not
  expose R5 output or remove default-ignorables.
- ICU4X `icu_normalizer 2.2.0` and `icu_casemap 2.2.0` are well-established and Unicode-3.0
  licensed; their default `compiled_data` features provide the four normalization forms and full
  locale-independent folding separately. No combined `NFKC_CF` mapping API or simple public
  Unicode-version constant was found. Composing these crates would still recreate the derived
  fixed-point/default-ignorable algorithm and add `icu_casemap` plus data dependencies.
- `utf8proc-sys 0.1.2` exposes the exact native `utf8proc_NFKC_Casefold` function, equivalent to
  `COMPOSE | COMPAT | CASEFOLD | IGNORE`. Its safe `utf8proc 0.1.2` wrapper exposes equivalent
  transform flags and runtime `unicode_version()`. The safe wrapper is MIT and defaults only to
  `inline-more`; the no-feature sys crate is `MIT AND Unicode-3.0`. The release pins mature C
  `utf8proc 2.11.0` with Unicode 17, but the Rust wrapper was first published in 2025 and had only
  1,268 registry downloads at inspection. It adds native C/static linking; the safe simple API
  also forbids `Allow` for unassigned scalars, whereas exact R5 preserves omitted unassigned
  scalars. This option needs the advanced/raw API, allocates a complete result, and does not meet
  the workspace's established-crate bar without explicit Operator approval.

The blocker is therefore limited and concrete: an off-the-shelf, established, current pure-Rust
R5 API was not found. Phase 75 can remain within existing policy by owning the pinned Unicode 17
mapping and using the established normalization crate for NFC. If owned generated data is outside
the authorized implementation slice, the dependency choice needs Operator direction rather than
substituting an inexact pipeline.

## Output bounds and streaming

The Unicode 17 `NFKC_CF` section explicitly maps 10,583 code points. An exhaustive parse of that
section found the largest single-scalar mapping at `U+FDFA`: 18 output scalars and 33 UTF-8 bytes
from one 3-byte input scalar, an 11x byte expansion. The transform can also delete all input.

A mapping iterator followed by `.nfc()` permits a checked output sink to count `char::len_utf8()`
and stop as soon as the applicable normalized-field byte ceiling would be exceeded. Reject
oversize output; never truncate it. Keep a separate input ceiling because canonical normalization
may buffer a pathological run of non-starters until a safe boundary. The Unicode stream-safe
process is not a workaround: it inserts `U+034F` and changes the required R5 key.

Do not assume a 64 KiB admitted path remains within 64 KiB after folding. Avoid unchecked
`11 * input_len` reservations; use checked arithmetic/`try_reserve` or incremental growth and the
authoritative output ceiling. A whole-result FFI API such as `utf8proc` cannot reject at the
output boundary before its native allocation.

## Conformance tests

Unicode 17 publishes no separate `NFKC_Casefold` conformance file. Use the two normative data
oracles that define its stages:

- Exhaustively test every valid Rust `char` against the Unicode 17 `NFKC_CF` section, treating
  omitted values as identity and then applying NFC. Check explicit empty mappings and the
  `Changes_When_NFKC_Casefolded` property as consistency checks.
- Run the complete Unicode 17 `NormalizationTest.txt` invariants against the pinned final-NFC
  implementation. UAX #15 conformance clause C3 requires those results.

Add catalog-level tests for the listed edge vectors, idempotence, publication/query byte identity,
and preservation of original visible text. Include boundary tests using repeated `U+FDFA`, long
non-starter sequences, an all-removed input, `U+2065` versus `U+0378`, and an output exactly at and
one byte beyond each field ceiling. Test that an entirely removed key follows an explicit catalog
policy instead of being confused with absent data.

## Unresolved risks

- Confirm whether the feature authority intends R5 directly or D147 identifier caseless matching
  with its leading NFD step.
- Decide the product behavior for a nonempty visible field whose derived key is empty. Unicode
  specifies the transform, not catalog matching semantics.
- Preserve the Unicode data copyright/license notice when checking in generated tables or test
  data, and verify the repository's third-party-notice convention.
- Unicode guarantees `NFKC_Casefold` stability across versions only for strings restricted to the
  version's `XID_Continue` repertoire. Titles and paths are not so restricted, so future data
  upgrades require explicit versioning and rebuild rather than an in-place dependency bump.
- Recompute the mapping expansion bound whenever the fixed Unicode version changes. The measured
  11x result is specific to Unicode 17 data.

# Sources

All external sources below are primary sources and were accessed 2026-08-01.

## Unicode specification and data

- [Unicode Standard 17.0.0, Chapter 3](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-3/)
  defines R4 full default folding, R5 `toNFKC_Casefold`, and D144-D147 caseless matching. The key
  text is in sections 3.13.3 and 3.13.5.
- [DerivedNormalizationProps-17.0.0.txt](https://www.unicode.org/Public/17.0.0/ucd/DerivedNormalizationProps.txt),
  dated 2025-01-27, is the normative `NFKC_CF` mapping and documents identity-by-omission, repeated
  derivation, default-ignorable removal, and required final NFC. It supplied all mapping vectors
  and the exhaustive expansion measurement.
- [CaseFolding-17.0.0.txt](https://www.unicode.org/Public/17.0.0/ucd/CaseFolding.txt), dated
  2025-07-30, defines `C`, `F`, `S`, and `T` entries and states that default full folding uses
  `C + F`, excludes `T` by default, may expand, and does not preserve normalization.
- [UAX #15 revision 57](https://www.unicode.org/reports/tr15/tr15-57.html), Unicode 17.0.0 dated
  2025-07-30, specifies NFC, final-string interaction, buffering, stream-safe CGJ insertion, and
  conformance clause UAX15-C3.
- [UAX #44 revision 36](https://www.unicode.org/reports/tr44/tr44-36.html), Unicode 17.0.0 dated
  2025-08-27, makes released UCD mappings stable, directs implementations to use explicit derived
  property listings instead of rederiving them, and enumerates the available conformance files.
- [NormalizationTest-17.0.0.txt](https://www.unicode.org/Public/17.0.0/ucd/NormalizationTest.txt),
  dated 2025-06-30, is the normative normalization conformance suite and defines the NFC column
  invariants.

## Rust crate sources

- [`unicode-normalization 0.1.25` package documentation](https://docs.rs/crate/unicode-normalization/0.1.25)
  records its maturity, `std` feature, `tinyvec` dependency, and `MIT OR Apache-2.0` license.
  [`UNICODE_VERSION`](https://docs.rs/unicode-normalization/0.1.25/unicode_normalization/constant.UNICODE_VERSION.html)
  is public; the published
  [generated table source](https://docs.rs/unicode-normalization/0.1.25/src/unicode_normalization/tables.rs.html)
  fixes it at `17.0.0`.
- [`unicode-casefold 0.2.0` registry page](https://crates.io/crates/unicode-casefold/0.2.0), its
  [iterator source](https://docs.rs/unicode-casefold/0.2.0/src/unicode_casefold/lib.rs.html), and
  [published table source](https://docs.rs/unicode-casefold/0.2.0/src/unicode_casefold/tables.rs.html)
  establish the license/features, full non-Turkic iterator API, and Unicode `9.0.0` data.
- [`caseless 0.2.2` registry page](https://crates.io/crates/caseless/0.2.2),
  [implementation source](https://docs.rs/caseless/0.2.2/src/caseless/lib.rs.html), and
  [published data source](https://docs.rs/caseless/0.2.2/src/caseless/case_folding_data.rs.html)
  establish its MIT/no-feature package, comparison pipelines, dependency, and Unicode `16.0.0`
  data.
- [`icu_normalizer 2.2.0` documentation](https://docs.rs/icu_normalizer/2.2.0/icu_normalizer/)
  lists only NFC, NFD, NFKC, and NFKD and its iterator adapter. The
  [`icu_casemap 2.2.0` case mapper](https://docs.rs/icu_casemap/2.2.0/icu_casemap/struct.CaseMapperBorrowed.html)
  documents separate locale-independent `fold`/`fold_string` APIs. Their official crates.io
  metadata supplies the Unicode-3.0 license and default `compiled_data` features.
- [`utf8proc-sys 0.1.2::utf8proc_NFKC_Casefold`](https://docs.rs/utf8proc-sys/0.1.2/utf8proc_sys/fn.utf8proc_NFKC_Casefold.html)
  documents the exact native flags. The wrapper's
  [`TransformOptions` source at `v0.1.2`](https://github.com/Techcable/utf8proc.rs/blob/v0.1.2/src/transform/options.rs)
  documents flags and the advanced-only unassigned-scalar option. Rust wrapper release
  [`v0.1.2`, commit `94f54438bbb770a5fa6dd146ebd669cab7b11215`](https://github.com/Techcable/utf8proc.rs/commit/94f54438bbb770a5fa6dd146ebd669cab7b11215)
  pins C submodule
  [`d7bf128df773c2a1a7242eb80e51e91a769fc985`](https://github.com/JuliaStrings/utf8proc/tree/d7bf128df773c2a1a7242eb80e51e91a769fc985),
  whose official release history identifies `utf8proc 2.11.0` as Unicode 17. Registry metadata
  supplied feature, license, publication, and download facts.

## Local sources inspected

- Root `Cargo.toml` and `Cargo.lock` for workspace policy and exact resolved dependencies.
- `crates/beryl-state/Cargo.toml`, `crates/beryl-state/src/catalog/value.rs`, and catalog codec
  sources for the current caller-normalized boundary and schema.
- `crates/beryl-state/doc/design.md`, `doc/features/conversation-threads/design.md`, and
  `doc/plan.md` for Phase 75 authority, shared implementation ownership, version declarations,
  visible-text preservation, and byte ceilings.
