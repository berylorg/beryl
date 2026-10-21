# Admitted Image Native Retirement

The first shared GPUI image implementation treated zero pin, frame and staging counts as sufficient
evidence to remove a native texture and its charge. Independent source review invalidated that
assumption during Phase807: `DirectXRenderer::draw_image_sprites` bound the admitted view through
`draw_with_texture`, while domain collection considered only Rust ownership counters. DirectX
shader-resource bindings retain interfaces, and submitted commands/deferred destruction can outlive
those counters.

A concrete failing sequence is painting an admitted image as the last textured primitive, replacing
the retained scene with empty/quads content, dropping the final pin and retiring the consumer. With
no later textured draw or context reset, the old native view may remain retained even though the
logical ledger reports zero. The initial passing counter tests did not establish native release.

The correction unbinds every used shader slot, retains retiring allocations and their charges,
and proves completion with a preallocated event query on the original context before releasing
views/textures and flushing deferred destruction. Partial allocation follows the same custody.
Foreground maintenance operates without windows, clears obsolete retained scene guards without
reindexing primitives, and reports native failure while preserving ownership. Fresh consumer scopes
on a surviving window require the old consumer's complete drain.

Maintenance notifications must cover device availability/generation changes even with no entries.
An unavailable interval can outlive old-entry drain; a later recovery then has no retiring entry
or completion failure to trigger a wake. Manually pulsing App progress in a mounted test hides that
gap. Qualification must use the production maintenance notification to resume visible interests.

The native qualification's initial three-second drain deadline also proved too short. A controlled
observation retained one 16-byte texture at 3.011 seconds with no pins, frames or staging owners,
a healthy original device (`S_OK`) and an incomplete event (`S_FALSE`); that same event completed
at 3.991 seconds and the original charge drained. The test's cross-FFI assertion produced a Windows
abort status, which did not establish a memory overwrite. The correction changes only the bounded
functional test wait; production still requires actual completion. These observations do not define
a GPU completion latency guarantee.

The owning contracts are the fork's `doc/design-image-resources.md` and App's
[image processing supplement](../../crates/beryl-app/doc/design-image-processing.md).
Qualification must exercise the actual renderer/context owners and the last-image-to-empty and
zero-window sequences. Logical generation invalidation is not proof of physical device removal.
The [assembled qualification](../audits/bounded-image-rendition-qualification.md) records final
accepted source/native evidence and its limits.

The native semantics are supported by Microsoft's
[shader-resource binding](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-pssetshaderresources),
[asynchronous Flush/deferred destruction](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-flush)
and [GetData completion](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata)
contracts, inspected 2026-10-21. Completion distinguishes exact `S_OK` from `S_FALSE`.
