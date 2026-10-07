# Accepted Boundary

Codex App Server runtimes retain an explicit standalone-server or Codex CLI launch form.
The standalone binary receives server arguments directly; the CLI receives exactly one
`app-server` subcommand. The form follows durable registration, managed launch provenance,
ordinary preparation and same-home recovery. Both retain the pinned CAS 0.146.0 admission
contract and initialize product `<client_name>/<codex_version>` (`beryl/0.146.0` for Beryl).

Canonical executable identity remains unique independently of form. A duplicate keeps its
registered form. Runtime record codec V2 requires a closed launch-form tag and rejects unknown,
missing and legacy records rather than inferring a form. Native picker/menu mounting and the
unfinished atomic runtime/root admission capability have separate acceptance boundaries.

# Verification

Qualification used an isolated checkout of accepted `6a1c0dfb`, overlaid with only the 38 owned
source/test files. Ignored local Cargo configuration and unaccepted admission source were excluded.
Windows Rust/Cargo performed builds and nextest runs; no installations, production CAS launch,
native Beryl GUI launch or Operator clipboard access occurred.

- Model, State and backend run `3aeadadd-775f-423a-b204-44c9c512294a`: 34 passing cases; one newly
  added malformed-record injection fixture failed because its passthrough probe accepted the
  supplied malformed envelope. This was a fixture error, not a production decoding failure.
- Corrected State run `bb43d892-8934-4768-acfe-e119b965a94e`: both record-version cases passed.
  The dedicated rejecting injection probe is disposed before real BerylState performs decoding
  and explicit schema validation. Unchanged source retains the earlier passing evidence.
- App run `89be9c09-c82d-4cd8-a900-415e668e8c2f`: 25 cases passed, including actual fixture-process
  argv observations for both ordinary and selected same-home recovery launch forms, plus original
  runtime cleanup and lifecycle cases. The fixture rejects unexpected or doubled CLI prefixes.
- Coverage includes closed serialized forms, Host/WSL server arguments and authentication,
  managed identity, durable roundtrip/reopen and duplicate uniqueness, invalid tags/versions,
  existing release/profile/configuration admission and normal/recovery preparation.

There are 60 distinct passing cases across these runs. Logs and inventories are retained in
`.tmp/runtime-launch-forms-evidence`, bounded to 8 MiB. The 38-file identity is SHA-256
`0C356965072BEAE73A271C5C21D0310532919B3D62D440D95BB302120A2D55C7`, computed from sorted
`path SPACE uppercase-file-SHA256` lines joined with LF and no terminal LF. The main checkout
and isolated qualification checkout matched exactly. Independent semantic review covered the
shared value, persistence, provenance, command, production consumer and fixture boundaries.

Final locked offline all-target check passed for `beryl-model`, `beryl-state`, `beryl-backend`,
`beryl-app` and `syndic-storage`, with app test-fault support enabled (2m 48s). The reviewed source
identity remained unchanged. Independent semantic completion review accepted this boundary.
