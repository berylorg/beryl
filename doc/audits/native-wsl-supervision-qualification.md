# Native WSL Supervision Qualification

Qualification on 2026-10-07 covers the companion, backend managed launch and fixed filesystem
observations, desktop artifact composition, and application disposal consumers. It preserves WSL
interoperability while guaranteeing closure only for the original owned namespace and companion
roles. Service-created outside work may survive. Actual CAS release admission and atomic runtime/root
registration remain the next implementation boundary.

The reviewed artifact is 94 changed or added source, manifest and test files over
`c24ce82ceaf9b587af89f4b09cb739cc4557bd46`. Its SHA256 identity is
`FFBD9DB96971F4016A3AC76D3291E9B8DB29315BF8BF88AF729085052A00DE5D`, calculated over UTF-8
sorted Git paths followed by a space and uppercase file SHA256, joined by LF without a terminal LF.
The isolated canonical checkout and main checkout matched every source and manifest input.

# Execution Evidence

Windows Rust 1.97.1 and its bundled LLVM linker built static, non-PIE
`x86_64-unknown-linux-musl` artifacts. Windows nextest 0.9.129 invoked the Linux test binaries through
the package's thin Rust WSL target runner in the exact Ubuntu distribution. Linux Rust and Cargo
were not required. The standalone Linux nextest installed with Operator authorization remains
available; qualification used the Windows runner route after the archive path issue recorded in
[failure evidence](../failures/runtime-root-admission.md).

- Companion run `9478044b-9acb-4513-9b61-9cd95171d752`: 22 passed; four fixture entries excluded.
  Covers malformed control/context/ancillary descriptors, original peer pidfd authentication,
  credentials and pre-exec refusal, namespace failure, normal/abnormal exit, role and channel death,
  deadline expiry with original-owner retry, detached/double-forked/nested descendants, and fixed
  home observation. Actual Windows interoperability creates outside work that survives positive
  namespace closure; test cleanup uses its separately retained original pidfd.
- Windows backend run `cb465875-26b1-4002-a740-d534e5ce1816`: 21 passed. Covers Host launch,
  immutable WSL artifact selection, control progress and refusal, bounded observations, and queued
  positive closure consumption after control-writer disconnection without accepting invalid proof.
- Native Windows backend run `cccc1582-28c1-41c6-8b5e-540f57ce7c03`: all seven qualified tests
  passed, with no skips. Covers production managed launch/observations, unrelated work survival,
  partial launch failure, both control-channel losses, original Windows launcher/reader joins and
  token erasure, and deadline expiry followed by same-owner retry. Supervised UID/GID/groups/HOME/USER
  match an ordinary exact-distribution invocation. The ordinary account was UID/GID 1000 with seven
  supplementary groups; the bounded, childless identity probe joined with exit zero.
- Application run `52089d33-c339-4d67-bd5e-8c8fb902c5e8`: 80 of 81 passed. The former expectation
  of successful terminal closure after failed connection disposal was invalid; the fixture now
  verifies the retained failure and original home lock.
- Run `fe47bbb4-59e8-4598-91d6-9cddf6d79a14`: all six passed, including that corrected fixture,
  two actual outer process-owner shutdown/recovery tests, and three desktop artifact tests.
  Pending disposal retains the original runtime, home lock and replacement fence; successful retry
  adds one retirement attempt without launching a replacement. Desktop tests exercise ELF bounds,
  mapping, digest/version refusal and Windows write/delete pinning across retained `Arc` ownership.
  The build consumed the exact real Linux companion artifact to bind its digest and protocol.

Locked metadata and the focused Cargo check passed before the successful language-server refresh.
Native artifacts and all exercised test targets compiled from the isolated canonical workspace.
Final locked all-target checks passed for all four affected Windows packages with the lifecycle
test features, and for all Linux companion targets. Independent privilege/lifecycle/integration
review accepted the exact frozen source identity with no remaining findings.
No native Beryl application or Operator clipboard was accessed. Raw bounded logs and input hashes
are retained under `.tmp/native-wsl-supervision-evidence`; they are local evidence, not runtime input.

# Corrections And Limits

Preserve positive execution and namespace-closure proof. The raw-clone/libc identity correction is
recorded in [its failure note](../failures/native-wsl-supervision.md). Disposal consumes queued
original proof even when another control write fails. Terminal connection/worker/home errors retain
their original custody and cannot become successful closure on retry; supported pending runtime
disposal retries use the same original owner.

Qualification applies to the supported kernel primitives and selected artifact contract in
[system authority](../systems/backend-runtime/design.md#native-wsl-supervision-privileges-and-proof).
Missing capability or positive proof remains unavailable. Abrupt supervisor death does not create
a backend success claim: native companion tests verify parent-death handling, while Windows consumer
tests verify channel-loss paths that can still deliver positive closure. No excluded external-service
work is adopted, waited for, or killed by runtime disposal.
