# Fragment Hit Ownership Does Not Cover Every Selection Position

The mounted indexed Code selection consumer initially accepted only direct
`StreamingLayoutHit::Gap` results, with an exact trailing-coordinate fallback. That assumed a
fragment's hit API supplied a source caret throughout its visible extent.

Independent review disproved the assumption in the owned GPUI streaming fragments. Text fragments
return no hit when the nearest caret is their shared trailing endpoint. Oversize atoms likewise
leave their trailing side to adjacent ownership; inline objects may return an object identity
instead of a source gap. An adjacent fragment rejects coordinates before its leading edge, so
iterating direct hits alone leaves ordinary glyph tails and atom interiors unselectable. Exact
coordinate equality and a final-line boundary do not fill those gaps.

The bounded consumer keeps direct gap ownership preferred and resolves non-owned interiors from
the fragment's admitted leading and trailing source maps. Atom interiors use the nearer source
edge with a leading-side tie; text tails use their admitted trailing endpoint. Typed hit errors
remain failures. No pixel-to-byte ratio, source reconstruction or retained offscreen geometry is
needed. Selection highlighting uses the same admitted source maps.

A completed prepared line can have no separate Boundary fragment. Resolve an end-of-line click
from its final admitted map only when that map reaches the exact completed terminal width;
otherwise a realized fragment edge is not proof of the source's terminal endpoint.

The regressions `code_source_hits_cover_tabs_oversize_atoms_and_terminal_boundaries` and
`code_text_hit_owns_last_glyph_tails_at_canonical_and_terminal_boundaries` cover atom halves and
text tails at canonical and terminal boundaries. Their acceptance results remain in the active
[plan](../plan.md). The [Code layout contract](../systems/transcript-presentation/renderer-architecture.md#indexed-no-wrap-code-layout)
and [widget contract](../gui/widgets/code-panel/spec.md) remain authoritative. Future consumers
must distinguish unique fragment ownership from nearest-source selection resolution.
