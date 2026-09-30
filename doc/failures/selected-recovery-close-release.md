# Recovered Host Close Ticket Survives Service Release

The actual same-home selected recovery regression advanced past the corrected draft-state guard
in run `08f99be3-2133-4e4f-89c8-ce8c87c83028`. It passed graph publication, widget/process admission,
preserved native/editor/selection identity, cancelled-request completion and duplicate refusal.
The following explicit fixture close failed with `conversation composer close gate failed: a
target activation is already pending`. The test compiled but aborted; this is not accepted full
recovery evidence.

Independent review traced a production lifecycle mismatch rather than fixture timing.
`ComposerHostRetiredClose::reconstruct_candidate` begins a WindowClose flush, retaining host barrier
and close-ticket custody. Recovered mount settlement calls `release_window_close_gate(ticket, None)`.
That operation clears the slot gate, but calls the host's release only when a flush ticket is supplied.
The next `ensure_live` rejects the retained host close ticket. Moving cleanup into an earlier GUI
update cannot remove this retained state; other ordinary resumed operations also use that check.

The recommended correction is to settle the exact reconstructed host flush during recovered
service-close release, preserving existing binding/ticket validation, nonblocking access and local
interaction fences. Verify that host and slot close custody both end, stale/busy calls retain
custody, repeated settlement is safe, and the native regression can perform its next close and
cleanly reopen the same home. Do not bypass `ensure_live` or alter the fixture to discard the host.
App shell lifecycle and backend Interrupted Exit settlement remain the controlling contracts.

Phase 853 records the separate prerequisite; phase 851 remains pending. No production correction
for this defect has been attempted. The failed process exited; its exact printed home
`C:\Users\user\AppData\Local\Temp\.tmpJJo51f` was verified, removed with `cleanup-dir.exe`, and
confirmed absent. Successful selected preparation home `.tmp14P2TB` cleaned itself up.
