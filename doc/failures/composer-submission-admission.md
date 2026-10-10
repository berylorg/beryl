# Native Composer Submission Admission

The first native submission qualification reached a real FirstAcceptance command on GPUI's
unchanged Windows worker and overflowed its 1 MiB stack. This was not a window-claim guard lock
cycle: the worker overflow left its composer-slot mutex held, and the UI subsequently waited for
that slot while applying the worker result and rendering status.

A first-chance CDB capture localized the overflow to candidate-disposal history authentication
inside admission preparation. The live stack retained roughly 160 KiB in `load_base`, 126 KiB in
the records dispatcher and 128 KiB in erased mutation preparation before nested history reads.
Prior storage-only tests used larger Rust test stacks and did not establish this native path.

Boxing the private Idle/Accepted variants alone moved the overflow to a deeper checkpoint read;
the route dispatcher still retained an inline acceptance base. The bounded correction also boxes
that base, so both the generic Home preparation envelope and route dispatcher transfer compact
handles. The corrected native route reached successful first acceptance and successor publication
before a separate fixture timestamp assertion. Persistent codecs,
atomic gate admission, reconciliation custody and native stack limits remain authoritative.
Native Idle and Active/Accepted admission on the unchanged worker stack passed combined acceptance
with both admission-route regression checks. Independent phase completion review accepted these
results. This does not establish remaining recovery or repair adapter acceptance.

Independent review consumed the bounded task's first-chance stack captures and corrected native
receipts. Combined acceptance run `1a37ad44-b324-46a5-8fd1-1b33e91868a1` passed 75/75 focused
App cases; default App/backend/Home/Syndic checks also passed. The diagnostic task artifacts have
reached their final consumer and may be reclaimed. The root plan owns remaining product work.

The native indeterminate-publication probe must compare acceptance and route custody, rather than
require an unchanged whole-Home revision through successor publication. Run 047 reached successful
settlement but aborted on that overly broad assertion: opening the successor legitimately advanced
Home from revision 42 to 43. The corrected probe compares the exact retained input gate, checks
reconciliation disposal and successor publication, and does not forbid the successor's own write.
