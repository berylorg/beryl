# Theme Repository Limits

## Invalidated Assumption

A theme operation's physical source allowance cannot be treated as the independent limit of the
file named by that operation. HomeStore also uses it to validate the operation's manifest snapshot,
and multi-file mutation staging shares that allowance between the manifest and document.

## Evidence

Independent review and the focused repository boundary tests exposed both directions of the
coupling. A small caller manifest allowance rejected a valid larger theme document. A supported
manifest above the 256 KiB document ceiling rejected document loading and Save/update even when
the document was valid. Small manifest reads also failed when the requested physical range exceeded
the caller's source allowance.

The first small-allowance fixture enlarged the source with comments, which canonical serialization
discarded. It therefore did not demonstrate the intended boundary. Generic physical limit errors
also lacked enough provenance to justify reporting every execution failure as manifest overflow.

## Correction

Keep the manifest policy allowance strict for manifest acquisition, decoding, freshness checks,
and complete transformed-output preflight. Give shared repository operations an envelope that can
accommodate both admitted file kinds, while enforcing the document ceiling separately. Audit final
document freshness observations as well as initial loads and mutation staging. Clamp physical read
range requests to the operation's source allowance.

Use canonical document content in regression fixtures and assert the published byte length before
testing readback. Exercise document operations against a legal manifest larger than the document
ceiling. Preserve typed manifest count and byte causes; generic repository errors remain repository
errors when their file origin is not known.

Known manifest-reader boundaries also preserve physical byte-limit provenance when external growth
occurs before execution; errors from document operations are not reclassified by that mapping.

The correction implements the existing Theming, theme-runtime, and state theme-service contracts.
Focused state/app verification and follow-up semantic review passed, including small-caller
readback, capacity operations, complete-output byte refusal, and external manifest growth.
