# Native Save Dialog Qualification

A bounded Windows save-choice test initially drove the common dialog before its filename and
controls had settled. It found an enabled Save button but reached no acceptance callbacks. Delayed
initialization plus checking the native filename reached the actual overwrite prompt.

That prompt used modern DirectUI controls whose child IDs were all zero. Traditional IDYES/IDNO
button routing could not dismiss it. The timer remained active, but cancelling the disabled parent
picker did not settle the nested confirmation. Run `code-save/run-016` exceeded its intended six-
second cutoff and was reclaimed by exact owned executable, PID and test arguments after 142.909
seconds. The initial synchronous-message suspicion was disproved: commands were asynchronous.

The accepted test drives the exact owned confirmation with `TDM_CLICK_BUTTON`, retains exact
thread/dialog ownership, uses a six-second graceful cancellation deadline and an independent
fifteen-second nextest process deadline. Native callback output stays bounded and every timer/COM
subscription is released. No Operator window or clipboard is targeted.

The accepted rich-choice boundary now obtains explicit native Yes/No inside its overwrite callback,
instead of inferring acknowledgement from a later accepted result. Stable native/clipboard run
`908f3ef1-f450-4c9f-abbc-f746a604e649` passed all twenty-three cases in 2.713 seconds. Actual native
confirmation, declined replacement followed by both a different new choice and an externally
deleted same-name new choice, and cancellation passed. Typed Show classification and callback-
failure followed by cancellation also passed. An internal modal timeout alone is insufficient
evidence that a native qualification process is bounded.
