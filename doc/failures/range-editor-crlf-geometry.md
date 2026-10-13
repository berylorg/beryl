# Exact Range Editor CRLF Geometry

Native mapped Quote exposed a range-backed layout assumption about line delimiters. Inserting
the exact selected Markdown containing CRLF committed the correct 52-byte draft and successor
positions, but geometry rejected the new segment and retained the preceding editor surface.
Quiescence and correct draft length therefore did not establish usable successor geometry.

The native Quote test in nextest run `f8a627af-b92f-4467-8ad4-21aefcd8bdc1` reproduced the
readiness failure. A subsequent bounded diagnostic showed the current binding at revision 7,
the retained surface at revision 6, and `ExactGeometryLayoutInvalidSegment` during finalization.
Drawing genuine frames was necessary for the test's layout synchronization but did not correct
the rejection.

Unicode segmentation represents CRLF as one grapheme. The owned text-input fork's
`range_geometry/exact/scan.rs::complete_grapheme` recognized only a lone LF as a line delimiter,
so it passed CRLF as ordinary segment text. GPUI correctly rejects ordinary segments containing
LF; its existing logical-line finalization already accepts a nonempty two-byte source delimiter.

The correction recognizes LF and CRLF in both delimiter admission conditions and retains the
exact original delimiter range. It neither normalizes source bytes nor weakens GPUI validation.
Delimiter recognition retains at most two bytes even at the supported one-byte text-segment
limit; ordinary text limits and existing allocation admission remain unchanged. Whole-page,
split-page and minimum-segment regressions require current coherent geometry, exact delimiter
ranges including trailing CRLF, line advance, terminal caret mapping, preserved lone carriage
returns and no response rejection.

The corrected native Quote workflow passed in app run
`08380955-0e90-40fc-af32-19844cbcb59d`. All 132 widget range tests passed in the corrected run;
the source is published as widget revision `1366c97b0e56c63fee12e0bd0dddb888bb8bad14`.
