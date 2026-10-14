# Active Transcript State Supersession Acceptance

The bounded Syndic lifecycle correction is accepted on 2026-10-14. It closes the race where binding
activation or TurnActivated changes a captured turn state while a selected transcript build is
Collecting or Publishing. Previously publication refused the stale snapshot without any relevant
thread/head/build change, closing selected construction and leaving its initial display empty.

Syndic now atomically publishes changed turn state, the superseded captured build and its next
stale selected head through one guarded contribution. The selected constructor observes actual
head/build progress and resumes ordinary Start/Advance; it does not clear original failure custody,
guess progress, expand its six-point-read envelope or construct item projections in the provider.

## Preserved Boundaries

- Current heads retain their existing neutral path-state refresh. Uncaptured turns, stale heads
  without active builds and unrelated selected paths keep their existing behavior.
- Activation cancellation uses the same bounded state effect. Added head/build reservation covers
  its exact atomic publication and reconciliation keys.
- Existing checked revision/generation arithmetic, selected tail/digest and non-future source
  guards remain intact. Old path/entry rows remain derived history without rewriting their pages.
- The helper uses a fixed selected path point and bounded single-record active-build queries;
  there is no full history scan, source replay, external effect or new capability.

## Verification

- Storage library check passes in 10.85 seconds.
- Focused run 003 passes 11/11 in 12.018 seconds, ID
  `57d188e6-a96d-477f-8d4d-32052ff3013c`. It covers both activation transitions during collected
  and partially published builds, ordinary resumption, Current/no-build/uncaptured behavior,
  cancellation, unrelated-path isolation and atomic guard/revision-exhaustion refusal.
- Initial nine-target storage run 004 passes 156/164, ID
  `e2ff2929-2b9c-404d-8b71-b430455f373d`. Eight recovery cases expose a missing fixture item
  projection. Preparing that exact derived projection preserves complete TurnState and the
  FinalizingHistory assertion, without freezing, finalizing or releasing a gate.
- Corrected run 006 passes the entire affected DeliveryRecovery target, 46/46 in 97.740 seconds,
  ID `ee7387aa-f3ec-4cf9-aa4d-ad0b4bbaf7ec`. With run 004's 118 unchanged other-target passes,
  this covers all 164 current storage cases. Independent review approves the fixture correction
  and this unchanged-input evidence reuse.
- Mounted/owner run 005 passes 28/28 in 181.705 seconds, ID
  `aa060200-c0cc-4abc-b449-09ad97b13911`: six ordinary/saved streaming cases, both complete native
  workflows, sixteen construction/custody/peer cases and four publication convergence cases.
  The previously failing fragmented tilde workflow passes in 22.427 seconds.
- Canonical run 007 passes locked metadata and offline app/executable production checks in
  3 minutes 20 seconds. Its accepted base is `7e35c562`; the five-file storage source patch SHA256
  is `8C588B166834A5F74C6638FF229FB04CEA36AF9F9CD9F9ADAF8F0B2BC7C3ABD8`.
  Mounted production inputs also include the separately qualified six-file publication patch
  `BF4999B69A731EA92284F1B28AD9806335AABDDF052C4D77C9DF43152CF6BF16`.
  The canonical/root lockfile is unchanged, SHA256
  `7DF5A2D363EBCC584ABF397B76238B5D0D16D5909ABC0696EC700EC0A71B763F`.

Independent semantic/adversarial review accepts atomic state/build/head publication, exact stale
authority refusal, bounded work and original outcome custody. Completion review accepts this
prerequisite. Whole mounted workflow acceptance remains a separate boundary; broader catalog,
Settings, bootstrap and conditional unavailable repair gates are not accepted by this record.
