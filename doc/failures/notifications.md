# Notifications

## 2026-05-06: End-turn sound suppressed after unfocusing Beryl

Manual live testing configured a valid WAV path, but no end-turn sound was heard.

The invalid assumptions were that active turn worker polling could rely on GPUI redraw callbacks while Beryl was unfocused, and that `Window::is_window_active()` was a reliable Windows focus gate. In GPUI 0.2.2 on Windows, the active-status callback path is not sufficient for Beryl's notification decision, so a stale focused state can suppress playback. Redraw-dependent polling can also delay processing a terminal turn until the operator focuses Beryl again, at which point the focus gate correctly suppresses the sound.

The course adjustment is to poll active worker channels from a short background timer while work is pending, and to focus-gate Windows playback by comparing Beryl window HWNDs against `GetForegroundWindow()` at notification time. The notification remains suppressed when focus cannot be determined.

## 2026-05-06: First end-turn sound was truncated

Manual live testing showed the first configured WAV began playing for a few milliseconds and then cut off, while later notifications played fully.

The invalid assumption was that a fresh per-sound `rodio::Player` could be created, appended to, waited with `sleep_until_end()`, and dropped for each notification without affecting first playback. In rodio 0.22.2, dropping a `Player` stops its sounds, and the first cold Windows stream/player path can reach the player end signal before the audible output path has fully settled.

The course adjustment is to keep one persistent `rodio::Player` alive with the worker-owned `MixerDeviceSink`, append each notification WAV to that player, and continue waiting for each appended source on the audio worker thread.
