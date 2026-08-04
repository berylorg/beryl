# Scope

Phase 29 foreground approval retention before ordered turn-stream binding.

# Invalidated Approach

The first implementation used an inline 64-entry ring for the pre-bind approval prefix in addition
to acquiring one `ForegroundCompactControlSlots` reservation per approval.

# Evidence

The bounded-resource and backend authorities make the shared process capability the sole count
authority for retained pre-bind compact controls. An existing app integration policy registers 128
`ForegroundCompactControlSlots` in `crates/beryl-app/tests/phase10_projection/syndic.rs`, so the
local ring could reject the sixty-fifth approval while 64 valid process slots remained available.

# Why It Failed

The ring introduced an independent local governor unrelated to the active process policy. It could
close an otherwise admitted candidate early, made runtime capacity changes ineffective above 64,
and contradicted the requirement that each admitted slot itself bound the retained prefix.

# Course Correction

Use no local cardinality constant. Acquire the process slot before retaining approval identities,
then let that reservation cover exactly one queue node and travel with the approval through FIFO
drain, ordered acknowledgement, or abandonment. Prefix exhaustion must therefore occur only when
the shared capability denies the next slot.

Phase 29 verification must exercise a capability above 64 entries and prove that the prefix has no
earlier local limit, then separately prove exact release when the configured capability is
exhausted.
