# Executable Bootstrap Integration

## Native Font Qualification

The first focused bootstrap run (`5f12cc93-30c0-421b-8d9f-a25fb085b227`) passed 47 of
52 cases in 25.048 seconds. Three new native startup cases aborted on `Inter font not found`.
This was initially mistaken for missing production font packaging. The pinned GPUI Windows
`direct_write.rs` instead makes missing fonts fatal only with `test-support` (or its own tests);
ordinary builds take the existing system-UI-font fallback. App development dependencies enable
the strict test branch.

Use the established native-font theme fixture for app-native lifecycle/input checks, and qualify
the actual executable's built-in fallback separately without app development/test-support feature
unification. Do not install fonts, change product typography, or widen production thread stacks
to make these native tests pass. The three exactly logged aborted fixture homes were reclaimed.
The same run exposed two old Exit-delivery fixtures that attempted to reuse producers created
before home binding or from a retired generation; those fixtures must preserve the generation
fence and create a current producer for subsequent fresh-activation assertions.

## Production Input Prerequisites

An immutable creation callback receiving only remembered runtime/root ids could not derive a
production execution binding while respecting recovered graph identity. Its existing worker
invocation now borrows the current graph's typed context. Regression evidence must include reuse
of the production callback with a replacement graph and rejection of retired/foreign references.

A startup-only token-directory list would leave an empty production profile unable to launch
any configured runtime, and would conflate configured runtime identities with active-runtime
capacity. The immutable directory-root input derives a path from the exact runtime at launch.
Private token files require protection before secret writes even under redirected temporary
directories. An accepted Windows security descriptor alone is insufficient on filesystems that
do not enforce ACLs; the exact opened file's volume is checked before writing. See the
[Windows investigation](../memory/crates.io/windows/0.61.3/private-token-file-creation.md).

## Bootstrap Acceptance

Accepted on 2026-10-03. The combined local all-target check passed for binary, app and
backend with the relevant test features. Corrected native run
`654e45b9-4e8b-45e1-ba33-e07bc1eb058a` passed all 53 cases in 29.425 seconds, including
production-input selected-thread restoration. Backend run `efdb3229-dc2a-4d5a-b94a-e21a419653cc`
passed all nine private-file and managed-launch cases.

All three actual-executable lifecycle cases passed with the child process's `RUST_MIN_STACK`
removed, covering startup cancellation, exact restored window identity, and input/output loss.
The executable run's only failure was a test unwrapping Clap's correct rejection of an empty
path. The fixture now checks both parser rejection and direct resolver rejection.

Independent review required diagnostic shutdown state to include an active Exit after its
queued flag is consumed, and to avoid claiming a ready main window during zero-window teardown.
The corrected read-only predicate preserves ordinary command semantics.

The isolated canonical checkout contained no local dependency overrides. Locked metadata and
combined binary/app/backend all-target checks passed. Final canonical nextest evidence:

- App/native and initial/recovery runtime preparation: 65/65 passed in 81.677 seconds,
  `8a98a15a-d009-43f6-bea6-4d2a7442d344`, including the final active-Exit diagnostic regression.
- Actual executable options, private home registration, startup/restoration and channel-loss
  shutdown: 10/10 passed in 7.735 seconds, `9b97a173-c5b0-4f35-b96b-89e46ffa52e0`.
- Backend private-file and managed-launch lifecycle: 9/9 passed in 0.078 seconds,
  `1c968872-95b2-412f-bbf1-2f42ee58e715`.

Independent lifecycle/persistence/security review accepted the boundary with no remaining
blocking findings. The reviewer recomputed all 52 changed source/test/manifest/lock hashes against
the qualified source with zero mismatches. Bounded logs and the inventory remain under
`.tmp/bootstrap-app-evidence`; the canonical checkout and exactly logged native fixture homes
were verified absent. Production worker stacks were unchanged. Real production panic/report-window
evidence remains the separate crash-reporting mount boundary.
