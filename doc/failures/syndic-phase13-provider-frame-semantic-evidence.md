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

The first attempted locator correction remained only a scheme, character, and percent-escape
filter. It called that shape an absolute URI without tracking authority, path, query, fragment, IP
literal, or delimiter grammar.

The first storage-integration pass also treated the structural result as sufficient for atomic
publication even though the streaming decoder validated and discarded two bounded facts: an
agent-message phase and a user-message submitted-content reference. Reconstructing either fact by
materializing an arbitrarily large frame would violate the same bounded-memory contract.

That pass also proposed retaining only the latest sealed frame plus cumulative history support.
After an item start followed by one or more deltas, that snapshot no longer contains the original
start timestamp needed to reject a completion timestamp that moves backward. Replaying the whole
item stream before every publication would be constant-resident but quadratic in the number of
frames and therefore is not an acceptable substitute.

# Evidence

`ProviderWebSearchActionV1::Other` must be retained because it is an observed pinned value, but it
cannot authorize complete history because the upstream `serde(other)` branch has already lost the
unknown action's required fields. The initial streaming validator accepted that tag and returned an
ordinary completed Web-search frame with no history-blocking evidence.

The first incremental JSON-ingress replacement then drifted in the opposite direction: its closed
Web-search action table rejected every unknown discriminator, so the observed pinned `Other` value
could no longer reach the history-blocking validator at all. Treating that rejection as stricter
schema validation would silently remove an accepted CAS 0.144.1 observation from the target.

A whitespace-prefixed `data:` value could also cross the typed dynamic-image locator boundary, and
a metadata value at the exact structured-depth ceiling could encode successfully but fail both
decoders.

Post-remediation review then proved that malformed locators such as `https://[`, `https://[xyz]`,
`https://host:port/image`, and `x:a#b#c` still passed both materialized and streaming validation.

The provider-frame publication design requires exact assistant-phase audit and exact correlation
between a provider user message and already sealed composer content. Kind, lifecycle, digest, and
text-span evidence alone cannot prove either relationship after restart.

# Why It Failed

Byte validity and lifecycle validity do not prove that captured history is complete. If semantic
support is recomputed only from a materialized value, arbitrarily large frames and reopen validation
cannot enforce the same rule without violating the bounded-memory architecture. Divergent encoder
and decoder validators likewise make a sealed frame impossible to replay authoritatively.

Likewise, a decoder that merely consumes bounded semantic fields is not enough when later atomic
publication and reopen must compare those fields with canonical authority. Discarding them would
force either unverified caller assertions or a second unbounded decode.

# Course Correction

Materialized and constant-resident frame paths carry the same typed history-support outcome.
Unsupported observations remain durably representable while their reason survives item-stream
validation and later publication, so no terminal audit can reinterpret them as complete.

Incremental backend ingress maps the pinned Web-search catch-all to one explicit typed `Other`
control. It then consumes the remainder of that action structurally through fixed state while
discarding its unknown payload bytes. Syndic retains the marker and
`UnsupportedRequiredPayload`, not an opaque container or raw JSON. This is the sole pinned lossy
exception and does not admit a generic unknown-variant fallback.

Typed image locator admission uses one shared fixed-state RFC 3986 component parser that requires a
nonempty valid non-`data:` absolute ASCII URI without decoding or generically scanning provider
text. It validates authority/user-info/host/port structure, bracketed IPv6 and IPvFuture literals,
path/query/fragment delimiters, and percent escapes identically across arbitrary input chunk
boundaries, including RFC-valid empty paths after a scheme. MCP image metadata counts the
already-consumed top-level object identically in encode,
bounded decode, and streaming decode. Standalone image-generation status is also a closed pinned
value whose lifecycle validity is checked in both materialized and streaming paths.

The constant-resident structural result also returns the exact optional agent-message phase and
the exact optional submitted composer-content reference. Publication and reopen can therefore
validate canonical phase and submitted-input correlation directly from the sealed frame without
copying or materializing its other payload.

Every sealed provider snapshot also carries one bounded resumable item-stream state: exact item
identity and kind, next frame ordinal, retained start timestamp, completion state, and cumulative
history support. The next frame advances that state with the same lifecycle validator used by
reopen, preserving constant-time incremental publication without weakening timestamp or
completion-only checks.

# Authority And Verification

Phase 13 of `doc/plan.md`, `crates/syndic-storage/doc/design.md`, and the CAS-live Syndic transcript
system design own the corrected boundary. Exact max-and-overflow depth tests, locator parity tests,
history-support round trips, image-generation lifecycle tests, large streaming validation, reopen
proofs, and terminal audit must preserve it.
