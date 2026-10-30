# Local Kache fixes

The package stays on v1.2.2 rather than adopting the 2.0 release-candidate
engine. `local-cache-fixes.patch` contains these fixes and their regressions:

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

The Nix build runs workspace library/binary tests. Also run the complete
workspace integration/doc suite with the repository's resource guard, a clean
Cargo configuration, wrapper disabled, and its pinned Rust toolchain. Some
fixtures require temporary files on the same filesystem as the built binary.
Validate the resulting Nix binary with isolated real Clippy/Cranelift and
build-script cache-hit fixtures before activation. Do not use the production
cache for intentionally corrupt metadata or missing-directory tests.

Wrapper and daemon must continue to select the same package. No index/key
schema upgrade is intended. Rotation archives add optional history which the
older binary will not read; rollback does not require deleting the cache.
The live limit remains 150 GiB while collecting a fixed-configuration baseline;
100 GiB is a later measured trial, not an assumption from survivor statistics.

Upstream issues and PRs must not be posted without explicit approval. Recheck
upstream fixes and refresh this patch before changing the pinned source version.
