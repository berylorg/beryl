# Syndic Phase 9 Active Terminal Authority

## Invalidated Approach

The first Phase 9 binding implementation allowed a source-less terminal lifecycle event to move an
`active` binding to `valid`. That transition advanced the CAS-represented prefix through the
submitted turn even when no exact CAS turn identity had been durably published.

The first correction still admitted a source-less local terminal under every binding state except
`active`. That included a usable `valid` binding which represented only the pending turn's parent;
the local terminal advanced the selected Syndic path without advancing CAS authority and produced
state that authoritative reopen validation correctly rejected.

It also prohibited every `active`-to-`stale` transition, leaving no valid durable path for process
or loaded-session loss during an active turn.

A later draft described every abandonment as restoring the submitted turn for retry, even after
an exact activation event had been admitted. The implemented turn state remained active or
unknown-terminal, while activation of another binding correctly required a pending turn. Resetting
that state would erase immutable execution evidence; accepting another CAS identity against the
same source sequence would conflate two execution attempts and could duplicate external work.

## Why It Failed

A terminal Syndic lifecycle fact proves that local turn capture ended. It does not independently
prove that one exact CAS thread and turn accepted and now represent the submitted input. Advancing
the represented prefix from a source-less event could therefore authorize later native CAS
continuation with context CAS never accepted.

Treating process loss as an ordinary stale-binding publication was also insufficient because an
active gate may own bounded steering routes that must be preserved atomically when its projection
authority disappears.

## Replacement

An `active` binding becomes `valid` only when the terminal event carries the exact CAS thread and
turn identity already published for its immutable execution snapshot. Reopen validation requires
the same historical correlation.

Process or loaded-session loss instead uses one explicit active-abandonment mutation. It publishes
the exact stale provenance, permanently retires the CAS thread, returns the gate to the same
submitted Syndic turn, and reroutes all bounded live steering input to ordered next-turn work under
`ProjectionLost` in one command. A turn abandoned before exact activation remains pending and may
be rebound. A turn abandoned after activation is never replayed automatically; a later source-less
interrupted, failed, incomplete, or unknown-terminal update may converge it locally without
restoring or advancing CAS authority. Source-less activation, output, item completion, and
successful completion are rejected.

## Later Delivery Correction

Phase 11 proved that the blanket steering reroute above is safe only for work that was not
dispatched. A `Delivering` steering request may already have been accepted by CAS when its response
is lost, so moving it to retryable next-turn work permits duplicate input. The accepted correction
terminalizes such a fragment as delivery-unknown and reroutes only admitted or retryable
undispatched work. `doc/failures/cas-active-input-delivery-ambiguity.md` owns that later lesson.

Source-less local convergence is accepted only after the current projection has been retired to
`stale`, or while the thread is already `unbound`. A usable `valid` projection must first be
retired; being merely non-active is not sufficient authority.

Recovered-lineage activation additionally cannot predate the recorded successful injection
completion time.

## Reuse Rule

Never infer external represented-prefix authority from local lifecycle alone, and never reset an
activated immutable turn to manufacture retryability. Any transition that claims externally
retained context requires the exact durable provider identity proof; loss of that proof must
preserve local work while retiring external authority.
