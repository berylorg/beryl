# Goals

Report user-visible errors, recovery states, and completion attention signals without replacing the active conversation shell or changing backend conversation semantics.

## Non-goals

- Using notices as persistent logs.
- Playing notification sounds for background maintenance work.
- Letting the model choose sound paths, volume, resume prompts, or compaction strategy.

# Decisions

## GUI Supplement

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for mounting and
  configuring the project-local `main-window notice` widget. Reusable notice anatomy and variant
  presentation live in that widget's spec.

## Main Conversation Notices

- Main conversation notices are bounded transient messages associated with one main conversation
  window. Their overlay placement and widget composition are owned by `gui.md`.
- Notices report localized errors and recovery information that should not replace the active conversation shell.
- Notifications is the sole per-window presentation owner for every main-conversation notice. Beryl-home,
  backend-runtime recovery, status-line stop feedback, and ordinary feature errors contribute
  bounded design-owned notice records; they do not mount or order competing notice
  widgets independently.
- Each window renders exactly zero or one active notice. Notice panels
  never stack, overlap one another, or reserve multiple overlay positions. A replacement reuses the
  same widget anchor and publishes one visible record transition.
- Priority from highest to lowest is: persistent Beryl-home failure or reopening; exact stop
  feedback for the selected request; lifecycle-yield review, operator-attention, completion, or
  continuation-failure records; persistent selected-thread runtime/backend unavailability; ordinary
  turn or command error; successful home/runtime recovery information; ordinary warning, including
  the best-effort-home warning; and ordinary information. Records remain FIFO within one priority.
- A higher-priority arrival preempts the visible lower-priority record. The preempted record returns
  to the front of its own priority when it is still eligible; equal- or lower-priority arrivals wait.
  Preemption is replacement, not dismissal, and never acknowledges the underlying condition.
- Pending notices have a fixed count capacity and each notice has a fixed byte ceiling. A notice derived
  from an arbitrarily large backend or storage error retains only a bounded, explicitly truncated
  display projection or compact summary; it never clones the complete source payload.
- The bound always leaves simultaneous Beryl-home, selected runtime/backend, and selected exact
  stop-feedback conditions representable. Repeated reporting of one condition updates its one
  eligible notice rather than consuming another pending position.
- Dismissing the visible notice advances to the highest-priority oldest eligible record when
  present. A persistent record has no close action and remains eligible until its owning condition
  ends, although a higher-priority record may temporarily preempt it.
- Another report for the same eligible condition updates that notice's bounded count, content,
  commands, or summary in place. Persistent Beryl-home, exact-stop, and selected-runtime conditions
  are never discarded for an ordinary notice. At capacity, the newest queued record at the lowest
  lower priority is replaced; when no lower-priority record is replaceable, the new ordinary
  arrival is omitted and counted only in content-free diagnostics. Unrelated conditions are never
  merged.
- Repeated reports for the same selected foreground turn failure are deduplicated so Beryl enqueues at most one notice for that failed turn.
- Notice text is selectable and ordinary copy commands copy only the bounded notice text.
- The visible close action dismisses only the current notice and must not mutate transcript, thread, backend, settings, or persistence state.
- Notice title, detail, background, border, and warning/error/info variants resolve from active theme notice roles.
- Contributing feature designs own when their exact notice state exists, its bounded content,
  commands, and resolved outcome. Notifications owns admission, priority, preemption, persistence,
  replacement, dismissal, and the single visible-record projection.
- Home and runtime failure records are persistent condition records. Reopening or retry progress
  updates the same stable record rather than stacking another notice. When the condition resolves,
  Notifications removes that persistent record; a feature-owned recovery confirmation is a distinct
  dismissible informational record and waits behind every still-eligible higher-priority record.
- Ordinary error, warning, recovery, and informational records are dismissible unless their owning
  feature explicitly defines an unresolved condition as persistent. Dismissal never suppresses a
  later distinct condition or operation identity.

## Best-Effort Home Warning

- Each main conversation window restored or created after a successful best-effort Beryl-home open
  presents one warning that Beryl acquired exclusive ownership for the current home but the
  filesystem has reduced durability guarantees compared with native local NTFS.
- The Beryl-home feature supplies only the successful best-effort-open trigger. Notifications owns
  the warning record, priority, at-most-once admission, and dismissal lifecycle.
- Notifications admits exactly one warning for each `(startup generation, main-window identity)`.
  Repeated triggers, rerenders, queue replacement, manual dismissal, and automatic dismissal never
  enqueue it again; a main window created later in the same process startup receives its own one
  warning.
- The warning is dismissible manually. Manual dismissal removes that exact visible warning,
  cancels its timer, and advances the queue without changing home or shell state.
- Its automatic-dismiss timer runs only while that exact warning is the active visible notice, not
  while it waits or is preempted. Preemption cancels the current arm; if the warning becomes visible
  again, a fresh five-second arm starts. Expiry dismisses only that still-active identity and
  revision and advances to the next eligible notice; a stale timer cannot dismiss its replacement.
- The warning never blocks the shell or changes the selected thread.

## Turn Error Notices

- When a selected user-visible parent turn fails with backend error detail or local turn-delivery
  failure, Beryl enqueues a `Turn error` notice with the bounded available-detail projection.
- Turn-error notices enter the error priority and therefore precede queued warnings and information
  after the current visible notice ends. Beryl must not stack notice popups or merge unrelated
  errors into one body.
- Interrupted turns without actual error payload update turn status but do not enqueue a turn-error notice by themselves.

## Exact Stop-Feedback Notices

- The status-line feature contributes at most one exact stop-feedback record for the selected
  request when its anchored popup can no longer retain required request progress or outcome
  feedback. Notifications uses the opaque exact feedback association supplied by the stop system;
  it never reconstructs a key from visible target facts or assumes that a durable stop operation
  exists. Feedback therefore cannot move to another turn or compaction.
- While exact feedback says the request still awaits terminal or authority-loss convergence, its
  notice is a persistent warning. Updates replace that same record in place; dismissal, duplicate
  activation, timeout, or missing backend error detail cannot create another interruption request
  or make the record claim completion.
- A separately eligible durable stop outcome that is proven locally nondispatched becomes a
  dismissible error and may report that `Soft stop` is available again only when the status line
  receives a fresh system eligibility fact. Nondispatch alone never implies eligibility.
- Volatile nondispatch becomes a final dismissible request-failure error. It offers no command,
  re-enable, retry, join, durable-stop claim, or terminal-turn claim.
- Exact interrupted terminal completion becomes a dismissible informational record. Final
  authority loss or unknown-terminal convergence becomes a dismissible warning with the bounded
  known outcome.
- Stop-feedback title and detail are derived from exact stop state, not from the presence of an
  error payload. An interrupted turn with no backend error payload therefore still receives the
  exact stop-completion feedback record when notice fallback is required, while it remains
  ineligible for the separate `Turn error` notice above.
- When the status popup can safely retain the feedback, no duplicate notice record is contributed.
  If popup eligibility later disappears while feedback is still required, Notifications receives the
  same exact record identity and latest bounded revision.

## End-Turn Sound

- Beryl supports an optional app-wide end-turn sound for completed user-visible parent turns.
- The default setting is empty, so no end-turn sound plays by default.
- V1 sound files are WAV files selected by full host filesystem path in settings.
- When configured, the sound plays only after a user-visible parent conversation turn reaches terminal state and at least one known attention trigger below is active; lack of Beryl-window focus is one trigger, not a separate prerequisite.
- A Beryl window is focused when either a main conversation window or settings window has OS focus.
- Terminal states eligible for sound include successful completion, interruption, and failure.
- Beryl does not play ordinary end-turn sound for title-generation maintenance, catalog projection maintenance, lazy metadata resolution, context compaction, automatic lifecycle continuation, startup validation, settings changes, or other background/status-only work.
- Playback is best-effort, must not block conversation-shell interaction while the sound is read or
  played, and must not affect backend turn completion semantics.
- All notification sounds share one process-wide playback lane. At most one event may be reading,
  decoding, or playing and at most the latest eligible event may wait. A later eligible event
  replaces the waiting event without interrupting active playback, so sounds never form an
  unbounded backlog.
- After the active attempt ends, the lane may start only the latest still-waiting eligible event. A
  replaced waiting event never sounds. Orderly application shutdown ends active playback, discards
  the waiting event, and starts no further sound.
- Changing or clearing the configured sound applies to later events; once an already-started
  playback ends, the prior selection is no longer retained for notification use.
- If the configured WAV file is missing, unreadable, unsupported, exceeds a finite notification-
  audio limit, or cannot be played at turn completion time, Beryl treats playback failure as non-
  fatal, leaves turn state unchanged, and records the failure through normal diagnostics.

## Attention Triggers

- User-visible turn-completion sound is a GUI-local desktop notification side effect.
- It may be emitted only when at least one Beryl-owned attention trigger is active.
- Beryl-owned attention triggers include no Beryl window focused, no host-reported local mouse or keyboard input for 30 seconds, a locked desktop session, a closed laptop lid, or a host-reported off or dimmed session display.
- Eligibility is the logical OR of those known trigger states: any one active trigger is sufficient even when every other trigger is inactive, including when a Beryl window remains focused.
- Unsupported or unknown trigger states do not make a notification eligible by themselves and must not suppress another known active trigger.

## Lifecycle Notifications

- AI lifecycle yield notifications are separate from ordinary end-turn sound.
- For each exact accepted yield attempt whose outcome requests review, operator attention, or plan
  completion, Notifications admits at most one lifecycle-yield record after that turn reaches its
  terminal state. A `phase_continue` attempt contributes at most one continuation-failure record
  only when Lifecycle Yield reports its bounded continuation-failure outcome. User-input
  precedence, soft-stop cancellation, window-close cancellation, and successful automatic
  continuation do not contribute that failure record.
- One lifecycle notice is associated with the destination window, exact yield attempt, and selected
  outcome. Repeated terminal observation, rerender, sound handling, or failure reporting updates
  that record in place and never presents a duplicate.
- The destination is the main conversation window that owned the yielding turn. The record is not
  broadcast or moved to another window, even when another window later selects the same thread.
- Lifecycle-yield records use the priority declared above: exact stop feedback preempts them, and
  they preempt runtime-unavailability, recovery, and ordinary records. Preemption returns an
  unacknowledged record to the front of its priority and does not acknowledge or remove it.
- A lifecycle-yield record does not auto-dismiss. Its visible close action acknowledges and removes
  only that exact record; it does not validate the model-reported outcome, change plan state, resume
  work, or alter the originating turn.
- Content is concise, localized, and host-owned: a bounded title identifies review readiness,
  operator attention, plan completion, or continuation failure, and bounded detail may identify the
  originating thread without copying model-supplied explanation or transcript content.
- If the bounded pending-notice surface cannot admit the record or the destination window no longer exists, Beryl
  shows no substitute record, does not retry, broadcast, or displace a protected condition record,
  and records only content-free diagnostics. The accepted lifecycle outcome and any separately
  requested sound remain unchanged.
- Outcomes that stop for operator attention or report plan completion may use event-specific sounds chosen by Beryl policy.
- The model never supplies a sound path, sound identity, volume, resume prompt, or compaction strategy.
