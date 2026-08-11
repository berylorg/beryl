# Scope

Hard-stop termination of command processes created inside a CAS turn.

# Invalidated Approaches

Phase 64 first treated `CommandExecution.processId` as a handle for
`command/exec/terminate`. After source disproved that namespace mapping, it
briefly treated the same id plus loaded thread and provider item as an exact
handle for `thread/backgroundTerminals/terminate`.

# Decisive Evidence

Pinned source evidence is retained in
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/hard-stop-primitives.md`.

`command/exec/terminate` addresses only standalone `command/exec` sessions in
an app-server manager keyed by originating connection and caller-supplied
string. Turn-owned commands live in a separate loaded-thread unified-exec
manager. Sending a turn item process id to the standalone method cannot reach
that process.

The thread-owned manager uses random numeric ids from a finite range and
reserves only currently live values. Removing a completed or terminated process
immediately makes its number eligible for reuse within the same loaded-thread
generation. The individual terminate request compares only thread and numeric
process id; it cannot carry or compare the provider item identity. A frozen id
can therefore terminate a later unrelated process after ABA reuse. A preceding
list read is only a TOCTOU check.

# Course Correction

Pinned CAS 0.144.1 exposes no exact individual turn-process hard-stop
capability. Beryl rejects both mappings before serialization and reports the
identity-unsafe target family as unsupported.

Coarse `thread/backgroundTerminals/clean` is a separate thread-wide request.
Its empty response is request acceptance only, it runs after individually
addressable future targets, and it is never represented as per-process or
selected-turn completion evidence. Its intentionally broad effect is the only
pinned command-process cleanup admitted by hard stop.

# Affected Authority

Phase 64 reconciles the CAS-live and backend-runtime systems, status-line
feature, backend package, app package, and the Phase 66 backend implementation
boundary. Neither standalone command probing nor the reusable-id thread
termination family can substitute for lifetime-stable process-instance
identity.

# Later Course Correction

The admission of coarse `thread/backgroundTerminals/clean` above is superseded. Beryl now supports
only exact soft interruption and does not invoke hard stop, individual command-process termination,
or thread-wide background-terminal cleanup. A future cleanup capability requires both an exact safe
target and meaningful completion evidence; request acceptance or a thread-wide effect is
insufficient.

The ABA and namespace evidence still explains why the pinned individual termination methods are
unsafe. It no longer justifies broad cleanup as a fallback.
