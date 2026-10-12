# Native Read Retirement

Native mounted fixtures must settle Running threads read custody before retiring its provider.
Calling `retire_running_thread_views` immediately after successful transcript assertions can fail
with `Running threads still owns unsettled read or activation work.` In a native GPUI callback,
the resulting panic can abort the test process instead of unwinding normally. The saved streaming
Code fixture reached that teardown failure after its growth and matching-completion checks passed
(`de1bbdd5-29fd-4dbd-bd7a-eed982ac953c`).

Suspending reads once and then polling `release_suspended_running_thread_sources` is also racy.
`suspend_running_thread_reads` owns an asynchronous drain task that resumes reads when custody
settles. That task can resume the source between polling updates, leaving the release operation
ineligible even though workers drained. The bounded polling attempt failed at its drain assertion
(`3d291567-4232-4d28-964a-f6503388d390`, 51 of 52 selected cases passed).

Separating successful release from view retirement into a following window update leaves another
restart interval. That version drained successfully but failed at retirement
(`7a443b3f-0439-4284-b597-11b30c00360d`, 51 of 52 selected cases passed).

For explicit fixture retirement, each bounded polling update suspends reads, checks source release
and immediately retires the views on success within the same native window update. Then retire the
panel and quit through the existing fixture path. Keep production custody fences and normal temporary
suspension/resumption behavior intact. This correction belongs to fixture teardown, not streaming
publication or indexed admission.

The corrected native streaming fixture passed with all three native history-revision cases and
all 48 mounted-panel cases (`6d36c537-3173-41b4-a8df-990042fc06db`, 52 passed in 29.543 seconds).

Fragmented opening-info delivery added two coherent empty snapshots before body growth. Run
`c8df48ee-cb2f-471e-803a-72d77b39c3d6` passed all streaming, growth, completion and scrub assertions,
then exhausted the fixture's two-second retirement poll; 52 other cases passed. The reported Windows
`0xc0000409` followed that assertion unwinding through a native callback and is not evidence of a
source-buffer overrun. The fixture retains atomic suspend/release/retire and uses a six-second
bounded drain window, matching its existing indexed-delivery wait. Production retirement fences
remain unchanged; a bounded wait failure must still fail the fixture.
