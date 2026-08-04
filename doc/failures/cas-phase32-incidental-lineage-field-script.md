# CAS Phase 32 Incidental Lineage Field Script

## Scope

Checkpoint 3 Phase 32 incremental decoding of `thread/start`, `thread/resume`, and `thread/fork`
responses in `beryl-backend`.

## Invalidated Approach

The first lineage result and nested thread machines encoded every field emitted by the pinned CAS
0.144.1 producer as one exact mandatory script. The status machine also rejected unrecognized
object members and active-flag strings.

## Evidence

Independent Phase 32 completion review compared the implementation with the root plan and
`crates/beryl-backend/doc/design.md`. Both authorities require arbitrary incidental response fields
and unknown status flags to be structurally discarded. The original fragmented malformed-response
test even asserted the contrary behavior for an unknown active flag.

## Why It Failed

Pinned producer order is evidence for locating Beryl's required semantic fields; it is not authority
to promote every incidental producer field into Beryl's acceptance contract. The exact script made
unretained history, path, source, and future metadata schema-significant and contradicted the
bounded structural-discard boundary.

## Course Correction

The result, thread, and status machines now classify only their required retained targets, enforce
those targets once and in relative pinned order, and route every other member through the fixed
structural value tracker. Known duplicate flags remain malformed, while unknown flag strings are
discarded. Fragmented tests inject 96 KiB of incidental content at all three nesting levels and
prove fixed buffering and exact compact publication.

## Affected Authority And Verification

The target design and Phase 32 plan did not change; the implementation was brought back into
conformance. The pinned-source memory note remains producer-order evidence, not a decoder
acceptance specification. The corrected 49-test ingress and Phase 32 suite, complete 223-test
backend suite, backend/app all-features checks, formatting and static audits, and fresh independent
completion review all passed.
