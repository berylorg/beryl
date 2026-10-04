# Checked Native Clipboard Qualification

Status on 2026-10-04: implementation and independent semantic review complete; native acceptance
and canonical dependency publication remain blocked. The controlling reusable boundary is the
[fork design](../../../../zed-fork/doc/design.md#checked-windows-clipboard-boundary).

# Implemented Boundary

GPUI exposes checked acquisition and complete write acknowledgement through `App`, with typed
failures and caller text, metadata, image and aggregate byte limits. Windows checks actual native
backing and encoding/output capacities; unsupported backends return unavailable. Legacy clipboard
convenience methods remain separate. No decoding, transcoding, automatic retry or readback success
heuristic is introduced.

Images publish their standard format and a 24-byte private companion: little-endian version 1,
stable format identifier, exact encoded length and SeaHash content consistency digest. Actual
image and companion padding is charged. Companion validation precedes output allocation and
returns the exact encoded prefix; absent companion returns the entire bounded foreign native
representation. The digest does not authenticate application provenance. Partial publication or
close failure never acknowledges success.

# Verification

Focused nextest qualification passed 23 of 23 deterministic cases, run
`05d876c0-f24e-4977-96f5-0d0a0de6c8cc`. Coverage includes injected native failures,
malformed UTF-16 and metadata, exact/one-over bounds, snapshot identity, padded image/companion
allocations, malformed companion/version/format/length/digest, both publication failures, foreign
images and preservation sequence gates. Independent review accepted the complete corrected
boundary and the harness diagnostic correction, with no remaining source findings. Formatting and
whitespace checks passed.

The isolated native test creates only a message-only owner window, preserves bounded supported
original formats, and fences every mutation/restoration against captured sequence. It exercises
text with metadata and all four supported image formats at payload lengths 1, 3, 5 and 9. The
diagnostic run `2748cf87-998c-4dad-984e-c9ea3a25d8ef` failed at `native.open` during preparation:
Win32 error 5, access denied. No clipboard read or mutation followed. A separate content-free,
read-only `OpenClipboard(NULL)` probe from the execution session also returned access denied;
`GetOpenClipboardWindow` reported no holder. Do not interpret this as successful native qualification
or add production retries. No Beryl GUI was launched.

# Resume Gate

Run the native target from an interactive Windows execution session with clipboard access:

```text
cargo +stable --config .cargo/local.toml --config ../beryl/.tmp/checked-clipboard-qualification/build.toml nextest run --locked -p gpui --no-default-features --test checked_clipboard_native --run-ignored only --test-threads 1
```

The command runs from the GPUI fork. The retained ignored local build configuration supplies the
qualified one-job LLVM/no-debug/nonincremental envelope. After native success, publish the accepted
fork source revision, align scrollbar, text-input and settings-window dependency pins in order,
and qualify locked Beryl canonical app/executable composition without local overrides. Until those
gates pass, source remains unaccepted working material and Beryl pins remain unchanged.
