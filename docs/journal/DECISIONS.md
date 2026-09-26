# Decisions

- 2026-09-20: Build before seal, never at deployment. The plan's build-after-review would change the artifact. Reverse only with a new release format.
- 2026-09-20: Receipts belong outside releases. An immutable pressing cannot contain mutable deploy.json.
- 2026-09-20: Separate studio.mazzeleczzare.com from the existing Pages blog. Preserve /studio/ as the bench.
- 2026-09-20: v0.1 is static-only with no policy bypass. SVG, PDF, CSS resource loads and active HTML are rejected explicitly. This is a narrow complete product, not an incomplete sanitizer.
- 2026-09-20: ASCII paths only in v0.1; reject Unicode rather than claim incomplete cross-platform case folding.
- 2026-09-20: Full-tree hashes detect corruption, not hostile re-signing. Deploy requires an independently reviewed seal digest.
- 2026-09-20: Astro is fully static; no Cloudflare adapter or runtime Worker is needed. Cloudflare _headers supplies the CSP.
- 2026-09-24: Resume one small checkpoint at a time to respect the user's constrained budget. Delegate test drafting to the on-device model; retain final validation before pushing. Reverse by explicitly selecting a different checkpoint, not silently expanding scope.
- 2026-09-24: Reject the local-model test draft: it simulated fixtures, changed process-global environment, and claimed success without runnable evidence. Replace it with real-fixture tests and isolated child-process environment. Local model output remains a draft, never validation evidence.
- 2026-09-26: Pin checkout to its verified v4 commit, remove the third-party Rust setup action, and install the exact numeric version from rust-toolchain.toml with runner-provided rustup. Pin the runner OS to ubuntu-24.04, not ubuntu-latest. The hosted image still receives updates; this is not a hermetic build. Reversal requires a reviewed workflow change.
