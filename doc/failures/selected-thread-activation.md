# Selected-thread activation

## Renderer-owned admission boundary failure

- Scope: startup restore and explicit existing-thread activation after sliding-window transcript residency, prepublication preparation, and media admission were introduced.
- Invalid assumption: a selected thread could remain staged until renderer-owned Markdown/media admission reported every completed media candidate settled.
- Evidence: live testing on June 12, 2026 against copied real Beryl home `real-home-copy-20260612-181411` and thread `019eba02-fba7-72e2-8e4e-18ab69e5e978` showed the backend fetch completed and staged 52 turns while the GUI either stayed on `New conversation` at startup or kept rendering the previous selected thread during explicit switch. Diagnostics showed `backendWorkReceivers = 0`, staged target id present, structural readiness settled, prepublication settled, but media admission still pending.
- Why it failed: selected-thread publication was an app-state transition, but liveness depended on renderer-owned Markdown parsing, media admission, and cache scope belonging to the currently visible transcript. The previous selected thread could continue driving renderer cache work while the hidden staged thread waited for admission state that was not required to select coherent resident text rows.
- Course correction: selected-thread publication now depends on activation-owned readiness: fetched full-detail resident rows, structural row presentation readiness, and bounded prepublication preparation. Renderer-owned Markdown/media admission remains as post-publication warming and stable row-owned media placeholder/fallback work, but it cannot veto selecting the fetched thread.
- Affected design docs: `doc/features/conversation-threads/design.md` and `doc/features/transcript/design.md` now distinguish activation-owned publication readiness from renderer-owned media admission.
- Affected tests: keep `selected_thread_publication_is_not_gated_by_renderer_media_admission` in `conversation_shell_source` and retain live diagnostic coverage through `read_ui_state.pendingActivation` and `read_ui_state.markdownCache`.

## Build-target verification failure

- Scope: live diagnostic verification of selected-thread activation fixes.
- Invalid assumption: `cargo build -p beryl-app` was sufficient before launching `target\debug\beryl.exe`.
- Evidence: after rebuilding only `beryl-app`, the diagnostic child still exhibited the old media-gated publication behavior. Rebuilding the actual root binary with `cargo build -p beryl` made the same copied-home test publish the target thread and restore it on restart.
- Why it failed: the diagnostic child runs the root `beryl.exe` binary, not a crate-local `beryl-app` executable.
- Course correction: when live-testing Beryl GUI behavior through the diagnostic child, rebuild the binary package that owns the executable path being launched. For `target\debug\beryl.exe`, run `cargo build -p beryl`.
