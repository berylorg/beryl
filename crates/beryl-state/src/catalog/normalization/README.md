# Unicode NFKC Casefold Data

`unicode-17-nfkc-cf.bin` is the compact catalog-owned form of the normative Unicode 17
`NFKC_CF` property. It is used only for Unicode R5 `toNFKC_Casefold`; the final NFC pass is
provided by the exactly pinned `unicode-normalization` dependency.

The source is Unicode 17
[`DerivedNormalizationProps.txt`](https://www.unicode.org/Public/17.0.0/ucd/DerivedNormalizationProps.txt),
whose SHA-256 digest is
`71FD6A206A2C0CDD41FEB6B7F656AA31091DB45E9CEDC926985D718397F9E488`.

The generated binary SHA-256 digest is
`62B4A3BE942FC9EBB13042F2553E4821013F4E0C625B100AF4F35DBB90A38EDB` and contains 10,583
explicit mappings. Omitted Unicode scalars map to themselves; explicit empty mappings emit
nothing.

Regenerate from `crates/beryl-state` with a temporary executable:

```text
rustc --edition 2024 tools/generate_nfkc_cf.rs -o <temporary-executable>
<temporary-executable> <DerivedNormalizationProps.txt> src/catalog/normalization/unicode-17-nfkc-cf.bin
```

The derived data remains covered by [UNICODE-LICENSE.txt](UNICODE-LICENSE.txt).
