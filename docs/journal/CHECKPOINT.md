# Checkpoint

## Active checkpoint: CI pinning (2026-09-26)

- [x] Clean checkout synchronized with origin/main at d53ca6c.
- [x] CI for d53ca6c passed: https://github.com/mazze93/rustpress/actions/runs/36273726212
- [x] Immutable checkout reference and explicit repository-pinned Rust setup.
- [x] CI-equivalent local checks pass: lock metadata, formatting, Clippy, 19 tests, release build.
- [x] Pushed e2899309b1f26328eb5a6ff33f0b3fbd727f2c62 and confirmed successful remote CI:
  https://github.com/mazze93/rustpress/actions/runs/36273904679

Known non-blocking runner annotation: pinned checkout v4 declares Node 20 and GitHub
forces it onto Node 24. The run passed; review a current checkout-major upgrade in a
separate dependency checkpoint. No action upgrade was silently bundled into pinning.

This checkpoint is complete. Next bounded feature-validation task: browser smoke
test of the sealed Studio output, without rebuilding or publishing to production.

Stop after recording this result. No deployment or credential changes.

- [x] Read prior blueprint and current blog.
- [x] Resolve GitHub account and repository inventory.
- [x] Compiler and initial adversarial tests: 13 tests, formatting, and Clippy pass.
- [x] Sealed Astro output and deployment CLI implemented; actual deployment untested.
- [x] Studio UI implemented; browser validation and main-site integration incomplete.
- [x] Source and sealed commissioning fixture pushed to public `mazze93/rustpress`.
- [ ] CI, preview, production verification.

## Active checkpoint: 2026-09-24

- [x] Repository is clean and synchronized with origin/main at 05f3af0.
- [x] Scope bounded to release-integrity regression tests.
- [x] On-device draft reviewed; rejected because it substituted mock data for the real fixture and did not demonstrate validation.
- [x] Replaced with six real-fixture integration tests; all 19 tests pass.
- [x] Formatting, Clippy, and pinned commissioning seal verification pass.
- [x] Results recorded for the completed checkpoint commit.
- Push confirmation: compare this checkpoint commit with origin/main.

Stop after this checkpoint. No production deployment is authorized by this work.

Concurrent upstream CI commits through 55095e4 were preserved by rebasing the test
checkpoint, then rerunning all local checks successfully.

Next bounded checkpoint: inspect the existing GitHub CI result and pin mutable
action/toolchain references. Do not add deployment credentials or deployment jobs.

## Production boundary

Cloudflare account, hostname, and DNS inspection is incomplete.
Production payload and any hostname changes require review. Never include credentials in exports.

## To resume

User redirected work to immediate source export on September 23, 2026.
Read HANDOFF.md first. Source, locks, and commissioning-001 pressing are packaged.
Run cargo test in this repository. Read DECISIONS.md before changing release semantics.
Verified pressing seal: 7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16.

The user subsequently authorized pushing the checkpoint to their public `mazze93/rustpress`
repository, without further implementation or production deployment. The commissioning
snapshot is preserved under `pressings/commissioning-001/`.
