# Checkpoint

## Active checkpoint: corrected release deployed (2026-09-27)

- [x] Found and fixed a real cross-platform bug: `stage`/`deploy` could not run at
  all on stock macOS. `tempfile::tempdir()` resolves under `$TMPDIR`
  (`/var/folders/...`), and `/var` is a standard macOS symlink to `/private/var`;
  `paths::no_symlinks` walked from the root and rejected `/var` itself as if it
  were an attacker-planted symlink, before reaching any real content. Fixed by
  canonicalizing the build/deploy temp-workspace root once, immediately after
  creation, in both `stage()` and `deploy()` (`src/lib.rs`). `no_symlinks` is
  unchanged for user-supplied `site`/`source` trees and everything written inside
  the canonicalized workspace; all 19 tests (including the symlink-rejection
  tests) still pass unmodified.
- [x] Restored `pressings/commissioning-001` to local state, verified it (seal
  matches), then staged a fresh `commissioning-002` release from
  `content/reviewed` against the corrected site config. Verified: 59 sealed
  files, seal `1b5979f2cd05549b4abf96feb571f4d608b6c190bf4e45170e2c2e71cf1a3485`.
- [x] Dry-run deployed (no credentials/network), confirmed only the expected
  4 changed URLs, then deployed for real via `cargo run --locked -- deploy`
  using a scoped Cloudflare Workers API token pulled through `pass-cli run`
  (never viewed, only injected into the deploy subprocess) and the already-known
  non-secret account ID from the "Production boundary" section below.
- [x] Live at `press.mazzeleczzare.com`, Cloudflare Version ID
  `ac267804-8ed5-4d0a-8c42-84e1599f2de4`. Verified post-deploy (with a
  cache-busting query param, since Cloudflare briefly served a cached copy of
  the old page): canonical, `og:url`, `/sitemap.xml`, and `/robots.txt` all now
  say `press.mazzeleczzare.com`. Confirmed stable on repeat fetches without
  cache-busting a few seconds later.
- [x] Confirmed `studio.mazzeleczzare.com` untouched before and after (still its
  own Cloudflare-Access login redirect, unrelated live site — see DECISIONS.md).
- [ ] Not yet done: exercise a deliberate deployment-failure path to confirm the
  `unknown`-status receipt behavior; rollback path untested.

Incident note: while investigating which Proton Pass item held the Cloudflare
credentials, `pass-cli item view --output json` on a *different*, unrelated
item (`claude-cloudflare-access`, a Cloudflare Access service-token pair, not
usable for this deploy anyway) printed its `CF-Access-Client-Id`/`Secret` in
plaintext into the session transcript despite no `--show-secrets` flag being
passed — that was wrong to assume safe. That item was never used for anything;
flagged to the user as likely needing rotation. Corrected process for this
project going forward: only ever use `pass-cli run --env-file`/`pass:// `
references piped straight into the target command, never `item view`/`list`
on anything in a real credential vault.

## Active checkpoint: canonical-hostname audit (2026-09-26)

- [x] Live check: `press.mazzeleczzare.com` already resolves and serves HTTP 200 — the
  earlier "expected to be absent" assumption below is stale. It is serving a build of
  this site (edition `commissioning-001`, matching the sealed pressing), but that build
  predates the fixes below and its canonical/OG tags and sitemap still point at
  `studio.mazzeleczzare.com`.
- [x] Live check: `studio.mazzeleczzare.com` returns a Cloudflare Access login redirect —
  confirmed a separate, unrelated, auth-backed live site (owner's music project), not a
  Rustpress artifact. Never a valid deploy target for this project. See DECISIONS.md.
- [x] `astro check`: 0 errors, 0 warnings, 0 hints. Nothing to fix.
- [x] Fixed stale `studio.mazzeleczzare.com` references the 177ad10 retarget commit
  missed: `site/astro.config.mjs` (`site`), `site/src/data.ts` (`site` const),
  `site/public/robots.txt` (sitemap URL). `wrangler.json` was already correct.
- [ ] Not yet done: re-`stage` a new release and `deploy` so the live site actually
  carries corrected canonical/OG/sitemap URLs — the currently-live deployment still
  has the stale ones baked in.
- [ ] `cargo audit` / `npm audit` — see next checkpoint section below.

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
