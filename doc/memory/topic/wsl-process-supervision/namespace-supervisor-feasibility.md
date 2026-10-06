# Reason For Investigation

Runtime/root admission is blocked because the existing WSL supervisor cannot safely
prove disposal using a saved numeric process-group identifier. The Operator approved
investigating a narrow Rust Linux companion on 2026-10-07 while retaining `wsl.exe`.
This note records feasibility evidence and a reviewable proposal. It is research,
not target-state authority or permission to install software or change launch privileges.

# Outcome

A private Linux PID namespace is a promising ownership boundary. In the inspected WSL
kernel, namespace-init termination disables further PID allocation, kills remaining
namespace tasks, and waits for their removal before init can be reaped. This covers
ordinary Linux descendants that change sessions or process groups, including descendants
in nested PID namespaces. The owning supervisor must retain a pidfd for the original init
and reap that exact child; a numeric PID, leader exit or successful signal is insufficient.
This is source-backed feasibility, not qualification of an implemented supervisor.

Both unprivileged user-plus-PID namespace creation and privileged PID-only creation
succeeded in tiny local probes. The unprivileged probe changed supplementary group
visibility. It therefore cannot be adopted as an account-preserving replacement based
on that success alone. Ordinary-user cgroup-root write access was absent; delegation
elsewhere was not exhaustively investigated. No claim that all unprivileged solutions
are impossible is made.

Independent factual and authority review on 2026-10-07 supported recording this proposal
while implementation remains blocked. Its identity correction is incorporated here and
in the plan: namespace-visible GIDs do not establish changed underlying group membership
or filesystem permissions. Review accepted no production or native cleanup behavior.

The recommended candidate uses a minimal privileged namespace bootstrap and launches
the actual workload with the original account credentials. This introduces a material
privilege and process-view choice that must be selected by the Operator and recorded
in backend system/package authority before implementation. The agreed investigation
does not silently authorize that choice. Local Linux compilation is also unavailable
in the inspected environment; artifact build and deployment remain a readiness gate.

## Proposed Ownership And Launch Contract

- Keep the exact configured distribution and `wsl.exe`. A Beryl-owned Linux companion
  supervises one managed launch, with no shared daemon, arbitrary remote command API,
  distribution fallback, or installation during runtime admission.
- Identify the distribution's normal launch account before privileged construction.
  Preserve its UID, primary and supplementary groups, home, working directory and
  required environment. Never infer the workload account from the root bootstrap's
  environment. Identity capture and privileged consumption need an authenticated,
  bounded contract; an untrusted request cannot select arbitrary privileged work.
- The candidate bootstrap creates a private PID namespace without a new user namespace.
  Namespace init is a dedicated reaper, separate from CAS. If correct `/proc` views
  require a mount namespace, make propagation private and mount a namespace-local proc
  before running the workload. Filesystem/network access and CAS-owned policy remain
  unchanged except for explicitly selected process/proc visibility.
- Drop workload credentials before executing the selected CLI, shell/profile code or
  filesystem observation. Privileged parent/init execute only Beryl-owned supervision
  mechanics and retain no arbitrary root command surface. Exact syscall and capability
  requirements, executable trust and privilege lifetime require design review.
- Retain the original init's pidfd before releasing a launch gate. Prefer atomic
  creation with `CLONE_PIDFD`; otherwise establish the documented fork/pidfd conditions
  that prevent a child from being reaped or its PID reused before acquisition.
- Namespace init reaps adopted children. A single-threaded supervising parent owns
  its lifetime. Establish init's parent-death signal and validate parent liveness before
  releasing workload execution; account for credentials resetting that signal and
  for the creating thread, rather than an unrelated surviving parent thread.
- Disposal signals the exact pidfd, waits for the exact init to be reaped, then joins
  transport readers and the Windows launcher. Only that complete outcome proves
  disposal. A deadline, blocked kernel teardown, missing acknowledgment or transport
  failure returns Unavailable and preserves the existing replacement fence.
- The original bounded control channel carries launch state and disposal proof; CAS
  stdout/stderr cannot forge supervisor messages. Protocol framing/version, identity,
  parser limits, queues and shutdown deadlines must be fixed in authority before code.
  Control EOF requests disposal; losing the proof never becomes an implicit success.
- This owner serves managed-runtime retirement and fixed filesystem observations.
  It adds no turn-stop authority or interpretation of quiet CAS streams.

## Proposed Artifact Contract

Bundle a versioned Linux Rust executable with the trusted Windows Beryl distribution,
preferably a static artifact for the supported architecture. Execute its validated
drive-backed path through `wsl.exe`; do not compile, download or install it during launch.
Missing, mismatched, unmappable or unsupported artifacts make WSL admission unavailable.
Root execution requires a reviewed artifact trust boundary; a checksum alone does not
protect a writable binary from replacement between validation and execution.

The companion should have a narrow package boundary and package design, with backend
owning its protocol and executable composition owning artifact location. Exact build
target, linker, artifact publication/version binding and supported architecture must
be resolved before implementation. The local Ubuntu account reported no `rustc`,
`cargo` or `cc` in PATH, and Windows Rust had only its MSVC target installed. No missing
compiler was invoked and no software was installed. Machine details belong in `ENV.md`.

## Required Qualification And Remaining Limits

Production acceptance needs native Linux cases for normal exit, detached sessions,
double-forked descendants, nested namespaces, blocked/ignored graceful shutdown,
namespace-init death, supervisor death during every setup gate, control loss, malformed
output, permission failures, deadline expiry, and preservation of an unrelated process.
Verify exact identity and supplementary groups, home/environment, namespace-local proc,
and pinned CAS admission/configuration behavior. Simulated cleanup tests are insufficient.

The namespace proof covers Linux processes created inside that namespace. It does not
prove disposal of work started by external services, a Windows executable through WSL
interoperability, or a new `wsl.exe` invocation outside the namespace. Microsoft documents
that WSL can run Windows programs. The existing broad process-tree contract therefore
needs an explicit supported envelope and Windows interoperability qualification; do not
claim the kernel proof covers those routes or disable them as an incidental workaround.

Other unresolved boundaries are the privileged artifact's trust, credential capture,
bounded control/stream framing and exact supported kernel/syscall qualification. These
must be settled in owning design authority before the archived atomic-admission draft
is resumed. That draft remains unaccepted and unchanged.

# Sources

- Microsoft, WSL2-Linux-Kernel, canonical repository
  `https://github.com/microsoft/WSL2-Linux-Kernel`, requested tag
  `linux-msft-wsl-6.6.87.2`, resolved commit
  `427645e3db3a8896714f22a3d3fe0c3f7b317ad4` through GitHub's tag-ref API.
  Inspected [kernel/pid_namespace.c](https://github.com/microsoft/WSL2-Linux-Kernel/blob/427645e3db3a8896714f22a3d3fe0c3f7b317ad4/kernel/pid_namespace.c),
  `zap_pid_ns_processes` and `pidns_install`, on 2026-10-07. Establishes the
  namespace teardown/reaping invariant and rejection of ancestor namespace entry.
- Linux man-pages project, Michael Kerrisk, man-pages 6.19,
  [pid_namespaces(7)](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html),
  page date 2026-05-13, accessed 2026-10-07: init lifetime, ancestor SIGKILL,
  nested membership and namespace-correct proc mounting.
- Linux man-pages project, man-pages 6.19,
  [pidfd_open(2)](https://man7.org/linux/man-pages/man2/pidfd_open.2.html),
  accessed 2026-10-07: Linux 5.3 interface and acquisition/reaping conditions.
  A pidfd identifies one process; it is not itself proof of descendant disposal.
- Linux man-pages project, man-pages 6.19,
  [PR_SET_PDEATHSIG(2const)](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html),
  page date 2026-02-08, accessed 2026-10-07: parent-thread semantics, setup race,
  fork behavior and credential/privileged-exec reset conditions.
- Linux man-pages project, man-pages 6.19,
  [user_namespaces(7)](https://man7.org/linux/man-pages/man7/user_namespaces.7.html),
  accessed 2026-10-07: identity mappings and capability changes; informs why the
  successful unprivileged probe cannot establish unchanged execution identity.
- Microsoft Learn, [Working across file systems](https://learn.microsoft.com/en-us/windows/wsl/filesystems),
  accessed 2026-10-07: WSL interoperability runs Windows programs; informs the
  limit of an exclusively Linux namespace ownership proof.

# Local Probe Evidence

All namespace probes ran only installed `unshare` and `/bin/sh`, printed PID/identity,
and exited successfully. They launched neither CAS nor Beryl and touched no clipboard.
No descendant-disposal, parent-death, pidfd or native companion case was run.

```text
wsl.exe --version
wsl.exe --list --verbose
wsl.exe --distribution Ubuntu --exec /bin/sh -c 'uname -r; id; stat -f -c %T /proc; stat -f -c %T /sys/fs/cgroup; command -v unshare rustc cargo cc; test -w /sys/fs/cgroup'
wsl.exe --distribution Ubuntu --exec /usr/bin/unshare --user --map-current-user --pid --fork --kill-child=SIGKILL /bin/sh -c 'printf "namespace-pid: %s\n" "$$"; id'
wsl.exe --distribution Ubuntu --user root --exec /usr/bin/unshare --pid --fork --kill-child=SIGKILL /bin/sh -c 'printf "namespace-pid: %s\n" "$$"'
rustup target list --installed
```

The metadata command above is the concise reproducible subset of the original longer
read-only query; its final write-access test intentionally returns false on this machine.
Both actual namespace probes returned PID 1 and exit zero. Probe shell processes joined;
the shared WSL distribution was left running rather than stopped as task-owned state.
