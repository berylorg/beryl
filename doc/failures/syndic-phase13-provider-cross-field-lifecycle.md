# Provider Field Grammar Does Not Prove Cross-Field Lifecycle

## Context

Syndic's unpublished provider validator independently proves exact field types and enum domains
before item 8 may trust a sealed observation for atomic publication.

## Invalidated Shape

Validating a status enum against its field vocabulary and validating the observation lifecycle
independently is insufficient. The first exact-schema correction allowed a completed standalone
image-generation observation to retain `InProgress`: the status was legal for that item kind and
all required fields were present, but the lifecycle/status combination was not a legal completed
state.

The same class of error can occur for any status-bearing item whose started and completed
observations share one field schema. Upstream validation is not a substitute because sealed Syndic
staging is the restart-safe publication input.

## Replacement

Destination validation owns explicit cross-field completion rules in addition to field-local value
rules. Completed command-execution, file-change, MCP-tool, dynamic-tool, collaboration-tool, and
standalone-image observations all reject their kind's `InProgress` value, while started and
completed legal statuses retain their exact pinned vocabulary. Backend ingress enforces the same
closed relationship early; neither boundary substitutes for the other.

Tests pair started and completed positive cases with illegal lifecycle/status combinations and
prove restart/replay cannot erase the discriminating status state.
