# Transcript Copy Source Context

The first stripped-text converter interpreted each source line independently and recognized outer
emphasis or link delimiters before protecting embedded literal Code. Semantic review demonstrated
literal-character loss in multiline inline code, code inside emphasis/link labels, and quoted or
nested indented/fenced code. It also found that a malformed whitespace-bearing link destination
could be removed as though it were recognized syntax.

Source spans introduce the same risk: a continuation can begin inside Code opened in an unloaded
span. A selected source fragment is not proof of a complete Markdown document.

The accepted approach must preserve uncertain source verbatim, including remaining source when
container or literal context cannot be established. Continuation spans retain their raw spelling;
known complete simple markup may be stripped within bounded worker scanning. Tests cover these
forms in `tests/unit/transcript_provider/copy_text.rs`. The source provider charges original and
derived representations before admission; no GUI parser or painted-text reconstruction is allowed.

This lesson supports the transcript feature's literal-Code and uncertain-syntax Copy contract.
Runtime acceptance is recorded in the
[mounting audit](../audits/transcript-mounting-readiness.md#shared-transcript-keyboard-copy-acceptance).
