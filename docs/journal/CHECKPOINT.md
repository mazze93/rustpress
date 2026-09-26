# Checkpoint

## Active checkpoint: hostname retarget (2026-09-26)

- [x] Cloudflare account confirmed: `Personal and Nonprofit Projects` / `a478cd1d`.
- [x] `studio.mazzeleczzare.com` has an existing live Worker (17 deployments, last 3 months ago). Must not be overwritten.
- [x] Deployment retargeted to `press.mazzeleczzare.com`; Worker renamed `rustpress-press` in `wrangler.json`.
- [x] `npm ci` clean: 0 vulnerabilities, 303 packages. Four packages have unapproved install scripts (esbuild x2, workerd, fsevents); review before deploy.
- [ ] `press.mazzeleczzare.com` DNS not yet verified — expected to be absent; Wrangler will create on first deploy.
- [ ] `astro check`, `npm audit`, `cargo audit` not yet run.
- [ ] Local build and smoke test not yet performed.
- [ ] Production deployment not yet performed.

## Active checkpoint: CI pinning (2026-09-26)

- [x] Clean checkout synchronized with origin/main at d53ca6c.
- [x] CI for d53ca6c passed: https://github.com/mazze93/rustpress/actions/runs/36273726212
- [x] Immutable checkout reference and explicit repository-pinned Rust setup.
- [x] CI-equivalent local checks pass: lock metadata, formatting, Clippy, 19 tests, release build.
- [x] Pushed e2899309b1f26328eb5a6ff33f0b3fbd727f2c62 and confirmed successful remote CI:
  https://github.com/mazze93/rustpress/actions/runs/36273904679
- [x] Upgraded actions/checkout to v7.0.1 (SHA 3d3c42e5, Node 24); Node 20 deprecation resolved at 0983b22.
- [x] Regressive PR #3 (floating runner/toolchain/checkout) closed without merge.

This checkpoint is complete. Next: run audits, local build, smoke test, then deploy.

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

Cloudflare account confirmed: `Personal and Nonprofit Projects` / `a478cd1da11cbde86fb26a63697c34e0`.
`studio.mazzeleczzare.com` is occupied by a separate live Worker — do not deploy there.
`press.mazzeleczzare.com` is the confirmed deployment target.
Never include credentials in exports.

## To resume

User redirected work to immediate source export on September 23, 2026.
Read HANDOFF.md first. Source, locks, and commissioning-001 pressing are packaged.
Run cargo test in this repository. Read DECISIONS.md before changing release semantics.
Verified pressing seal: 7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16.

The user subsequently authorized pushing the checkpoint to their public `mazze93/rustpress`
repository, without further implementation or production deployment. The commissioning
snapshot is preserved under `pressings/commissioning-001/`.
