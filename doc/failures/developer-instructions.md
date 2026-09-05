# Developer Instructions Must Not Depend On Collaboration Presets

## Invalidated Assumption

Beryl sent hidden per-turn context through `collaborationMode.settings.developer_instructions` and tested its JSON shape with a fake server. Correct serialization did not prove that CAS included the text in model input.

## Evidence

CAS commit `2c005abb0765bfe3ef42a23fe88d5b806184fa83` (July 31, 2026, `Use model catalog collaboration mode messages`) made catalog mode text take precedence in `core/src/context/world_state/collaboration_mode.rs`. Even an empty catalog entry suppresses the custom field. Beryl still sends that field in `crates/beryl-backend/src/turn.rs`; the request can succeed while its hidden instructions are ignored.

The alternative `additionalContext` application channel is additive developer-role context but deduplicates unchanged entries and truncates each entry to 1,000 estimated tokens. It is not a lossless per-turn replacement without extra adaptation.

## Correction And Evidence Boundary

The composer authority selects a dedicated turn-scoped developer-instructions channel in the Beryl CAS fork, independent of collaboration presets. Verify actual mocked model-request payloads for complete hidden text, retained catalog instructions, repeated turns, changes, clearing, and long inputs. Fake transport shape checks alone cannot establish delivery.

This is a source-confirmed regression mechanism. No retained incident trace establishes which installed binary and model-catalog data caused the Operator's observed failure.

The implemented fork channel is verified by `app-server-protocol/tests/turn_developer_instructions.rs`, `app-server/tests/suite/v2/turn_developer_instructions.rs`, and `core/tests/suite/turn_developer_instructions.rs` under the scoped fork project. Accepted evidence covers complete model payloads with catalog coexistence, repeated/changed/cleared turns, transcript exclusion, immutable steering, malformed requests, long text, stream retries/tool continuation, and effective WebSocket context. Stable and experimental generated protocol exports are checked against source. Independent model-input boundary review found no blocking issue.

Beryl serialization and startup compatibility are verified in `crates/beryl-backend/tests/launch_and_protocol.rs`: only the dedicated field carries hidden text, model/effort remain independent, blank text is omitted, and missing or malformed support cannot become a ready connection. App tests cover graph/global ordering, model-independent attachment, queued/lifecycle and edit-replacement routing, and exclusion from steering and maintenance. These source changes require matching rebuilt Beryl and CAS executables; the investigation did not replace installed binaries.
