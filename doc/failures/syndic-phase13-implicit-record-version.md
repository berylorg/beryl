# Syndic Phase 13 Implicit Record Version

## Scope

Phase 13 incompatible Syndic V2 record-shape replacement and physical codec registration.

## Invalidated Approach

The shared `Family` codec trait defaulted every family to record V1, and the compact secondary-
family macro relied on that default instead of making the persisted version explicit.

## Evidence

The authoritative V2 schema assigns `accepted-order` record V2 because the replacement record adds
route-generation authority. Its codec still inherited record V1 from the trait. The physical-
corruption fixture also hard-coded versions one and two, so malformed-payload and unsupported-
version cases could exercise the wrong failure for V2 families while still observing a generic
validation rejection.

## Why It Failed

A persisted record version is part of the schema, not an implementation convenience. An implicit
V1 default allows an incompatible value shape to change without a corresponding physical-version
change, and a non-family-relative corruption fixture cannot prove the selected version boundary.

## Course Correction

Every `Family` implementation must now declare its record version explicitly; the trait supplies
no default. `accepted-order` declares V2, every unchanged or new family declares V1, and the other
changed families remain V2. Unit proof asserts the complete changed-family V2 set. Physical-
corruption fixtures derive the malformed record's supported version and the unsupported successor
from the exact family under test, so V1 and V2 codecs exercise the intended rejection paths.
