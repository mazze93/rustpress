# Authority puzzle — commissioning-006 (2026-11-10)

Interactive workshop page shipped as a **trusted Astro shell** route
(`/authority-puzzle/`), not as corpus HTML. Corpus policy remains
`rustpress-static-v1` (no active content). Rustpress staged, sealed, and
deployed the Astro site plus the unchanged `content/reviewed` specimen.

## Changes

- `site/src/pages/authority-puzzle.astro` — folds, toolbar, counterexample bench
- `site/src/pages/index.astro` — nav link (not listed among sealed corpus editions)
- `site/src/pages/sitemap.xml.ts` — includes `/authority-puzzle/`
- Sealed bundle preserved at `pressings/commissioning-006/`
- `site/public/pressing.json` synced to release `commissioning-006`

## Seal

```text
5beac40fd9b8c798172536d1526d47c0e85c9e4c4216070e69fd72b5558a1485
```

## Deploy

- Worker: `rustpress-press`
- Host: `press.mazzeleczzare.com` only
- Version ID: `65f981cb-338e-47a1-9aa3-f8a1f47619a4`
- Live: https://press.mazzeleczzare.com/authority-puzzle/

Deploy note: ProtonVPN DNS failed to resolve `api.cloudflare.com`; a temporary
`/etc/hosts` override was used for Wrangler and removed after deploy.
`studio.mazzeleczzare.com` was not a deploy target.
