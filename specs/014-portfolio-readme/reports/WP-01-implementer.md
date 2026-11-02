## WP-01 — done (GIF not produced: fallback)
Tasks:    T003 ✓, T004 ✗ (timeboxed out, switch reverted, fallback to stills), T005 ✓ (stills; board-*.png kept for now)
Files:    app/e2e/screens.spec.ts — new "Playground (oscuro|claro)" tests + playgroundScene(); seed skips the inbox gate and PR linking only when DELILAH_SCREENS_ONLY=Playground; the pantallas tests skip in that case
          docs/images/playground-light.png (105 KB), docs/images/playground-dark.png (104 KB)
Tests:    `cd app && npx tsc --noEmit` pass. Capture: `DELILAH_SCREENS=despues DELILAH_SCREENS_LANG=en DELILAH_SCREENS_ONLY=Playground DELILAH_SCREENS_OUT=<scratch> npx playwright test screens` passes (3 passed, 11 s).
Notes:
- The image is the task Office tab (LIBRE-6 floor), not the Playground list (the list is only small miniatures). The seed alone showed one working session, so the test routes GET /api/v1/sessions and /api/v1/activity inside the capture (R2): five demo desks on LIBRE-6 (Edit, Read, Bash, waiting, paused) plus a researcher helper. No product code touched.
- FR-004: only demo data visible (signal/delilah fixture projects, LIBRE-n, PROJ-1246, invented file/command names). Viewed both images; nothing real.
- GIF: not attempted past the stills; ~1 h of the budget went on seed drift and Playwright runs that hung ~15-50 min on failure. MOTION switch and frame loop NOT added (revert is a no-op). Existing screens unchanged (reducedMotion still "reduce"). Use the quickstart hand-recording fallback.
- Pre-existing seed drift (not fixed): on a fresh daemon the chat agent no longer leaves the "ls -la" permission card in the inbox and LIBRE-5's PR link never lands, so the full `screens` run fails at the seed. Worked around only for ONLY=Playground. Worth a separate task.
- board-light/dark.png are still referenced by current README.md (lines 12-13); WP-03 rewrites README, so delete them there/after only if the new README doesn't use them. Not deleted.
- A failing Playwright run here can hang for a long time; kill nothing by name, just wait or use --timeout.
