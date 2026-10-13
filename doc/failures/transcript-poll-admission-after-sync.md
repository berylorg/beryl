# Transcript Poll Admission After Synchronous Suspension

The native adjacent-disposal fixture fenced a genuine admitted worker, yet retained work never
drained. The task-registration fence synchronously suspended reads inside `sync_running_transcript`.
Its caller, `sync_running_threads`, then continued: suspension had removed the poll task, so the
caller replaced the cancelled token and created a new poller with a retained worker lifetime.
That poller waited while reads remained suspended and kept the drain count nonzero.

Checking admission only before a nested synchronous operation was insufficient. Poll admission must
recheck the existing `running_threads_enabled` predicate after transcript synchronization. That
predicate covers suspension, retirement, close/shutdown gates and current-reader identity. A closed
boundary must return before replacing cancellation or acquiring another worker lifetime.

The regression uses an actual saved-history adjacent request, fences at admitted task registration,
and requires cancelled work/output to drain before source retirement. Prior immutable publication
must remain unchanged, and later callbacks must not resurrect it. Production checking and independent
lifecycle review complement that native proof; polling delays cannot repair reopened admission.

Permanent fixture teardown must also set the existing ordinary-close interaction gate before
suspending reads. Suspension alone is temporary: its drain task deliberately resumes reads after
work settles. The fragmented-tilde Code fixture omitted that gate and could miss the drained state
between its observations. Using the same close gate as adjacent disposal and Code history-revision
fixtures preserves the retirement proof without changing runtime resume behavior or test timeouts.
