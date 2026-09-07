# Reason For Investigation

Audit ST-008 evaluated whether ICU4X could replace Beryl's generated Unicode 17 NFKC_Casefold table
and normalization adapter while preserving persisted catalog-title bytes.

# Outcome

Retain the current implementation. The inspected icu_normalizer 2.3.0 API offers NFC/NFKC, and
icu_casemap provides separate case mapping. Their composition has not been proved equivalent to
the exact version-pinned R5 transformation, including default-ignorable removal, full expansions,
ordering and canonical composition. Generic lowercase, NFKC or case-insensitive equality is
insufficient. A future adapter needs complete pinned UCD corpus equivalence and bounded expansion
evidence before any deletion estimate is credible.

ICU4X is maintained by the Unicode Consortium, licensed Unicode-3.0 and implemented in Rust with
MSRV 1.88. Normalizer defaults include compiled_data, utf8_iter and utf16_iter. Coordinated exact
component and baked-data versions would be required. This investigation did not qualify the
dependency closure or advisories and claims no measured savings or security clearance.

Beryl currently holds 10,583 mappings and 8,343 mapped values in a 160,384-byte Unicode 17 table,
then applies pinned unicode-normalization 0.1.25 NFC. The table generator reads NFKC_CF from
DerivedNormalizationProps. Existing default-ignorable, expansion, composition and table-digest
tests protect a durable contract. Changing normalized bytes requires an explicit format decision.

# Sources

- [Normalizer API](https://docs.rs/icu_normalizer/latest/icu_normalizer/struct.ComposingNormalizer.html)
  and [CaseMapper API](https://docs.rs/icu_casemap/latest/icu_casemap/struct.CaseMapper.html), inspected
  with 2.3.0 release metadata on 2026-09-06.
- [Tagged workspace manifest](https://raw.githubusercontent.com/unicode-org/icu4x/icu@2.3.0/Cargo.toml)
  and [normalizer manifest](https://raw.githubusercontent.com/unicode-org/icu4x/icu@2.3.0/components/normalizer/Cargo.toml).
- [Unicode-owned project](https://github.com/unicode-org/icu4x) and
  [current NFC dependency](https://docs.rs/unicode-normalization/0.1.25/unicode_normalization/).
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: catalog/normalization.rs,
  generated table, generator and catalog tests, all read by the state auditor. See
  [ST-008](../../../../audits/code-simplification/reviews/state-findings.json).
