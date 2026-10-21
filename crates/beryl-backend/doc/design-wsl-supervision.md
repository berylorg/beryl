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

## Original-Namespace Generated-File Reader

The backend-generated source capability may request a fixed reader from the original retained
server supervision owner. This extends its closed supervisor protocol; it is not a filesystem
observation launch. The request binds original launch nonce, live loaded source, resource/item
correlation, a fresh sixteen-byte reader ID and one exact absolute POSIX path of at most 4,096
UTF-8 bytes. The owner admits at most its configured generated-worker count, with no waiting
path queue. A closed namespace or lost original authority refuses admission.

The original namespace init creates the fixed reader child and installs its pidfd cleanup owner
before fallible setup. The child receives original broker groups/GID/UID and filesystem context,
drops credentials before opening the regular file and inherits only its necessary bounded read
channel. It executes fixed Rust behavior, no caller command or path-discovered executable.

Open, positional page request/response, cancel and close follow supervisor protocol V2. Each page
request qualifies reader ID, next request sequence, offset and allowance; responses repeat those
facts and report exact bytes, EOF or closed failure. The same open descriptor serves every pass.
At most one request/page is outstanding per reader, with 8,192-byte data frames and framing scratch.
The existing control pump continues independently of data consumption and prioritizes retirement
between frames. A full page inbox never blocks stop/close or acquires another uncharged frame.

Reader completion, a validated close receipt, child reaping and reader-channel drain are distinct
facts. Complete source close requires that reader's positive close/reaping proof or the original
namespace's positive closure proof; either path also joins/drains its channels. Loss, malformed
responses, timeout or cancellation retains disposal-only reader/namespace custody until joined
cleanup is proved. Namespace retirement cancels all its readers and closes their channels; source
leases cannot outlive unproved native cleanup by being dropped or handed to replacement services.

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
