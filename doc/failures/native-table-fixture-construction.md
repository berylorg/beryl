# Native Table Fixture Construction

A native fixture used a 120-row, twelve-column table and required its structural companion to
be ready within a fixed ten-second test wait. The Table descriptor, metadata and mount appeared,
but indexed reads remained Pending. Extending the wait or changing the constructor without
observing its state would have confused valid bounded work with a mounting failure.

A bounded read-only observation of the positively attributed retained fixture found exact Table
metadata, 54,224 source bytes and a Building companion at revision 124. The observation preserved
the home revision and released all frozen custody. The cell-construction pass processes one closed
row per step; the header and 120 body rows require at least 121 such steps in addition to row,
column and transition work. The fixed test wait was below that fixture's legitimate work.

The native fixture now uses 24 body rows and twelve columns. It still exceeds the admitted row
and column window and one 8,192-byte source page, preserving scrolling, demand, Copy and streamed
Save objectives without changing production construction or test timing. Clipboard-cap refusal
uses a separate complete-table selection case. Diagnose Pending through exact construction state
before changing timeouts, source producers or readiness behavior.

The read-only observation passed as `0a2282fd-79d2-4952-a77e-12a20f2be427`. Its temporary ignored
diagnostic case was removed after the last consumer; the retained outcome is the evidence.

Combined qualification exposed another fixture assumption: valid small Markdown Tables remain
inline until they exceed 8,192 source bytes, 32 body rows or twelve columns. A native indexed-panel
test cannot infer Table-resource mounting solely from valid table syntax. Padding a header past
8,192 bytes also exceeds the independent 4,096-byte resource-preview bound. Keep the original
short header and selected body/raw fallback row, and append bounded body padding rows to force
externalization. Complete-source expectations include those rows and still preserve CRLF.

A single 70,000-byte raw row exceeds the projector's 16,384-byte undecided-line bound and ends
the Table before that row. Native clipboard-cap qualification instead uses ten individually
bounded body rows whose complete source exceeds 65,536 bytes, selecting its complete resident
rectangle. The fixture uses two columns because the existing classifier requires more than one;
reducing three columns to two bounds irrelevant one-item read work without reducing the source
size or clipboard-refusal assertion. The separate native surplus-row case and focused raw-row-cap cases retain their
respective coverage without inventing a production-recognized huge fallback row.

The resize fixture also minimized its window before requesting client resize. The resolved GPUI
Windows implementation ignores minimized size notifications for viewport/renderer updates; moving
restore placement does not prove client remeasurement. The shared wait helper also minimized
after every poll, so bypassing only readiness's explicit minimization was insufficient. Keep the
resize case unminimized through every draw/wait while
the other cases retain best-effort minimization. This is a test visibility correction, not a
production geometry change or timeout extension.

The native quick-second Copy test waited for the first asynchronous clipboard write before its
second keypress. With source paging, that wait could exceed the shared 600 ms gesture interval,
making the expected stripped output invalid. First qualify original Markdown and held-repeat
behavior, then reset the pair without changing the rectangle and dispatch first, held and distinct
second presses together. Assert the first Copy is pending before the second press and that only
the superseding stripped intent publishes. Gesture timing must be established by input dispatch,
not assumed from worker completion.
