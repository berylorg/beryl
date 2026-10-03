# Reason For Investigation

The process notification lane needed finite WAV acquisition and decoded storage, cancellable
playback, and complete owned teardown. The older Windows-playback note described a detached FIFO
and persistent player; it did not establish these bounded decoder and final-drain mechanics.

# Outcome

With only `playback` and `hound` enabled, Rodio's WAV decoder delegates to Hound 3.5.1. The inspected
reader parses scalar format fields and skips chunks with fixed stack storage; sample iteration
does not materialize a second decoded buffer. A bounded in-memory cursor, strict RIFF/format/data
prevalidation, exact sample-count allocation and iteration checks therefore bound this path.
The inspected supported subset is PCM 8/16/24/32-bit and 32-bit IEEE float with the corresponding
supported 16/18-byte format headers. PCM32 with an 18-byte header is rejected by the selected path.
Unsupported or inconsistent headers must fail before playback; decoder exhaustion and nonfinite
samples remain explicit errors. This finding does not establish general WAV/codec support.

`SamplesBuffer` conversion can introduce a second decoded allocation. A source owning the decoded
vector's iterator avoids that copy. Rodio's stopped-player flush in `Player::append` is unreachable
when appending once to a fresh player, permitting a short control fence around that append after
file, decoder and device preparation have finished off the GUI thread.

A generated silence tail adds no sample buffer and allows the player/device to remain alive after
the final decoded sample. Cancellation/device-error/deadline polling then stops and drops the
player and device. CPAL's WASAPI stream drop sends termination and joins its stream thread. The
tail addresses the older note's immediate-drop truncation concern, but no audible hardware result
was established in this investigation. Device opening and stream teardown remain best-effort.

Synchronous regular-file calls are not made preemptible by these APIs. Cancellation checks can
surround calls and bounded blocks; an owned background join must await an in-flight OS call.
The implementation and acceptance boundary remain in the app design and root plan.

# Sources

- crates.io `rodio` 0.22.2, `hound` 3.5.1 and `cpal` 0.17.3; resolution checked against the tracked
  `Cargo.lock` and the local development lockfile on 2026-10-03.
- Root `Cargo.toml`: Rodio defaults disabled, features `playback` and `hound`; `beryl-app` selects
  that workspace dependency for Windows. No Symphonia decoder is enabled.
- Registry sources: `rodio-0.22.2/src/decoder/wav.rs`, `src/decoder/mod.rs`, `src/player.rs`,
  `src/source/mod.rs` and `src/stream.rs`; Hound `hound-3.5.1/src/read.rs`; CPAL
  `cpal-0.17.3/src/host/wasapi/stream.rs`.
- Local use sites: `crates/beryl-app/src/notification_audio/wav.rs` and `playback.rs`, plus real
  acquisition/decoder and controlled cancellation tests in `tests/notification_audio.rs`.
- Prior investigation: [Windows notification playback](windows-notification-playback.md).
