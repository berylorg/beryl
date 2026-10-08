# Frozen Read Retention

Removing a released slot from the admission registry before draining its admitted requests does
not release its snapshot ownership. Independent integrity review on 2026-10-08 found that another
capture could reuse the advertised finite budget while the removed snapshot still pinned its
generation; concurrent draining releases could therefore exceed that budget indefinitely.

The registry must keep a closing slot charged until its admitted requests and internal snapshot
references have drained. Its identity refuses new requests during that interval. Only completed
drain removes the slot and frees admission; capture identities remain non-reused. The held-request
release/admission test in Home's frozen-read lifecycle suite covers the distinction. This corrects
implementation of the existing generation-owned finite-retention contract without changing its
design, capacity or phase boundary.
