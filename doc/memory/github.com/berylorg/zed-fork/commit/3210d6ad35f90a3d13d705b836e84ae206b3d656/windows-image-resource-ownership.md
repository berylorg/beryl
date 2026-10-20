# Reason For Investigation

Asset readiness needed to determine whether the pinned GPUI image path could implement bounded
original-image decoding and shared process GPU residency for Beryl's composer and transcript.

# Outcome

The current Windows path shares a DirectX device, but each `DirectXRenderer::new` creates its own
`Arc<DirectXImageResources>`. `WindowsWindow::image_resources` exposes that renderer-local store
internally, and each GPUI Window also owns a `SourceBackedImageStore`. A shared device therefore
does not supply a shared texture cache, one process budget or cross-window pin ownership.

`prepare_image_upload_blocking` uses `fs::read` for file sources. `prepare_raster_bytes` decodes a
full source frame before fitting or downsampling. Its existing source-pixel limit is useful but
does not supply Beryl's preflighted thumbnail/tile output and process surface reservations.
Calling `img(path)` cannot establish that stronger boundary.

`ImageResource`, `PlatformImageResources` and `Window::paint_image_resource` are crate-private.
Prepared-upload fields are private and current construction follows the preparation path above.
The App has no public seam for uploading an already admitted bounded pixel surface and sharing
an opaque renderer resource across windows with one charged lifetime.

Device recovery calls each renderer-local resource store's `handle_device_lost`, replacing its
device/context and clearing its map. The inspected store has no explicit upload-generation token
that a late App completion can validate. Existing renderer release behavior is real, but it does
not establish the separate shared-generation contract required by Beryl's image system.

The pinned `image_render_resources` tests exercise genuine standalone upload, clipped demand,
pending cancellation, budget rejection/release and resource-clear reload within one window.
They do not qualify a new shared App CPU/staging/GPU owner, multiple-window pins or late device-
generation rejection. Earlier City/App image evidence at another commit remains historical.

# Sources

- Repository: <https://github.com/berylorg/zed-fork>, resolved commit
  `3210d6ad35f90a3d13d705b836e84ae206b3d656`, accessed 2026-10-21.
- Beryl root `Cargo.toml` pins this Git revision with default features disabled and
  `windows-manifest` enabled. `git rev-parse HEAD` confirmed the inspected local source; scoped
  `git diff --name-only` reported no changes to the source files below.
- `crates/gpui/src/platform/windows/platform.rs`, `WindowsPlatformState` and `DirectXDevices`:
  shared platform device ownership.
- `crates/gpui/src/platform/windows/directx_renderer.rs`, renderer field, constructor and device
  recovery: renderer-local image-resource allocation and recovery call.
- `crates/gpui/src/platform/windows/directx_image_resources.rs`, `handle_device_lost` and platform
  resource implementation: device replacement, map clearing and current resource scope.
- `crates/gpui/src/platform/windows/window.rs`, `image_resources`; `crates/gpui/src/window.rs`,
  Window construction, source-backed store and image paint boundary.
- `crates/gpui/src/elements/image_prepare.rs`, `prepare_image_upload_blocking` and
  `prepare_raster_bytes`: whole-file acquisition and full-frame decode before fitting.
- `crates/gpui/src/platform.rs`: current platform image-resource visibility.
- `crates/gpui/tests/image_render_resources.rs`: existing bounded standalone image lifecycle cases;
  inspected as test-source inventory, without claiming a fresh execution.
