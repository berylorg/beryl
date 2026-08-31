# Physical Repositories

This supplement is normative only for physical installed-theme and sidecar file layout,
publication inputs and ordering, bounded evidence, and watcher guarantees. It is governed by
[the package design](design.md), including that design's engineering-rigor contract. Theme
interpretation and product behavior remain with the
[theme-runtime system](../../../doc/systems/theme-runtime/design.md).

## Installed-Theme Repository

- The physical layout is `<beryl-home>/themes/manifest.toml` with user-editable stable documents at
  `<beryl-home>/themes/installed/<stable-theme-id>.toml`. A root-level `theme.toml` is never part of
  this repository.
- The API accepts package-neutral validated file identities, byte ranges, exact expected file and
  manifest identities, bounded staged byte streams, and explicit operation limits. The package
  derives stable relative filenames and never accepts an arbitrary relative or absolute path or
  exposes raw paths or handles.
- `manifest.toml` is Beryl-owned membership and ordering authority. An installed document is a
  supported external-edit input, while a file absent from the manifest is inert. This package does
  not parse theme documents, assign or interpret theme ids, names, or order, resolve appearance,
  arbitrate preview, or publish a theme generation.
- Readers observe one complete old or new manifest generation and only complete atomically replaced
  document files. Publication completely writes and flushes each staged file before atomic stable
  replacement and applies the home tier's directory-durability sequence.
- A new document is inert until manifest publication admits it. Beryl-authored in-place updates
  flush and atomically replace only the stable document. Membership, name, and order changes use
  manifest-last publication; delete first removes manifest authority, and later file removal or a
  retained file is non-authoritative.
- A terminal result proves non-publication, exact durable publication, or indeterminate owner-
  manifest publication and carries only bounded exact old/intended file identities, lengths,
  digests, and replacement evidence required by the caller's natural-record reconciliation. It
  cannot fabricate a repository generation or authorize retry, rollback, parsing, cleanup, or
  appearance publication.
- Cancellation may win only before repository admission. Admitted staging and publication drain to
  an exact result or targeted reconciliation. Temporary, staged, or unreferenced files remain inert
  after cancellation, failure, or process exit; routine open and reconciliation neither adopt nor
  guess that they are safe to delete.
- One bounded coalescing watcher emits only package-neutral stable-file hints, manifest-change hints,
  and overflow. Signals carry no path or bytes and are wakeups, never content, ordering, or commit
  evidence. Duplicate, reordered, and overflow signals require a bounded coherent refresh. Store
  failure, shutdown, and same-home recovery release the old generation's queue and subscription
  rather than adopting them.

## Sidecar Publication

- A sidecar path is
  `sidecars/<namespace>/<first-two-SHA-256-hex>/<full-SHA-256-hex>`. Typed durable metadata owns the
  namespace, SHA-256 digest, and exact byte length; admission and verification also require an
  explicit nonzero caller byte limit.
- Admission creates required digest-path directories, writes one unique temporary file, flushes its
  complete bytes, and renames without replacement to the final digest path where supported. On
  fully supported local NTFS it synchronizes each newly published directory link and the containing
  directory before an admission token returns; best-effort tiers perform the strongest available
  equivalent sequence.
- Fresh publication, existing reuse, and a concurrent no-replacement winner all verify the final
  file's exact length and SHA-256 digest. Reuse and a concurrent winner additionally compare the
  final file byte-for-byte with the exact staged source through caller-bounded pages; any difference
  is a collision invariant failure.
- `AdmittedSidecar` is an opaque token bound to the exact healthy store generation. The first
  metadata command referencing those bytes retains it through the batch and `SyncAll`; failure or
  an obsolete token cannot authorize metadata publication. Registered domains may use bounded
  `SidecarVerifier` reads to prove typed references name final files with the declared length and
  digest.
- Failed admission may leave an inert temporary or unreferenced final file. The package never
  deletes either and exposes no cleanup operation before a separately authorized home-wide garbage-
  collection design.
