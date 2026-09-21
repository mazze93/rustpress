# Decisions

- 2026-09-20: Build before seal, never at deployment. The plan's build-after-review would change the artifact. Reverse only with a new release format.
- 2026-09-20: Receipts belong outside releases. An immutable pressing cannot contain mutable deploy.json.
- 2026-09-20: Separate studio.mazzeleczzare.com from the existing Pages blog. Preserve /studio/ as the bench.
- 2026-09-20: v0.1 is static-only with no policy bypass. SVG, PDF, CSS resource loads and active HTML are rejected explicitly. This is a narrow complete product, not an incomplete sanitizer.
- 2026-09-20: ASCII paths only in v0.1; reject Unicode rather than claim incomplete cross-platform case folding.
- 2026-09-20: Full-tree hashes detect corruption, not hostile re-signing. Deploy requires an independently reviewed seal digest.
- 2026-09-20: Astro is fully static; no Cloudflare adapter or runtime Worker is needed. Cloudflare _headers supplies the CSP.
