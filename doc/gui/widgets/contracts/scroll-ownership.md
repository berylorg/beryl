# Name

Canonical name: scroll-ownership

# Purpose

Defines Beryl's shared ownership rules for scroll containers and scrollbar affordances.

# References

Widgets:

- scrollbar

# Applies To

Every Beryl UI region that owns a scrollable viewport unless an owning feature doc explicitly defines a narrower exception.

# Rule

A scroll container owns its viewport routing, scroll extent semantics, scroll position, and scroll-state callbacks.

Beryl uses one shared app-wide scrollbar affordance instead of per-widget custom scrollbar chrome. A scroll container renders that affordance unless its owning feature doc explicitly removes visual scrollbar chrome.

The shared scrollbar renders a thumb-only default style. The full outline or track remains visually invisible unless a concrete widget contract requires visible track treatment.

The thumb appears only after pointer movement, active scrolling, or direct scrollbar interaction within an overflowed scroll container. After pointer movement and scrolling stop, the thumb fades in and out around a short inactivity delay.

Pointer-wheel, touchpad, touch, keyboard, and programmatic scrolling act on the routed scrollable viewport. Thumb dragging in either orientation and vertical lane clicks originate from scrollbar chrome and route through the owning scroll container's callbacks. Horizontal lane clicks do not request scrolling.

Hover, wheel, fade, and thumb activity may invalidate scrollbar chrome, but they must not force broad content recomputation when the owning content viewport and scrollbar visibility state have not meaningfully changed.

Nested scroll containers do not automatically take pointer-wheel ownership merely because the pointer hovers over them. The owning feature or widget contract must define how a nested scroll container becomes the routed target.

Streaming scroll containers that omit continuous pixel extent for unrendered content preserve visual anchors for continuous pointer-wheel and touchpad input. They may lazily expand the rendered frame in the scroll direction, but they must not reinterpret a small continuous scroll delta as a command to place the next semantic segment at the top or bottom of the viewport.

Streaming scroll containers may opt into a bounded virtual trailing scroll allowance that increases scroll range without adding fake content. The allowance is capped by the owning viewport and caller's visual anchor so at least one real content line remains visible for orientation.
