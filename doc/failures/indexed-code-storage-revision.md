# Indexed Code Stored Revision Qualification

A native extent-replacement fixture must update the resource metadata, its owning projection's
source range and the projection-resource index together with the history head and summary.
Replacing metadata alone leaves the selected-history transport facing conflicting stored facts.

Run `102b82ca-e33e-41e0-a068-04e7ff9c3334` changed the Code metadata to ten lines but kept the
projection and resource index at the predecessor range/revision. The native range wait timed out:
the panel still exposed provider revision 5 and selected line 1 instead of requested revision 7
at the clamped origin. The native callback panic aborted the process; its startup log, rather
than the completion-only retained-fixture ledger, identified the exact temporary home for cleanup.

The corrected fixture publishes all five stored facts in one command, keeps the immutable
composer source, resource identity and block start, and supplies the exact shortened-range digest.
Run `d3336bbd-b071-4da0-874c-818c90989cf4` passed all 51 selected cases, including native shrink,
growth, ownership release and fresh acquisition. This qualifies provider-backed stored extent
replacement, not producer-driven source edits or complete storage-replay validation. Future
capture tests must use the admitted producer workflow instead of treating fixture replacement
as proof of that workflow.
