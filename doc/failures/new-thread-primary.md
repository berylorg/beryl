# Primary New Thread Integration

## Competing Picker Command Custody

A primary-pending host flag alone does not prevent an open picker from enrolling Confirm,
Add runtime or Add root before its host rejects the callback. Primary failure has no matching
picker command to finish, so that widget can remain permanently busy. A direct host duplicate
test misses this entrance. Fence actual widget acceptance before enrollment, preserve its original
command state, then verify failure restoration with a legitimate later widget dispatch.

## Unavailable Inspection And Input Admission

Blocking every key under an external unavailable reason also blocks Tab and unavailable-control
inspection, while pointer focus and scrollbars can still move. Reject command acceptance and
query mutation while preserving inspection focus, keyboard traversal and viewport scrolling.

Rejecting a query only after its Changed event is too late. Restoring bytes with `set_text` resets
caret selection, undo/redo, marked text and horizontal scroll. Reject the actual input mutation
through its existing admission mechanism and qualify the same real edit entrance with enabled
and fenced controls; query-byte equality alone does not prove retained edit state.

The pinned owned-value TextInput's enabled flag removed input routes on the next paint, but its
native replacement/IME callbacks and editing actions did not check that flag at delivery. A
previously painted Windows input handler could therefore mutate disabled search before repaint.
Keyboard capture and `simulate_input` missed that entrance. Guard the source-owned mutators and
qualify retained callback and action delivery directly, with the same enabled positive controls.
See [delivery investigation](../memory/github.com/berylorg/gpui-text-input/commit/f9651f8909f67f52f322b78547cb22f297572c0a/disabled-input-delivery.md).

The same retained-route check exposed selection-drag and multiline wheel callbacks that could
still change disabled selection or scroll. Include those existing mutation entrances in the
delivery guard and verify enabled drag/wheel controls. Removing routes on a later paint alone
does not establish disabled-state preservation.

## Keyboard Reachability And Geometry

Directly focusing both split-button segments did not prove real Tab reachability. The pinned GPUI
Div applies tab-stop settings only to internally created handles; configure each owned tracked
handle explicitly and test rendered Tab and Shift+Tab order. The focused investigation is retained
in [tracked focus notes](../memory/github.com/berylorg/zed-fork/commit/edd4928c5be424630da49f872e00dafbf94cf0b2/tracked-focus-tab-stops.md).

Changing an intrinsic label from `New Thread` to `New Thread…` can grow the joined button while
pending. Keep the label allocation fixed, place feedback within it, and compare actual bounds
through pending and unavailable presentation. Theme-owned feedback and visible inset focus must
not alter segment or toolbar geometry.

## Canonical Dependency Publication

Updating only Beryl's direct text-input pin leaves the Settings consumer on another Git source
identity. Cargo can retain both identities even when their package versions or resolved commits
agree; a precise update does not rewrite the consumer's manifest revision. Align the owned
consumer pin, qualify its unchanged source against the published correction, then regenerate the
root lockfile and verify one text-input package identity. Local path patches hide this mismatch.
