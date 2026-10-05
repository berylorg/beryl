# Clipboard Provenance Replay Collision Termination

Scope: captured composer paste, widget provenance replay qualification, 2026-10-05.

A proposed public replay wrapper delegated acknowledgement directly to the existing internal
provenance collection. The internal collection returns a collision classification while retaining
the current page; its original coordinator separately terminates the owner. The wrapper omitted
that terminal transition, allowing a caller to acknowledge the original page after a same-key
structural collision and continue to a valid closure.

Independent review identified the missing transition before publication. The corrected wrapper
drops its collection on a collision, releases the current bounded allocation and rejects every
later push, emission, acknowledgement and closure. A wrong-key acknowledgement still preserves
the current page. A public regression proves both terminal release and rejection of continuation;
canonical clipboard qualification passes all 24 cases.

When exposing a lower-level helper, preserve the lifecycle behavior supplied by its former owner,
including terminal transitions outside the delegated method.
