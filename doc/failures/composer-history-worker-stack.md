# Composer Historical Undo Worker Stack

Quote mounting exposed a pre-existing stack-size assumption in ordinary historical Undo.
After a native mapped Quote inserted the exact selected Markdown and preserved focus and the
clipboard, Undo overflowed a Windows GPUI WinRT worker's fixed OS stack. Increasing
`RUST_MIN_STACK` to 64 MiB did not help because it configures Rust-created threads.

The native failure was reproduced in nextest runs `c0ef055a-271d-4b84-8558-248e316ce749` and
`52f78939-e603-494c-8553-508496e14c50`. CDB showed a finite call chain through ordinary composer
dispatch, historical root adoption, retention authentication and transition decoding rather than
recursion. Unoptimized historical retention and adoption preparation reserved approximately
238 KiB and 233 KiB, with another 116 KiB in erased mutation preparation and repeated transition
read frames. Witness-heavy transition values copied through these frames exhausted the worker
stack even for a small draft.

The correction heap-owns private prepared and authenticated transitions,
retention records, lineage cursors and temporary witnesses. Public record representations,
canonical encoding, digest inputs and authentication remain unchanged. A larger test-thread
stack is not acceptance evidence for the OS worker boundary. Qualification passed ordinary native
Quote/Undo/Redo on that worker and constrained-stack historical Undo/Redo. The combined 66-case
mapped-source, history, retention, marker and failure run passed as
`1ce9564c-ad09-451d-8d98-c7769f32fee0`; the native Quote workflow passed in app run
`08380955-0e90-40fc-af32-19844cbcb59d`. No schema, public record or authentication change was needed.

## Historical Replacement Import Regression

Native replacement qualification on 2026-10-13 reproduced a background-worker stack overflow for
both text-only and marker-only historical input in nextest run
`8dded570-e9a9-4fb5-bb2a-b8e22ff38077`. Both cases reached fixture readiness, coherent ordinary
editor readiness and `import requested`, then aborted on an `<unknown>` thread with `0xc00000fd`
before imported-editor readiness. The named fixture preparation thread used 16 MiB; this did not
change the Windows background worker's stack or qualify import.

The same inputs passed the non-native host import tests, which did not qualify the native
preparation, witness and quantum frames.
Do not substitute a larger fixture stack, another runtime or successful host tests for that
production-worker evidence. At discovery, Edit remained disabled pending this correction and the
separate [retirement/recovery custody correction](historical-replacement-recovery-custody.md).

CDB reproduced a finite chain with approximately 287 KiB in host advancement, 215 KiB in import
preparation, 145 KiB in initial transition construction and further authenticated transition/read
frames. The correction separates private host streaming/image preparation frames and heap-owns
private decoded source records, session witnesses and initial transitions without changing durable
schema, encoding or command outcomes. Native run `8a0a8613-e134-4842-be32-ea584f0cf47b` passed that
worker boundary and exposed a different failure: host adoption returned Progress before the slot
could adopt its successor identity. The Ready result now accompanies host adoption in the same
service-lock call. Complete native import/Undo/Redo remains the acceptance evidence.

Further native traces isolated fresh-page imported-checkpoint authentication, then first-edit
candidate-history and bounded Advance preparation. Heap-owned checkpoint and mutation witnesses,
separate imported/published/unpublished validation frames, and an end-to-end private boxed advance
value removed the dominant enclosing allocations without changing equality checks or budget
accounting. Run `4d8dd1cb-7f50-4c70-99b4-fed05c294e2e` passed both native text/marker import,
first-edit, Undo/Redo and cancellation cases on the default Windows worker. A broader native Code
activation case exposed another finite-stack regression; this passing workflow did not waive
that shared-boundary failure.

The broader Code activation trace in `run-037` reaches a finite ordinary saved-checkpoint chain:
activation preparation, candidate session publication, transition receipt authentication and exact
build-record decoding. The imported receipt path was already heap-owned, but the ordinary receipt
still accumulated large by-value witnesses beneath retained activation frames. Correct the private
ordinary reader using the same bounded boxing discipline, then qualify both the native Code
activation and affected storage checkpoint/mutation cases. Preserve every exact receipt comparison.

With that stack corrected, the same native activation reached a separate constructor deadlock.
The `run-039` thread capture shows publication holding the composer-slot mutex while construction
attempts to read submission intent through the same service mutex. Capture intent in the prepared
presentation before election, authenticate it against the exact pending selection under the existing
publication lock, and pass the captured value into construction. Construction must not reacquire
the slot lock within that election. This is a lifetime integration defect, not evidence for changing
the worker runtime or weakening publication authentication.

Native Code activation and fresh range routing subsequently passed in
`2e4c3ff6-68ae-454a-8299-ac4290ae7b74`. All 149 affected storage checkpoint/import/mutation/history/
encoding cases and all 118 final selected panel/native cases passed. Complete independent review
accepted the [mounted workflow](../audits/transcript-mounting-readiness.md#mounted-historical-replacement-editing-acceptance),
including production Edit enablement; no worker-stack or constructor-lock blocker remains.
