# Goals

Let users organize Beryl work into durable semantic workspaces that can span host-Windows and WSL filesystem roots without making backend conversation history the source of workspace identity.

## Non-goals

- Treating a Beryl workspace as one filesystem root.
- Requiring successful `codex app-server` startup before a workspace can open.
- Supporting concurrent work in multiple Beryl workspaces inside one GUI instance.
- Deleting backend-owned Codex data when deleting Beryl-owned workspace state.

# Decisions

## GUI Supplement

- `gui.md` is a normative supplemental GUI composition file for workspace toolbar controls, workspace picker layout, and member-column layout.

## Workspace Identity

- A Beryl workspace is GUI-owned durable state stored under the configured Beryl home directory, whose default is `~/.beryl`.
- On startup, Beryl opens the previously active persisted workspace. If it cannot be resumed, Beryl creates and opens a fresh untitled workspace.
- New workspaces begin untitled and may remain untitled until manually renamed or best-effort auto-titled after the first completed assistant turn.
- A freshly created workspace renders through the normal main workspace window as a pending-new-thread draft. It does not replace the main workspace window with a separate startup screen.
- Untitled workspace labels use a monotonically increasing sequence and are not renumbered after deletions.
- Workspace display titles map to filesystem-friendly workspace id slugs by deterministic transliteration and normalization.
- A title change is rejected when the derived slug is empty or already belongs to another workspace. Beryl does not auto-suffix colliding names.
- Manual workspace rename is available only when no workspace-scoped work is in progress or queued, and a manual name prevents later automatic overwrite.
- A workspace title change that changes the workspace id is a repository-level move of the workspace-scoped state directory plus manifest and cross-workspace metadata updates. If that operation fails, the old title and id remain authoritative.
- Workspace `last updated` metadata changes when durable workspace content changes, including title, graph content, thread refs, default runtime, member registration, or primary-member designation. It does not change merely because the user opens, views, or activates the workspace.

## Runtime Environments And Members

- A runtime environment is either host-Windows or one specific WSL distro. Runtime environment determines backend launch mode, path interpretation, and home-directory fallback.
- A workspace may have explicit workspace members from host-Windows and any number of WSL distros.
- A newly created workspace uses host-Windows as the default runtime environment. A workspace with no default runtime is a legacy or recovery state.
- Changing the default runtime affects future member attachment and implicit-home fallback only. It does not move existing members, rewrite thread refs, or restrict the workspace to one runtime.
- Each explicit workspace member is one attached directory inside its own runtime environment. Files are not valid members.
- Explicit members within one workspace and runtime environment must not overlap after canonicalization. Ancestor, descendant, symlink, and alias duplicates are rejected.
- The same textual path in different runtime environments or WSL distros is a different member identity.
- If an attached explicit member path cannot currently resolve to a live directory, the member remains attached, is displayed as unavailable, and is excluded from new-thread execution and thread inventory until it becomes available or is detached.
- A workspace with a default runtime and no available explicit members exposes that runtime's home directory as an implicit undeletable member.
- The implicit home member acts as primary while no available explicit member exists.
- Attaching the first available explicit member removes the implicit home member from the visible member list and makes that explicit member primary unless the user later changes the primary selection.
- If the primary explicit member is detached or becomes unavailable while other available explicit members remain, Beryl durably promotes the earliest available explicit member by stable attach order.
- If no available explicit members remain, the implicit home member reappears and durably becomes primary.
- Runtime-boundary conversions are limited to explicit GUI-owned cases: WSL-to-host UNC paths for OS-open actions and generated-image reads, post-picker validation, and Beryl image asset paths converted into backend-runtime-readable paths for local-image submission.
- For local-image submission to a WSL runtime, a Windows-hosted Beryl image asset is validated through its original Windows path and submitted as the corresponding WSL `/mnt/<drive>/...` backend path. Beryl must not validate Windows drive assets through a `\\wsl.localhost\<distro>\mnt\<drive>\...` path.

## Workspace Picker UI

- Workspace selection and member management use one popup opened from the main toolbar rather than a separate workspace screen.
- The popup contains a left `Workspaces` column and a right `Members` column separated by a vertical divider.
- The `Workspaces` column has a filter field above one divided list. The first row is `Create new workspace`, followed by existing workspaces ordered by most recently opened first.
- The workspace filter matches workspace names and explicit workspace member paths shown in rows, including unavailable member paths.
- Workspace rows show the workspace name and explicit member paths, one member per line. They do not show implicit-home paths or `last updated` metadata.
- The currently active workspace row uses only a left-edge accent marker, not full-row primary highlighting or redundant active text.
- Activating another workspace row switches to that workspace and closes the picker. Activating the current workspace row closes the picker without reloading.
- Each ordinary workspace row exposes a row-edge action menu with `Rename` and a held delete action.
- Completing hold-to-delete for the active workspace opens a fresh untitled workspace using host-Windows and the implicit home member.
- Completing workspace delete removes only Beryl-owned local workspace state.
- If switching to a selected workspace fails, Beryl keeps the current workspace active, closes no existing transcript state, and records the failure through normal diagnostics.
- The picker closes on outside click, on `Escape`, and after accepted row activation. V1 does not require keyboard row traversal or `Enter` activation inside the picker.

## Members Column UI

- The `Members` column manages the active workspace's default runtime and explicit members without replacing the main workspace screen.
- The column has a fixed runtime selector row above a divided member list. It does not have a separate filter field.
- The runtime selector controls both default runtime and the runtime used by the next attachment. It remains enabled when explicit members exist because it does not rewrite existing members.
- WSL distro rows render with a `WSL: ` prefix.
- When no default runtime is selected, `Attach member` is disabled until the user chooses host-Windows or a WSL distro.
- `Attach member` opens the native OS picker and validates that the selected path belongs to the runtime being attached. Host-Windows attachments reject WSL UNC paths; WSL attachments accept only UNC paths inside the selected distro.
- Member rows use a primary display label and secondary full filesystem path. Long labels and paths soft-wrap and may increase row height.
- Unavailable explicit members remain visible, append `- path not found` to the primary label, and do not expose `Make primary`.
- The current primary member uses the shared left-edge accent marker without redundant primary/current text.
- Explicit member rows expose one row-edge action menu. Non-primary rows include `Make primary`; explicit member rows include a detach action that asks for confirmation.
- Backend-unavailable state for the current primary runtime does not disable runtime selection, member attachment, detach, or `Make primary` for available member paths.

## Persistence And Recovery

- Workspace-scoped GUI-local state is stored in one directory per workspace under the configured Beryl home directory's `workspaces/` child.
- Workspace-scoped state includes default runtime, runtime-bound members, primary-member designation, active-thread state, thread title metadata, thread/member binding metadata, semantic graph state, image asset metadata, window state, splitter positions, activity panel mode and height, and similar workspace-local state.
- Shared cross-workspace runtime metadata remains minimal and includes the monotonically increasing untitled-workspace sequence counter.
- Shared cross-workspace metadata may use last-write-wins or atomic-replace semantics across GUI instances.
- Opening a workspace requires GUI-owned workspace state only. Backend launch, compatibility probing, and backend thread enumeration are not prerequisites for rendering the workspace shell.
- Missing or unavailable member paths do not make a workspace unavailable. Beryl opens the workspace, keeps those members attached, marks them unavailable, and applies primary fallback rules.
