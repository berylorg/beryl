# Recovery Admission Fixture Cleanup

On 2026-09-30, the unhealthy-home recovery-admission test incorrectly used a second successful
graph retirement as fixture cleanup. Its admission assertions passed, but retirement returned
`Incomplete` in run `18e79fd3-845a-47cb-89a6-d5b44a2992ab`. Intermediate cleanup variants stalled
and were stopped; their exact blocked call was not established. Do not attribute that stall to
handoff shutdown merely retaining a start token: handoff shutdown already cancels its start gate.

Source review established the relevant distinction: cancellation of a never-started persistent
failure coordinator skips its cut worker and leaves `Stopped`, while recovery retirement requires
a `Finished` failure cut. Terminal disposal of an unstarted graph is a separate existing path.
The final fixture retains the start owner through refusal assertions, then drops it and calls
`dispose_unstarted`. It makes no second-recovery claim. All 14 focused publication/admission tests
passed in run `4f76d389-b904-4050-b2db-2bed6bc556d7` after this correction.

The two interrupted runs' exact processes were verified and stopped. Their random temporary-home
paths were not delivered before interruption and may remain under the OS temporary directory;
ownership cannot be established for safe deletion. Known printed fixture paths were checked and
were absent. Use uncaptured test output when investigating stalls so fixture paths arrive promptly.
