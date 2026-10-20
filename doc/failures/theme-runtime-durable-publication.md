# Durable Theme Publication

## Invalidated Assumption

A committed repository operation does not imply that the active appearance should refresh.
Likewise, a prepared theme from before scalar commit is not proof of the document currently on
disk, and a cached rejected appearance is not the result of a fresh Retry.

## Evidence

Independent review of the mounted Settings implementation found that its broker prepared the
active theme for every repository commit, including Save As. That could advance appearance or
apply an unrelated external edit despite Save As preserving appearance and preview. Settings
outcome delivery checked scalar identity but could publish document bytes prepared before an
external replacement. Retry replayed a cached candidate, or reported unchanged when preparation
had failed before any candidate existed.

The same distinction matters in feedback delivery: an unrelated repository completion that does
not publish appearance cannot clear an existing persistent application failure.

## Correction

Carry the original command's affected installed-document identity through execution and exact
reconciliation. Only an update of the current active document can publish its repository result;
manifest-only operations preserve appearance. Suppress a watcher refresh when the active physical
document remains identical, so an operation's own manifest hint does not undo that guarantee.

After confirmed scalar commit, validate the current stored selection and exact repository/document
proof off GPUI before publication. Keep the committed scalar values, prior coherent appearance and
compact original publication intent when validation fails. Retry resolves the durable selection
and reads its current document afresh; an unavailable or invalid document retains the failure.
Successful delivery clears appearance feedback only when the durable appearance is resolved.

Stop Preview also resolves the current durable selection and document off GPUI. A cached prior
baseline cannot restore a theme after a newer selection or document has committed but failed
preparation. Failure preserves the existing preview. Successful watcher or active-document Save
publication consumes any retained committed Settings intent through the same publication path,
including its requirement to end preview.

Read-only staged-selection qualification is separate from original command custody. It may enable
Apply for an exact draft but never allocates an operation, installs a Home receipt or executes a
command. Actual Apply still prepares and validates its own operation.

## Mutable Custody And Published Readers

Moving the service owner to the appearance worker originally made ordinary reader access return
missing. Shell projection interpreted that as generation retirement and cancelled still-current
read state. Waiting in test fixtures could avoid a snapshot panic but could not repair that
production lifetime defect. Initial Settings projection also runs before ordinary command mounting,
and recovery rebinding runs before completion consumes the restored owner.

Retain only the finite existing generation-qualified read facades for the exact worker flight.
Their weak currentness checks remain authoritative, and a returned unavailable graph never uses
the retained readers. Restore the owner and release those facades together before reopening
admission. Gate graph loans until initial mounting and interrupted recovery completion finish;
preserve deferred requests rather than consuming them before that boundary.

Distinguish an armed command listener from an admitted command. The running owner's
`waiting_for_exit` remains true throughout idle listening, so using it as an appearance exclusion
prevented every ordinary appearance flight. Gate queued/active Exit through `exit_requested()`;
retain the actual startup, recovery, shutdown and teardown custody checks. A held-flight regression
must prove that the worker really acquired mutable custody before checking reader continuity.

The same temporary loan also closes the Exit Settings-reconciliation gate. Generic New Window
publication treated that gate as terminal refusal and abandoned an already admitted hidden shell.
Exact refusal diagnostics show ordinary commands mounted, no Exit or shutdown and appearance busy;
the resulting abandonment exposed the separately corrected native retirement-stack defect.
Keep this original construction entry through the exact appearance wait with its existing capacity,
deadline and cancellation. Restore services and project current gates before waking its original
owner/home-qualified driver; revalidate every real publication guard. An Exit-specific temporary
gate cannot itself prove that native publication failed. Qualification remains in the Settings audit.
The controlling contracts remain the [Theming feature](../features/theming/design.md) and
[theme runtime](../systems/theme-runtime/design.md). Owning handoff, repository, watcher and mounted
Settings regressions qualify these corrections; final acceptance belongs to the active plan and
its audit.
