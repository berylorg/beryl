# Accepted Source Actions Need A Terminal Refusal Path

Mounted Code Copy and selection remapping reuse ordinary indexed resource demand. Initially their
action state treated an absent matching response as pending, even after the containing panel had
rejected that exact demand. The panel's rejection list correctly suppresses the read, but it does
not itself finish the action that requested it.

Independent review found that a rejected Copy Source query retained its assembly and reserved
capacity, repeatedly notified rendering and prevented ordinary layout advancement. A rejected
selection Anchor query likewise retained unresolved endpoints indefinitely. The running-resource
callback also silently retried persistent provider errors after clearing its worker state. No
response could finish these states.

Terminal refusal must reach the exact current action through the existing request, owner and
publication fences. A matching Copy refusal drops its assembly and reservation, preserves valid
selection and reports local failure. A failed selection remap closes invalid endpoints instead of
exposing old positions against a successor source. The obsolete action query must be cleared so
ordinary foreground work can resume. Unrelated, superseded or retired failures must not cancel a
new action, and permitted manual retry needs no general automatic-retry framework.
A fresh explicit retry must retire its exact previous rejection marker or fail before admission;
changing only the action sequence cannot make an identical suppressed Source query readable.
Successful admission also wakes the ordinary resource owner once: retry changes eligibility even
when its Source query equals the previous tuple. Notifying only the panel, while emitting the root
resource event only for changed queries, leaves that fresh eligible command without a reader.

The same ledger also governs streaming Save. Testing only a direct action-refusal helper misses
production demand suppression and can hide a retry that leaves its staging writer waiting forever.
Save shares exact rejection retirement with Copy. Clearing is a one-shot admission fact for each
newly requested page: successful write progress admits the next page; rerendering an unchanged
waiting query cannot clear its terminal refusal or issue another read. Qualification must use the
production rejection route and prove that explicit same-source retry produces the complete file,
while the failed action releases staging custody and preserves its destination.
The relevant consumers are `SyndicTranscriptPanel::reject_resource_demand`, mounted Code source
actions and `MainWindowShellRoot::sync_running_transcript_resource`. Acceptance belongs in the
active [plan](../plan.md), under the [presentation boundary](../systems/transcript-presentation/shell-boundary.md)
and [Code widget contract](../gui/widgets/code-panel/spec.md). A bounded suppressed read is not a
terminal command outcome.
