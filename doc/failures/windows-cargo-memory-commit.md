# Windows Cargo Memory Commitment Exhaustion

## Scope

The 2026-09-08 Windows memory-exhaustion event during app acceptance, and subsequent bounded
build/linker measurements. This concerns memory commitment; the earlier
[parallel-link artifact failure](windows-parallel-rust-link-artifact-contention.md) concerns disk
capacity and remains a separate root failure.

## Invalidated Approach

Serial test execution was treated as adequate concurrency control for a large nextest run.
`--test-threads 1` does not limit the preceding Cargo compilation. The final invocation omitted
`--build-jobs`, and repository Cargo configuration then had no job cap. High long-lived semantic
tool commitment left little room for concurrent full-debug Rust compilation and linking.

## Decisive Evidence

Times below are UTC on 2026-09-08. Retained logs and Windows Resource-Exhaustion-Detector Event
2004, record 41485, establish this order:

- The earlier settlement worker's explicit four-job release/debug verification and cleanup
  preceded commit `27aef103c1491d839a0483f70d455228991c0db4`, created at 21:27:13.
  This ordering is retained conversation evidence; exact worker build timestamps are unavailable.
- The isolated production app check log spans 21:29:46.6508125–21:29:51.4281126 and reports success
  in 4.75 seconds. It ended about 89 seconds before the exhaustion detector timestamp.
- The event XML's `ExhaustionEventInfo.Time` is 21:31:20.4640502. Reported system commitment was
  67,506,335,744 of 67,768,442,880 bytes, leaving approximately 250 MiB of commitment headroom.
- The final nine-target nextest build log spans 21:31:24.3632342–21:31:39.6759232. Its child was
  one `cargo test --no-run` with `test-faults`, without a build-job limit. Cargo could use the
  32-logical-CPU default; this does not establish that 32 compiler processes actually ran.
  The log's waiting-for-other-jobs message proves internal parallel compilation.
- Snapshot `rustc.exe` PID 66660 was created at 21:31:24.3876245, about 24 ms after that log began.
  The event record was emitted at 21:31:25.6674557. Thus its process list is a later snapshot,
  not the process state at the earlier exhaustion-detector timestamp.
- That snapshot attributed private commitment of 12,462,452,736 bytes (11.61 GiB) to
  rust-analyzer, 4,105,093,120 bytes (3.82 GiB) to Python, and 3,730,681,856 bytes (3.47 GiB) to
  rustc. These values do not identify the initiating allocation. Compiler metadata mapping
  failed with Windows error 1455 before the compiler ICE; an application popup later reported
  `0xc000012d`, consistent with commitment exhaustion.

No surviving evidence places a second known Cargo invocation alongside the final build. The
earlier worker's four-job setting increased its own potential pressure but had finished minutes
earlier. The final unbounded compilation entered an already exhausted condition about four
seconds after detection and likely worsened it. The original trigger remains unknown: the
retained evidence does not provide a complete machine-wide process timeline.

## Bounded Verification And Linker Comparison

The same nine app targets later passed all 145 cases on ordinary stacks with one build job and
one test thread. Compile/link took 45.78 seconds against existing artifacts; tests took 260.285
seconds. Peak guarded build commitment was 4.448 GiB. This verifies the bounded rerun, not the
original OOM trigger.

A controlled `syndic_composer_history` link probe reused full-debug compiled inputs and common
`/INCREMENTAL:NO /OPT:REF /OPT:NOICF` arguments:

- MSVC `/DEBUG:FULL`: linker private peak 3.164 GiB; PDB 387,534,848 bytes.
- MSVC `/DEBUG:NONE`: linker private peak 0.633 GiB; no PDB.
- Bundled LLVM `/DEBUG:FULL`: linker private peak 1.445 GiB; PDB 458,502,144 bytes.

All probes exited successfully. The isolated LLVM linker peak was approximately 54% lower than
MSVC with full debug; disabling PDB output reduced MSVC's peak approximately 80%. Rustc private
commitment stayed around 3.60–3.61 GiB. These are single sampled comparisons, not general
benchmarks; probe executables were not runtime-qualified by those links alone.

The permanent LLVM configuration subsequently passed locked canonical/local metadata validation,
a focused local `beryl-app` check and all 26 mounted-composer/history integration tests. The new
profile build took 5m45s; tests took 42.405s; peak guarded job commitment was 3,690,471,424 bytes
(3.44 GiB). An additional cached run after making stripping explicit passed the same 26 cases in
42.297s. This is runtime evidence for LLVM-linked output, not a cold-build speed comparison.

The guard assigned the child to a Windows Job Object before execution, with kill-on-close,
12 GiB hard commitment cap, 10 GiB soft stop and minimum system headroom of 12 GiB, sampled every
100 ms. All completed verification jobs reported the root reaped and no remaining job members.
The Operator enabled a pagefile during the new-profile run, increasing system headroom; that
change must not be attributed to compiler configuration. All measurements finished before the
Operator requested returning the pagefile to zero.

## Accepted Correction And Limits

The [root build policy](../design.md#implementation-technology) now selects bundled LLVM linking
on Windows MSVC, one default Cargo job, debug information disabled for development/test
compilation across the dependency graph and an opt-in `debugging` profile. Large verification
commands remain serial, with memory-headroom checks. A one-job default is not a machine-wide
memory limit and can be overridden by command arguments or environment settings.

`debug=0` plus `strip="debuginfo"` does not guarantee PDB-free Windows output. Rust's documented
[stripping behavior](https://doc.rust-lang.org/stable/rustc/codegen-options/index.html#strip)
retains MSVC PDB creation since Rust 1.79, so linked input CodeView data can remain. Current
Rust 1.97.1 linker implementation ignores the strip argument in `MsvcLinker::debuginfo` and adds
`/DEBUG`. A global `/DEBUG:NONE` flag would also defeat the opt-in debugging profile; no such
override was adopted. Exact attribution of residual PDB contents was not performed.

The [Serena investigation](../memory/topic/semantic-tool-memory/serena-cache-controls.md) found
no configurable eviction for its symbol dictionaries. Project-scoped Cargo concurrency/debug
settings are saved for the next full Serena service launch; they do not cap semantic heap use.
Removing the pagefile returns the system to a smaller commitment budget, so future verification
must check current headroom even though the completed bounded runs passed.
