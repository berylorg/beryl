# Staged Outcome Stack Frames

The first staged-command wrapper kept large command, capture and result payloads inline. Windows
tests exhausted the default stack during transfer and later terminal preparation. Measurements
showed a 52,408-byte prepared owner and a 102,624-byte outcome flight before the correction.

Boxing the retained payloads alone was insufficient: the serialized preparation dispatcher still
held large locals for multiple mutation branches in a single debug stack frame. Instrumentation
localized the overflow to underlying preparation after entering that dispatcher.

The partial implementation now boxes private payloads and passes boxed arguments/results through
separate per-mutation preparation functions. Transfer fault, ambiguous progress/settlement, and
terminal-election tests pass on the default stack. No runtime stack increase is required. Keep
public ownership move-only and bound both retained heap data and transient call-frame storage.

This correction does not establish staged-outcome acceptance. Remaining behavior and tree-builder
failures are recorded in [the implementation plan](../plan.md).
