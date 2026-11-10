# Local Kache fixes

The package stays on v1.2.2 rather than adopting the 2.0 scheduler.
`local-cache-fixes.patch` contains these fixes and their regressions:

- Probe the underlying compiler with `-vV`, check its exit status and require
  a compiler identity and nonempty host. Refresh older incomplete version
  memos, including Clippy-only results. Keep Clippy's own identity separate.
- Treat declared inputs below a regular file as missing, as Cargo does, while
  preserving permission and I/O errors. Cover both prediction hashing and the
  post-build input timestamp guard so worktree predictions can be relearned.
- Backport the indexed removal of unreadable metadata from the first commit
  of upstream PR #1742, `f2da86d3189c7d997aba5df5f21cc24a3dbcb706`.
  Do not enable that PR's separate stricter held-byte budget feature.
- Backport merged PR #1646, `036b22c4e99d6610e5404180bb4d2aa90c0ad5ee`,
  for windowed CLI, daemon and report reads across event rotation. Its compressed
  archive is bounded to eight days and 512 MiB. Lifetime savings remain separate.
- Update tests for indexed corruption removal and home-abbreviated paths;
  serialize the default version-memo cache test as in #1646.

`failure-diagnostics.patch` records the emitting engine's executable path and
BLAKE3 digest on compiler failures and build-script bypasses. This distinguishes
pinned build-script engines that survive a package upgrade. Bypass reasons keep
the failing stage and OS error kind/code without copying inner paths into the
event. Compiler failures include their exit code and compile duration. These
optional fields were introduced in local event schema 27. Successful
hits do not hash the engine. Diagnostics do not record arguments, environment
values or compiler output, and do not change scheduler or cache-key policy.

`admission-diagnostics.patch` adds optional local schema 28 observations of the
existing flight, discovery and capacity decisions. It retains at most 16 decisions
per wrapper, counts overflow and snapshots without draining on event reconstruction.
The per-wrapper observation ID and decision index allow consumers to deduplicate
decisions if a wrapper emits a secondary event.
Capacity evidence distinguishes base weight, maximum request and minimum observed
free slots. Successful lock scans stop early, so availability is a lower bound;
extrema need not occur together. The actual admitted count is recorded separately.
Unreadable test-marker observations do not hide measured occupancy counts.
A peer-completed flight means lock release, not successful compilation or publication.
Test-lease acquisition is not reported as
compile admission, and scheduler-disabled events omit these decisions.

Pressure evidence reuses the existing 250 ms sample and file reads, retaining the
source and actual observation timestamps. Disabled, unsupported and unavailable
monitoring are distinguished from numeric samples. Full PSI is observational only;
Linux's some-PSI threshold, macOS policy, cgroup fallback and opt-out remain unchanged.
No background sampler, paths, arguments, environment values or compiler output are
added. Normal hits do not probe pressure. Counters expose errors swallowed by the
committed-entry query and terminal local-lookup miss fallbacks without changing
those fallbacks or their retry policy. A missing row is not an error.

Local and upstream event schema numbers can diverge. Readers should identify these
optional fields by their presence, not assume that an upstream schema number means
the same thing. Older event files and daemon handoffs remain readable.

Scheduler policy adoption remains gated on a bounded real-Cargo comparison on a
dedicated host. It must record pressure, successful foreground latency and failure
counts, throughput and resource bounds with matched source/configuration. A policy
replay or passing scheduler unit tests is not a throughput or latency comparison.
Do not induce pressure on the shared laptop or change capacity to qualify a policy.

`build-script-correctness.patch` backports external native link-input tracking
from #1864 (`43b564c0218868283bbee8618ddb3a307c3bb9df`). Its prerequisites are
the build-unit ownership/source-link work from #1593
(`d2932194991878df979124068f2d7d814f2b4408`) and declared-input timestamps from
#1779 (`129a967f8676edc9fb3dd0a3c67f3b66da09bd34`). External inventories have a
shared 1,024-entry/64 MiB bound checked before content hashing. Scripts with
external link inputs use the ordinary run cache instead of sealed hermetic runs.
The patch includes shared-store regressions; stable 1.2.2 does not implement the
newer build-script readonly-store restoration feature.

The same patch includes only the Git-input commits from #1874:
`26b7e09f39774828f7b289fd802b22f8fc711bf0`,
`bcaa10f94763a8665ee844bd4471c742a49107a0`, and
`0805bd7a101c3c8e8761eb5cf74fbd7a6ea6f282`.
Local regressions cover already-expanded `.git/worktrees/<id>/HEAD` paths,
loose/packed refs, detached HEAD, and loose-ref deletion during a script run.
Resolution watches the relevant files and parent-directory timestamps. Commit
resolution does not cover `git describe --dirty`; scripts must declare their
working-tree inputs separately.

`scheduler-workload.patch` is opt-in through `enableWorkloadScheduler = true`.
The default package retains the installed scheduler while its reservation changes
remain under qualification. The patch keeps the existing pool, pressure threshold, timeout,
test leases and fail-open behavior. Completed deferred Rust compiles join the
publication flight without taking another capacity permit. Successful Rust and
C-family compiles record the RSS returned by waiting for their own compiler PID;
failed or interrupted compiles do not train estimates. Rust estimates distinguish
toolchain, target, codegen settings, crate/output kind and incremental mode.
C-family estimates distinguish compiler, source, target flags and codegen mode.
The new estimate namespace starts unmeasured and does not import crate-name-only
weights. This cold-estimate behavior also needs the dedicated-host qualification
above before activation.

`reuse-sharing.patch` backports #1874's profile-sharing commit `b9e998b2`
and discovery-flight release `f7d7be18`. Equivalent Cargo profiles share
generated-source identities, compiler remaps, artifacts and restored dep-info.
The local adaptation applies profile remapping only when `OUT_DIR` is present
and adds a scoped key marker. Unrelated compiler entries keep recipe version 32;
affected entries miss once instead of colliding with older remapped artifacts.
Different generated contents still produce different keys. A discovery waiter
whose source tree cannot use the owner's prediction releases the flight before
compiling, while a compatible waiter retains publication ownership.

## Weekly reuse and disk review

The package reports `1.2.2+local-reuse-20261109` in ordinary events; the opt-in
scheduler build adds `-scheduler-trial`. Group observations by engine version,
host and workload. Target-local build-script launchers can retain an older engine.

Workstations enable `record_sessions` for bounded report and GC history in
`~/.cache/kache/telemetry/`. Compiler events retain the existing eight-day,
512 MiB compressed archive. Check archive coverage and dropped observations
before interpreting a seven-day window; bounds can shorten retained history.

Review expensive misses and their recorded key-field changes, repeated demand
after eviction, duplicate publications, admission waits, hit restore time, and
new versus already-present blob counts and copy/reflink bytes. Use successful ordinary builds separately
from experiments and failures. Session closure is inferred from inactivity and
does not establish complete Cargo command wall time.

For disk comparisons, capture the cache and relevant target directories with
`btrfs filesystem du -s` at both endpoints. Keep exclusive and shared extents
separate from logical output bytes, indexed blob bytes and GC accounting.
Reflinked targets can retain extents after cache eviction. A better hit percentage
alone is not evidence of faster builds or lower physical disk use.

The Nix build runs workspace library/binary tests. Also run the complete
workspace integration/doc suite with the repository's resource guard, a clean
Cargo configuration, wrapper disabled, and its pinned Rust toolchain. Some
fixtures require temporary files on the same filesystem as the built binary.
Validate the resulting Nix binary with isolated real Clippy/Cranelift and
build-script cache-hit fixtures before activation. Do not use the production
cache for intentionally corrupt metadata or missing-directory tests.

Wrapper and daemon must continue to select the same package. The compiler cache
and index schemas stay unchanged. Build-script action and hermetic namespaces
retire older records so they cannot bypass the new input checks. Existing
target-local launchers can still pin an older engine until Cargo rebuilds them.
Rotation archives add optional history which the
older binary will not read; rollback does not require deleting the cache.
The live limit remains 150 GiB while collecting a fixed-configuration baseline;
100 GiB is a later measured trial, not an assumption from survivor statistics.

Upstream issues and PRs must not be posted without explicit approval. Recheck
upstream fixes and refresh this patch before changing the pinned source version.
