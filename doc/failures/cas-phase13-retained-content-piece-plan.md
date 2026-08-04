# Phase 13 Retained Content-Piece Plan

An implementation exploration proposed preparing streamed `turn/start` input by retaining a vector
of content-range segments while scanning Syndic `ContentPieceRecord` values.

That shape is not bounded by image markers. Syndic records a text piece/span for each physical
content chunk, so a text-only multi-million-token draft could produce an input-sized resident plan
even though no text bytes were assembled.

The corrected plan scans pieces through bounded cursor pages and retains only marker-bounded logical
content ranges plus generated image-label fragments. Absolute source reads locate and validate the
underlying physical spans again in bounded pages. Descriptor memory is therefore controlled by the
existing marker bound rather than logical text length or physical chunk count.

The controlling system and app package design documents explicitly forbid per-chunk prepared
descriptors.
