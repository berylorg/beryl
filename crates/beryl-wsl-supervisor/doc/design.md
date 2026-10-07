# Goals

Provide Beryl's Linux supervision artifact and bounded control codec, preserving the ordinary
WSL launch context while proving disposal of the exact owned Linux namespace.

## Non-goals

- Owning CAS protocol, authentication, policy, runtime selection or durable application state.
- Owning Windows programs or Linux work created by external services outside the namespace.
- Installing software, changing distribution configuration, or providing a reusable root daemon.

# Decisions

## Package And Artifact

The package supplies a platform-neutral control library and one Linux executable. Linux mechanics
are isolated from the codec; Windows consumers use only the codec. The production artifact is
static `x86_64-unknown-linux-musl` under the
[system distribution contract](../../../doc/systems/backend-runtime/design.md#native-wsl-composition-and-distribution).
Non-Linux invocation reports unsupported without starting work. Linux operation requires WSL2,
x86-64, Linux 6.6 or later, root namespace construction, clone3/CLONE_PIDFD, pidfd signalling,
waitid(P_PIDFD), private proc mounting, Unix FD passing, socket-peer pidfds and ptrace exec-event
confirmation. Unsupported primitives fail closed;
there is no numeric-ID disposal fallback.

The development-only `wsl-test-runner` executable runs on Windows. Nextest supplies an exact
Linux test binary and its arguments; the runner accepts an explicit distribution, validates and
maps the canonical drive-backed binary path, preserves the working directory, and invokes the
binary through `wsl.exe --exec` without a shell. It forwards output and exit status without
compiling in WSL. An explicit `--root` test-runner option selects root only for tests of namespace
mechanics; ordinary-context qualification still launches a separate default-account broker.
Native tests own their private processes and cleanup; this launcher supplies
no production supervision or cleanup proof.

## Ordinary Context Broker

The `context-broker` mode runs through ordinary exact-distribution `wsl.exe --exec`, with no
explicit user override or shell/profile. It captures its actual UID/GID, supplementary groups
and complete inherited environment. It performs no NSS reconstruction, child creation, selected
CLI execution or filesystem observation. The broker remains alive until the supervised workload
is disposed, preserving the ordinary WSL user/session and interoperability context.

The broker accepts one bounded initialization frame on its original stdin and creates one
single-use abstract Unix socket named from the SHA-256 of its 256-bit launch nonce, keeping the
nonce out of the public socket name. It holds its own pidfd.
The root supervisor connects rather than exposing a privileged listener. The broker accepts only
a root peer with the matching nonce, sends its bounded context and original self pidfd using
SCM_RIGHTS, and accepts no second connection. The context handshake accepts exactly one pidfd;
unexpected, missing or truncated ancillary data is refused with every received FD closed. No socket
file or context file is written.
The broker sends context first to an authenticated root peer. Root verifies the original nonce,
socket-peer credentials/groups and original peer pidfd against the transmitted context/capability
before sending a nonce-bearing acknowledgement. A replacement socket cannot obtain the launch
nonce from the root's first message or supply arbitrary workload credentials.
Windows receives only bounded readiness/control facts; inherited environment values stay inside
the Linux peer channel and are never diagnostics or public application values.

Socket loss, original stdin EOF, cancellation or initialization timeout ends the broker's fixed
control loop. A normal close emits its exact closed record and exits without children. There is
no shell or arbitrary-command path in this role. Failure to prove its own joined closure stays
unavailable; a Windows-wrapper error does not manufacture Linux closure.

## Root Supervisor And Namespace Init

The `supervise` mode accepts one typed launch request on original stdin, authenticates the broker
channel and original pidfd, and retains those owners before creating a namespace. It does not
read `/etc/wsl.conf`, infer the default user from the registry, reuse root's HOME/session values
or synthesize another user's environment.

A single-threaded supervisor creates init with CLONE_NEWPID, CLONE_NEWNS and CLONE_PIDFD. Init
sets private mount propagation and namespace-local proc. The parent retains init's pidfd before
opening the workload gate. Init retains an inherited pidfd for the original supervising parent,
sets SIGKILL parent-death handling and checks that stable parent capability before release;
getppid in the child namespace is not parent-incarnation evidence. Init does not change its own
credentials after establishing parent-death handling.

The workload child receives the broker's exact groups/GID/UID and environment, then changes to
the requested execution root and executes the exact CLI or fixed observation. It inherits no
supervision/control sockets, pidfds or privileged context. Credential/setup failures cannot
execute selected code as root. A normal distro account that is itself root remains that account;
bootstrap never silently selects it as a substitute.

Init reaps adopted children. Ordinary CLI exit ends init and disposes remaining namespace members.
On disposal, init may forward SIGTERM to the exact workload pidfd; after at most two seconds the
parent sends SIGKILL to the original init pidfd. Only exact init reaping establishes namespace
closure. The parent then requests broker retirement and retains the broker's original pidfd and
peer channel until positive pidfd exit evidence establishes that single-process role's closure.
It cannot exit after namespace closure while broker disposal remains unproved. Parent/broker/control
loss requests the same disposal; no stale PID/PGID is resolved. Kernel teardown exceeding a wait
deadline remains pending with retained custody, not success.

## Control Contract And Bounds

Protocol version 1 uses a 48-byte header: eight-byte `BRYLWSL1` magic, little-endian u16 version,
u16 closed frame tag, u32 payload length and the original 32-byte launch nonce. Payloads are at
most 128 KiB. Incremental readers use at most 8 KiB scratch space and reject unknown versions,
tags, invalid lengths, nonce mismatch, trailing data or illegal stage transitions.

Requests are closed initialization, server launch, executable/directory/home observation, stop
and broker close. Server launch carries the backend-constructed CLI arguments; observations
execute fixed Rust filesystem behavior after credential drop, never caller scripts. Paths retain
at most 4,096 UTF-8 bytes; groups at most 256; arguments at most 64; environment at most 128
entries. Each argument/value retains at most 8,192 bytes and each environment name at most 256.
The aggregate frame bound also applies; exceeding a limit refuses rather than truncates context.
Environment values preserve native non-NUL bytes. Secret-bearing context is private, cleared at
terminal release and excluded from logs; it does not transport Beryl's generated launch bearer.

Events are readiness, workload-started, shutdown-pending, owned-namespace-closed,
Linux-companions-closed and typed failure.
A workload-started event requires positive credential/chdir/exec setup evidence; CLI stdout cannot
forge it. The init confirms the exact child's ptrace exec event and detaches before publishing
server startup; error-pipe EOF alone cannot prove exec after pre-exec child death. Fixed observations
use their own positive setup confirmation. Closed may carry one validated observation or workload exit status. Observation result
payloads remain at most 8,192 bytes under the backend transport contract. CAS stdout/stderr use
separate drained pipes with bounded 4,096-byte diagnostic tails and truncation facts.

Handshake waits are at most five seconds. Observation deadlines are caller-supplied nonzero
values at most 30 seconds. Each explicit disposal attempt waits at most five seconds, preserving
the same owner on expiry. Each role retains at most eight pending control frames; blocking output
does not block namespace disposal. An owned-namespace-closed event is issued only after exact init
reaping and workload-pipe disposal. It proves that namespace, not broker or Windows closure.
Linux-companions-closed additionally requires positive original-broker pidfd exit evidence and
Linux peer-channel disposal. Only then may the supervising parent exit normally. Windows child
and reader joins still belong to backend and are not inferred from either Linux event.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `privileged-access/v1`
- `external-side-effects/v2`

The trusted release artifact, selected normal account and original private channels are the trust
boundary. Malformed or detached input cannot grant root workload execution or cleanup authority.
Independent semantic review of privilege, FD inheritance, parent death and original-owner disposal
is required. Native Linux evidence must cover detached/nested descendants, identity/environment,
setup failure, role/channel death, timeout, repeated disposal and an unrelated surviving process.
