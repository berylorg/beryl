# Candidate Worker Custody

A preparation worker retaining its own slot until completion does not alone preserve abandoned
recovery custody. During phase 749 review, dropping the consumer left the detached task as the
last slot owner. It returned the candidate and keyed result, then dropped both after notification.
`HomeRecoveryCandidate::drop` deliberately forgets its home lifecycle custodian until process exit
(`crates/beryl-home-store/src/recovery.rs`); dropping this value is not explicit abort or cleanup.
A weak-pointer lifetime test incorrectly treated disappearance after completion as success.

The corrected worker returns a separate recovery-owned custody receiver. The notification consumer
may disappear while that receiver retains the returned candidate, source and keyed completion.
The abandonment regression must account for the exact completion and explicitly abort/close the
candidate, not merely observe that the task ended. The enclosing recovery owner must retain this
receiver through settlement; phase 744 connects that owner. No change to the app
catalog/composer recovery contract is required.

During phase 751 review, returning an owning completion while clearing the retained result slot
proved insufficient for cleanup acknowledgement. The caller could keep a returned page alive,
cancel the session, and drain its ledger after the slot appeared empty. The corrected API permits
borrowed inspection and delivers owned results directly into the exact widget session; cancellation
discards results under the retained owner. An empty slot then means delivery or disposal actually
occurred. Verify unconsumed results fence acknowledgement and mismatched sessions preserve custody.
Cancellation must discard its payload before terminal marking and notification; otherwise the
notification can drain cleanup while the return task still holds that payload. Undispatched effects
also need settlement through the retained receiver after the GUI worker disappears. A positive
abandonment-before-dispatch test must exercise a real emitted effect, not an empty remainder queue.

During phase 752 verification, expanding a shared mounted-test function with repeated owning
resource-result assertions overflowed the Windows test executor stack during authentication.
The reconstructed source occupied 51,424 bytes in that build; each owning result temporary added
to the caller's stack frame. Factoring extraction assertions and explicit resource closure into
small test helpers restored all seven scenarios without increasing stack limits or changing
production ownership. Avoid accumulating large owning-result temporaries in lifecycle fixtures.

During phase 744 review, chaining preparation advances with `App::defer` did not establish a
frame boundary. GPUI drains newly deferred effects during the same update, so successive widget
service calls could reset the per-frame realization budget without yielding a frame. Schedule
realization through `Window::on_next_frame`; use a retained window-close subscription and bounded
delayed cleanup when that window disappears. One queued continuation per flight prevents duplicate
wakes from accumulating. Cancellation must supersede the frame wake with an exact token because
minimized Windows windows suspend frames. Retain a weak reference to any abandoned frame wake and
refuse successor preparation until it disappears; otherwise repeated cancelled attempts can leave
an accumulating callback queue in a minimized window.

Native Code recovery qualification on 2026-10-11 confirmed that manually calling
`draw_and_present_for_test` on a minimized window does not consume the queued recovery frame wake.
Two bounded attempts retired the graph but remained pending until timeout
(`cca99907-9b18-44d6-b09a-9abff2e599ec`, `0a390528-5db3-4e3e-8db8-c4c603f05361`).
Use the existing test-only resident frame capture and dispatch seam when qualifying recovery with
the native window minimized. The corrected native case passed with all 47 panel regressions
(`97eaffe7-6e46-41a0-8add-2c95a2f8ad2a`). This qualifies recovery and provider-backed range delivery
under controlled frame dispatch; it does not prove native frame scheduling while minimized.

The first native phase 744 tests exposed a Windows background-thread stack overflow during source
authentication, leaving the worker pending. CDB stopped at `__chkstk` in history authentication
under saved-checkpoint validation and service reconstruction. Widget executor tests had missed the
native stack limit. Boxing only the final source, or splitting its construction, did not resolve
the nested large-value frames. Keeping the composer slot boxed inside its service and retaining
the existing box during reconstruction removed those copies; all seven native owner scenarios
then passed with unchanged thread-stack settings. Preserve native worker evidence for this path.
