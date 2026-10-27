# Phase 4 contract review (32af6c3..f2bec6a)

Findings: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 3 LOW.

[OK] PR DTO `method`, `base_branch`, `expected_base` — crates/delilah-daemon/src/pr_api/dto.rs:44-50 <-> app/src/api.ts:745-751
All three are additive and serialized as `null` (no skip). The TS fields are optional, so old and new daemons both parse. Every PrView producer (task_prs, link, open, merge) goes through `PrView::with`. `From<&PrRow>` leaves method/expected_base null and is used only inside `with`. Test coverage: PrView tests in merge.rs (JSON) and PrView.test.tsx (UI).

[OK] Merge default method — pr_api/merge.rs:~112 <-> app/src/components/PrView.tsx:329
Server fallback (`ctx.method(...)`, else the first allowed) agrees with the UI (`pr.method ?? merge_methods[0]`). An explicit request method is still validated against the allowed list. The contract shape is unchanged.

[OK] base-status `base` — base_update.rs:28 <-> api.ts:781
The field already existed. The subtask value `origin/<parent branch>` is the same shape. A new 409 `parent-task-not-started` or `parent-task-branch-missing` can now come from base-status/update-branch. The only caller (TaskScreen.tsx:93) swallows errors.

[RISK, LOW] `method` TS type — api.ts:750
The server sends `null` (when the methods are unknown), but the type is `method?: MergeMethod` without `| null`. The code uses `??`, so it is harmless. Fix: `method?: MergeMethod | null`.

[RISK, LOW] `subtask_commits` — contracts/api.md vs BaseStatus (base_update.rs:28)
The contract lists `subtask_commits` on base-status. The Rust struct does not emit it, and the TS field is optional. This is probably a later work package. Fix: confirm it is scheduled, or drop it from the contract.

[RISK, LOW] `expected_base` is silently `null` when `base_for` errors (dto.rs), so the mis-base warning is suppressed. This is by design (commented), but it is worth a note in the contract.

Contract text says base-status `base: string`. Actual is `string | null`, which the TS type already reflects. Update the doc.

VERDICT: ✅ COMPATIBLE
