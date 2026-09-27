# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Rustpress is a Rust CLI for deliberate, sealed static publishing, plus the Astro
site (`site/`) it publishes. It is not a general-purpose static site generator: the
core design goal is that once a corpus is "pressed" into a release, that exact byte
content is what gets deployed — never rebuilt, never silently modified. Read
`README.md` for user-facing usage and `HANDOFF.md` for the current verification
status, known security limitations, and what is/isn't done yet — check `HANDOFF.md`
before asserting something is "safe" or "complete."

## Commands

```sh
cargo test --locked --all-features           # unit + integration tests
cargo fmt --all -- --check                    # formatting (CI enforces)
cargo clippy --locked --all-targets --all-features -- -D warnings   # lint (CI enforces, warnings are errors)
cargo build --locked --release
cargo test --locked --test security <name>    # single test in tests/security.rs
cargo test --locked --test release_integrity <name>  # single test in tests/release_integrity.rs
cargo audit                                   # advisory scan (non-blocking CI job; install via `cargo install cargo-audit --locked`)
cd site && npm run check                      # astro check (type/template diagnostics)
cd site && npm audit                          # dependency vuln scan (non-blocking CI job)
```

CI (`.github/workflows/rustpress.yml`) runs exactly: lockfile validation, `cargo fmt
--check`, `cargo clippy -D warnings`, `cargo test --locked --all-features`, `cargo
build --locked --release`, on the exact Rust version pinned in `rust-toolchain.toml`
(currently 1.98.1) — not just "a recent stable." `RUSTFLAGS="-Dwarnings"` is set
globally in CI, so any warning fails the build.

The Astro site under `site/` has its own toolchain (Node version pinned in
`site/.nvmrc`, npm scripts `dev`/`build`/`check`/`preview`). It is not built directly
from a clean checkout — see "The site cannot build standalone" below.

`cargo audit` and `npm audit` also run as a separate, non-blocking `audit` job in CI
(`continue-on-error: true`) so new advisories are visible without gating merges —
review findings deliberately rather than treating a red audit job as a required
check. Last manually reviewed 2026-09-26: both clean (0 advisories / 0
vulnerabilities); see `HANDOFF.md`.

**`press.mazzeleczzare.com` is the only valid Rustpress deploy target.**
`studio.mazzeleczzare.com` is a separate, unrelated, auth-backed live site (the
owner's music project) — never point `wrangler.json`, `astro.config.mjs`, `site/src/data.ts`,
or `site/public/robots.txt` at it.

## Architecture

### The three-command lifecycle: stage → verify → deploy

- **`stage`** (`rustpress::stage` in `src/lib.rs`): takes a source corpus + the
  `site/` directory, compiles/validates the corpus (`src/content.rs`), builds Astro
  in a *temporary* workspace (never in-place in `site/`), diffs the Astro output
  against the pressed corpus bytes to catch template tampering, and seals everything
  (corpus, built `dist/`, and the exact site inputs used) into
  `site/.rustpress/releases/<release>/` under a sealed digest ("the seal"). It never
  overwrites an existing release label — labels are one-shot.
- **`verify`** re-derives all hashes from a sealed release directory and checks them
  against the recorded seal, and optionally against an independently-retained
  digest string. It needs *only* the sealed bundle — no source corpus, no `site/`
  config — this is deliberate and tested (see `tests/release_integrity.rs`).
- **`deploy`** re-verifies the seal, projects the sealed `dist/` + locked
  `package.json`/`wrangler.json` into a fresh temp workspace, `npm ci`s the *exact*
  locked Wrangler version recorded in the seal, and invokes it — it never re-runs
  `astro build`. Deploy requires `CLOUDFLARE_API_TOKEN`/`CLOUDFLARE_ACCOUNT_ID` env
  vars unless `--dry-run` is passed (dry-run does no network I/O and needs no
  credentials).

The seal is a SHA-256 file inventory, **not a cryptographic signature** — anyone
who can rewrite the whole bundle can forge a new internally-consistent seal. The
mitigation is procedural: the digest printed by `stage` must be independently
retained and passed to `--expect` on `verify`/`deploy`.

### Untrusted corpus vs. trusted site template

`src/content.rs` enforces a strict allowlist policy (`POLICY` const in
`src/lib.rs`) over the corpus only: a fixed HTML element/attribute allowlist,
tokenized CSS with all at-rules/`url()`/resource loads rejected, raster-image
signature sniffing (PNG/JPEG/GIF/WebP + WOFF2), ASCII-only portable paths, and
draft exclusion via YAML frontmatter (`draft: true`). Internal links are rewritten
relative-to-absolute against a computed route table; anything not resolvable
inside the corpus is a hard error, not a warning.

The Astro template in `site/` is **trusted build code** — it is not run through
this policy. The boundary is intentional: `content::validate_pressed` re-validates
the corpus subset of a sealed release against the same policy, but the surrounding
site template is out of scope for that check.

### The site cannot build standalone (mostly)

`site/src/data.ts` reads `public/pressing.json` at build time with no fallback —
without it, `astro build`/`astro check` fail outright. `pressing.json` is
normally a generated artifact of `rustpress stage` (written into the temp build
workspace), but a snapshot of it **is** checked into `site/public/pressing.json`
specifically so a fresh checkout can run `astro check`/`npm run build` directly.
Whoever presses a new release should update that checked-in copy from the new
release's `pressing.json` (matching content, just a different `release` label)
so it doesn't silently describe a stale edition — nothing enforces this
automatically. `public/published/` (the actual corpus HTML) is **not** checked
in and stays a generated, gitignored-in-spirit artifact; to exercise the full
`stage`/`verify`/`deploy` pipeline (not just `astro check`) you need a real
pressed release — see the "Verify the included pressing" steps in `README.md`
to restore `pressings/commissioning-001/` into `site/.rustpress/releases/`
first, then run `stage`/`verify` against it, or press a new release from
`content/reviewed/`.

`site_snapshot` in `src/lib.rs` also enforces invariants on `site/` itself before
staging will proceed: `package.json`'s build script must be exactly `astro build`,
Wrangler must be pinned to an exact three-part version, and `wrangler.json` is
checked field-by-field (`validate_wrangler`) — e.g. `workers_dev`/`preview_urls`
must be `false`, `assets.directory` must be `./dist`, exactly one `custom_domain`
route. Changing `site/wrangler.json` or `site/package.json` outside these
constraints will cause `stage` to refuse.

### Path/filesystem safety primitives (`src/paths.rs`)

Nearly all filesystem access funnels through `paths::read`/`paths::snapshot`, which
reject symlinks (`no_symlinks`, walking every path component), enforce portable
ASCII-only relative paths with no reserved Windows device names, and detect
TOCTOU by re-checking file metadata (size, mtime, ctime) after reading. When
adding new file-reading code paths, use these helpers rather than raw `std::fs`
calls — the existing security tests (`tests/security.rs`) specifically probe for
path traversal, case collisions, and symlink bypass.

### Locking and atomicity

`stage` and `deploy` each take an exclusive `flock` (`press.lock` /
`deploy.lock` under `site/.rustpress/`) so concurrent invocations refuse rather
than race. Sealed releases are written into a temp dir and moved into place with
a single `fs::rename` (atomic on the same filesystem) so a crash mid-stage can't
leave a half-written release directory that satisfies a later `verify`.

## Testing conventions

- `tests/security.rs` is adversarial/property-style: path traversal, label
  injection, HTML/CSS policy bypass attempts, srcset parsing edge cases.
- `tests/release_integrity.rs` operates on the real committed fixture at
  `pressings/commissioning-001/` (seal digest hardcoded as `SEAL` in that file) —
  it copies the fixture into a temp dir per test and never mutates the tracked
  copy. It covers modified/extra/missing-file rejection, wrong-digest rejection,
  and dry-run non-mutation (asserting the site directory's file inventory is
  byte-identical before and after a dry-run deploy).
- If you regenerate `pressings/commissioning-001/` (e.g. because `content/`
  changed), the `SEAL` constant in `tests/release_integrity.rs` and the `--expect`
  digest in `README.md` must be updated together — they will silently fall out of
  sync otherwise since nothing cross-checks them against each other.
