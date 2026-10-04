# Checked Native Clipboard Qualification

Status on 2026-10-04: implementation, focused compilation, deterministic verification and independent
semantic review complete. Native acceptance and canonical dependency publication remain pending the
private-station run from an Administrator terminal. The controlling reusable boundary is the
[fork design](../../../../zed-fork/doc/design.md#checked-windows-clipboard-boundary).

# Implemented Boundary

GPUI exposes checked acquisition and complete write acknowledgement through `App`, with typed
failures and caller text, metadata, image and aggregate byte limits. Windows accounts actual native
backing and encoding/output capacities; unsupported backends return unavailable. No decoding,
transcoding, automatic retry or readback success heuristic is introduced.

Images publish their standard format and a 24-byte private companion: little-endian version 1,
stable format identifier, exact encoded length and SeaHash content consistency digest. Native image
and companion padding is charged. Validation precedes output allocation and returns the exact
encoded prefix; absent companion returns the entire bounded foreign native representation. The
digest does not authenticate application provenance. Partial publication or close failure never
acknowledges success.

# Verification

Locked fork metadata and focused Cargo check passed after enabling the existing Windows crate's
StationsAndDesktops feature. The analyzer restarted successfully after those checks. The corrected
feature set and private harness passed all 20 deterministic nextest cases, run
`2564abce-e7a5-4095-8011-b39473089be6`; the sole native case remained ignored. Coverage includes
injected native failures, encoding/metadata validation, exact/one-over limits, immutable snapshot
identity, image and companion padding/consistency, complete acknowledgement and foreign images.
The obsolete shared-clipboard restoration helper and its three cases were removed. Formatting and
whitespace checks passed. Independent review accepted
the complete production boundary and private harness with no remaining source findings.

# Native Isolation

The sole native case is `native_ownership_format_publication_on_private_window_station`. It creates
a unique PID/time-named window station with `CWF_CREATE_ONLY`, verifies its identity differs from
the borrowed original, creates one private desktop with a 512 KiB heap, and binds the test thread
before creating its message-only owner window or invoking any clipboard API. Each station owns its
clipboard. It has no shared-clipboard query, backup, reuse or fallback path. The Operator actively
uses the working clipboard; qualification must never acquire or modify it again.

The case exercises text/opaque metadata and all four image formats at lengths 1, 3, 5 and 9. Its
sequence witness compares the captured read snapshot against native state while ownership is held,
not the writer's pre-close sequence. Cleanup destroys the owner window, restores and verifies the
exact borrowed process-station/thread-desktop bindings, then closes only owned handles. Setup or
cleanup failure prevents success. No visible desktop switch or Beryl GUI is involved.

Shared-clipboard experiments were retired after a trace observed sequence 2522 becoming 2525 and
format count three becoming six across close, with no data-read sequence change. Implicit format
synthesis is consistent with that trace, but owner equality cannot exclude foreign bonus-format
writes. The stale-restoration guard refused rather than overwrite a changed clipboard. The
[research note](../../memory/topic/windows-clipboard-qualification/native-sequence-and-private-station.md)
and [preservation lesson](../../failures/clipboard-qualification-preservation.md) retain the evidence.

# Resume Gate

The current unelevated process cannot create a named station (Win32 error 5); unnamed create-only
creation refuses an already-existing logon-derived station (error 183). Neither preflight created
handles or changed bindings. Do not reuse that unrelated station. Microsoft requires Administrators
group membership for explicit station naming; this is a test-fixture prerequisite.

In an Administrator PowerShell terminal, run:

```powershell
Set-Location 'C:\Users\user\p\berylorg\zed-fork'
cargo +stable --config .cargo/local.toml --config ../beryl/.tmp/checked-clipboard-qualification/build.toml nextest run --locked -p gpui --no-default-features --test checked_clipboard_native --run-ignored only --test-threads 1 -E 'test(native_ownership_format_publication_on_private_window_station)'
```

Report the final nextest summary and any test diagnostics. The command uses the qualified one-job
LLVM/no-debug/nonincremental envelope and runs only the private native case. Operator clipboard
preparation is unnecessary. After native success, publish the accepted fork source revision, align
scrollbar, text-input and settings-window pins in order, and qualify locked Beryl canonical
app/executable composition without local overrides. Until those gates pass, source remains
unaccepted working material and Beryl dependency pins remain unchanged.
