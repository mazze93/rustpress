# Rustpress implementation handoff

Exported September 23, 2026 at the user's request to preserve the work immediately, then prepared for the public `mazze93/rustpress` repository. This is actual implementation code and a sealed build, not the earlier blueprint. Repository publication is not production deployment; the product is not yet shipped end to end.

## Verified in this workspace

The following checks were rerun successfully immediately before export:

| Check | Result |
| --- | --- |
| `cargo test --locked --quiet` | 13 tests passed |
| `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo run --locked -- verify --site site --release commissioning-001` | Passed; 59 sealed files |

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
- `site/`: Astro Studio interface, local font dependencies, sitemap, CSP headers, pinned Wrangler config.
- `content/reviewed/`: authored commissioning specimen only, not copied private writing.
- `Cargo.lock` and `site/package-lock.json`: resolved dependency graphs.
- `docs/journal/`: plan, decisions, and checkpoint.
- `pressings/commissioning-001/`: verified sealed pressing, preserved in Git. Restore it to `site/.rustpress/releases/commissioning-001/` for CLI verification; the original ZIP already included it at that local-state path.

## Not completed or verified

- The public repository is a preservation checkpoint, not a production release.
- No CI workflows or protected deployment environment were implemented.
- No change was made to the existing `mazze-leczzare-blog` repository.
- `studio.mazzeleczzare.com` is the configured destination, not a confirmed live deployment.
- Cloudflare hostname/account/DNS inspection and production deployment remain unfinished.
- Actual Wrangler deployment, remote version capture, rollback, and post-deploy HTTP checks have not been exercised.
- Browser screenshots, mobile/accessibility checks, and full internal site-link crawling remain unperformed.
- `astro check`, npm vulnerability audit, and Cargo vulnerability audit have not been completed.
- Release-level regression coverage is incomplete: full CLI deployment failure paths, concurrent races, atomic durability under crashes, and extra-file seal tampering need broader integration tests.

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

1. Save this checkpoint to a repository without treating it as a production release.
2. Expand release-level tests; verify the sealed output in a browser; complete dependency auditing.
3. Add separate secret-free verification and digest-pinned, approval-gated deployment workflows.
4. Inspect Cloudflare accounts, zone, hostname records, and any existing Worker bindings.
5. Review the exact sealed public payload and hostname mutation before deploying.
6. Verify HTTP status, security headers, public hashes, and deployment version after upload.
7. Add a main-site Studio link only after the subdomain works.

## Deployment design references

Workers Assets supports `_headers` for static-response headers, so this site does not require a runtime Worker merely to supply CSP: [Cloudflare static asset headers](https://developers.cloudflare.com/workers/static-assets/headers/).

Wrangler custom domains create DNS and certificate resources. Inspect the existing hostname before enabling deployment, rather than assuming configuration is inert: [Cloudflare Workers custom domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
