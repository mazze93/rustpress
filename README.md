# Rustpress

A local-first Rust CLI for deliberate static publishing. This repository contains working compiler code, security tests, an Astro Studio site, and a verified commissioning pressing. It is an in-progress delivery, not a completed production release.

Repository: [mazze93/rustpress](https://github.com/mazze93/rustpress).

## What is implemented

- `stage`: inspect an explicitly selected HTML corpus, rewrite local links, exclude drafts, build Astro in a temporary workspace, and seal the complete deployment output.
- `verify`: check the sealed inventory and hashes, reapply corpus policy, and optionally compare against an independently retained digest. Does not need the original corpus.
- `deploy`: require a reviewed seal digest, project the sealed output, install locked tooling, and invoke the pinned Wrangler executable without rebuilding the site. Deployment receipts live outside the pressing.
- A static Astro archive configured for `studio.mazzeleczzare.com`, with local fonts, a public manifest, sitemap, security headers, and a commissioning specimen.

See [HANDOFF.md](HANDOFF.md) for exact validation evidence, remaining work, and security limitations.

## Requirements

- Rust 1.98.1, selected by `rust-toolchain.toml`.
- Node 22.23.2, selected by `site/.nvmrc`, and npm.
- Network access for initial Cargo/npm dependencies.
- Cloudflare credentials only for actual production deployment, not verification or deployment dry runs.

## Validate the source

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

## Verify the included pressing

The repository preserves the commissioning bundle under `pressings/commissioning-001/`. Restore it to the CLI's gitignored local state before running these commands. If using the original source ZIP, the bundle is already at that local-state path.

```sh
mkdir -p site/.rustpress/releases
# Run only when the local-state pressing does not already exist:
test -e site/.rustpress/releases/commissioning-001 || \
  cp -R pressings/commissioning-001 site/.rustpress/releases/

cargo run --locked -- verify \
  --site site \
  --release commissioning-001 \
  --expect 7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16

cargo run --locked -- deploy \
  --site site \
  --release commissioning-001 \
  --expect 7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16 \
  --dry-run
```

Dry-run verifies and prints the target without network access, credentials, or site mutation. Do not remove `--dry-run` until the hostname and full publication payload have been reviewed.

## Make a new pressing

```sh
cargo run --locked -- stage \
  --source content/reviewed \
  --site site \
  --release commissioning-002 \
  --previous commissioning-001
```

Choose a new release label every time. The `stage` command installs locked site dependencies and builds Astro before sealing. It prints the seal digest that must be independently retained for deployment. The source corpus and site can subsequently change without changing the pressing.

The site is not intended to build independently from an empty checkout: `stage` supplies the real `public/pressing.json` and pressed corpus to its temporary build workspace. There is no fabricated sample manifest fallback.

## Publication policy

Corpus HTML accepts a limited element/attribute allowlist. Scripts, forms, embedded documents, inline styles, remote links/resources, SVG, PDF, and JavaScript files are rejected. CSS is tokenized; resource loads and at-rules are rejected. Images are PNG, JPEG, GIF, or WebP; WOFF2 signatures are recognized. Paths are portable ASCII, not silently Unicode-normalized.

The surrounding Astro template is trusted build code, separate from the untrusted corpus. Its build runs without Cloudflare credentials. No active-content override is implemented.

## Save to your own repository

Extract the archive, enter its `rustpress/` directory, then:

```sh
git init -b main
git add .
git commit -m "Import Rustpress implementation checkpoint"
```

Build caches, dependencies, secrets, and working release-state directories are excluded by `.gitignore`. The deliberately preserved commissioning snapshot under `pressings/` is tracked. No API credential is included.
