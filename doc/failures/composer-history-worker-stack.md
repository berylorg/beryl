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

## Combined Replacement Clearing Regression

Combined mounted qualification subsequently reached a different ordinary mutation path: Select All
and Backspace after importing and cancelling historical input with image markers. Native run
`224c18b1-44d0-45c2-8d85-8e1f970e1f63` reproduced a default-worker `0xc00000fd` after the fixture
waited for exact admitted selection positions. Earlier input races were separately corrected;
they do not explain this settled-input failure.

CDB captured the Windows Dispatcher/WinRT threadpool worker in a finite chain through staged
terminal-mutation preparation, reconciliation reservation and the ordinary composer staging and
dispatch path. `TerminalMutation::prepare` reached `__chkstk`; its capture preparation
caller alone retained approximately 232 KiB. This requires a focused private witness/frame
correction and affected staged mutation/recovery qualification within the existing worker stack.
The no-normal-debug binary can label farther frames with a nearest or folded public symbol;
those labels do not independently prove a defect in the named recovery helper.
The earlier passing import, first-edit and Undo/Redo cases do not cover this combined clear path.
Do not alter durable encoding, authentication or command semantics, or treat a larger fixture
stack as acceptance of the production worker.

## Deferred Marker Validation During Mixed Clearing

After the sealed staging flight and acknowledged-session predecessor reads, native run
`8f6809a2-9ecc-4af0-82b9-768e13957ac6` reaches deferred marker processing and overflows
the ordinary WinRT worker (`0xc00000fd`). Debugger runs 046 and 047 reproduce the finite
mapping → deferred-prefix → marker-program → active-effect validation chain. The validator
requests 48,936 bytes before `__chkstk` fails; retained marker, deferred-prefix and mapping
frames occupy approximately 178,368, 90,032 and 71,040 bytes respectively.

Private validation repeatedly copies fixed active-effect aggregates through by-value getters;
monolithic dispatch also retains temporary slots for unrelated branches. Correct private
validation borrowing and separate dispatch from branch construction, preserving every exact
comparison, transition order, durable field and work limit. Renew affected storage checks and
native execution on the unchanged worker stack before accepting this correction. Debugger
termination is diagnostic evidence, not a passing test.

The six-file private correction borrows active/continuation validation inputs and extracts
marker frontier, mapping and deferred-prefix dispatch branches with explicit non-inlined
boundaries. Independent comparison against the qualified storage snapshot finds no semantic
blocker: public getters and durable records are unchanged, and comparisons, transition order,
lookahead acquisition and the single acquisition ledger remain equivalent. The focused storage
check passes in run 048; all 14 mapping/deferred cases pass in run 049
(`62a9ed5f-682e-4d62-84df-3572cd479a8e`, 209.217 seconds). Native run 050
(`688f991c-1664-4035-aa80-848a3e335cf8`) completes ordinary clearing to zero bytes, quiescent
widget, ready exact position proof and no active flight, then reenters historical Edit. It fails
the later submission assertion, causing a non-unwinding panic (`0xc0000409`), not another
`0xc00000fd` worker overflow. This proves progress through the formerly failing chain; it is
not a passing full workflow. A focused clearing/reentry/disposal native case and broader affected
storage/host renewal remain required for this correction's acceptance.

Keeping the terminal input/result boxed and separating target capture reduced that capture frame
from approximately 232 KiB to 2 KiB. Native run `bd9e20af-643f-4446-9448-e5765aeacdb6` nevertheless
still overflowed. The next debugger trace reached a roughly 10 KiB SHA compression allocation
under accumulated existing-build receipt/authentication preparation frames. The hash call was the
limit crossing, not evidence for changing SHA or the worker stack. Keep intermediate receipts,
terminal build tuples and writer witnesses heap-owned through preparation, and isolate their raw
construction frames. Reaching farther into the finite call chain is progress, not passing evidence.
