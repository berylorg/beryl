# Image Memory Investigation

## Invalid Preload Size: Outer Tile Instead Of Image Content

Phase 11 live testing showed that Beryl transcript preloads initially requested the outer media tile size, while GPUI visible `img()` rendering requested the inner image content box after the tile border had been applied by layout.

The symptom was duplicate source-backed resources in the City Image Generation thread: preload requests were about `629 x 419` device pixels, while visible final-scene requests for the same sources were about `625 x 416` or `626 x 416`.

Course adjustment: Beryl preload sizing must use the same image content box as the visible `img()` child, not the decorated media tile bounds. The implementation now subtracts the media tile border before converting the preload size to device pixels.

## Invalid Request Identity Assumption: Exact Pixels Are Too Fine

After correcting the media content size, City live testing still showed first-frame duplicate resources for some images. GPUI/Taffy snapped equivalent visible children to neighboring device-pixel widths, so a preload request at `626 x 416` and a visible request at `625 x 416` could be queued in the same frame before either resource existed.

A live-resource reuse check was not enough because the duplicate decode/upload work was already scheduled by the time any resource became live.

Course adjustment: GPUI source-backed image request identity must absorb subpixel layout jitter before decode/upload work is queued. The implementation now canonicalizes source-backed request dimensions to an even device-pixel bucket and can also reuse an already-live same-source resource within a small device-pixel tolerance.

## Invalid Preload Filter Placement: Filtering After Media Lookup Is Too Late

The Phase 11 preload implementation initially filtered to loaded source-backed file images inside `preload_media_item`, after `TranscriptMediaRenderContext::media_for` had already looked up each media item. That meant Markdown images or inline byte-backed generated-image fallbacks in the preload band could schedule backend file reads or retained-byte media-cache work even though overscan preload is meant to prepare only source-backed GPU resources.

Course adjustment: preload runs now return before media lookup unless every item in the run is a native generated image with a non-empty `savedPath`. Visible rendering still uses the ordinary media-cache path for Markdown and byte-backed media, but overscan preloading cannot wake those loads solely because they are near the viewport.
