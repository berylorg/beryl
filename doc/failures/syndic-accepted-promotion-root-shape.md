# Syndic Accepted-Promotion Root Shape

## Invalid Approach

Treat accepted-promotion `root/nonroot turn shape` verification as requiring promotion to
create both a root turn and a non-root turn.

## Evidence

Accepted-input admission is available only while the input gate is non-idle and names an existing
blocking turn. When that gate later becomes idle, promotion parents the fresh pending ordinary turn
to the precommit committed tail.

The controlling contracts are
`doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, and
`doc/features/composer/design.md`.

## Why It Fails

A production accepted input cannot exist on a thread with no committed tail. A root promoted turn
would therefore require an impossible or corruption-only fixture and would not verify supported
behavior.

## Course Correction

Verify promotion when the precommit tail is a root turn and when it is a deeper non-root turn.
Both promoted successors are non-root; the cases prove depth-two and deeper parent, chain-digest,
and ancestor-skip construction.

## Remaining Question

None.
