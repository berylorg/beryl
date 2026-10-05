# Captured Atomic Composer Paste Qualification

Status: accepted after independent consequential review, 2026-10-05. This is evidence for root plan phase 730.
Large-draft clipboard refusal preservation remains a separate acceptance boundary.

## Controlling Boundary

The [composer](../../features/composer/design.md#image-clipboard-semantics),
[private source system](../../systems/image-assets/design.md#private-composer-clipboard-sources),
[app ownership](../../../crates/beryl-app/doc/design-catalog-and-composer.md#clipboard-operation-ownership)
and [Syndic readiness](../../../crates/syndic-storage/doc/design-draft-storage.md#marker-readiness-inputs)
control the implementation. Paste captures the exact selected destination before asynchronous
acquisition and completes through one general mutation with ordinary settlement and history.

The selected composer consumes Paste, retains its immutable checked snapshot and directed
replacement intent, and gates dependent actions until an exact terminal disposition. Private
source selectors require complete current token/text correlation and serialized origin admission.
Observable fallback mismatch expires only the captured matching source. Committed begin replay
uses immutable admitted evidence even after source expiry or origin retirement.

Queue capacity and sidecar page size are validated process-service configuration carried through
startup and recovery. Queue admission precedes clipboard acquisition. Worker and staging custody
retain permits through cancellation, stale completion and disposal until their work drains.
Image sidecars stream through bounded pages; asset admission precedes draft mutation. Fresh marker
identities follow the application's random domain-ID convention and remain stable across replay.

## Verification Scope

The canonical checkout excludes ignored local Cargo overrides. GPUI remains at
`edd4928c5be424630da49f872e00dafbf94cf0b2`, scrollbar at
`b9e591820b61fc788f148bca1f6f80b6341a692c`, text input at
`f9651f8909f67f52f322b78547cb22f297572c0a` and Settings at
`dddd30ad461cecf4d7176a2d6bad4b0e1fa0f962`, with one identity for each widget.

Checks use the locked graph, one Cargo job, LLVM linking, no normal debug information,
nonincremental compilation and serial nextest. Mounted tests use isolated GPUI contexts and
injected checked clipboard readers/writers. The accepted selection excludes native-window
harnesses and Operator clipboard access. Bounded local logs live under
`.tmp/captured-composer-paste-evidence`.

## Prerequisites And Corrections

Bounded home-store sidecar qualification passed 32/32 cases, run
`599d8a31-51d2-490a-9b27-9b47bf625382`. Streaming checks cover exact length and digest,
reopened-source consistency, cancellation, reuse and publication barriers.

The published text-input provenance replay prerequisite passed its canonical all-target check,
24/24 cases and independent review, run `a932f515-0aab-4ed3-98a1-bfc40ba64f35`.
The Settings pin alignment passed canonical checks and independent review. The
[replay termination correction](../../failures/clipboard-provenance-replay-termination.md)
records why a provenance collision must terminalize its owning wrapper.

Review corrected resettable marker IDs, lazy resource configuration, private fallback mismatch
expiry and flattened storage refusal. See the
[implementation correction record](../../failures/composer-paste-identities-and-resource-config.md).
These corrections preserve the existing product and system authority.

The [test qualification correction](../../failures/composer-paste-test-qualification.md)
records an overbroad initial selector that reached a native-window harness and aborted after 83
passes, and storage fixtures that incorrectly expected reads/retry on a failed home generation.
The native harness is excluded from the focused acceptance run; its exact abandoned fixture was
reclaimed. Physical storage refusal checks retain the editor and typed Notice while respecting
ordinary failed-generation read rejection.

## Completion Evidence

All 49 source, test and manifest inputs match the canonical checkout by SHA-256, independently
recomputed. Exact-path formatting and scoped whitespace checks pass.

The final captured/image run passed 24/24 cases, run
`6d8079e8-7efe-48aa-95dd-01cb82567ba1`: all 21 mounted captured cases and three bounded image
header, streaming admission/reuse and size/cancellation cases. This includes persisted-root
restart identity, local/foreign labels, exact refusal and noncommit, correlation expiry, ambiguity,
Escape before/after durable begin, directed text replacement, one-step history and queue drain.

Twenty-nine unchanged host evidence, mounted copy/cut, selection-routing and Notice cases passed
in run `d1ede89d-e488-486e-9345-9ec82c36030e`. Seven unchanged pure service-graph/startup and
recovery cases passed in run `c97cc565-e569-4abc-9541-df342366380e`. These results qualify 60
distinct app cases through bounded reuse of unchanged inputs. The final continuation correction
also passed all 18 targeted host/private cases in run `df76ad2e-b035-4508-a88c-b0bb58966ecd`.

The final canonical locked app/executable all-target check with both fault-feature sets passed
in 1m25s. Independent consequential review accepted the complete capture, preparation,
evidence/staging, origin fencing, cancellation, settlement, retirement/recovery, resource,
Notifications and history boundary, with no remaining blocking findings. Earlier failed artifacts
are diagnostic history, not acceptance evidence. Large-draft clipboard refusal remains separate.
