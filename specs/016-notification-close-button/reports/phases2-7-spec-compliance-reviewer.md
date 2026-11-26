## Spec compliance — phases 2-7 (WP-01..WP-08)
Checks run: `npx tsc --noEmit` clean; `npx vitest run`: 116 files, 1404 tests passed.

Covered: all of N1-N12 have the shared X (Notice/CloseButton, `.ntc-x`, name "Cerrar", last child).
- N1 Toast.tsx:195 CloseButton, "×" gone; Cerrar todo untouched.
- N2/N3 GlobalNotices.tsx:83-123 useDismissals; sigs full/hot, reason|since, N; leaving the threshold prunes the id (A->B->A shows again). X only hides, no API.
- N4 UpdateNotice.tsx: upToDate/error -> close, available -> later, confirmSessions -> cancel; checking(manual)/downloading/installing/waiting hide only (sig kind:version, null for other kinds). Text "Cerrar" buttons removed; Más tarde / Cancelar kept.
- N5 App.tsx:347,403 + DisconnectedBanner: banner hidden via useDismissal("offline"); is-offline/inert remain driven by `offline` (not touched); returns on next loss since sig null while online. Reintentar ahora is inside the banner so unreachable when closed (as spec).
- N6 TaskScreen.tsx:121-147 X -> api.dismissNotice + onChanged when notice id, else local hide; "Descartar" text button removed (remaining Descartar at :898 is a dialog, not a notification).
- N7 SetupBanner, N8 HandoffActions, N9 TaskView (limit/failed/waiting/review), N10 SubtaskList, N11 PrView (problem, subtask commits, wrong base, ready hint), N12 JiraView: all hide-only, no API calls, sigs per FR-006.
- FR-007 per-id dismissals; FR-010 IconButton; FR-011 t("Cerrar"). Every touched component has new/updated tests.

Missing: none CRITICAL/HIGH.

Deviates (LOW):
- PrView.tsx:213 subtask-commits warning sig is only the commit count; FR-006 also lists "the base changes". A base change with an unchanged count would not re-show it. (The wrong-base warning at :394 does include the bases.)
- SetupBanner.tsx:50 sig is "failed"/"running"; FR-006 "or a new run" (running -> running, failed -> failed on a new run) is not distinguished. Spec examples (running <-> failed) are covered.
- TaskView.tsx:146-149 waiting sig is the constant "waiting"; it re-shows only because the id is pruned when the status leaves it. Fine for "status changes".

Extra: none beyond the Notice helpers (planned). Jira stale class untouched (jv-body keeps `stale` from `expired`), as required.

TaskView `keeps` bar (TaskView.tsx:253-268): NOT required by the spec. It is not in the inventory N1-N12 (it is the response to a failed archive and carries a decision, "Archivar igualmente"; arguably inline feedback). FR-001 therefore does not apply and it already has a working X (hide-only, `setKeeps(null)`). Only SC-005 ("exactly one visual style of close button") is arguably at odds, since it uses a hand-written `button.ib` with the close icon instead of `.ntc-x`. Converting to `<Notice>` is optional polish; if converted, behaviour is unchanged (hide-only). Not a gap against the spec.

Untested: nothing in scope found without a test.

VERDICT: COMPLIANT (3 LOW notes; 0 CRITICAL, 0 HIGH, 0 MEDIUM)
