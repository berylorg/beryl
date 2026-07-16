# Scope

Phase 13 typed provider-frame encoding, constant-resident validation, and later history-complete
publication in `syndic-storage`.

# Invalidated Approach

The first pure frame codec treated successful structural decoding as sufficient semantic evidence.
Its constant-resident result retained frame identity, kind, lifecycle, digest, and text-span facts,
but not whether an admitted typed value represented an unsupported public payload.

Related validation paths also drifted: typed image locators used only a literal five-byte prefix
check, and MCP inline-image metadata counted its enclosing object differently between encoding and
decoding.

# Evidence

`ProviderWebSearchActionV1::Other` must be retained because it is an observed pinned value, but it
cannot authorize complete history because the upstream `serde(other)` branch has already lost the
unknown action's required fields. The initial streaming validator accepted that tag and returned an
ordinary completed Web-search frame with no history-blocking evidence.

A whitespace-prefixed `data:` value could also cross the typed dynamic-image locator boundary, and
a metadata value at the exact structured-depth ceiling could encode successfully but fail both
decoders.

# Why It Failed

Byte validity and lifecycle validity do not prove that captured history is complete. If semantic
support is recomputed only from a materialized value, arbitrarily large frames and reopen validation
cannot enforce the same rule without violating the bounded-memory architecture. Divergent encoder
and decoder validators likewise make a sealed frame impossible to replay authoritatively.

# Course Correction

Materialized and constant-resident frame paths carry the same typed history-support outcome.
Unsupported observations remain durably representable while their reason survives item-stream
validation and later publication, so no terminal audit can reinterpret them as complete.

Typed image locator admission uses one shared semantic rule that requires a nonempty valid
non-`data:` locator without decoding or generically scanning provider text. MCP image metadata
counts the already-consumed top-level object identically in encode, bounded decode, and streaming
decode. Standalone image-generation status is also a closed pinned value whose lifecycle validity is
checked in both materialized and streaming paths.

# Authority And Verification

Phase 13 of `doc/plan.md`, `crates/syndic-storage/doc/design.md`, and the CAS-live Syndic transcript
system design own the corrected boundary. Exact max-and-overflow depth tests, locator parity tests,
history-support round trips, image-generation lifecycle tests, large streaming validation, reopen
proofs, and terminal audit must preserve it.
