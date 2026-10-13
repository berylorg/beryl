# Transcript Narrative Publication Election

The initial narrative publication callback repeated complete request validation while running
inside the home store's coherent election. Successful publication stalled in
`narrative_pending_seed_prepares_only_after_publication_and_adopts_exact_source`; rejected late
results passed because they never entered that callback.

The health election holds its mutex through the callback. Complete validation calls
`generation_identity`, which reacquires that mutex. The callback must therefore perform only
the existing non-reading request and cancellation check. Complete identity validation remains
before election, and election authenticates the expected home generation and observation.

The implementation now follows the resource publication pattern. The successful publication
test, rejection tests and native delivery must verify this correction before mounting acceptance.
No product or storage authority changed.
