# Clipboard Qualification Preservation

Scope: isolated GPUI native clipboard test harness.

An initial harness captured the original clipboard sequence but checked ownership only
before final restoration. Independent review found that a foreign replacement between
preparation and the first write, or between subsequent writes, could be overwritten and
then replaced by the stale backup. A final restoration fence cannot protect earlier writes.

The corrected harness retains the original sequence and checks expected sequence under
native ownership before every clear and publication, including raw restoration. Unknown or
foreign sequence permanently invalidates mutation eligibility. Three deterministic cases
exercise the same helper used by the native harness, covering first-write replacement,
later replacement and restoration, and ownership for each publication. Native execution
is now attempted after image representation resolution. Earlier access denial cleared; the
harness correctly refuses the current `CF_BITMAP` original before mutation pending Operator
plain-text clipboard preparation. Independent review accepted the
correction and focused verification passed all 16 deterministic cases (nextest run
`c577f550-2573-426f-beba-890257a1ce37`); the native test remained ignored.
