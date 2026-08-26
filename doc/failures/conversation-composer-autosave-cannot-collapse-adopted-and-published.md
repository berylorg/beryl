# Conversation Composer Autosave Cannot Collapse Adopted And Published Frontiers

## Scope

Phase 181 autosave publication while the selected composer continues accepting later settled edits.

## Invalidated Approach

Represent adopted and published draft state as two complete host bindings, and complete a save only
when the currently adopted binding still equals the captured binding. On success, replace both
frontiers with the host's current binding.

## Decisive Evidence

The product contract keeps editing available while autosave publishes. If edit A is captured and
edit B settles before A's durable publication completes, the host may correctly publish A while B
remains the newest adopted editor state. Requiring the adopted binding to equal A rejects that
valid completion; replacing both bindings with the host's current B state would falsely mark B as
crash-durable even though only A was published.

## Course Correction

- Retain the newest adopted editor binding separately from compact exact published
  candidate-generation, root, and history facts.
- Authenticate completed publication from the host's durable published frontier.
- Advance only the published facts when an older capture succeeds, while refreshing session
  metadata on the adopted binding without replacing its candidate.
- Keep the successor dirty and its existing autosave deadline armed.
- Store no text, marker collection, inverse value, history graph, or whole draft in either
  frontier.
