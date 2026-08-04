# CAS Phase 67 Unbound Exact Interruption Target

## Invalidated Approach

The first Phase 67 completion candidate carried runtime, managed-process, loaded-thread, CAS
thread, and CAS turn identities inside a non-cloneable interruption authorization. The
authorization was minted by the selected managed session, but that session stored no corresponding
exact foreground target and therefore compared none of those identities before request bytes.

Focused tests fabricated the target values, minted the authorization, and proved the request wire.
They treated a later authorization epoch as the complete stale-target gate.

## Why It Failed

A session-minted wrapper proves only which Rust session created the wrapper. It does not prove that
the supplied runtime and loaded-session generations, thread, or turn belong to that session.
Retaining exact values for correlation is not the same as validating them against current
authority.

This left a cleanly compiling path where a stale or foreign loaded target could authorize
`turn/interrupt` or coarse thread cleanup bytes. Green request, response, retirement, and
correlation tests could not detect the missing comparison because their fixture never established a
real session-owned target.

The same review also found that the exact request test had normalized Beryl's generic
`jsonrpc: "2.0"` envelope instead of the pinned producer's member order and omission, and that a
matching cleanup rejection was retired but publicly mislabeled as completion ambiguity rather than
session-authority invalidation.

## Course Correction

The sole foreground driver now binds one authenticated exact target into the managed session.
Authorization and dispatch both compare the complete target against that binding. Replacing it
requires an explicit unbind cut which revokes every earlier authorization, and retirement removes
the binding.

Tests vary every exact target component and prove refusal before request bytes, then exercise the
explicit revoking replacement cut. The specialized request envelope matches the pinned producer
exactly, and matching cleanup rejection has a distinct session-authority-invalidated disposition.
A fresh independent completion review is required after remediation.
