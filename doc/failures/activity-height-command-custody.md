# Activity Height Command Custody

The first Activity mount executed height writes inside its cancellable selected-window read loop.
Although indeterminate reconciliation was installed synchronously, cancelling that loop could
discard the original result, including committed local-finalization capability. A subsequent read
failure or save could also overwrite the last window-owned outcome. Independent source review
identified this before mounting acceptance.

Admitted height writes now reserve bounded process-owned outcome custody before execution, separate
from read publication. Source execution stores the original result before returning across any
asynchronous boundary. Pending, indeterminate and committed local-finalization outcomes prevent
replacement. Full store-instance/generation identity qualifies recovery retirement; a generation
counter alone cannot identify a reopened store. Pending old-generation custody remains until its
execution finishes. Settled ordinary outcomes can be reclaimed, so the window limit does not
become a lifetime quota for all windows ever opened.

Run 017 also exposed a verification mistake: its initial-open test captured identity, aborted and
reopened the home, then incorrectly expected equality with the reopened store. That is evidence
for distinct openings, not publication changing identity. Preserve the full lifecycle when
interpreting identity assertions. Operator authorized supplying Activity's identity and resource
reference together at the owning publication transition, making the exact published owner
explicit without adding a fallible step after successful publication.

The owning contract is [App adapters](../../crates/beryl-app/doc/design-feature-adapters.md#activity-status-notices-and-audio).
The integration case `height_save_outcomes_remain_owned_after_window_read_retirement` covers clean
commit, definite noncommit, indeterminate and committed-after-persistence failure. Actual shell
and native lifecycle acceptance is recorded in the [mounting audit](../audits/activity-panel-mounting.md).
