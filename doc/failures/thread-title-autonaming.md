# Thread Title Auto-Naming

## 2026-04-30: Backend propagation was not sufficient for Beryl-created threads

The first implementation focused on consuming backend-provided thread names from thread summaries and `thread/name/updated` notifications. Live testing showed Beryl-created threads still remained `Untitled` after the first completed turn and after restart.

The invalid assumption was that `codex app-server` would auto-generate a backend thread name after a completed turn. Local schema and source inspection showed that app-server exposes optional thread-name storage plus `thread/name/set`, and emits `thread/name/updated` when a name is set, but does not itself derive a name for Beryl-created threads.

The initial course adjustment was to keep backend-provided names as the preferred non-manual source while adding GUI-local fallback title generation. Later exploration rejected that fallback as too weak for Beryl-created threads.

## 2026-04-30: Refreshed inventory snapshots could publish stale generated titles

After GUI-local fallback generation was added, live testing showed the thread strip updating promptly while the thread selector popup still showed `Untitled thread` until later thread switching refreshed it.

The invalid assumption was that updating the current workspace conversation state plus the existing inventory row would be enough for all selector timing paths. In practice, a member-thread inventory refresh can already be running with an older cloned workspace state. If that worker finishes after the GUI-local title worker, it can publish a complete snapshot whose row labels were resolved before the generated title existed.

The course adjustment is to treat worker-built member-thread inventory snapshots as derived data that must be reconciled against the current workspace conversation state before publication to UI state.

## 2026-04-30: Prompt-prefix fallback was too weak

After GUI-local fallback generation shipped, recent Beryl-created threads received titles that were visibly just normalized prefixes of the first user prompt.

The invalid assumption was that a local best-effort fallback would be acceptable until app-server produced a better backend name. Further schema and source inspection showed app-server exposes no direct title-generation request or standalone non-history model call, so waiting for an app-server-generated title will not solve Beryl-created thread naming.

The then-current course adjustment was to make automatic thread naming a Beryl-owned maintenance workflow: use a centralized app-server ephemeral thread path to ask the model for a short title, create a fresh maintenance thread for each title attempt, clean it up after the attempt terminates, keep that maintenance thread out of every user-visible thread inventory and activation path, and publish the accepted title to the target thread through `thread/name/set`. The 2026-06-18 workspace-plus-Syndic catalog rework superseded the publication step.

## 2026-05-02: Publication was coupled to foreground turn completion

While investigating intermittent new-thread sessions that stayed `Untitled thread`, source inspection showed two coupling mistakes in the automatic title path.

The first invalid assumption was that publishing the generated title should wait for the target foreground turn to finish successfully. That made the title worker's timeout include foreground turn duration, so a long target turn could cause a title attempt to fail after the title itself had already been generated. It also made failed or interrupted target turns suppress a prompt-derived thread title even though the title attempt uses a separate background client and only mutates backend thread-name metadata.

The second invalid assumption was that title eligibility only needed to be emitted from the pending new-thread submit path. Beryl can create an unnamed backend thread before the first user prompt through graph or checklist start flows; the later first prompt then runs against an already selected thread and can miss the automatic-title trigger.

The then-current course adjustment was to make eligibility depend on Beryl ownership, missing manual/backend title, first submitted prompt, and known backend thread id. Title generation and `thread/name/set` publication ran on the background maintenance connection without waiting for the target turn's first assistant response or terminal state. The 2026-06-18 workspace-plus-Syndic catalog rework superseded the backend-name precedence, publication, and failure semantics described here.

## 2026-06-18: CAS thread-name publication is obsolete under workspace-plus-Syndic catalog authority

The workspace-plus-Syndic thread catalog rework invalidated the remaining backend-name publication step. Completion review showed automatic title generation still publishing accepted titles through CAS `thread/name/set`, and backend-name metadata could still suppress Beryl-generated titles in some paths.

That contradicted the target authority split in `doc/features/conversation-threads/design.md`: accepted title authority is Beryl workspace title metadata, while CAS thread names and live thread-name notifications are not selector, restore, or title authority.

The course correction is to keep the maintenance turn only as the live model worker, persist accepted titles as generated workspace title metadata, refresh catalog rows from workspace-plus-Syndic state, and remove automatic-title coupling to backend-name metadata. CAS `thread/name/set` is not part of the title workflow.
