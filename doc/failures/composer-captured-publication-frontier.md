# Scope

Captured composer-candidate publication while later candidates continue to adopt.

# Invalidated Approach

Finish a changed-marker seal and only then prepare the captured Syndic publication by reloading the
captured history reference from the editor session's mutable live-frontier key.

# Decisive Contradiction

Candidate generations in one editor session deliberately reuse that mutable frontier key. If
candidate N+1 adopts while captured candidate N is still sealing, the key contains N+1 by the time
publication preparation reloads it. Comparing that record with N's captured reference returns
`Invariant`, even though N's immutable adoption settlement, root, transition, and history closure
remain valid and final publication already permits a newer live candidate.

Preparing after N's seal while delaying N+1 would violate the product contract that editing remains
available during a captured save. Preparing the complete publication before sealing is also not a
valid correction because changed-marker evidence does not exist until the seal completes.

# Accepted Correction

Late reconstruction from candidate N's immutable adoption closure is not a general correction.
Ordinary candidates expose their committed piece-settlement identity through the candidate root,
but direct undo/redo candidates do not: their root names the original content operation while their
historical adoption record is keyed by a separate undo/redo operation identity that the final
publication request and frontier reference do not retain.

The bounded correction is a Syndic-issued opaque publication-source capture taken before
marker sealing and retained as one bounded host custody value. It must include and authenticate N's
exact frontier and ordinary or historical adoption identity; final preparation reauthenticates that
source beside derived marker evidence and the live session. It is process-local and creates no
durable record; the existing final canonical request/receipt remains replay authority. Adding the
historical adoption identity to durable request codecs, creating a generation index, walking
predecessors, assuming exactly one successor, or pausing editing is not part of the correction.

# Affected Work And Residual Risk

Phase 172, `syndic-storage` publication preparation, and `beryl-app` publication integration must
prove both ordinary and undo/redo candidate N can seal and publish after multiple later candidates
adopt, leaving the newest candidate dirty. Review must reject any correction that infers history
from a mutable frontier, walks predecessor history, assumes one successor, delays editing until seal
completion, creates a whole-history snapshot, or weakens exact adoption and final-command
authentication.
