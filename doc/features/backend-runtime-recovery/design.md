# Goals

Keep Beryl workspaces usable when runtime targets or backend connections are unavailable, while preserving exact backend ownership, security, and thread/runtime bindings.

Let users understand which backend-dependent actions are unavailable, which runtime target is affected, and which recovery actions are available without silently switching context.

## Non-goals

- Bundling, installing, or replacing Codex.
- Exposing operator-managed unauthenticated app-server listeners.
- Silently switching users to a different backend process, runtime target, workspace member, or thread after failure.
- Treating backend thread enumeration as the source of workspace identity.

# Decisions

## Implementation References

- Backend launch, listener security, capability probing, process lifecycle, connection recovery, and protocol ownership are defined in `doc/systems/backend-runtime/design.md`.
- CAS-live transcript capture and selected-history authority are defined in `doc/systems/cas-live-syndic-transcript/design.md`.

## Backend-Unavailable State

- Backend availability is visible per runtime target.
- Missing `codex`, managed process spawn failure, probe failure, incompatible required capability, or connection loss marks only the affected runtime target backend-unavailable.
- Backend-unavailable targets disable backend-required operations for that target until successful retry or runtime configuration change.
- Backend-unavailable state must not detach workspace members, change default runtime, promote another primary member, make the workspace unavailable, or require application exit.
- Backend-unavailable user-facing states identify the affected runtime target.

## Workspace Behavior During Backend Failure

- Opening the workspace shell requires only GUI-owned workspace state.
- Successful startup does not require backend launch, compatibility probing, or backend thread enumeration.
- If the current primary runtime target cannot launch or probe during startup, Beryl still opens the workspace, keeps workspace/member management available, and disables conversation operations for that target.
- Workspace selection, workspace picker interaction, default-runtime selection, member attachment, member detachment, and primary-member selection remain available while the primary runtime target is backend-unavailable.
- Composer submission, new-thread creation, existing-thread activation, thread selector activation, inventory refresh, title generation, backend-derived model/reasoning status, backend-derived context status, context compaction, and turn-control interactions are disabled for the affected backend-unavailable target.
- A backend-unavailable host-Windows target does not disable usable WSL targets, and a backend-unavailable WSL distro does not disable host-Windows or other WSL distros.

## Disabled Paths

- Operations that target incompatible or unavailable backends fail or present localized recovery for that target and must not silently switch runtime target, workspace member, backend process, or thread.
- Missing branch backend primitives disable branch actions rather than allowing local transcript-copy emulation.
- Missing edit backend primitives or unprovable rollback scope disable edit actions rather than allowing local transcript mutation emulation.
- Missing hard-stop support disables only affected hard-stop escalation controls and must not disable soft interruption.
- CAS historical turn reads are not a user-visible recovery path for selected transcript rendering after CAS-live Syndic capture cutover.
- If a live stream ends because the user deleted the active turn, no durable transcript turn remains for that deleted work.
- If a live stream is lost without user-requested active-turn deletion, Beryl preserves explicit incomplete, failed, or unknown-terminal transcript state rather than silently recovering from CAS historical reads.

## Connection Loss Recovery

- If the foreground backend connection or managed process is lost, the GUI keeps the current workspace, semantic graph state, checklist selection, runtime state, member state, active transcript selection, and GUI-local drafts intact.
- If a background backend connection for title generation, inventory refresh, or lazy maintenance fails while the managed process remains usable, Beryl reports or logs only that operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable for that runtime target, not as application startup failure.
- On backend disconnect, the GUI presents a blocking recovery path rather than silently switching backend process.
- Recovery actions may include relaunching a managed backend for the same runtime and resumed thread binding or closing the application instance.
- The GUI must not silently switch the user to a different backend process after disconnect.
