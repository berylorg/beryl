# Compaction Terminal Ordering

## Scope

Context-compaction terminal settlement, compact-start response reconciliation and ordered
router handoff in the mounted CAS projection coordinator.

## Invalidated Assumptions

Durable settlement, router terminal publication, request disposition and driver authority are
separate completion boundaries. Local terminal completion does not prove router publication;
the provider settles durably before its publication permit finishes. A consumed operation also
cannot satisfy a mutation requiring a live operation.

The first response correction read typed terminal status before mutation, but still released
the original command permit in `complete_local`. Production response reconciliation and
`await_terminal` then rejected that same driver before the typed read or router handoff.
The synthetic test harness hid the fault by constructing another local operation with a newly
authorized permit after settlement. Its 29 passing tests in run
`251be357-8caf-4a2c-9cec-9364f6e0e61d` were insufficient evidence; independent review found the
remaining real-driver failure.

## Accepted Correction

Phase 323 was accepted on 2026-09-07. Request reconciliation authenticates Syndic's typed
live/consumed disposition before attempting a live mutation. Exact acknowledgement or an
authenticated matching terminal successor requires no new write. Only `Prior` proceeds through
the existing command/custody path; collision or unreadable proof fails closed.

The original driver retains its exact command permit through response reconciliation and router
handoff. A scoped driver guard releases it on every driver exit, even when another observer holds
the local operation. Durable terminal completion removes admission state without ending driver
authority. No fresh permit, weaker home-only check, new dispatch or replacement operation exists.

A matching late acknowledgement preserves settlement and waits for the proven router terminal.
Same-attempt completion uncertainty preserves settled state but retires the connection.
Contradictory rejection, nondispatch or attempt fails closed. Indeterminate command outcomes retain
their installed reconciliation custody through shutdown.

## Verification

The corrected local-mode locked nextest run
`85b3e78f-19b2-4915-a7be-2709435b801d` passed all 40 selected cases in 30.986 seconds:
23 context-compaction, six lifecycle-content, two original-driver/router, five provider-broker
compaction-marker, two existing router and two coordinator-join cases; 200 unrelated cases were
filtered out. The focused stop run `cc15d67b-eaeb-4673-b801-a2a9c64ea99b` passed all 13 selected
cases in 10.571 seconds; 198 unrelated cases were filtered out.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --lib --test context_compaction --test lifecycle_content_staging -E 'binary(context_compaction) | binary(lifecycle_content_staging) | test(context_compaction) | test(compaction_marker)' --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --lib -E 'test(/cas_projection::stop::tests::/)' --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --locked
cargo +stable --config .cargo/local.toml check -p beryl-app --features test-faults --lib --locked
```

The runs used a 16 MiB process test-stack setting and a temporary 60-second slow-test period with
three-period termination. Both locked checks, scoped Rust-2024 formatting and whitespace checks
passed. Stale non-Copy handle and include-context fixtures were repaired without weakening their
assertions; the provider-broker fixture now retains its mounted harness through event processing.

The integration harness retains the original mounted local through real terminal settlement and
response reconciliation. The real router tests hold publication at `Quiet`/`TargetNotTerminal`,
then prove terminal handoff and command release, and reject original-epoch loss while waiting.
These tests compose with inspection of the production wait loop; they are not a complete provider
transport simulation. Independent semantic review found no remaining production defect; root
verified the corrected fixture lifetime and final test results. The eight direct close-cancellation cases and focused stop cases also supplied the separately
reviewed Phase 319 acceptance evidence. Exact waiting and OS-close mounting remain unaccepted.

## Controlling Authority

- [CAS-live system](../systems/cas-live-syndic-transcript/design.md)
- [App live control](../../crates/beryl-app/doc/design-live-control.md)
- [Consumed compaction witness](syndic-phase72-consumed-compaction-witness.md) records the separate
  storage successor proof.
