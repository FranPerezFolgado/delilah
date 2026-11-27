# Phase 2 (WP-01) architecture review

Verdict: PASS. No CRITICAL or HIGH findings. 2 MEDIUM-ish/LOW notes.

Checked: app/src/components/ui/Notice.tsx, Notice.css, index.ts diff against contracts/ui-notice.md.

Module boundaries / consistency
- Matches the ui kit layout (Name.tsx + Name.css + Name.test.tsx, barrel export in index.ts, uses IconButton from ./Button, i18n via t like ConfirmDialog/Segmented).
- Exports exactly the contract surface. CloseButton has class ntc-x; Notice renders one close button after children.
- CSS: Notice.css is imported after Button (via IconButton import), so `.ntc-x` (same specificity as `.ib`) wins on size; order is import-dependent but deterministic.
- Hooks and component share one file; small (69 lines), acceptable. Could split into useDismissals.ts if it grows.

Hook correctness
- No stale dismissal: `hidden` compares dismissed[id] to current[id] at render time, so a changed signature un-hides immediately, without waiting for the cleanup effect. The effect only prunes state; dep [key] with currentRef is correct (JSON key captures content, ref avoids identity churn).
- Pruning on id removal means a returning id with the same signature shows again (matches "forgets the dismissal").
- dismiss is stable (useCallback, ref read). Writing currentRef during render is a common pattern; fine here.

Findings
[LOW] Notice.tsx:55  `id in current` also matches prototype keys ("constructor", "toString"); harmless since dismissed[id] would not equal a string, but `Object.hasOwn`/`current[id] !== undefined` is cleaner.
[LOW] Notice.tsx:6/14  focusAfter does not skip hidden/offscreen elements (display:none, aria-hidden), so focus may land on an element that cannot take it and silently stay lost. Optional: filter with `el.offsetParent !== null` (not testable in jsdom, so weigh cost) or accept.
[LOW] Notice.tsx:23  Focus moves before onClose; if the "next" element is itself a sibling notice removed by the same state change, focus is lost anyway. Edge case, no action needed.
