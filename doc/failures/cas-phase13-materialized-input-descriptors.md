# Materialized Submitted-Input Descriptors

## Scope

Phase 13 submitted-image preparation and exact streamed `turn/start` correlation.

## Invalidated Approach

The first streamed-text boundary kept the complete CAS `UserInput` shape in memory. App preparation
retained one vector or map entry per protocol item, text run, text part, marker, distinct image
label, verified sidecar, and broker source. The backend then froze that input into another boxed
item sequence and built another vector while serializing the request.

Bounded text pages did not make this architecture bounded. Arbitrarily many markers or distinct
images still increased resident descriptors, paths, file handles, and hash-map capacity before the
first request byte. The started and completed echo verifiers depended on that same retained
sequence. The implementation therefore contradicted the package design's compact-cursor claim and
the process-wide bounded-resource requirement.

The same review found that input-local label comparison was not historical label authority. Asset
set staging proved only that repeated labels inside one set named the same asset. It did not compare
a reused current or inherited thread label with the immutable origin span and the origin set's
label-first entry. A later input could therefore reuse a historical label for another asset.

## Required Replacement

- Preflight traverses content, marker references, label origins, label-first entries, asset
  metadata, sidecars, and runtime paths with fixed resident state.
- Preflight publishes only compact immutable source authority: exact home/content/set revisions,
  item count, and a canonical descriptor-sequence digest.
- One replayable descriptor cursor produces the protocol sequence for request encoding, the started
  echo, and the completed echo. Every pass recomputes and checks the same count and digest.
- Text bytes move through caller-supplied admitted pages. A descriptor page never becomes a CAS
  protocol item, and no compatibility `Vec` is reconstructed below the cursor.
- One descriptor and one bounded text page are live at a time. Sidecar verification and path
  projection use fixed admitted guard slots and retain no handle per logical image.
- Admission resolves every reused current or inherited label through Syndic's immutable origin span
  and Beryl-state's label-first point read. Missing entries inside a reserved span and different
  assets reject before publication. The command's exact Syndic and Asset revision fences make the
  preflight proof safe against concurrent mutation.

## Trust-Boundary Resolution

The Operator explicitly placed arbitrary same-user filesystem tampering outside Beryl's correctness
contract. Beryl guarantees that its own processes never replace or delete a referenced
content-addressed sidecar. A verification handle therefore remains live only while Beryl performs
bounded length/digest verification and derives the runtime path; the handle is released before the
cursor advances. No handle per logical image remains resident.

CAS 0.144.1 still materializes `TurnStartParams.input` as `Vec<UserInput>` and clones the complete
content while constructing lifecycle items. That is an upstream CAS limitation, not permission for
Beryl-owned app, broker, encoder, or correlation layers to retain another proportional topology.
The Beryl boundary must still use the replayable descriptor source and fixed pages described above.

## Verification Lesson

A streamed payload is not fixed-resident when its topology remains materialized. Plateau tests must
vary marker count, distinct label count, text-run count, and image count while measuring descriptor,
page, path, and file-handle high-water marks. Exact request and both lifecycle echoes must consume
the same replayable authority rather than a retained item list.
