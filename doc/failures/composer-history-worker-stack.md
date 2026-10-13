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
