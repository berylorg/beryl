# New Thread Callback Admission

Native New Thread confirmation runs inside a `MainWindowShellRoot` picker subscription. The
creation adapter previously admitted its selection lease by asking the running owner to read
every published shell's retained window identity, including the invoking root still being updated.
Virtual confirmation fixtures supplied both a reader and a preadmitted lease and did not exercise
that native admission path.

Run `8bb4b819-56f0-418f-a45a-11222142a0ce` aborted after 5.818 seconds in the recovered New Thread
Code-loading case. The trace reports `cannot read MainWindowShellRoot while it is already being
updated` and follows `start_thread_creation`, `admit_thread_creation`, and `retained_window_id`.
Supplying only a fixture lease does not bypass admission: the fixture override requires its
published reader too. The native case now supplies neither override.

Reuse `MainWindowSelectionInvocation`, already used by Running Threads admission. Capture the
invoking root entity and controller window identity inside the callback, require exactly one such
published root within the existing membership bound, and read only the other shells. Keep the
existing service-owned lease, durable creation command and original-target recovery lifecycle.
This is a bounded admission correction required by native replacement loading, not a new
membership registry or retry policy.
