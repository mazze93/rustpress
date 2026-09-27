# Rustpress implementation handoff

Exported September 23, 2026 at the user's request to preserve the work immediately, then prepared for the public `mazze93/rustpress` repository. This is actual implementation code and a sealed build, not the earlier blueprint. Repository publication is not production deployment; the product is not yet shipped end to end.

## Verified in this workspace

The following checks passed at the September 24, 2026 release-integrity checkpoint:

| Check | Result |
| --- | --- |
| `cargo test --locked --quiet` | 19 tests passed: 13 corpus/security tests plus 6 release-integrity tests |
| `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo run --locked -- verify --site site --release commissioning-001` | Passed; 59 sealed files |
| `astro check` (site/, 2026-09-26) | Passed; 0 errors, 0 warnings, 0 hints |
| `cargo audit` (2026-09-26, 135 crates) | Passed; 0 advisories |
| `npm audit` (site/, 2026-09-26, 426 packages) | Passed; 0 vulnerabilities |
| `cargo run --locked -- stage` (2026-09-27, `commissioning-002`) | Passed; 59 sealed files, seal `1b5979f2cd05549b4abf96feb571f4d608b6c190bf4e45170e2c2e71cf1a3485` |
| `cargo run --locked -- deploy` (2026-09-27, `commissioning-002`, real Wrangler deploy) | Passed; live at `press.mazzeleczzare.com`, Version ID `ac267804-8ed5-4d0a-8c42-84e1599f2de4` |

CI pinning checkpoint (September 26): full-SHA checkout, repository-defined Rust
1.98.1, and Ubuntu 24.04 passed remote metadata, formatting, lint, tests, and release
build at commit e2899309b1f26328eb5a6ff33f0b3fbd727f2c62:
https://github.com/mazze93/rustpress/actions/runs/36273904679.
Checkout upgraded to actions/checkout v7.0.1 (SHA 3d3c42e5, Node 24) at commit
0983b22e3c2797190eee6f819c54b38d566c1738; Node 20 deprecation warning resolved.

Commissioning seal:

```text
7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16
```

The completed pressing contains the built Astro site plus the original site-input snapshot, corpus manifest, changed-URL list, and release descriptor. The Astro data-file lookup failure from the earlier run was corrected; a completed pressing now exists and passes verification. No claim of clean-machine reproducibility or identical output across rebuilds has been established.

## Included source

- `src/main.rs`: command-line interface for stage, verify, and deploy.
- `src/paths.rs`: path policy, symlink rejection, bounded reads, snapshots, route normalization, hashing.
- `src/content.rs`: YAML frontmatter, drafts, HTML policy and rewriting, CSS token checks, asset signatures, generated index, and manifest verification.
- `src/lib.rs`: site capture, temporary Astro build, bundle sealing, verification, and deployment projection.
- `tests/security.rs`: adversarial and property tests.
- `tests/release_integrity.rs`: real sealed-fixture verification, modified/extra/missing file rejection, wrong-digest rejection, and a credential-free CLI dry-run with before/after file and directory inventory checks.
- `site/`: Astro Studio interface, local font dependencies, sitemap, CSP headers, pinned Wrangler config.
- `content/reviewed/`: authored commissioning specimen only, not copied private writing.
- `Cargo.lock` and `site/package-lock.json`: resolved dependency graphs.
- `docs/journal/`: plan, decisions, and checkpoint.
- `pressings/commissioning-001/`: verified sealed pressing, preserved in Git. Restore it to `site/.rustpress/releases/commissioning-001/` for CLI verification; the original ZIP already included it at that local-state path.

## Not completed or verified

- The public repository is a preservation checkpoint, not a production release.
- Secret-free Rust CI passed for the release-integrity checkpoint: https://github.com/mazze93/rustpress/actions/runs/36273726212. Checkout is now full-SHA pinned and Rust is selected from the exact repository toolchain version; remote validation of this workflow change is recorded in docs/journal/CHECKPOINT.md. No protected deployment environment is configured.
- No change was made to the existing `mazze-leczzare-blog` repository.
- `press.mazzeleczzare.com` is the sole canonical Rustpress destination. `studio.mazzeleczzare.com` is a separate, unrelated, auth-backed live site (the owner's music project, protected by Cloudflare Access) and must never be a Rustpress deploy target.
- Cloudflare DNS for `press.mazzeleczzare.com` already existed from an earlier out-of-band deployment (confirmed 2026-09-26, HTTP 200, edition `commissioning-001`, but with stale `studio.mazzeleczzare.com` canonical/OG tags and sitemap URLs baked in). **Corrected 2026-09-27**: pressed `commissioning-002` from the fixed source (`astro.config.mjs`, `site/src/data.ts`, `site/public/robots.txt`) and deployed it for real via `cargo run --locked -- deploy` — live site now verified serving `press.mazzeleczzare.com` in its canonical/OG tags, sitemap, and robots.txt (Cloudflare Version ID `ac267804-8ed5-4d0a-8c42-84e1599f2de4`). `studio.mazzeleczzare.com` confirmed untouched (still its own Cloudflare-Access-protected login redirect) before and after.
- Discovered and fixed in the same run: `stage`/`deploy` could not complete on stock macOS at all — `tempfile::tempdir()` resolves to `$TMPDIR` (`/var/folders/...`), and `/var` is a standard macOS symlink to `/private/var`, which `paths::no_symlinks` rejected as if it were an attacker-planted symlink, before ever reaching real content. Fixed by canonicalizing the build/deploy temp-workspace root once, immediately after `tempfile::tempdir()?` creates it (`src/lib.rs`); `no_symlinks` still fully applies to everything inside that root and to all user-supplied `site`/`source` trees, so the security guarantee for those is unchanged (`tests/security.rs` symlink tests still pass unmodified).
- Actual Wrangler deployment, remote version capture, and post-deploy HTTP verification have now been exercised through this CLI end to end (2026-09-27). Rollback and deployment-failure-path behavior remain untested.
- Browser screenshots, mobile/accessibility checks, and full internal site-link crawling remain unperformed.
- `astro check`, `cargo audit`, and `npm audit` all passed (2026-09-26); see the table above. Both audits are now wired as a non-blocking `audit` job in CI (`.github/workflows/rustpress.yml`) so future advisory disclosures are surfaced without gating merges — a human reviews findings deliberately rather than being auto-blocked.
- Release-level regression coverage now includes modified, extra, and missing files plus digest mismatch and dry-run non-mutation. Actual deployment failure paths, concurrent races, and atomic durability under crashes remain untested.

## Security boundaries and known limitations

- The seal is a SHA-256 inventory, not a digital signature. Someone who can rewrite the bundle and descriptor can forge a new internally consistent seal. Independently retain the reviewed digest; deployment requires it.
- Immutability is enforced by refusing label reuse and detecting changed bytes, not by filesystem write protection or WORM storage.
- The filesystem code rejects symlinks and checks bounded file reads, but it is not a capability-based defense against a malicious local actor concurrently replacing parent directories. Use an operator-controlled workspace.
- HTML parsing is policy enforcement, not complete HTML conformance validation. Do not claim malformed HTML is comprehensively rejected.
- Fragment identifiers are preserved but their target IDs are not checked.
- Asset checks inspect signatures, not full decoding or polyglot analysis.
- CSS support is intentionally narrow: all at-rules and resource-loading constructs are rejected. WOFF2 acceptance does not imply corpus CSS supports `@font-face`.
- Only the corpus passes through the untrusted-content policy. Astro templates and their dependency graph are trusted build inputs.
- Public source fingerprint includes the hashes of selected input files, including excluded drafts. Draft bytes and names are not published, but the fingerprint can change when a draft changes.
- The deployment path reinstalls the locked package graph; lockfiles constrain resolution but do not eliminate tooling supply-chain risk.
- The CLI currently reports an outer exit code of 1 for errors, rather than preserving subprocess-specific exit codes.
- A failed deployment receipt uses `unknown`, because remote acceptance can precede a local failure. Do not assume rollback occurred.
- `changed-urls.txt` is an operational hint, not a comprehensive cache invalidation proof for arbitrary site-template changes.

## Next honest engineering steps

1. ~~Press a fresh release and deploy it now that the canonical-hostname fix is in~~ — done 2026-09-27: `commissioning-002` staged, verified, and deployed live to `press.mazzeleczzare.com` (Version ID `ac267804-8ed5-4d0a-8c42-84e1599f2de4`); confirmed `studio.mazzeleczzare.com` untouched.
2. Read docs/journal/CHECKPOINT.md for the latest CI evidence before selecting the next bounded task.
3. Run `astro check`, `npm audit`, and `cargo audit` locally on future changes; both audits are also wired as a non-blocking CI job.
4. Add a main-site link now that the corrected subdomain content is confirmed live.
5. Exercise a deployment-failure path deliberately (e.g. an invalid Wrangler config) to confirm the `unknown`-status receipt behavior documented under "Security boundaries."

## Deployment design references

Workers Assets supports `_headers` for static-response headers, so this site does not require a runtime Worker merely to supply CSP: [Cloudflare static asset headers](https://developers.cloudflare.com/workers/static-assets/headers/).

Wrangler custom domains create DNS and certificate resources. Inspect the existing hostname before enabling deployment, rather than assuming configuration is inert: [Cloudflare Workers custom domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
