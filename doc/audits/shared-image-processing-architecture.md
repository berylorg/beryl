# Shared Image Processing Architecture

## Boundary And Authority

This is architecture-readiness evidence for the asset-backed rendition implementation. It is not
runtime qualification. The accepted [asset inventory](asset-processing-readiness.md) supplies the
existing/missing source map; actual Ready preview consumers remain implementation work.

- The [image system](../systems/image-assets/design.md) routes to its normative
  [rendition contract](../systems/image-assets/renditions.md): source authority, PNG subset,
  configuration values, stage accounting, pins, graph retirement and device loss.
- The [App entry point](../../crates/beryl-app/doc/design.md) routes to its normative
  [image-processing boundary](../../crates/beryl-app/doc/design-image-processing.md): process CPU
  owner, graph attachments, background source resolution, workers and existing feature mounts.
- The owned [GPUI entry point](../../../zed-fork/doc/design.md) routes to its normative
  [admitted-image boundary](../../../zed-fork/doc/design-image-resources.md): shared native domain,
  checked pixel/staging upload, unique window pins, frame references and device generation.

Product marker identity, visible states, focus and independent original-image commands remain
owned by the [image feature](../features/image-assets/design.md) and its GUI specifications.
Existing storage/reference contracts and renderer cleanup remain in their owning entry points.
Selective routes for image implementation and GPUI integration require only the corresponding
entry point, this bounded supplement and its named higher-level contracts.

## Evidence And Limitations

The pinned [GPUI investigation](../memory/github.com/berylorg/zed-fork/commit/3210d6ad35f90a3d13d705b836e84ae206b3d656/windows-image-resource-ownership.md)
establishes renderer-local texture ownership and full-source/full-frame preparation at the runtime
dependency revision. The target public domain replaces those assumptions for canonical Beryl media;
it does not claim that a shared DirectX device already provides a shared cache.
Owned GPUI target authority is committed/pushed at `18a7e7402e8071a85dfe69632a20431d3e643f50`;
the runtime pin still identifies `3210d6ad35f90a3d13d705b836e84ae206b3d656` until implementation.

The exact [PNG investigation](../memory/crates.io/png/0.18.1/streamed-fitted-rendition.md) supports
incremental static RGB/RGBA8 rows into a fitted output with width/chunk-bounded requested workspace.
The design explicitly requires allocation-growth qualification before implementation acceptance.
Neither `png::Limits::bytes` as a total private-heap cap nor strict zlib-trailer conformance is
claimed. Full sidecar identity verification and chunk CRC/IEND/physical-EOF preflight are separate
from successful decoder row/finish calls. No PNG decoder was executed for this architecture phase.

The effective contracts remain image-system/App `production-application/v2`, the image feature's
declared external-effect/irreversible modifiers, and GPUI `production-application/v1`. No rigor
upgrade or software installation was made. No Cargo manifests, runtime source, test fixtures or
GUI layout were changed. Runtime dependency pins remain unchanged until the implementation adopts
and qualifies the corresponding native changes.

## Review And Verification

Root verified normative routing, existing relative links and heading spacing for the eight owning
and supporting documents, plus scoped whitespace checks. Independent review covers the complete
cross-package source-to-paint, accounting, replacement and retirement contract. Its correction
places GUI occurrence capture in the feature adapters and Syndic/State reads in the admitted
attachment worker/source provider, preserving the existing no-storage-in-render boundary.

Independent complete-boundary review accepted the architecture, including the corrected background
source boundary and explicit whole-consumer native drain/fresh epoch. No material architecture
choice remains unresolved for the rendition implementation. Implementation acceptance must subsequently
prove mounted asset-backed pixels, actual two-window texture sharing, private decoder workspace,
capacity refusals, cancellation/stale completions, frame-safe release and shared device-loss/lazy
re-upload. Architecture review alone supplies none of that runtime evidence.
