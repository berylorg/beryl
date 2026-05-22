# Scope

Implement and harden transcript Markdown links of the form `[label](beryl_threadid://<percent-encoded-thread-id>)` so activating the rendered label opens the exact registered conversation thread when that thread belongs to the current Beryl workspace.

The feature must preserve the existing ownership boundaries:

- Transcript history remains backend-owned and is only rendered as transient presentation state.
- `beryl_threadid://` is a Beryl-internal link scheme, not an OS URL.
- Link activation uses already known workspace thread registration and the existing exact thread activation path.
- A click must not synchronously enumerate backend threads, infer another runtime/member, create a GUI-local thread record, or fall back to any alternate thread.
- Unknown, malformed, empty, unregistered, out-of-scope, or rebind-required thread links fail with bounded Beryl UI notice behavior and leave the active thread unchanged.

Current code already contains partial support: branch bootstrap messages emit `beryl_threadid://` destinations, the parser round-trips percent-encoded thread ids, transcript inline rendering records Beryl thread-link hit ranges, and `ShellView::activate_beryl_thread_link` routes to the existing thread activation path. This plan focuses on documenting the product contract, filling test gaps, and making any small code corrections found while verifying the click path.

# Phase 1: Document Contract, Activation Gates, And Nested Update Safety (wip)

Tasks:

- Update `doc/features/transcript/design.md` to explicitly define `beryl_threadid://` Markdown links as Beryl-internal transcript thread links.
- Update or cross-reference `doc/features/conversation-threads/design.md` if needed so transcript thread-link activation is governed by the same exact activation, runtime/member scope, rebind, backend-availability, and busy-work gates as thread selector activation.
- Extract or reuse a small pure availability helper if the transcript link path duplicates graph thread-ref availability logic in a way that can drift.
- Fix the observed transcript link activation panic by ensuring thread-link clicks do not update `ShellView` synchronously while `TranscriptPanel` is already being updated.
- Add focused unit coverage for activation eligibility:
  - registered workspace thread is openable;
  - missing registration is rejected without inventory lookup;
  - rebind-required registration reports `Thread requires rebind`;
  - registered target outside current workspace scope is rejected;
  - implicit-home workspace scope only accepts the exact implicit-home target;
  - fallback label for blank resolved titles is stable.

Edge cases and verification:

- Percent-encoded thread ids, malformed percent escapes, invalid UTF-8, empty decoded ids, and non-Beryl schemes must not become activation targets. Existing parser tests cover part of this; add missing cases if any are absent.
- Activation failure must preserve active-thread state and show a bounded surface notice rather than mutating transcript state.
- Transcript link activation must be deferred out of the transcript panel mouse handler to avoid GPUI nested entity update panics, including double-click paths.
- Verify with focused `cargo nextest run -p beryl-app --test branch_bootstrap_core --test thread_selection` and any new focused test target added for the helper.

Resumable milestone:

- 2026-05-22 progress: fixed the observed nested update panic by deferring transcript thread-link activation out of `TranscriptPanel::handle_transcript_mouse_down`; added a source-level regression guard; recorded the failure in `doc/failures/transcript-presentation.md`; verified with `cargo fmt`, focused `cargo nextest run -p beryl-app --test conversation_shell_source`, `cargo check --workspace --all-targets`, and `cargo build --release -p beryl`.
- Stop after the remaining docs and eligibility tests pass. Phase 2 can then assume the activation contract is explicit and test-covered.

# Phase 2: Verify Transcript Link Rendering And Hit Behavior (pending)

Tasks:

- Add focused tests around transcript inline Markdown rendering or a small extracted pure helper so `beryl_threadid://` links produce `TranscriptSelectableThreadLink` ranges only for visible rendered link text.
- Cover link text split across inline styles such as emphasis, strong, and inline code so adjacent fragments for the same thread id merge into one clickable range.
- Ensure ordinary Markdown copy/selection semantics still preserve the original link Markdown source.
- Decide and implement the least disruptive pointer behavior for V1:
  - single activation click should open the link;
  - text selection must remain possible for surrounding transcript text;
  - if GPUI span-level pointer affordance is not practical, do not add a misleading whole-line pointer cursor.

Edge cases and verification:

- Non-Beryl links, malformed Beryl links, Beryl image links, and link destinations inside unsupported fallback source must not create thread activation hit ranges.
- Thread-link hit ranges must be based on display text offsets after image-marker atom substitution and inline styling, not Markdown source offsets.
- Existing image marker click behavior must continue to win only on image marker spans; thread-link clicks should not open image previews.
- Verify with focused `cargo nextest run -p beryl-app --test transcript_selection` plus the new transcript thread-link tests.

Resumable milestone:

- Stop after rendered transcript lines produce correct selectable thread-link hit metadata and selection/copy behavior remains intact.

# Phase 3: End-To-End Activation Smoke And Final Review (pending)

Tasks:

- Add a diagnostic-child or shell-level smoke path where a rendered transcript containing `beryl_threadid://<registered-thread>` is activated and the selected thread becomes the target thread through the normal activation worker.
- Smoke the rejection path with an unregistered or out-of-scope thread id and confirm a surface notice appears while the selected thread remains unchanged.
- Run formatting and broader verification:
  - `cargo fmt`
  - `cargo check --workspace --all-targets`
  - focused `cargo nextest` tests added or touched by this plan
  - a release build if shell activation code changes in a way that merits it.
- Request reviewer coverage when all phases are complete because this touches transcript rendering and thread activation behavior.
- Leave `doc/plan.md` empty after the reviewer issues are addressed and all phases are finished.

Edge cases and verification:

- Backend-unavailable target state must reject only that target and must not switch to another runtime or workspace member.
- Busy workspace/transcript/status/turn work must block activation with the existing busy rejection.
- Activating an already selected linked thread should be a no-op success, matching existing selector behavior.
- Thread activation pending state should show the target label rather than leaving the old transcript looking idle.

Resumable milestone:

- Finish with documented verification results and reviewer outcome before clearing this plan.
