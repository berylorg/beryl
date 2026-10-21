# Bounded Image Rendition Qualification

## Boundary And Inputs

This records implementation qualification for the shared
[rendition contract](../systems/image-assets/renditions.md), App's
[source-to-paint ownership](../../crates/beryl-app/doc/design-image-processing.md) and the owned
[GPUI resource contract](../../../zed-fork/doc/design-image-resources.md).
The final mounted and component receipts cover the implemented source-to-paint and retirement
boundary; independent assembled acceptance is recorded below.

The coherent Git dependency cohort is GPUI `93f0584c01b6b4fd09409d0e1ee502535c020b5b`,
scrollbar `a2596519d98d0b9f04ecc4e4bb21d4ac77231a59`, text input
`d6fba6a83e1b5011b204d0c97751f419355bb50f` and Settings
`3400fed9de82ec3c99f13d7d4718be3843c2548d`. All three widget library checks and their locked
metadata validation passed. Beryl's production library check passed in 38.79 seconds against this
Git cohort with no local dependency patch; locked metadata validation passed before restarting
the language server. The lock resolves one GPUI and one revision of each owned consumer.

The canonical checkout uses the shared target with one Cargo job, debug information disabled and
test optimization level zero. Native tests use Windows manifest support and process-local Windows
error mode `0x8003`; this suppresses system crash dialogs without changing product behavior.

## Component Evidence

The final clean GPUI run `d51c7890-272d-42f9-92bc-4eff25c647de` passed all thirteen cases
(three public-boundary and ten native cases) in 9.091 seconds. Its nineteen source/test inputs match
the committed GPUI implementation. The source manifest SHA-256 is
`9B2EE1FDD7DA16D673A7EC9B07BE7AE32F83B74344925DC61452291DEAAB9E3B`, the five-input model
manifest is `EE5EAF804AE91BB032B26298994A0E8088295A9F4F37C944A69748B098A61A13` and the raw
test log is `1554DC2166AC4FA9528EC15609CB182E9B700749B6AE4BCCB195EE5D4EFA8A02`.
An independent reviewer verified these inputs and accepted the component before its prerequisite
commit. This run used an isolated harness and temporary local patch; Beryl's final pin adoption is
qualified separately.

These native cases cover real allocation ownership, two-window sharing and unique pins,
frame/native completion custody, eviction with pinned peers, consumer retirement, same-window
scope renewal, late upload refusal and availability/recovery notifications including an empty
domain. The [retirement failure record](../failures/admitted-image-native-retirement.md) preserves
the actual submitted-work accounting correction and the evidence behind the functional wait.

The prior focused App run `64a9973c-a35c-4076-b4bc-70d8565865bf` passed twenty-four cases:
authenticated source admission, exact occurrence and Home scope, publication fencing, PNG framing,
CRC and physical EOF, bounded row fitting, cancellation, capacity ownership, CPU eviction, exact
native identity and bounded recovery notification/exhaustion. Home run
`18d4c535-82a8-49ce-a802-bd86fd73a1ed` passed nineteen verified-reader, streaming and scope cases.
Native App run `7c6b00dd-44cc-4cd1-91b6-a53605c1514c` passed both shared-flight/staging-custody
and configured source-stage lease cases.

Against the final pinned Git cohort, App run `8ebcdaee-6227-45a9-9da9-3e6c819cf3a1` passed all
twenty-six focused and native ownership cases; the separately selected mounted case failed its
composer-menu wait after passing transcript paint/recovery. This is a partial acceptance receipt,
not a successful whole run. Its raw log SHA-256 is
`7F63008D32204CC6DABE5E0CB8752A20F8899A2719250ECFACEC69727A8778CF`.
Final pinned Home run `7c2158a5-09ac-4941-9221-763c982a4ce9` passed all nineteen cases in
1.470 seconds; its raw log SHA-256 is
`00EEA7FDAA218418E64587011270128CE7454806DB6D901D40B781FC102E6703`.

## Mounted And Integrated Evidence

Final pinned mounted App run `0e2b4ad3-2221-4065-b7c2-1e9257a53fe9` passed
`native_mounted_image_renditions_paint_in_transcript_and_composer_and_release_on_close` in
5.235 seconds. It demonstrated actual Ready paint plus RGBA readback for both transcript and
composer marker occurrences, exact marker identity, production-driven lazy recovery for both a
previously Ready and a never-pinned interest, cancellation without revival, thread-switch
invalidation and zero CPU/native/staging/worker media ledgers before disposing the original graph.
Recovery used the production maintenance notification and new authenticated ordinary requests,
without a manual App progress pulse. Cached CPU pixels survived the tested generation change
without another decode. The fixture uses actual painted composer/window coordinates and real
pointer events, separating menu mount, View and Escape across callbacks.

The mounted raw log SHA-256 is
`B36E0F83A70608C5B3D342F1266B8953AE3D999378EC3E0B3941E43906E18117`; its source/model manifest
is `BAAEE075F23BC51F32EA7B1F6BB26083EF14E1DC441BF8D5A161C72EA8E11FCD`. All manifest files
were rehashed after execution with zero changes in both the source and canonical checkouts.
The exact focused union comprises fifty-nine distinct cases across these scoped runs; this is
not a whole-workspace test claim or a single combined successful run.

Independent assembled source review found no remaining source blocker, including graph replacement,
startup/final shutdown, foreground native maintenance, recovery fanout and the observational
readback accessors. Independent final review accepted the complete declared bounded rendition
boundary, verified all final sixty-three manifest files in both checkouts with zero mismatches,
and confirmed that only the mounted fixture changed between the final App receipts. The twenty-six
earlier passing cases remain reusable. No unmet guarantee remained within this boundary.

Task-owned raw runs and ledgered fixture Homes were retained through final review and then
reclaimed. The canonical checkout and shared build target remain available for subsequent work;
unrelated working-tree changes and resources were preserved.

## Evidence Limits

Device faults are logical device-owner loss/recovery; no physical OS device-removal claim is made.
Two-window sharing is native component evidence rather than a two-window App workflow. The mounted
case explicitly drains the original media owners before fixture disposal; it does not exercise
ordinary Exit or Home replacement end to end. Actual native window removal paths are exercised
without independent HWND enumeration.

PNG allocation evidence combines bounded fixtures, charged caller-owned capacities and exact
[decoder/allocation source analysis](../memory/crates.io/png/0.18.1/streamed-fitted-rendition.md).
It establishes the documented finite workspace model, without universal allocator instrumentation
or whole-process RSS bounds. Driver-private allocations are outside the RGBA texture estimate.
Original-image Copy/Save and generated-media finalization remain later implementation boundaries.
