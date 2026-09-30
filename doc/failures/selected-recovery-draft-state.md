# Selected Recovery Draft-State Mismatch

Phase 851 attempted to qualify existing selected resident attachment and whole-graph publication
using phase 849's real same-home native fixture. Ordinary startup, failed Exit with proven
noncommit, old-resource retirement, fresh private graph preparation and resident/appearance
attachment passed. The final publication/completion driver refused
`recovered composer autosave is not clean and idle`; completion and interaction release did not pass.

The new `native_exit_selected_session_publication_completes_same_home_recovery` regression uses
actual process, mount and draft custody. Run `26900ef6-8b98-49a2-bf37-0dfc01493728` failed at the
driver result. Diagnostic run `ae499685-9f69-48e1-ad55-85a75fc92fed` established the exact guard state:
`draft_dirty=true`, `host_dirty=false`, `timer=None`, `joined=None`. The temporary diagnostic was
removed. Both diagnostic test builds compiled successfully.

`MainWindowComposerSlot::prepare_recovered_autosave` requires both app draft state and host to be
clean. App draft-state comparison includes adopted/published root and history as well as candidate
generation; the host's dirty check compares candidate generation. Independent source review traced
the mismatch to an unchanged opening: Syndic's `has_saved_identity` accepts authenticated opening
history distinct from durable base history. Host reconstruction preserves that checkpoint and the
durable published pair, while app draft-state initialization treats their history inequality as
dirty. The observed flags match this production semantic mismatch. Earlier selected component
tests did not prove this real process path through completion.

Operator authorized correction on 2026-09-30. Phase 852 removes only history-reference inequality
from local dirtiness, retaining generation and root checks. Storage's real edit and historical
adoption paths advance candidate generation even when content returns to the same root. Exact
Syndic saved-checkpoint authentication remains in flush and reconstruction; the local predicate
does not replace it. No retained state, compensating publication or release-guard bypass was added.

The regression covers real empty and populated nonzero-generation openings, new edits, Undo back
to the published root while remaining dirty, and completed publication restoring cleanliness.
Existing captured-publication tests preserve newer edits. All 49 composer/lifecycle/opening cases
passed in `e89ca25e-2d5c-4ab7-a5b4-5e4a574836f0`; six selected preparation and shell-recovery cases
passed in `c19b1baf-d9ce-4efd-8079-8046e3174752`. Production compilation, scoped formatting and
independent review passed. The selected full regression now passes this guard and its recovery
completion assertions, then exposes the separate [host close-ticket defect](selected-recovery-close-release.md).
Phase 852 is accepted. After the separate host close-ticket correction, phase 851's one-window
proven-noncommit recovery case also passed with full cleanup and same-home reopen.

The failed test processes exited. Exact printed homes
`C:\Users\user\AppData\Local\Temp\.tmpK5QQNY` and
`C:\Users\user\AppData\Local\Temp\.tmpyKPKem` were removed with `cleanup-dir.exe` after verified
path/reparse checks and confirmed absent. No ambiguous temporary directory was swept.
