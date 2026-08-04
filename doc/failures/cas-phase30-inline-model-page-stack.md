# CAS Phase 30 Inline Model Page Stack ABI

## Scope

Checkpoint 3 Phase 30 bounded response decoding in `beryl-backend`.

## Invalidated Approach

`ModelPage` stored 64 fixed model records inline, and both `CompatibilityProbeResult::ModelList`
and `BoundedResponseResult::ModelList` retained that page directly. The representation was bounded
and content-independent, but every enclosing enum and function return ABI inherited the maximum
page size even for non-model paths.

## Evidence

`cargo check -p beryl-backend --all-features` passed. Then
`cargo nextest run -p beryl-backend --features lifecycle-test-support --test incoming_json_ingress`
compiled and began 35 tests. All four predispatch gap tests passed. Twelve tests, including ordinary
classifier and approval paths, exited on Windows with stack overflow `0xc00000fd`; nextest cancelled
the remaining nineteen. The first observed abort was
`responses::a_previously_poisoned_expectation_never_recovers`.

## Why It Failed

The fixed page is roughly 100 KiB. Nesting it inline in the public result enums made
`DecodedIncoming` and intermediate result returns equally large, causing repeated stack moves and
reservations throughout all ingress paths rather than only during `model/list` decoding.

## Course Correction

The Operator approved one fixed-size heap allocation selected only for the `model/list` family and
carried unchanged through the decoder and both result variants. This is not a logical-size escape:
the page remains exactly 64 fixed slots, and Phase 31 must reserve its exact fixed retained byte
cost before installing a live expectation. Verification must prove the correction removes the
large enum ABI without introducing a second page allocation or changing predispatch behavior.

## Verification

The corrected boundary has one `Box::new(ModelPage::new())` site, compact outer result enums no
larger than 1,024 bytes, and no second page allocation. The focused 43-test response suite and full
200-test backend suite pass on Windows, including the formerly overflowing ingress paths and all
four predispatch gaps. Formatting, all-features checking, forbidden-surface scans, and fresh
independent completion review also pass.
