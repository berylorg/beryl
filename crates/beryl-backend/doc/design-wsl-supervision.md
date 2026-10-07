# Native WSL Supervision

This supplement is normative for companion orchestration, bounded launch states and joined WSL
disposal, governed by [design.md](design.md). System privilege, artifact and cleanup scope come from
[backend runtime](../../../doc/systems/backend-runtime/design.md#native-wsl-supervision-privileges-and-proof).
The [companion package](../../beryl-wsl-supervisor/doc/design.md) owns wire bounds and Linux role
mechanics. Transport/authentication and fixed observation semantics remain in
[transport and admission](design-transport-and-admission.md).

## Launch And Context Custody

Backend uses the composition-supplied immutable artifact descriptor. It mints one random launch
nonce, starts the ordinary `context-broker` in the exact distribution, and retains its original
Windows child, stdin/stdout/stderr readers and channel before fallible setup. Server brokers use
the caller-validated execution root as their WSL working directory; observation brokers use `/`.
Ordinary WSL supplies the effective account and session, including cached user configuration and
systemd behavior. Backend does not select an account using a registry UID or reread distro config.

After matching readiness, backend starts the same artifact's root `supervise` mode in the same
distribution. Root receives only the private launch request and broker channel identity; broker
environment is passed through the Linux peer channel. Broker is retained for the full workload
lifetime. Root's own account/session/environment cannot substitute for the ordinary broker's.

The original owner is installed before any post-spawn operation for either child. A failed root
spawn still retains broker cleanup custody. Namespace construction or peer connection failure
retains both launched roles. Backend cannot retry another broker, root helper or runtime while
the previous attempt's disposal remains unproved. No auxiliary cleanup launch, distribution
enumeration, numeric PID signal, or shared-service termination supplies missing proof.

## Managed Workload And Observation

The closed server request carries the exact caller-validated CLI, root and backend-built argument
vector, including the selected listener and existing release/configuration flags. Bearer bytes
remain in their existing private file/in-memory owner; they are not control arguments or logs.
Matching workload-started evidence can mint managed-launch provenance only for that same retained
launch. CAS release/profile/configuration admission remains a separate session proof.

Fixed executable/directory/home observations run inside the same owned namespace after normal
credentials are installed. Paths and results follow transport-and-admission bounds. No utility
installation or caller script is allowed. A result is publishable only after complete joined
disposal; canonical facts alone cannot admit a runtime or replace the production release proof.

## Disposal And Failure

Disposal first requests root namespace retirement and consumes only original nonce/stage-matching
closure evidence. Namespace closure is retained as progress while root retires the broker through
the original peer channel and pidfd. Root remains alive with that capability until Linux-companions-
closed proves both namespace and original broker closure. Backend then joins both original Windows
launchers and all readers; no surviving broker loses its acquired Linux owner through ordering.
Before root construction, broker-only cancellation requires its exact closed record and normal
launcher/reader joins; missing proof retains that original broker capability. All companion roles
and owned launch material must settle before complete managed disposal.
Kernel namespace proof does not stand in for broker, launcher, reader or token cleanup.

Each attempt is bounded under the companion wait contract and the observation caller's total
deadline. Cancellation still retains the complete original owner until disposal is joined. Lost
closure records, failed joins, malformed output or expiration produce typed Unavailable with the
same disposal-only capability; they do not become positive closure because a wrapper disappeared.
Repeated stop/close uses only the same owner and is idempotent. Namespace-closed evidence already
validated on that owner is retained monotonically while remaining role/reader disposal is retried.

Unexpected broker/control loss requests namespace disposal. Supervisor death invokes init's
parent-death handling, but lost positive proof remains unavailable to backend. A failed launch
returns its original typed error with the unsettled process/authentication/reader owner. Consumers
retain that capability through close and recovery and fence replacement until complete disposal.

Service-created outside work is excluded by the selected system envelope. Backend does not wait
for or terminate those programs, disable interoperability, or use their survival as failed closure
of a proven owned namespace. This supplies no turn-stop or quiet-stream completion authority.
