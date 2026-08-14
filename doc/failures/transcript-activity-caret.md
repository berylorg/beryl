# Transcript Activity Caret

## 2026-05-01: Client-area animation setting suppressed terminal caret blink

During live testing, the activity caret rendered steadily even though the operator expected an old-terminal-style blinking block. The implementation had tied activity-caret blinking to the broad Windows client-area animation setting through `SPI_GETCLIENTAREAANIMATION`.

On the live Windows environment, client-area animations were disabled while `GetCaretBlinkTime` still reported a normal 530 ms text-caret blink interval. That made the activity caret steady even though text-caret blink semantics would blink.

Course adjustment: the activity caret follows platform text-caret blink policy when available. General reduced-motion state remains useful for platforms without a text-caret blink source, but it is not the Windows source for this terminal-style caret.
