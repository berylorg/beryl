# Indexed Code Producer Revision Misdiagnosis

Operator corrected this diagnosis: CAS data goes through Beryl/Syndic chunks, is saved in history,
and the renderer consumes those history chunks throughout streaming. CAS message completion is
not the intended publication or rendering boundary. The earlier blocker and recommendation to
wait for final Code mounting are withdrawn.

Phase 762 requested native producer-driven source-changing revisions of an already mounted indexed
Code panel. A fixture admitted a Started assistant frame containing a closed 2,000-line fence,
projected its narrative through normal commands, and planned to replace that text with a Completed
frame after native scrolling.

Run `5a9b696e-9c1c-46a1-8cf2-6edfa62bf096` built successfully but failed after 15.253 seconds:
the initial native range wait reported `(None, None, [])`. The native window entered the scenario;
the streaming assistant had no mounted indexed Code resource. The earlier attempt to reopen an
active-turn fixture timed out during startup (`0117f38c-094f-4fff-9b81-49508a106a5c`, 35.280 seconds)
and does not qualify the running capture path.

The investigated finalized-history publication path explained why this fixture had no panel.
[Transcript publication](../../crates/syndic-storage/src/mutation/transcript/publication.rs) stopped
before items beyond `finalized_item_count` and requires `validate_projection_source` to return true.
[Source validation](../../crates/syndic-storage/src/mutation/projection/lifecycle.rs) returns the
provider stream's completion state for narrative sources. A Started item's closed fence can have
an item projection but could not become an indexed historical transcript entry in that path.

Once the item completes, [subsequent frame admission](../../crates/syndic-storage/src/mutation/live/event/item.rs)
requires the current canonical provider lifecycle to be Started. A second completion or delta cannot
legally edit an already finalized indexed historical item through ordinary capture. Directly changing
stored metadata or supplying a response would not prove producer-driven revision of that panel.

The invalid native fixture was removed. The mistake was elevating restrictions in one current
implementation path into product authority and concluding that streaming indexed Code required
a new design decision. These source restrictions do not establish that the intended Beryl/Syndic
chunk publication path cannot work. They identify a path to audit for a missing integration or
implementation mismatch.

Trace ingress, durable chunk publication, incremental Markdown projection and renderer demand
under Operator's clarified contract. Code mounting and growth must consume Beryl history chunks;
neither a parallel CAS presentation path nor finalization-only acceptance satisfies that direction.
The [chunk dataflow audit](../audits/transcript-chunk-dataflow.md) found and corrected the same
mistake in active presentation, capture, bounded-resource and widget documentation. It also
identified terminal-only app projection scheduling, finalized-only transcript publication and
completion-only native fixtures as remaining implementation and acceptance gaps. Completion is an
equality fence over the appended narrative, so the proposed arbitrary completion-text replacement
was also an invalid normal-capture scenario. Existing focused evidence remains valid only within
its stated boundaries. No phase acceptance follows this diagnosis.

The first storage correction publishes coherent live projection snapshots without changing the
finalized frontier or terminal completeness. Its initial streaming append regression also exposed
resource validation comparing a retained snapshot with the entire current source. Validation now
uses append-prefix compatibility, while captured-source and projection replay checks authenticate
the saved snapshot. Current transcript entries still require exact current revisions; historical
entries authenticate their recorded projection-set source revision. Native mounting and app
capture-driven scheduling remain separate unqualified integration work.

Run `83e61187-bcf6-4f9e-8708-2c9c905b5ca6` exposed that retained-resource assumption through
`projection resource metadata disagrees` immediately after ordinary append. A broader run exposed
the populated fixture publishing a captured frame's transcript before constructing its projection;
the old finalization gate had hidden that reversed order. The corrected storage path and fixture
passed all 61 cases in `0ac62f6f-1ae1-4583-9628-40ce090d47ec`, including streaming publication,
whole-home scrub, unfinished reopen, agreeing completion and retained-generation reopen.

The app append-race regression exposed the corresponding construction-progress assumption:
generation selection used only the last published head. An append supersedes an unfinished build
without publishing a new head, so selecting head-plus-one repeatedly returned that superseded
build and failed with `item projection build is not resumable`. The shared step must resume exact
active parsing or start after the latest construction generation, including multiple abandoned
generations ahead of the head. Syndic now supplies a one-record reverse indexed progress read for
ordinary and candidate access; the app confirms it with its other source observations. This avoids
scanning abandoned generations or deriving visibility from CAS completion. Capture scheduling and
native streaming remain unqualified.

The decisive app run was `f5e9ccb1-7191-4dfd-b9b7-cc9d87e0688c`: the streaming append race failed
with that invariant while fourteen selected terminal/candidate/startup cases passed. The strengthened
fixture now abandons two consecutive construction generations ahead of the published head, refuses
both obsolete commands without a commit, reopens unfinished history through candidate access and
resumes the exact successor before ordinary publication. It also checks byte-bound refusal on both
progress-read surfaces. The root plan records corrected verification before acceptance or cleanup.

The corrected combined run passed all 15 selected app-library/`normal_terminal` cases in
`78edd6a1-188a-433e-a957-3aa0b7ce495b` (119.820 seconds). This proves the construction restart and
terminal handoff; it does not establish broker-driven scheduling or native streaming visibility.

The production ordinary-driver loopback fixture exposed another identity mistake in
`fb4120b8-c186-4cbd-b6b6-a15f9776e13f` (11.645 seconds): it expected the immutable provisional Code
projection ID to survive an append. Initial unfinished publication and subsequent source growth
both occurred, but the growing exact source range produced a new ID. The history contract permits
replacement of the open trailing group; projection/resource identity hashing includes the exact
source range. Preserving an immutable storage snapshot ID across changed bytes would contradict
retained historical snapshots. This does not make completion a visibility boundary.

The corrected fixture checks the same saved item, stable range start, exact appended range length,
active turn/finalized count zero, and the latest projection ID through agreeing completion.
It passed in `44a43554-8f0a-423a-86f5-f8513e049034` (2.057 seconds). The real driver schedules
construction from a coalesced, payload-free post-publication notification under existing flight
custody; 89 router/construction/terminal/candidate/startup cases passed in
`a565a193-3953-4c35-bf6a-2ffd342dc498` (122.984 seconds). Native integration must still establish
stable presentation identity and manual source anchors across ordinary provisional replacement.
Do not overwrite retained immutable records or relax exact source/read fences to simulate that
continuity. These scheduling checks do not qualify a mounted Code panel.

The mounted virtual successor case initially retained the correct mount but supplied a binding
with the predecessor's 10,000-byte range for a 20,000-byte projection. Exact indexed range admission
correctly refused that response in `fdac10f2-f082-41f8-881b-fe8710dece74` (51 passed, one failed).
Its corrected fixture supplies append-compatible provider narrative references and matching
successor ranges rather than weakening admission or extending an immutable composer reference.
All 52 selected panel/presentation cases passed in `7191dc71-42d2-440f-9176-c01e77f0cb0a`
(1.241 seconds). This qualifies virtual snapshot replacement, mount and anchor continuity;
native capture-driven publication and provider delivery remain separate acceptance work.

## Fragmented Opening Info

Using the exact Code-body start as presentation identity conflates a read boundary with a Markdown
block boundary. Saved snapshots ` ``` `, ` ```rust ` and ` ```rust\n ` have empty body starts at
bytes 3, 7 and 8 respectively, while their opening fence starts at the same byte. Syndic already
derives a stable Markdown block digest from the saved item, opening-fence start and block kind.
The ordinary provider carries that saved identity into presentation provenance; Code presentation
uses the selected view and block digest. Successor mount retention requires the same saved block
and accepts a moving body start only while the predecessor body is empty. Indexed reads and source
anchor restoration retain their exact snapshot, source-prefix and body-range checks. Do not infer
block identity from CAS item lifecycle or parse opening text in the renderer.

The broader presentation run `7812f145-8cea-4f23-90b4-aab73796d9ad` passed 121 cases but exposed two
media-action fixture failures: two distinct image projections declared the same saved item and
source offset. Stable authored identity correctly treated those facts as one source position, so
the fixture could no longer claim its second image was unrealized. Give the fixture's second image
its distinct saved source position instead of restoring immutable projection IDs as presentation
identity. The corrected fourteen-target run `0993ae88-054a-4c90-9418-1157f15cbd30` passed all 123 cases.
