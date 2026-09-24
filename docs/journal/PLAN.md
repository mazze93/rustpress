# Rustpress delivery

## Active checkpoint: release-integrity regressions (2026-09-24)

Bounded scope: one integration-test module, using the existing sealed fixture.
No production-code, frontend, dependency, DNS, deployment, or CI changes.

1. Commit this plan before implementation.
2. Use the on-device model to draft release-integrity tests in an isolated temporary directory.
3. Review the returned file; run tests, formatting, Clippy, and fixture verification.
4. Record measured results in CHECKPOINT.md and HANDOFF.md, commit, and push.
5. Stop. Next checkpoint is secret-free CI, not production deployment.

Acceptance: verification without mutable source/config; rejection of modified,
missing, extra files and wrong reviewed digest; credential-free, non-mutating
deployment dry-run. Use existing dependencies and never mutate the tracked pressing.

## Delivery roadmap

1. Implement and test a deterministic static corpus compiler.
2. Build Astro before sealing; bind the complete deployment tree and site inputs.
3. Implement digest-pinned deployment, receipts outside pressings, and CI.
4. Build a restrained Studio archive and integrate the existing blog.
5. Validate, commit, push, preview, and deploy only after exact destination review.

No automatic corpus discovery, no publication of existing private material, no DNS replacement.
