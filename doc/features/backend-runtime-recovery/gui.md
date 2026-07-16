# Backend Runtime Recovery GUI

This is a normative supplemental GUI composition file for `design.md`. It owns placement and widget composition for backend-unavailable recovery in main conversation windows. User-visible recovery behavior, command availability, preserved state, and exact backend binding remain in `design.md`.

## Backend-Unavailable Notice

Mount-into: main-window.overlays

Each affected main conversation window mounts one `main-window notice` configured with the error and persistent variants. The notice is bound to that window's selected thread and identifies the exact unavailable runtime in its owner-supplied title and bounded detail.

The owner-supplied command region contains a `command button` labeled `Retry`. The persistent variant omits the close command, so the notice remains visually anchored near the top-trailing edge while the conversation shell stays in place.

Retry progress and later notice revisions reuse the same stable notice identity. The composition does not add a modal backdrop, replace transcript content, reserve conversation-body layout space, or mount one process-global notice across multiple windows.

The backend-unavailable notice remains the presentation for runtime launch, probe, compatibility,
connection, and active-turn failures. It is not duplicated for the native-lineage recovery decision
below when that prompt already presents the exact blocking condition.

## Native-Lineage Recovery Prompt

Mount-into: main-window.user-input-panel

When execution reaches recovery-decision-required, this feature mounts one project-local `native
lineage recovery prompt` in place of the ordinary composer. The prompt receives a concise
owner-supplied explanation and exactly two `command button` controls labeled `Retry` and `Recover
from Syndic history`.

`Retry` is the default and initially focused command. `Recover from Syndic history` remains visible
when unavailable and uses the expected-action-availability contract to explain the exact recovery
budget, representation, capability, active-turn, or stale-command gate. The feature supplies exact
command identities, pending state, and results; the canonical prompt owns only shared presentation,
focus, and layout.

The prompt uses the existing pinned user-input-panel allocation. It adds no backdrop, overlay,
dialog, persistent body row, or transcript replacement. Leaving the prompt returns focus to the
restored composer editor when eligible; otherwise focus moves to the exact still-valid pending-turn
control or thread selector chosen by the owning feature.
