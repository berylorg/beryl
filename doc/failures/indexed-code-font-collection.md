# Failure

The observed-font-environment prerequisite initially reused GPUI's existing Factory5 system-font
collection acquisition and queried that object for `IDWriteFontCollection3`. Native qualification
returned an unavailable environment key. The cast succeeded, but the resulting expiration handle
was null/invalid. Interface access alone did not establish an observable system collection.

# Correction

Acquire the qualified system collection directly through optional
`IDWriteFactory7::GetSystemFontCollection`, retaining the existing weight/stretch/style family
model and excluding downloadable fonts. The method explicitly returns `IDWriteFontCollection3`.
Verify its retained expiration handle with a zero-timeout wait before accepting qualification.
Use the same acquisition boundary for an admitted replacement. Unsupported capability retains the
legacy platform collection with unavailable qualification; it does not raise the supported OS
minimum or claim an exact generation.

Microsoft documents the direct collection result and family/download choices in
[`IDWriteFactory7::GetSystemFontCollection`](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwritefactory7-getsystemfontcollection).
Its [expiration-event contract](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwritefontcollection3-getexpirationevent)
owns handle lifetime and observation semantics. Neither a successful cast nor a fresh COM pointer
substitutes for those checks.

# Verification Boundary

The first correctly configured nextest run passed three cases and failed the native qualification
case before the remaining fifty-eight cases ran. Final run
`dfe2207c-b27f-4139-8a24-8d696f49dbe8` passed all sixty-three selected GPUI cases: twelve
font-environment and fifty-one canonical streaming-layout cases. Native
evidence distinguishes the legacy null handle from the directly acquired collection's unsignalled
event. It also exercises two stable missing families, forced adoption through the production
transition helper, immutable old glyph rasterization and typed face-cap refusal.

Forced adoption reuses the retained collection without changing installed fonts. This establishes
transition/cache/publication mechanics and actual event access, not real OS font-change delivery
timing. Whole mounted Code acceptance remains separate. Qualification uses stable Rust, both Cargo
and nextest local-configuration layers, locked offline resolution, one build/test job, disabled
normal debug information and nonincremental compilation.
