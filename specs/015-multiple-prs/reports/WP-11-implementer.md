## WP-11 — done
Tasks:    T029 ✓
Files:
- app/src/components/PrView.tsx — isOutside(pr) helper; outside-repo groups always get a `host/owner/name` heading; "agente" source tag (.pr-source) when linked_by is "agent".
- app/src/components/PrView.test.tsx — 2 tests added (outside heading + agent tag; none for workspace PR).
- app/src/i18n/en/PrView.ts — "agente": "agent" (Spanish is the source text, English lives in en/).
Tests:    cd app && npx tsc --noEmit && npx vitest run src/components/PrView.test.tsx src/i18n: 96 pass. Not run red first.
Notes:    node_modules symlink created and removed. No CSS for .pr-source (outside allowed files; it inherits .pr-meta styling).
