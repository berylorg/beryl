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

Operator authorized the correction on 2026-10-01. Phase 853 adds a recovered service-close operation
that checks the service ticket, obtains the existing slot lock nonblockingly, checks the exact slot
ticket and derives its selected host flush ticket under that lock. The existing release operation
rechecks service custody and releases the host barrier before slot/service gates. Ordinary close
behavior and local interaction fences remain unchanged; no worker or retained state was added.

Focused tests assert exact host ticket/barrier preservation after stale and busy calls and absence
after repeated successful mount settlement. The actual selected native regression now completes
recovery, another close, cleanup and same-home reopen. Seven focused tests passed in
`db062215-1638-4f62-a90f-8baf676b0595`; all 74 lifecycle/recovery tests passed in
`8a1c6e86-9950-4710-a762-3f8f421ac30a`. Production compilation, formatting and independent review
passed. This accepts phase 853's correction; phase 851 owns full selected-path qualification.
App shell lifecycle and backend Interrupted Exit settlement remain the controlling contracts.

The original failed process exited; its exact printed home
`C:\Users\user\AppData\Local\Temp\.tmpJJo51f` was verified, removed with `cleanup-dir.exe`, and
confirmed absent. Successful selected preparation home `.tmp14P2TB` cleaned itself up, as did the
corrected focused homes `.tmpECuq1c` and `.tmp6eziYk`; exact checks confirmed their absence.
