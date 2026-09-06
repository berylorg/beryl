# GPUI Mounted Scene Verification

## Invalidated Assumption

The Phase 287 appearance tests assumed a snapshot after `Window::draw` would retain the painted
scene and that the test text system would supply visible glyph raster bounds. Both assumptions
produced empty paint evidence despite the ordinary widget paint path executing.

## Evidence And Correction

- `../zed-fork/crates/gpui/src/window.rs` swaps the completed frame and clears `next_frame` in
  `Window::draw`; `paint_snapshot_for_test` reads `next_frame`. Capture inside the paint pass using
  a canvas sibling after the ordinary mounted child, rather than after `draw` returns.
- `NoopTextSystem::glyph_raster_bounds` in `../zed-fork/crates/gpui/src/platform.rs` returns zero
  bounds. `Window::paint_glyph` omits zero-size sprites. Seed the required font, size, glyph, and
  scale combinations through the existing `set_glyph_raster_bounds_for_test` API before painting,
  as demonstrated by `../zed-fork/crates/gpui/tests/streaming_text_layout/live_paint.rs`.

The accepted fixture is `../gpui-text-input/tests/range_widget/appearance.rs`. It observes ordinary
mounted editor paint and preserves the distinction between scene evidence and GPU presentation;
it adds no production observation seam or alternate fragment renderer. Its four appearance cases
and the complete 104-case range-widget target passed in explicit local Cargo mode.
